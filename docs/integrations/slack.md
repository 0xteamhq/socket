# Slack

**Status:** built and tested against a local server that answers as Slack's documentation says. Not yet run against the real Slack.

Socket's Slack integration gives a program 54 typed methods and the same 54 as operations callable by name with JSON, plus identity and channel lookup. This page shows how to connect, lists everything that is supported, and says what is not.

## Try it

`crates/facade/examples/slack.rs` is a runnable tour.

```sh
# List every operation. Needs no token.
cargo run -p socketkit --features slack --example slack -- operations

# Read-only: who am I, the workspace, channels, recent messages, members.
SLACK_TOKEN=xoxb-… SLACK_CHANNEL=C0123ABCD cargo run -p socketkit --features slack --example slack

# The same, then post, react, reply, edit, pin, schedule, and remove what it posted.
SLACK_TOKEN=xoxb-… SLACK_CHANNEL=C0123ABCD cargo run -p socketkit --features slack --example slack -- write
```

`SLACK_CHANNEL` must be a channel the bot or user is a member of. The read-only tour needs the scopes `channels:read`, `channels:history`, `users:read` and `team:read`; the write tour also needs `chat:write`, `reactions:write` and `pins:write`.

## Connect

Add the crate with the Slack feature:

```sh
cargo add socketkit --features slack
```

### With a token you already hold

For a bot token (`xoxb-…`) or user token (`xoxp-…`) from your Slack app's settings page.

```rust
use std::sync::Arc;
use socketkit::slack::Slack;
use socketkit::{ConnectionKey, ProviderId, Socket};

let slack = Slack::with_token("xoxb-your-token");
let socket = Socket::in_memory().integration(Arc::new(slack.clone())).build()?;
let connection = socket.connection(ConnectionKey::new(ProviderId::new("slack")?, "me")).await?;
```

Every call uses that token. The tenant name (`"me"`) is only a label.

### With your own OAuth app, to connect your users

```rust
use socketkit::slack::{Slack, SlackOAuth};
use socketkit::{OAuthClient, SecretString};

let slack = Slack::with_oauth(SlackOAuth {
    client: OAuthClient {
        client_id: config.slack_client_id,
        client_secret: SecretString::new(config.slack_client_secret),
        redirect_uri: "https://yourapp.example/oauth/slack/callback".parse()?,
    },
    // Bot scopes. `None` asks for the defaults: channels:history, channels:read, users:read, users:read.email.
    scopes: Some(vec!["chat:write".into(), "channels:read".into(), "channels:history".into()]),
    // Leave empty to act as the bot. See "Bot token or user token" below before setting this.
    user_scopes: Vec::new(),
});
let socket = Socket::builder(Arc::new(my_token_store)).integration(Arc::new(slack.clone())).build()?;

// When a user clicks "Connect Slack":
let key = ConnectionKey::new(ProviderId::new("slack")?, user.id);
let authorization = socket.begin_authorization(key.clone(), None)?;
// Keep `authorization.pending` in that user's session, then redirect them to `authorization.url`.

// At your callback route:
socket.complete_authorization(pending, code, state).await?;
let connection = socket.connection(key).await?;
```

`Slack::with_oauth(client)` with a plain `OAuthClient` also works when the default scopes are enough.

### Bot token or user token

Slack can issue two tokens from one approval: a bot token for `scopes`, and a user token for `user_scopes`. **When Slack returns both, Socket stores the user token, and every call then acts as the person with only the user scopes.** The bot scopes are not available to those calls.

So choose one:

- **Act as the bot** (the usual case): set `scopes`, leave `user_scopes` empty.
- **Act as the person** (needed for search, reminders and snoozing): put every scope the calls need in `user_scopes`, not only the extra one. For example, to search and also read channels: `user_scopes: vec!["search:read".into(), "channels:read".into(), "channels:history".into()]`.

## Use the typed methods

Methods are grouped by area. Each group is reached through the `Slack` value and a connection: `slack.chat(&connection)`.

**How arguments are split.** What identifies the thing acted on is a plain argument: a channel id, a user id, a message timestamp, a file id. Content and optional filters are structs from `socketkit::slack::models`, where every field you leave unset is not sent, so Slack applies its own default.

```rust
use socketkit::slack::models::{History, PostMessage, UpdateMessage};

let chat = slack.chat(&connection);
let posted = chat.post_message("C0123ABCD", PostMessage::text("Deploy finished")).await?;
chat.post_message("C0123ABCD", PostMessage::text("Details…").in_thread(&posted.ts)).await?;
chat.update("C0123ABCD", &posted.ts, UpdateMessage::text("Deploy finished ✅")).await?;
slack.reactions(&connection).add("C0123ABCD", &posted.ts, "tada").await?;
```

### `slack.chat(&connection)`

| Method | Returns |
| --- | --- |
| `post_message(channel, PostMessage)` | `PostedMessage` |
| `post_ephemeral(channel, user, PostMessage)` | the message timestamp |
| `update(channel, ts, UpdateMessage)` | `PostedMessage` |
| `delete(channel, ts)` | nothing |
| `schedule_message(channel, post_at, PostMessage)` | `ScheduledMessage` |
| `delete_scheduled_message(channel, scheduled_message_id)` | nothing |
| `scheduled_messages(ListScheduled)` | `Page<ScheduledMessage>` |
| `permalink(channel, message_ts)` | the link |

`PostMessage` has `text`, `blocks`, `attachments`, `thread_ts`, `reply_broadcast`, `mrkdwn`, `unfurl_links` and `unfurl_media`. A message needs text, blocks or attachments. To message a person directly, pass their user id as the channel. `post_at` is seconds since the Unix epoch.

### `slack.conversations(&connection)`

| Method | Returns |
| --- | --- |
| `list(ListConversations)` | `Page<Channel>` |
| `info(channel)` | `Channel`, with its member count |
| `history(channel, History)` | `Page<Message>`, newest first |
| `replies(channel, thread_ts, History)` | `Page<Message>` |
| `members(channel, Paging)` | `Page<String>` of user ids |
| `create(name, CreateConversation)` | `Channel` |
| `join(channel)` | `Channel` |
| `leave(channel)` | nothing |
| `invite(channel, users)` | `Channel` |
| `kick(channel, user)` | nothing |
| `archive(channel)`, `unarchive(channel)` | nothing |
| `rename(channel, name)` | `Channel` |
| `set_topic(channel, topic)`, `set_purpose(channel, purpose)` | nothing |
| `open(users)` | `Channel`: a direct message for one user, a group direct message for several |
| `mark(channel, ts)` | nothing |

`ListConversations.types` is a comma-separated list of `public_channel`, `private_channel`, `mpim` and `im`. `History` has `oldest`, `latest`, `inclusive`, `limit` and `cursor`.

### `slack.users(&connection)`

| Method | Returns |
| --- | --- |
| `list(Paging)` | `Page<User>` |
| `info(user)` | `User` |
| `lookup_by_email(email)` | `User` |
| `presence(user)` | `Presence` |
| `profile(user)` | `Profile` |

A member's email is present only when the token has `users:read.email`.

### The smaller groups

| Group | Methods |
| --- | --- |
| `slack.reactions(&connection)` | `add(channel, timestamp, name)`, `remove(channel, timestamp, name)`, `get(channel, timestamp)` |
| `slack.pins(&connection)` | `add(channel, timestamp)`, `remove(channel, timestamp)`, `list(channel)` |
| `slack.files(&connection)` | `info(file)`, `list(ListFiles)`, `delete(file)` |
| `slack.search(&connection)` | `messages(query, Search)` |
| `slack.reminders(&connection)` | `add(text, time)`, `list()`, `delete(reminder)`, `complete(reminder)` |
| `slack.bookmarks(&connection)` | `add(channel, title, link, emoji)`, `list(channel)`, `remove(channel, bookmark)` |
| `slack.usergroups(&connection)` | `list(ListUserGroups)`, `members(usergroup)` |
| `slack.workspace(&connection)` | `info()`, `emoji()`, `dnd_info(user)`, `dnd_set_snooze(minutes)`, `dnd_end_snooze()` |

An emoji name may be written with or without colons. A reminder's `time` is a Unix timestamp, a number of seconds from now, or words such as `in 15 minutes`. Search uses Slack's own query syntax, for example `in:#general from:@ada deploy`.

**Search, reminders and snoozing need a user token.** Slack does not allow a bot token for them.

### Identity and lookup

```rust
let me = slack.identity(&connection).await?;                 // Account { id, name, email }
let channel = slack.resolve(&connection, "#general").await?;  // Resource { id, label, description }
```

`resolve` accepts a channel id or a name with or without `#`, and confirms a public channel exists and the token can see it.

## Page through a list

Lists that Slack pages by cursor return a `Page` with `items` and `next_cursor`. Pass the cursor back for the next page; `None` means the last page.

```rust
let mut options = History { limit: Some(200), ..Default::default() };
loop {
    let page = slack.conversations(&connection).history("C0123ABCD", options.clone()).await?;
    for message in &page.items { /* … */ }
    match page.next_cursor {
        Some(cursor) => options.cursor = Some(cursor),
        None => break,
    }
}
```

`files.list` and `search.messages` are paged by number in Slack, so their options carry `page` and `count` instead.

## Call an operation by name

Every method is also an operation an agent, an MCP server or another language can call with JSON. The plain arguments and the options sit side by side in one object.

```rust
let output = socket
    .invoke(key, "slack.chat.post_message".into(), serde_json::json!({ "channel": "C0123ABCD", "text": "Deploy finished" }))
    .await?;
```

`socket.operations()` returns each operation's name, description, input schema, output schema, effect and scope. The effect lets a host ask a person before a change:

- **read** changes nothing.
- **write** adds something.
- **destructive** deletes, removes or overwrites what was there, or takes away someone's access.

The scope shown is the bot-token scope for a public channel. A private channel, a direct message or a user token needs Slack's matching scope for that case (for example `groups:history` instead of `channels:history`).

| Operation | Effect | Scope | What it does |
| --- | --- | --- | --- |
| `slack.identity.get` | read |  | Return the account this connection is authorised as, confirming the token still works. |
| `slack.resource.resolve` | read |  | Confirm that a resource exists and the account can reach it. Accepts a public channel as #name or a channel id. |
| `slack.chat.post_message` | write | chat:write | Post a message to a channel, a direct message or a thread. |
| `slack.chat.post_ephemeral` | write | chat:write | Post a message that only one member can see. Returns its timestamp. |
| `slack.chat.update` | destructive | chat:write | Replace the content of a message. |
| `slack.chat.delete` | destructive | chat:write | Delete a message. |
| `slack.chat.schedule_message` | write | chat:write | Schedule a message to be posted later. |
| `slack.chat.delete_scheduled_message` | destructive | chat:write | Cancel a scheduled message. |
| `slack.chat.scheduled_messages` | read |  | List messages that are scheduled and not yet posted. |
| `slack.chat.permalink` | read |  | Get the permanent link to a message. |
| `slack.conversations.list` | read | channels:read | List the conversations the token can see. |
| `slack.conversations.info` | read | channels:read | Get one conversation, with its member count. |
| `slack.conversations.history` | read | channels:history | Read the messages of a conversation, newest first. |
| `slack.conversations.replies` | read | channels:history | Read the messages of one thread. |
| `slack.conversations.members` | read | channels:read | List the ids of a conversation's members. |
| `slack.conversations.create` | write | channels:manage | Create a channel. |
| `slack.conversations.join` | write | channels:join | Join a public channel. |
| `slack.conversations.leave` | destructive | channels:manage | Leave a conversation. |
| `slack.conversations.invite` | write | channels:manage | Invite members to a channel. |
| `slack.conversations.kick` | destructive | channels:manage | Remove a member from a channel. |
| `slack.conversations.archive` | destructive | channels:manage | Archive a channel. |
| `slack.conversations.unarchive` | write | channels:manage | Restore an archived channel. |
| `slack.conversations.rename` | destructive | channels:manage | Rename a channel. |
| `slack.conversations.set_topic` | destructive | channels:manage | Set a channel's topic. |
| `slack.conversations.set_purpose` | destructive | channels:manage | Set a channel's purpose. |
| `slack.conversations.open` | write | im:write, mpim:write | Open a direct message with one member, or a group direct message with several. |
| `slack.conversations.mark` | write | channels:manage | Mark a conversation as read up to a message. |
| `slack.users.list` | read | users:read | List the members of the workspace. |
| `slack.users.info` | read | users:read | Get one member. |
| `slack.users.lookup_by_email` | read | users:read.email | Find the member with an email address. |
| `slack.users.presence` | read | users:read | Check whether a member is active. |
| `slack.users.profile` | read | users.profile:read | Get a member's profile. |
| `slack.reactions.add` | write | reactions:write | Add an emoji reaction to a message. |
| `slack.reactions.remove` | destructive | reactions:write | Remove the token's own reaction from a message. |
| `slack.reactions.get` | read | reactions:read | List the reactions on a message. |
| `slack.pins.add` | write | pins:write | Pin a message to its channel. |
| `slack.pins.remove` | destructive | pins:write | Unpin a message. |
| `slack.pins.list` | read | pins:read | List what is pinned to a channel. |
| `slack.files.info` | read | files:read | Get one file's details. |
| `slack.files.list` | read | files:read | List files, optionally those of one channel or member. |
| `slack.files.delete` | destructive | files:write | Delete a file. |
| `slack.search.messages` | read | search:read | Search messages. Needs a user token. |
| `slack.reminders.add` | write | reminders:write | Create a reminder. Needs a user token. |
| `slack.reminders.list` | read | reminders:read | List reminders. Needs a user token. |
| `slack.reminders.delete` | destructive | reminders:write | Delete a reminder. |
| `slack.reminders.complete` | write | reminders:write | Mark a reminder as done. |
| `slack.bookmarks.add` | write | bookmarks:write | Add a link bookmark to a channel. |
| `slack.bookmarks.list` | read | bookmarks:read | List a channel's bookmarks. |
| `slack.bookmarks.remove` | destructive | bookmarks:write | Remove a bookmark. |
| `slack.usergroups.list` | read | usergroups:read | List user groups. |
| `slack.usergroups.members` | read | usergroups:read | List the ids of a user group's members. |
| `slack.team.info` | read | team:read | Get the workspace's name and domain. |
| `slack.emoji.list` | read | emoji:read | List the workspace's custom emoji. |
| `slack.dnd.info` | read | dnd:read | Get a member's Do Not Disturb state. |
| `slack.dnd.set_snooze` | write | dnd:write | Turn on Do Not Disturb for the token's user. Needs a user token. |
| `slack.dnd.end_snooze` | write | dnd:write | Turn off the token's user's Do Not Disturb snooze. Needs a user token. |

## Handle errors

Every error has a kind a program can branch on, and a message that is safe to show.

| Kind | What it means for Slack | What to do |
| --- | --- | --- |
| `ReconnectRequired` | The token was revoked, expired or is invalid | Connect again or issue a new token |
| `AccessDenied` | A scope is missing (the message names it), or the bot is not in the channel | Add the scope, or invite the bot |
| `NotFound` | No such channel, user, message or file | Check the id |
| `InvalidInput` | An argument is wrong; the message names the field. Also Slack's own refusals such as `msg_too_long` | Fix the input |
| `RateLimited` | Slack is throttling; `retry()` says how long to wait when Slack said | Wait and try again |
| `Decode` | Slack answered success without what was asked for | Report it; this should not happen |

An input error names the field and never repeats the value you sent.

Reads are retried on a throttle or a server error. A write is retried only when Slack throttles it, because a throttled request was not carried out. **A write that fails any other way is never repeated**, so a message cannot be posted twice; if a write fails with a server error, check before sending it again.

## Not supported yet

- **Uploading a file.** Slack's upload sends raw bytes to a different host, which Socket's transport does not do yet.
- **Modals, views and interactive components.**
- **Admin and SCIM methods.**
- **Receiving events**: the Events API, slash commands and Socket Mode.
- **Editing a profile or status**, and **creating or editing user groups**.

Anything Slack offers that has no method here can still be called through the generic request, with the token, retries and error handling applied:

```rust
let response = socket.request(key, RawRequest::get("conversations.list").with_query("types", "im")).await?;
```
