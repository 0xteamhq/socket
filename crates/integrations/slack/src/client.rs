//! Slack's Web API as typed methods, grouped the way Slack groups them.
//!
//! Identifiers (a channel id, a user id, a message timestamp) are plain
//! arguments. Content and optional filters are structs from [`crate::models`].
//! Reads are sent as GET with query parameters; writes as POST with a JSON
//! body, so the transport never repeats a write that may have happened.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use socketkit_core::{Connection, Error, ErrorKind, Page, RawRequest, Result};

use crate::models::{
    Bookmark, Channel, CreateConversation, DndStatus, Emoji, File, History, ListConversations, ListFiles,
    ListScheduled, ListUserGroups, Message, Paging, Pin, PostMessage, PostedMessage, Presence, Profile, Reaction,
    Reminder, ScheduledMessage, Search, SearchResults, Team, UpdateMessage, User, UserGroup,
};

/// One connection's access to Slack's Web API.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Api<'a> {
    pub(crate) connection: &'a Connection,
}

impl Api<'_> {
    fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message).with_provider(self.connection.provider().id.clone())
    }

    /// Calls a method that reads, with `arguments` as query parameters.
    async fn get(&self, method: &str, arguments: Value) -> Result<Value> {
        let mut request = RawRequest::get(method);
        for (name, value) in arguments.as_object().into_iter().flatten() {
            let text = match value {
                Value::Null => continue,
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            request = request.with_query(name.as_str(), text);
        }
        Ok(self.connection.request(request).await?.body)
    }

    /// Calls a method that writes, with `arguments` as a JSON body.
    async fn post(&self, method: &str, arguments: Value) -> Result<Value> {
        Ok(self.connection.request(RawRequest::post(method, arguments)).await?.body)
    }

    /// Reads one field of a response as `T`.
    fn field<T: DeserializeOwned>(&self, body: &Value, name: &str) -> Result<T> {
        let value = body
            .get(name)
            .filter(|v| !v.is_null())
            .ok_or_else(|| self.error(ErrorKind::Decode, format!("slack answered without `{name}`")))?;
        serde_json::from_value(value.clone()).map_err(|e| {
            self.error(
                ErrorKind::Decode,
                format!("slack sent a `{name}` that could not be read"),
            )
            .with_source(e)
        })
    }

    /// Reads a list field and the cursor for the page after it.
    fn page<T: DeserializeOwned>(&self, body: &Value, name: &str) -> Result<Page<T>> {
        let next_cursor = body["response_metadata"]["next_cursor"]
            .as_str()
            .filter(|c| !c.is_empty())
            .map(str::to_owned);
        Ok(Page {
            items: self.field(body, name)?,
            next_cursor,
        })
    }

    fn required(&self, what: &str, value: &str) -> Result<()> {
        if value.trim().is_empty() {
            return Err(self.error(ErrorKind::InvalidInput, format!("{what} is required")));
        }
        Ok(())
    }
}

/// `base` with the set fields of `options` added. Unset options are left out,
/// so Slack applies its own defaults.
fn with(base: Value, options: &impl Serialize) -> Value {
    let mut merged = match base {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    if let Ok(Value::Object(extra)) = serde_json::to_value(options) {
        merged.extend(extra.into_iter().filter(|(_, value)| !value.is_null()));
    }
    Value::Object(merged)
}

macro_rules! group {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy)]
        pub struct $name<'a>(pub(crate) Api<'a>);
    };
}

group!(
    /// Posting, changing and scheduling messages.
    Chat
);
group!(
    /// Channels, direct messages, and what is in them.
    Conversations
);
group!(
    /// The members of the workspace.
    Users
);
group!(
    /// Emoji reactions on messages.
    Reactions
);
group!(
    /// Items pinned to a channel.
    Pins
);
group!(
    /// Files shared in the workspace.
    Files
);
group!(
    /// Searching messages. Needs a user token; Slack does not let a bot search.
    SearchApi
);
group!(
    /// Reminders. Needs a user token.
    Reminders
);
group!(
    /// Bookmarks at the top of a channel.
    Bookmarks
);
group!(
    /// User groups such as `@engineering`.
    UserGroups
);
group!(
    /// The workspace itself, its emoji, and Do Not Disturb.
    Workspace
);

impl Chat<'_> {
    /// Posts a message to a channel, a direct message, or a thread.
    pub async fn post_message(&self, channel: &str, message: PostMessage) -> Result<PostedMessage> {
        self.0.required("a channel", channel)?;
        self.content(&message)?;
        let body = self
            .0
            .post("chat.postMessage", with(json!({ "channel": channel }), &message))
            .await?;
        self.posted(body)
    }

    /// Posts a message only `user` can see. Returns its timestamp.
    pub async fn post_ephemeral(&self, channel: &str, user: &str, message: PostMessage) -> Result<String> {
        self.0.required("a channel", channel)?;
        self.0.required("a user", user)?;
        self.content(&message)?;
        let arguments = with(json!({ "channel": channel, "user": user }), &message);
        let body = self.0.post("chat.postEphemeral", arguments).await?;
        self.0.field(&body, "message_ts")
    }

    /// Replaces the content of a message.
    pub async fn update(&self, channel: &str, ts: &str, message: UpdateMessage) -> Result<PostedMessage> {
        self.0.required("a channel", channel)?;
        self.0.required("a message timestamp", ts)?;
        let body = self
            .0
            .post("chat.update", with(json!({ "channel": channel, "ts": ts }), &message))
            .await?;
        self.posted(body)
    }

    /// Deletes a message.
    pub async fn delete(&self, channel: &str, ts: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0.required("a message timestamp", ts)?;
        self.0
            .post("chat.delete", json!({ "channel": channel, "ts": ts }))
            .await
            .map(drop)
    }

    /// Schedules a message for `post_at`, in seconds since the Unix epoch.
    pub async fn schedule_message(
        &self,
        channel: &str,
        post_at: i64,
        message: PostMessage,
    ) -> Result<ScheduledMessage> {
        self.0.required("a channel", channel)?;
        self.content(&message)?;
        let arguments = with(json!({ "channel": channel, "post_at": post_at }), &message);
        let body = self.0.post("chat.scheduleMessage", arguments).await?;
        let mut scheduled: ScheduledMessage = serde_json::from_value(body).map_err(|e| {
            self.0
                .error(
                    ErrorKind::Decode,
                    "slack sent a scheduled message that could not be read",
                )
                .with_source(e)
        })?;
        if scheduled.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "slack answered without a scheduled message id"));
        }
        if scheduled.text.is_none() {
            scheduled.text = message.text;
        }
        Ok(scheduled)
    }

    /// Cancels a scheduled message.
    pub async fn delete_scheduled_message(&self, channel: &str, scheduled_message_id: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0.required("a scheduled message id", scheduled_message_id)?;
        let arguments = json!({ "channel": channel, "scheduled_message_id": scheduled_message_id });
        self.0.post("chat.deleteScheduledMessage", arguments).await.map(drop)
    }

    /// Lists messages that are scheduled and not yet posted.
    pub async fn scheduled_messages(&self, options: ListScheduled) -> Result<Page<ScheduledMessage>> {
        let body = self
            .0
            .post("chat.scheduledMessages.list", with(json!({}), &options))
            .await?;
        self.0.page(&body, "scheduled_messages")
    }

    /// The permanent link to a message.
    pub async fn permalink(&self, channel: &str, message_ts: &str) -> Result<String> {
        self.0.required("a channel", channel)?;
        self.0.required("a message timestamp", message_ts)?;
        let body = self
            .0
            .get(
                "chat.getPermalink",
                json!({ "channel": channel, "message_ts": message_ts }),
            )
            .await?;
        self.0.field(&body, "permalink")
    }

    fn content(&self, message: &PostMessage) -> Result<()> {
        if message.has_content() {
            Ok(())
        } else {
            Err(self
                .0
                .error(ErrorKind::InvalidInput, "a message needs text, blocks or attachments"))
        }
    }

    fn posted(&self, body: Value) -> Result<PostedMessage> {
        let posted: PostedMessage = serde_json::from_value(body).map_err(|e| {
            self.0
                .error(ErrorKind::Decode, "slack sent a message that could not be read")
                .with_source(e)
        })?;
        if posted.channel.is_empty() || posted.ts.is_empty() {
            return Err(self.0.error(
                ErrorKind::Decode,
                "slack answered without the message's channel and timestamp",
            ));
        }
        Ok(posted)
    }
}

impl Conversations<'_> {
    /// Lists conversations the token can see.
    pub async fn list(&self, options: ListConversations) -> Result<Page<Channel>> {
        let body = self.0.get("conversations.list", with(json!({}), &options)).await?;
        self.0.page(&body, "channels")
    }

    /// One conversation, with its member count.
    pub async fn info(&self, channel: &str) -> Result<Channel> {
        self.0.required("a channel", channel)?;
        let arguments = json!({ "channel": channel, "include_num_members": true });
        let body = self.0.get("conversations.info", arguments).await?;
        self.channel(&body)
    }

    /// The messages of a conversation, newest first.
    pub async fn history(&self, channel: &str, options: History) -> Result<Page<Message>> {
        self.0.required("a channel", channel)?;
        let body = self
            .0
            .get("conversations.history", with(json!({ "channel": channel }), &options))
            .await?;
        self.0.page(&body, "messages")
    }

    /// The messages of one thread, starting with the message that began it.
    pub async fn replies(&self, channel: &str, thread_ts: &str, options: History) -> Result<Page<Message>> {
        self.0.required("a channel", channel)?;
        self.0.required("a thread timestamp", thread_ts)?;
        let arguments = with(json!({ "channel": channel, "ts": thread_ts }), &options);
        let body = self.0.get("conversations.replies", arguments).await?;
        self.0.page(&body, "messages")
    }

    /// The ids of a conversation's members.
    pub async fn members(&self, channel: &str, paging: Paging) -> Result<Page<String>> {
        self.0.required("a channel", channel)?;
        let body = self
            .0
            .get("conversations.members", with(json!({ "channel": channel }), &paging))
            .await?;
        self.0.page(&body, "members")
    }

    /// Creates a channel.
    pub async fn create(&self, name: &str, options: CreateConversation) -> Result<Channel> {
        self.0.required("a channel name", name)?;
        let body = self
            .0
            .post("conversations.create", with(json!({ "name": name }), &options))
            .await?;
        self.channel(&body)
    }

    /// Joins a public channel.
    pub async fn join(&self, channel: &str) -> Result<Channel> {
        self.0.required("a channel", channel)?;
        let body = self.0.post("conversations.join", json!({ "channel": channel })).await?;
        self.channel(&body)
    }

    /// Leaves a conversation.
    pub async fn leave(&self, channel: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0
            .post("conversations.leave", json!({ "channel": channel }))
            .await
            .map(drop)
    }

    /// Invites members to a channel.
    pub async fn invite(&self, channel: &str, users: &[String]) -> Result<Channel> {
        self.0.required("a channel", channel)?;
        let body = self
            .0
            .post(
                "conversations.invite",
                json!({ "channel": channel, "users": self.ids(users)? }),
            )
            .await?;
        self.channel(&body)
    }

    /// Removes a member from a channel.
    pub async fn kick(&self, channel: &str, user: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0.required("a user", user)?;
        self.0
            .post("conversations.kick", json!({ "channel": channel, "user": user }))
            .await
            .map(drop)
    }

    /// Archives a channel.
    pub async fn archive(&self, channel: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0
            .post("conversations.archive", json!({ "channel": channel }))
            .await
            .map(drop)
    }

    /// Restores an archived channel.
    pub async fn unarchive(&self, channel: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0
            .post("conversations.unarchive", json!({ "channel": channel }))
            .await
            .map(drop)
    }

    /// Renames a channel.
    pub async fn rename(&self, channel: &str, name: &str) -> Result<Channel> {
        self.0.required("a channel", channel)?;
        self.0.required("a channel name", name)?;
        let body = self
            .0
            .post("conversations.rename", json!({ "channel": channel, "name": name }))
            .await?;
        self.channel(&body)
    }

    /// Sets a channel's topic.
    pub async fn set_topic(&self, channel: &str, topic: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0
            .post("conversations.setTopic", json!({ "channel": channel, "topic": topic }))
            .await
            .map(drop)
    }

    /// Sets a channel's purpose.
    pub async fn set_purpose(&self, channel: &str, purpose: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0
            .post(
                "conversations.setPurpose",
                json!({ "channel": channel, "purpose": purpose }),
            )
            .await
            .map(drop)
    }

    /// Opens a direct message with one member, or a group direct message with several.
    pub async fn open(&self, users: &[String]) -> Result<Channel> {
        let body = self
            .0
            .post(
                "conversations.open",
                json!({ "users": self.ids(users)?, "return_im": true }),
            )
            .await?;
        self.channel(&body)
    }

    /// Marks a conversation as read up to a message.
    pub async fn mark(&self, channel: &str, ts: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0.required("a message timestamp", ts)?;
        self.0
            .post("conversations.mark", json!({ "channel": channel, "ts": ts }))
            .await
            .map(drop)
    }

    fn channel(&self, body: &Value) -> Result<Channel> {
        let channel: Channel = self.0.field(body, "channel")?;
        if channel.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "slack answered with a channel that has no id"));
        }
        Ok(channel)
    }

    /// Slack takes several ids as one comma-separated string.
    fn ids(&self, users: &[String]) -> Result<String> {
        let ids: Vec<&str> = users.iter().map(|u| u.trim()).filter(|u| !u.is_empty()).collect();
        if ids.is_empty() || ids.iter().any(|id| id.contains(',')) {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "at least one user id is required, one per entry",
            ));
        }
        Ok(ids.join(","))
    }
}

impl Users<'_> {
    /// Lists the members of the workspace.
    pub async fn list(&self, paging: Paging) -> Result<Page<User>> {
        let body = self.0.get("users.list", with(json!({}), &paging)).await?;
        self.0.page(&body, "members")
    }

    /// One member.
    pub async fn info(&self, user: &str) -> Result<User> {
        self.0.required("a user", user)?;
        let body = self.0.get("users.info", json!({ "user": user })).await?;
        self.user(&body)
    }

    /// The member with this email address. Needs the `users:read.email` scope.
    pub async fn lookup_by_email(&self, email: &str) -> Result<User> {
        self.0.required("an email address", email)?;
        let body = self.0.get("users.lookupByEmail", json!({ "email": email })).await?;
        self.user(&body)
    }

    /// Whether a member is active.
    pub async fn presence(&self, user: &str) -> Result<Presence> {
        self.0.required("a user", user)?;
        let body = self.0.get("users.getPresence", json!({ "user": user })).await?;
        let presence: Presence = serde_json::from_value(body).map_err(|e| {
            self.0
                .error(ErrorKind::Decode, "slack sent a presence that could not be read")
                .with_source(e)
        })?;
        if presence.presence.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "slack answered without a presence"));
        }
        Ok(presence)
    }

    /// A member's profile.
    pub async fn profile(&self, user: &str) -> Result<Profile> {
        self.0.required("a user", user)?;
        let body = self.0.get("users.profile.get", json!({ "user": user })).await?;
        self.0.field(&body, "profile")
    }

    fn user(&self, body: &Value) -> Result<User> {
        let user: User = self.0.field(body, "user")?;
        if user.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "slack answered with a user that has no id"));
        }
        Ok(user)
    }
}

impl Reactions<'_> {
    /// Adds an emoji reaction to a message. `name` is the emoji's name without colons.
    pub async fn add(&self, channel: &str, timestamp: &str, name: &str) -> Result<()> {
        self.0
            .post("reactions.add", self.target(channel, timestamp, Some(name))?)
            .await
            .map(drop)
    }

    /// Removes the token's own reaction from a message.
    pub async fn remove(&self, channel: &str, timestamp: &str, name: &str) -> Result<()> {
        self.0
            .post("reactions.remove", self.target(channel, timestamp, Some(name))?)
            .await
            .map(drop)
    }

    /// The reactions on a message.
    pub async fn get(&self, channel: &str, timestamp: &str) -> Result<Vec<Reaction>> {
        let mut arguments = self.target(channel, timestamp, None)?;
        arguments["full"] = json!(true);
        let body = self.0.get("reactions.get", arguments).await?;
        let message: Message = self.0.field(&body, "message")?;
        Ok(message.reactions)
    }

    fn target(&self, channel: &str, timestamp: &str, name: Option<&str>) -> Result<Value> {
        self.0.required("a channel", channel)?;
        self.0.required("a message timestamp", timestamp)?;
        let mut arguments = json!({ "channel": channel, "timestamp": timestamp });
        if let Some(name) = name {
            let name = name.trim().trim_matches(':');
            self.0.required("an emoji name", name)?;
            arguments["name"] = json!(name);
        }
        Ok(arguments)
    }
}

impl Pins<'_> {
    /// Pins a message to its channel.
    pub async fn add(&self, channel: &str, timestamp: &str) -> Result<()> {
        self.0
            .post("pins.add", self.target(channel, timestamp)?)
            .await
            .map(drop)
    }

    /// Unpins a message.
    pub async fn remove(&self, channel: &str, timestamp: &str) -> Result<()> {
        self.0
            .post("pins.remove", self.target(channel, timestamp)?)
            .await
            .map(drop)
    }

    /// What is pinned to a channel.
    pub async fn list(&self, channel: &str) -> Result<Vec<Pin>> {
        self.0.required("a channel", channel)?;
        let body = self.0.get("pins.list", json!({ "channel": channel })).await?;
        self.0.field(&body, "items")
    }

    fn target(&self, channel: &str, timestamp: &str) -> Result<Value> {
        self.0.required("a channel", channel)?;
        self.0.required("a message timestamp", timestamp)?;
        Ok(json!({ "channel": channel, "timestamp": timestamp }))
    }
}

impl Files<'_> {
    /// One file's details.
    pub async fn info(&self, file: &str) -> Result<File> {
        self.0.required("a file", file)?;
        let body = self.0.get("files.info", json!({ "file": file })).await?;
        let file: File = self.0.field(&body, "file")?;
        if file.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "slack answered with a file that has no id"));
        }
        Ok(file)
    }

    /// Lists files, optionally those of one channel or member.
    pub async fn list(&self, options: ListFiles) -> Result<Vec<File>> {
        let body = self.0.get("files.list", with(json!({}), &options)).await?;
        self.0.field(&body, "files")
    }

    /// Deletes a file.
    pub async fn delete(&self, file: &str) -> Result<()> {
        self.0.required("a file", file)?;
        self.0.post("files.delete", json!({ "file": file })).await.map(drop)
    }
}

impl SearchApi<'_> {
    /// Searches messages, using Slack's own query syntax (`in:#channel`, `from:@user`, …).
    pub async fn messages(&self, query: &str, options: Search) -> Result<SearchResults> {
        self.0.required("a search query", query)?;
        let body = self
            .0
            .get("search.messages", with(json!({ "query": query }), &options))
            .await?;
        self.0.field(&body, "messages")
    }
}

impl Reminders<'_> {
    /// Creates a reminder. `time` is a Unix timestamp, a number of seconds
    /// from now, or words such as `"in 15 minutes"` or `"every Thursday"`.
    pub async fn add(&self, text: &str, time: &str) -> Result<Reminder> {
        self.0.required("the reminder's text", text)?;
        self.0.required("a time", time)?;
        let body = self
            .0
            .post("reminders.add", json!({ "text": text, "time": time }))
            .await?;
        self.reminder(&body)
    }

    /// The reminders the token's user created or was sent.
    pub async fn list(&self) -> Result<Vec<Reminder>> {
        let body = self.0.get("reminders.list", json!({})).await?;
        self.0.field(&body, "reminders")
    }

    /// Deletes a reminder.
    pub async fn delete(&self, reminder: &str) -> Result<()> {
        self.0.required("a reminder", reminder)?;
        self.0
            .post("reminders.delete", json!({ "reminder": reminder }))
            .await
            .map(drop)
    }

    /// Marks a reminder as done.
    pub async fn complete(&self, reminder: &str) -> Result<()> {
        self.0.required("a reminder", reminder)?;
        self.0
            .post("reminders.complete", json!({ "reminder": reminder }))
            .await
            .map(drop)
    }

    fn reminder(&self, body: &Value) -> Result<Reminder> {
        let reminder: Reminder = self.0.field(body, "reminder")?;
        if reminder.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "slack answered with a reminder that has no id"));
        }
        Ok(reminder)
    }
}

impl Bookmarks<'_> {
    /// Adds a link bookmark to a channel. `emoji` is optional, written `:book:`.
    pub async fn add(&self, channel: &str, title: &str, link: &str, emoji: Option<&str>) -> Result<Bookmark> {
        self.0.required("a channel", channel)?;
        self.0.required("a title", title)?;
        self.0.required("a link", link)?;
        let mut arguments = json!({ "channel_id": channel, "title": title, "type": "link", "link": link });
        if let Some(emoji) = emoji {
            arguments["emoji"] = json!(emoji);
        }
        let body = self.0.post("bookmarks.add", arguments).await?;
        let bookmark: Bookmark = self.0.field(&body, "bookmark")?;
        if bookmark.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "slack answered with a bookmark that has no id"));
        }
        Ok(bookmark)
    }

    /// A channel's bookmarks.
    pub async fn list(&self, channel: &str) -> Result<Vec<Bookmark>> {
        self.0.required("a channel", channel)?;
        let body = self.0.post("bookmarks.list", json!({ "channel_id": channel })).await?;
        self.0.field(&body, "bookmarks")
    }

    /// Removes a bookmark.
    pub async fn remove(&self, channel: &str, bookmark: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0.required("a bookmark", bookmark)?;
        self.0
            .post(
                "bookmarks.remove",
                json!({ "channel_id": channel, "bookmark_id": bookmark }),
            )
            .await
            .map(drop)
    }
}

impl UserGroups<'_> {
    /// Lists user groups.
    pub async fn list(&self, options: ListUserGroups) -> Result<Vec<UserGroup>> {
        let body = self.0.get("usergroups.list", with(json!({}), &options)).await?;
        self.0.field(&body, "usergroups")
    }

    /// The ids of a user group's members.
    pub async fn members(&self, usergroup: &str) -> Result<Vec<String>> {
        self.0.required("a user group", usergroup)?;
        let body = self
            .0
            .get("usergroups.users.list", json!({ "usergroup": usergroup }))
            .await?;
        self.0.field(&body, "users")
    }
}

impl Workspace<'_> {
    /// The workspace's name and domain.
    pub async fn info(&self) -> Result<Team> {
        let body = self.0.get("team.info", json!({})).await?;
        let team: Team = self.0.field(&body, "team")?;
        if team.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "slack answered with a workspace that has no id"));
        }
        Ok(team)
    }

    /// The workspace's custom emoji.
    pub async fn emoji(&self) -> Result<Emoji> {
        let body = self.0.get("emoji.list", json!({})).await?;
        self.0.field(&body, "emoji")
    }

    /// A member's Do Not Disturb state.
    pub async fn dnd_info(&self, user: &str) -> Result<DndStatus> {
        self.0.required("a user", user)?;
        let body = self.0.get("dnd.info", json!({ "user": user })).await?;
        self.dnd(body)
    }

    /// Turns on Do Not Disturb for the token's user for `minutes`.
    pub async fn dnd_set_snooze(&self, minutes: u32) -> Result<DndStatus> {
        if minutes == 0 {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "the number of minutes must be at least 1"));
        }
        // This method takes its argument as a query parameter even though it writes.
        let request = RawRequest::new("POST", "dnd.setSnooze").with_query("num_minutes", minutes.to_string());
        let body = self.0.connection.request(request).await?.body;
        self.dnd(body)
    }

    /// Turns off the token's user's Do Not Disturb snooze.
    pub async fn dnd_end_snooze(&self) -> Result<DndStatus> {
        let body = self.0.post("dnd.endSnooze", json!({})).await?;
        self.dnd(body)
    }

    fn dnd(&self, body: Value) -> Result<DndStatus> {
        serde_json::from_value(body).map_err(|e| {
            self.0
                .error(
                    ErrorKind::Decode,
                    "slack sent a Do Not Disturb state that could not be read",
                )
                .with_source(e)
        })
    }
}
