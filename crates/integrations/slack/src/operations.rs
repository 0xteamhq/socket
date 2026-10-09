//! Every typed method as a named operation.
//!
//! An operation's input is a JSON object with the method's plain arguments
//! and its options side by side: `{ "channel": "C1", "text": "hi" }`. Both
//! schemas are generated from the same types the typed methods use, so the
//! two ways of calling cannot drift apart.

use std::future::Future;
use std::sync::OnceLock;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use socketkit_core::{Connection, Effect, Error, ErrorKind, OperationInfo, Page, Result, schema_of};

use crate::Slack;
use crate::models::{
    Bookmark, Channel, CreateConversation, DndStatus, Emoji, File, History, ListConversations, ListFiles,
    ListScheduled, ListUserGroups, Message, Paging, Pin, PostMessage, PostedMessage, Presence, Profile, Reaction,
    Reminder, ScheduledMessage, Search, SearchResults, Team, UpdateMessage, User, UserGroup,
};

type Running = std::pin::Pin<Box<dyn Future<Output = Result<Value>> + Send>>;

/// One named operation: what it says about itself, and how to run it.
pub(crate) struct Operation {
    pub(crate) info: OperationInfo,
    run: Box<dyn Fn(Slack, Connection, Value) -> Running + Send + Sync>,
}

impl Operation {
    pub(crate) fn run(&self, slack: Slack, connection: Connection, input: Value) -> Running {
        (self.run)(slack, connection, input)
    }
}

fn invalid(message: String) -> Error {
    Error::new(ErrorKind::InvalidInput, message)
}

/// Builds an operation from a typed handler. The input type gives the input
/// schema and the parsing; the output type gives the output schema.
fn operation<I, O, F, Fut>(name: &str, description: &str, effect: Effect, scopes: &[&str], handler: F) -> Operation
where
    I: DeserializeOwned + JsonSchema + Send + 'static,
    O: Serialize + JsonSchema + 'static,
    F: Fn(Slack, Connection, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<O>> + Send + 'static,
{
    let info = OperationInfo {
        name: format!("slack.{name}"),
        description: description.to_owned(),
        input_schema: schema_of::<I>(),
        output_schema: schema_of::<O>(),
        effect,
        required_scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
    };
    let run = move |slack: Slack, connection: Connection, input: Value| -> Running {
        let provider = connection.provider().id.clone();
        match serde_path_to_error::deserialize::<_, I>(input) {
            Err(e) => {
                // serde's own message quotes the offending value, which may be
                // message text or a credential. Only the field's name, which
                // comes from our own types, goes into the error.
                let path = e.path().to_string();
                let inner = e.inner().to_string();
                let message = if inner.starts_with("missing field") {
                    inner
                } else if path == "." {
                    "the input has a field of the wrong type".to_owned()
                } else {
                    format!("`{path}` has the wrong type")
                };
                Box::pin(std::future::ready(Err(invalid(message).with_provider(provider))))
            }
            Ok(input) => {
                let output = handler(slack, connection, input);
                Box::pin(async move {
                    serde_json::to_value(output.await?).map_err(|e| {
                        Error::new(ErrorKind::Unexpected, "could not encode the result")
                            .with_provider(provider)
                            .with_source(e)
                    })
                })
            }
        }
    };
    Operation {
        info,
        run: Box::new(run),
    }
}

/// Defines an operation's input: its plain arguments, and optionally one
/// options struct whose fields sit beside them.
macro_rules! input {
    ($name:ident { $($(#[$doc:meta])* $field:ident : $kind:ty),* $(,)? }) => {
        #[derive(Debug, Deserialize, JsonSchema)]
        struct $name { $($(#[$doc])* $field: $kind,)* }
    };
    ($name:ident { $($(#[$doc:meta])* $field:ident : $kind:ty),* $(,)? } + $options:ty) => {
        #[derive(Debug, Deserialize, JsonSchema)]
        struct $name {
            $($(#[$doc])* $field: $kind,)*
            #[serde(flatten)]
            options: $options,
        }
    };
}

input!(Nothing {});
input!(InChannel {
    /// A channel id such as `C0123ABCD`.
    channel: String
});
input!(
    ChannelPost {
        /// A channel id, or a user id to message that person directly.
        channel: String
    } + PostMessage
);
input!(
    ChannelUserPost {
        channel: String,
        /// The member who will see the message.
        user: String
    } + PostMessage
);
input!(
    ChannelTsUpdate {
        channel: String,
        /// The timestamp of the message to change.
        ts: String
    } + UpdateMessage
);
input!(ChannelTs {
    channel: String,
    /// A message timestamp such as `1712345678.000100`.
    ts: String
});
input!(
    ChannelSchedule {
        channel: String,
        /// When to post, in seconds since the Unix epoch.
        post_at: i64
    } + PostMessage
);
input!(ChannelScheduled {
    channel: String,
    scheduled_message_id: String
});
input!(Permalink {
    channel: String,
    message_ts: String
});
input!(ScheduledList {} + ListScheduled);
input!(ConversationList {} + ListConversations);
input!(ChannelHistory { channel: String } + History);
input!(
    ThreadHistory {
        channel: String,
        /// The timestamp of the message that started the thread.
        thread_ts: String
    } + History
);
input!(ChannelPaging { channel: String } + Paging);
input!(
    Create {
        /// Lowercase, no spaces, at most 80 characters.
        name: String
    } + CreateConversation
);
input!(ChannelUsers {
    channel: String,
    /// User ids.
    users: Vec<String>
});
input!(ChannelUser {
    channel: String,
    user: String
});
input!(ChannelName {
    channel: String,
    name: String
});
input!(ChannelTopic {
    channel: String,
    topic: String
});
input!(ChannelPurpose {
    channel: String,
    purpose: String
});
input!(Open {
    /// One user id for a direct message, several for a group direct message.
    users: Vec<String>
});
input!(UserList {} + Paging);
input!(OneUser {
    /// A user id such as `U0123ABCD`.
    user: String
});
input!(Email { email: String });
input!(React {
    channel: String,
    /// The timestamp of the message.
    timestamp: String,
    /// The emoji's name, with or without colons.
    name: String
});
input!(ChannelTimestamp {
    channel: String,
    timestamp: String
});
input!(OneFile {
    /// A file id such as `F0123ABCD`.
    file: String
});
input!(FileList {} + ListFiles);
input!(
    SearchQuery {
        /// Slack's search syntax, for example `in:#general from:@ada deploy`.
        query: String
    } + Search
);
input!(AddReminder {
    text: String,
    /// A Unix timestamp, seconds from now, or words such as `in 15 minutes`.
    time: String
});
input!(OneReminder { reminder: String });
input!(AddBookmark {
    channel: String,
    title: String,
    link: String,
    /// Written with colons, for example `:book:`.
    emoji: Option<String>
});
input!(RemoveBookmark {
    channel: String,
    bookmark: String
});
input!(UserGroupList {} + ListUserGroups);
input!(OneUserGroup {
    /// A user group id such as `S0123ABCD`.
    usergroup: String
});
input!(Snooze {
    /// How long to snooze, in minutes.
    minutes: u32
});

// `Destructive` is anything that deletes, removes or overwrites what was
// there, or takes away someone's access. A host uses it to ask a person first.
use Effect::{Destructive, Read, Write};

/// Every Slack operation except identity and resource lookup, which the
/// integration adds itself.
pub(crate) fn all() -> &'static [Operation] {
    static ALL: OnceLock<Vec<Operation>> = OnceLock::new();
    ALL.get_or_init(build)
}

#[rustfmt::skip]
fn build() -> Vec<Operation> {
    vec![
        // ── chat ──
        operation("chat.post_message", "Post a message to a channel, a direct message or a thread.", Write, &["chat:write"],
            |s: Slack, c: Connection, i: ChannelPost| async move { s.chat(&c).post_message(&i.channel, i.options).await as Result<PostedMessage> }),
        operation("chat.post_ephemeral", "Post a message that only one member can see. Returns its timestamp.", Write, &["chat:write"],
            |s: Slack, c: Connection, i: ChannelUserPost| async move { s.chat(&c).post_ephemeral(&i.channel, &i.user, i.options).await as Result<String> }),
        operation("chat.update", "Replace the content of a message.", Destructive, &["chat:write"],
            |s: Slack, c: Connection, i: ChannelTsUpdate| async move { s.chat(&c).update(&i.channel, &i.ts, i.options).await as Result<PostedMessage> }),
        operation("chat.delete", "Delete a message.", Destructive, &["chat:write"],
            |s: Slack, c: Connection, i: ChannelTs| async move { s.chat(&c).delete(&i.channel, &i.ts).await as Result<()> }),
        operation("chat.schedule_message", "Schedule a message to be posted later.", Write, &["chat:write"],
            |s: Slack, c: Connection, i: ChannelSchedule| async move { s.chat(&c).schedule_message(&i.channel, i.post_at, i.options).await as Result<ScheduledMessage> }),
        operation("chat.delete_scheduled_message", "Cancel a scheduled message.", Destructive, &["chat:write"],
            |s: Slack, c: Connection, i: ChannelScheduled| async move { s.chat(&c).delete_scheduled_message(&i.channel, &i.scheduled_message_id).await as Result<()> }),
        operation("chat.scheduled_messages", "List messages that are scheduled and not yet posted.", Read, &[],
            |s: Slack, c: Connection, i: ScheduledList| async move { s.chat(&c).scheduled_messages(i.options).await as Result<Page<ScheduledMessage>> }),
        operation("chat.permalink", "Get the permanent link to a message.", Read, &[],
            |s: Slack, c: Connection, i: Permalink| async move { s.chat(&c).permalink(&i.channel, &i.message_ts).await as Result<String> }),

        // ── conversations ──
        operation("conversations.list", "List the conversations the token can see.", Read, &["channels:read"],
            |s: Slack, c: Connection, i: ConversationList| async move { s.conversations(&c).list(i.options).await as Result<Page<Channel>> }),
        operation("conversations.info", "Get one conversation, with its member count.", Read, &["channels:read"],
            |s: Slack, c: Connection, i: InChannel| async move { s.conversations(&c).info(&i.channel).await as Result<Channel> }),
        operation("conversations.history", "Read the messages of a conversation, newest first.", Read, &["channels:history"],
            |s: Slack, c: Connection, i: ChannelHistory| async move { s.conversations(&c).history(&i.channel, i.options).await as Result<Page<Message>> }),
        operation("conversations.replies", "Read the messages of one thread.", Read, &["channels:history"],
            |s: Slack, c: Connection, i: ThreadHistory| async move { s.conversations(&c).replies(&i.channel, &i.thread_ts, i.options).await as Result<Page<Message>> }),
        operation("conversations.members", "List the ids of a conversation's members.", Read, &["channels:read"],
            |s: Slack, c: Connection, i: ChannelPaging| async move { s.conversations(&c).members(&i.channel, i.options).await as Result<Page<String>> }),
        operation("conversations.create", "Create a channel.", Write, &["channels:manage"],
            |s: Slack, c: Connection, i: Create| async move { s.conversations(&c).create(&i.name, i.options).await as Result<Channel> }),
        operation("conversations.join", "Join a public channel.", Write, &["channels:join"],
            |s: Slack, c: Connection, i: InChannel| async move { s.conversations(&c).join(&i.channel).await as Result<Channel> }),
        operation("conversations.leave", "Leave a conversation.", Destructive, &["channels:manage"],
            |s: Slack, c: Connection, i: InChannel| async move { s.conversations(&c).leave(&i.channel).await as Result<()> }),
        operation("conversations.invite", "Invite members to a channel.", Write, &["channels:manage"],
            |s: Slack, c: Connection, i: ChannelUsers| async move { s.conversations(&c).invite(&i.channel, &i.users).await as Result<Channel> }),
        operation("conversations.kick", "Remove a member from a channel.", Destructive, &["channels:manage"],
            |s: Slack, c: Connection, i: ChannelUser| async move { s.conversations(&c).kick(&i.channel, &i.user).await as Result<()> }),
        operation("conversations.archive", "Archive a channel.", Destructive, &["channels:manage"],
            |s: Slack, c: Connection, i: InChannel| async move { s.conversations(&c).archive(&i.channel).await as Result<()> }),
        operation("conversations.unarchive", "Restore an archived channel.", Write, &["channels:manage"],
            |s: Slack, c: Connection, i: InChannel| async move { s.conversations(&c).unarchive(&i.channel).await as Result<()> }),
        operation("conversations.rename", "Rename a channel.", Destructive, &["channels:manage"],
            |s: Slack, c: Connection, i: ChannelName| async move { s.conversations(&c).rename(&i.channel, &i.name).await as Result<Channel> }),
        operation("conversations.set_topic", "Set a channel's topic.", Destructive, &["channels:manage"],
            |s: Slack, c: Connection, i: ChannelTopic| async move { s.conversations(&c).set_topic(&i.channel, &i.topic).await as Result<()> }),
        operation("conversations.set_purpose", "Set a channel's purpose.", Destructive, &["channels:manage"],
            |s: Slack, c: Connection, i: ChannelPurpose| async move { s.conversations(&c).set_purpose(&i.channel, &i.purpose).await as Result<()> }),
        operation("conversations.open", "Open a direct message with one member, or a group direct message with several.", Write, &["im:write", "mpim:write"],
            |s: Slack, c: Connection, i: Open| async move { s.conversations(&c).open(&i.users).await as Result<Channel> }),
        operation("conversations.mark", "Mark a conversation as read up to a message.", Write, &["channels:manage"],
            |s: Slack, c: Connection, i: ChannelTs| async move { s.conversations(&c).mark(&i.channel, &i.ts).await as Result<()> }),

        // ── users ──
        operation("users.list", "List the members of the workspace.", Read, &["users:read"],
            |s: Slack, c: Connection, i: UserList| async move { s.users(&c).list(i.options).await as Result<Page<User>> }),
        operation("users.info", "Get one member.", Read, &["users:read"],
            |s: Slack, c: Connection, i: OneUser| async move { s.users(&c).info(&i.user).await as Result<User> }),
        operation("users.lookup_by_email", "Find the member with an email address.", Read, &["users:read.email"],
            |s: Slack, c: Connection, i: Email| async move { s.users(&c).lookup_by_email(&i.email).await as Result<User> }),
        operation("users.presence", "Check whether a member is active.", Read, &["users:read"],
            |s: Slack, c: Connection, i: OneUser| async move { s.users(&c).presence(&i.user).await as Result<Presence> }),
        operation("users.profile", "Get a member's profile.", Read, &["users.profile:read"],
            |s: Slack, c: Connection, i: OneUser| async move { s.users(&c).profile(&i.user).await as Result<Profile> }),

        // ── reactions ──
        operation("reactions.add", "Add an emoji reaction to a message.", Write, &["reactions:write"],
            |s: Slack, c: Connection, i: React| async move { s.reactions(&c).add(&i.channel, &i.timestamp, &i.name).await as Result<()> }),
        operation("reactions.remove", "Remove the token's own reaction from a message.", Destructive, &["reactions:write"],
            |s: Slack, c: Connection, i: React| async move { s.reactions(&c).remove(&i.channel, &i.timestamp, &i.name).await as Result<()> }),
        operation("reactions.get", "List the reactions on a message.", Read, &["reactions:read"],
            |s: Slack, c: Connection, i: ChannelTimestamp| async move { s.reactions(&c).get(&i.channel, &i.timestamp).await as Result<Vec<Reaction>> }),

        // ── pins ──
        operation("pins.add", "Pin a message to its channel.", Write, &["pins:write"],
            |s: Slack, c: Connection, i: ChannelTimestamp| async move { s.pins(&c).add(&i.channel, &i.timestamp).await as Result<()> }),
        operation("pins.remove", "Unpin a message.", Destructive, &["pins:write"],
            |s: Slack, c: Connection, i: ChannelTimestamp| async move { s.pins(&c).remove(&i.channel, &i.timestamp).await as Result<()> }),
        operation("pins.list", "List what is pinned to a channel.", Read, &["pins:read"],
            |s: Slack, c: Connection, i: InChannel| async move { s.pins(&c).list(&i.channel).await as Result<Vec<Pin>> }),

        // ── files ──
        operation("files.info", "Get one file's details.", Read, &["files:read"],
            |s: Slack, c: Connection, i: OneFile| async move { s.files(&c).info(&i.file).await as Result<File> }),
        operation("files.list", "List files, optionally those of one channel or member.", Read, &["files:read"],
            |s: Slack, c: Connection, i: FileList| async move { s.files(&c).list(i.options).await as Result<Vec<File>> }),
        operation("files.delete", "Delete a file.", Destructive, &["files:write"],
            |s: Slack, c: Connection, i: OneFile| async move { s.files(&c).delete(&i.file).await as Result<()> }),

        // ── search ──
        operation("search.messages", "Search messages. Needs a user token.", Read, &["search:read"],
            |s: Slack, c: Connection, i: SearchQuery| async move { s.search(&c).messages(&i.query, i.options).await as Result<SearchResults> }),

        // ── reminders ──
        operation("reminders.add", "Create a reminder. Needs a user token.", Write, &["reminders:write"],
            |s: Slack, c: Connection, i: AddReminder| async move { s.reminders(&c).add(&i.text, &i.time).await as Result<Reminder> }),
        operation("reminders.list", "List reminders. Needs a user token.", Read, &["reminders:read"],
            |s: Slack, c: Connection, _: Nothing| async move { s.reminders(&c).list().await as Result<Vec<Reminder>> }),
        operation("reminders.delete", "Delete a reminder.", Destructive, &["reminders:write"],
            |s: Slack, c: Connection, i: OneReminder| async move { s.reminders(&c).delete(&i.reminder).await as Result<()> }),
        operation("reminders.complete", "Mark a reminder as done.", Write, &["reminders:write"],
            |s: Slack, c: Connection, i: OneReminder| async move { s.reminders(&c).complete(&i.reminder).await as Result<()> }),

        // ── bookmarks ──
        operation("bookmarks.add", "Add a link bookmark to a channel.", Write, &["bookmarks:write"],
            |s: Slack, c: Connection, i: AddBookmark| async move { s.bookmarks(&c).add(&i.channel, &i.title, &i.link, i.emoji.as_deref()).await as Result<Bookmark> }),
        operation("bookmarks.list", "List a channel's bookmarks.", Read, &["bookmarks:read"],
            |s: Slack, c: Connection, i: InChannel| async move { s.bookmarks(&c).list(&i.channel).await as Result<Vec<Bookmark>> }),
        operation("bookmarks.remove", "Remove a bookmark.", Destructive, &["bookmarks:write"],
            |s: Slack, c: Connection, i: RemoveBookmark| async move { s.bookmarks(&c).remove(&i.channel, &i.bookmark).await as Result<()> }),

        // ── user groups ──
        operation("usergroups.list", "List user groups.", Read, &["usergroups:read"],
            |s: Slack, c: Connection, i: UserGroupList| async move { s.usergroups(&c).list(i.options).await as Result<Vec<UserGroup>> }),
        operation("usergroups.members", "List the ids of a user group's members.", Read, &["usergroups:read"],
            |s: Slack, c: Connection, i: OneUserGroup| async move { s.usergroups(&c).members(&i.usergroup).await as Result<Vec<String>> }),

        // ── workspace ──
        operation("team.info", "Get the workspace's name and domain.", Read, &["team:read"],
            |s: Slack, c: Connection, _: Nothing| async move { s.workspace(&c).info().await as Result<Team> }),
        operation("emoji.list", "List the workspace's custom emoji.", Read, &["emoji:read"],
            |s: Slack, c: Connection, _: Nothing| async move { s.workspace(&c).emoji().await as Result<Emoji> }),
        operation("dnd.info", "Get a member's Do Not Disturb state.", Read, &["dnd:read"],
            |s: Slack, c: Connection, i: OneUser| async move { s.workspace(&c).dnd_info(&i.user).await as Result<DndStatus> }),
        operation("dnd.set_snooze", "Turn on Do Not Disturb for the token's user. Needs a user token.", Write, &["dnd:write"],
            |s: Slack, c: Connection, i: Snooze| async move { s.workspace(&c).dnd_set_snooze(i.minutes).await as Result<DndStatus> }),
        operation("dnd.end_snooze", "Turn off the token's user's Do Not Disturb snooze. Needs a user token.", Write, &["dnd:write"],
            |s: Slack, c: Connection, _: Nothing| async move { s.workspace(&c).dnd_end_snooze().await as Result<DndStatus> }),
    ]
}
