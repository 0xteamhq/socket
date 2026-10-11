# Microsoft

**Status:** built and tested against a local server that answers as Microsoft's documentation says. Not yet run against the real Microsoft Graph.

Socket's Microsoft integration is one provider for everything behind Microsoft Graph: Outlook, Teams, OneDrive, SharePoint and Entra ID share one sign-in. Today it gives a program the Outlook calendar, Outlook mail, Teams and Teams meetings as 60 typed methods, and 58 of them as operations callable by name with JSON, plus identity and lookup of a sharing link. The two that are typed only return a file as bytes, which an operation called by name never does. This page shows how to connect, lists everything that is supported, and says what was and was not confirmed against Microsoft's documentation.

## Connect

Add the crate with the Microsoft feature:

```sh
cargo add socketkit --features microsoft
```

### With a token you already hold

```rust
use std::sync::Arc;
use socketkit::microsoft::Microsoft;
use socketkit::{ConnectionKey, ProviderId, Socket};

let microsoft = Microsoft::with_token("eyJ0eXAi…");
let socket = Socket::in_memory().integration(Arc::new(microsoft.clone())).build()?;
let connection = socket.connection(ConnectionKey::new(ProviderId::new("microsoft")?, "me")).await?;
```

Every call uses that token. A Graph access token lasts about an hour and this way has no refresh token, so it suits a script, not a connection that has to keep working.

### With your own app registration, to connect your users

Register an application in the Microsoft Entra admin centre, add a client secret, and add your callback route as a **Web** redirect URI.

```rust
use socketkit::microsoft::{Microsoft, MicrosoftOAuth, Prompt};
use socketkit::{OAuthClient, SecretString};

let microsoft = Microsoft::with_oauth(MicrosoftOAuth {
    client: OAuthClient {
        client_id: config.microsoft_client_id,
        client_secret: SecretString::new(config.microsoft_client_secret),
        redirect_uri: "https://yourapp.example/oauth/microsoft/callback".parse()?,
    },
    // Graph permissions. `None` asks for the defaults: offline_access and User.Read.
    scopes: Some(vec!["User.Read".into(), "Calendars.Read".into()]),
    // Who may sign in. `None` is `common`.
    tenant: Some("organizations".into()),
    login_hint: None,
    prompt: Some(Prompt::SelectAccount),
});
let socket = Socket::builder(Arc::new(my_token_store)).integration(Arc::new(microsoft.clone())).build()?;

// When a user clicks "Connect Microsoft":
let key = ConnectionKey::new(ProviderId::new("microsoft")?, user.id);
let authorization = socket.begin_authorization(key.clone(), None)?;
// Keep `authorization.pending` in that user's session, then redirect them to `authorization.url`.

// At your callback route:
socket.complete_authorization(pending, code, state).await?;
let connection = socket.connection(key).await?;
```

`Microsoft::with_oauth(client)` with a plain `OAuthClient` also works when the defaults are enough.

| Setting | What it does |
| --- | --- |
| `scopes` | The Graph permissions to ask for, in place of the defaults. Every product needs its own: a calendar needs `Calendars.Read`, and so on. |
| `tenant` | `common` (any account, the default), `organizations` (work or school accounts), `consumers` (personal accounts), or a tenant id or domain for one organisation. A guest signing in to another organisation needs that organisation's id or domain. |
| `login_hint` | Prefills the sign-in page with an email address. |
| `prompt` | `Login`, `None`, `Consent` or `SelectAccount`: what the sign-in page asks of the person. |

Three things Socket does for every Microsoft connection:

- **A refresh token is always asked for.** `offline_access` is added to whatever scopes are given, in the settings or at `begin_authorization`. Without it Microsoft issues no refresh token and the connection stops working within the hour.
- **Each refresh saves the new refresh token.** Microsoft sends a new one every time and expects the old one to be discarded.
- **PKCE is on.** The code challenge is sent with the sign-in and the verifier with the code, beside the client secret.

A tenant that is not one of the three keywords, an id (`8eaef023-2b34-4da1-9baa-8bc8c9d6a490`) or a domain (`contoso.onmicrosoft.com`) is reported when the `Socket` is built, because the tenant is part of the address of the sign-in page. One word that is none of the three keywords, such as `contoso`, is neither an id nor a domain and is refused too.

### When a permission needs an administrator

Many Graph permissions, most of those for Teams among them, can only be approved by an administrator of the organisation. When Microsoft's token endpoint answers that consent is missing, Socket reports `AccessDenied`, not `ReconnectRequired`, with a message that says who has to approve:

- `AADSTS65001` or `AADSTS90008`, or the error `consent_required`: the person has to accept the permission at sign-in, or an administrator has to approve it.
- `AADSTS90094` or `AADSTS90095`: an administrator has to approve it.

The stored tokens are kept, since they work again once the permission is approved. Any other refused refresh, such as a refresh token that expired after 90 days unused or was revoked by a password change, is `ReconnectRequired`.

Most consent refusals never reach the token endpoint: Microsoft sends the person back to your callback with `error=access_denied` or `error=consent_required` and no code. That redirect is your application's to handle.

## Identity and lookup

```rust
let me = microsoft.identity(&connection).await?;        // Account { id, name, email }
let item = microsoft.resolve(&connection, link).await?; // Resource { id, label, description }
```

`identity` reads `GET /me` and needs `User.Read`. The email is the mailbox address, or the sign-in name (`userPrincipalName`) for an account without a mailbox.

`resolve` accepts a OneDrive or SharePoint sharing link, the kind the Share button copies, and confirms it leads to a file or folder the account can open. The resource's `id` is the Graph path that addresses the item, `drives/{drive id}/items/{item id}`, so it can be passed to a later request; the label is the item's name. It needs `Files.ReadWrite`: Microsoft lists no read-only permission for opening a sharing link.

## Use the typed methods

Methods are grouped by area. Each group is reached through the `Microsoft` value and a connection: `microsoft.events(&connection)`.

**How arguments are split.** What identifies the thing acted on is a plain argument: a calendar id, an event id. Content and optional filters are structs from `socketkit::microsoft::models`, where every field you leave unset is not sent, at any depth, so Graph applies its own default and a change touches only what you named.

**Names are Graph's own.** In Rust the fields are written the Rust way (`is_online_meeting`); in JSON they are written as Graph writes them (`isOnlineMeeting`, `onlineMeeting.joinUrl`), so Graph's documentation of a field holds here too.

```rust
use socketkit::microsoft::models::{Attendee, CreateEvent, DateTimeTimeZone, Paging};

let events = microsoft.events(&connection);
let week = events.list_between("2026-10-12T00:00:00Z", "2026-10-19T00:00:00Z", None, Paging::default()).await?;

let meeting = events.create(None, CreateEvent {
    subject: Some("Design review".into()),
    attendees: Some(vec![Attendee::required("grace@contoso.example")]),
    is_online_meeting: Some(true),
    ..CreateEvent::between(DateTimeTimeZone::utc("2026-10-12T16:00:00"), DateTimeTimeZone::utc("2026-10-12T17:00:00"))
}).await?;
let join_url = meeting.online_meeting.and_then(|m| m.join_url);
```

### `microsoft.calendars(&connection)`

| Method | Returns |
| --- | --- |
| `list(Paging)` | `Page<Calendar>`: the account's own calendars and those shared with it |
| `get(calendar)` | `Calendar` |

Needs `Calendars.Read`.

### `microsoft.events(&connection)`

| Method | Returns |
| --- | --- |
| `list_between(start, end, calendar, Paging)` | `Page<Event>`, with each occurrence of a repeating event as its own event |
| `get(event)` | `Event` |
| `instances(event, start, end, Paging)` | `Page<Event>`: the occurrences of one repeating event |
| `find_meeting_times(FindMeetingTimes)` | `MeetingTimeSuggestions` |
| `schedule(GetSchedule)` | `Vec<ScheduleInformation>`: free and busy times for a list of people |
| `create(calendar, CreateEvent)` | `Event` |
| `update(event, UpdateEvent)` | `Event` |
| `respond(event, EventResponse, RespondToEvent)` | nothing |
| `cancel(event, CancelEvent)` | nothing |
| `delete(event)` | nothing |

`calendar` is `Some(id)`, or `None` for the account's default calendar. Reading needs `Calendars.Read`; `find_meeting_times` needs `Calendars.Read.Shared`; everything that changes a calendar needs `Calendars.ReadWrite`.

**What an `Event` carries:** `id`, `subject`, `start` and `end`, `organizer`, `attendees` with each one's `status.response`, `location`, `body` and `bodyPreview`, `isOnlineMeeting`, `onlineMeetingProvider`, `onlineMeeting.joinUrl`, and also `isAllDay`, `isCancelled`, `isOrganizer`, `responseStatus`, `seriesMasterId`, `type`, `showAs`, `webLink`, `iCalUId` and `recurrence`.

**The range of a listing.** `start` and `end` are ISO 8601. An offset is honoured (`2026-10-12T00:00:00-08:00`); a time without one is in UTC.

**Times come back in UTC.** Every request that returns times sends `Prefer: outlook.timezone="UTC"`, so `start.timeZone` and `end.timeZone` are `UTC` whatever zone the event was made in. That zone is kept beside them, in `originalStartTimeZone` and `originalEndTimeZone`. Graph writes a time as `2026-10-12T16:00:00.0000000`, with seven decimal places and no offset; read it together with its `timeZone`.

**Times you send** are a `DateTimeTimeZone`: `{ "dateTime": "2026-10-12T16:00:00", "timeZone": "Pacific Standard Time" }`. Both parts are required. A time without its zone is refused, not assumed to be UTC: a wrong guess would put a meeting hours from where it was meant, with the invitations already sent. In Rust, `DateTimeTimeZone::utc("…")` and `DateTimeTimeZone::in_zone("…", "…")` write both.

**An online meeting.** Set `isOnlineMeeting` when creating an event and the event that comes back carries `onlineMeeting.joinUrl`. Socket asks for a Teams meeting (`onlineMeetingProvider: "teamsForBusiness"`) unless you name another provider. For a Teams meeting the join link is also how its transcript is found, so keep it.

**Changing an event.** `update` sends only the fields you set. `attendees` is the whole list: anyone left out is removed. To add one person, read the event, add them to its `attendees`, and send that list back; each attendee's `status` is Graph's to fill in and is left out of what is sent. Changing the `body` of an online meeting must keep the part that holds the join details, and a `body` must carry its `content`. To make an event stop repeating, send `recurrence: null`: it is the one field where `null` is sent as it is, and not read as "leave this alone". An update with nothing set is refused.

**Answering an invitation.** `EventResponse` is `Accept`, `TentativelyAccept` or `Decline` (`accept`, `tentatively_accept`, `decline` in JSON). `RespondToEvent` has `comment`, `sendResponse` and `proposedNewTime`; another time can be proposed only with a decline or a tentative acceptance.

**Cancel or delete.** `cancel` is for a meeting the account organised: it tells the attendees and moves the event to Deleted Items; an attendee who calls it is refused by Graph. `delete` removes the event from the account's own calendar, and for a meeting the account organised it also sends a cancellation.

**Availability.** `find_meeting_times` and `schedule` read other people's calendars and change nothing. Both are for work and school accounts only. `schedule` reads at most 20 people, lists and rooms at once, over less than 62 days, and more than 20 addresses are refused; an address Graph could not read comes back with an `error` in its place, not as a failure of the call.

### `microsoft.mail(&connection)`

```rust
use socketkit::microsoft::models::{DraftMessage, GetMessage, ItemBody, ListMessages, Recipient, ReplyContent};

let mail = microsoft.mail(&connection);
let unread = mail.list(ListMessages {
    folder: Some("inbox".into()),
    filter: Some("isRead eq false".into()),
    ..Default::default()
}).await?;
let message = mail.get(&unread.items[0].id, GetMessage::default()).await?;

// Write a reply for a person to look at, without sending anything.
let draft = mail.create_reply(&message.id, ReplyContent {
    comment: Some("Thanks, Monday works.".into()),
    ..Default::default()
}).await?;
```

| Method | Returns |
| --- | --- |
| `list(ListMessages)` | `Page<Message>`: one folder, or the whole mailbox |
| `get(message, GetMessage)` | `Message` |
| `conversation(conversation, Paging)` | `Page<Message>`: every message of one thread, oldest first |
| `attachments_list(message, Paging)` | `Page<Attachment>`, without the files |
| `attachment_get(message, attachment)` | `Attachment`: its name, type and size, without the file |
| `attachment_content(message, attachment, Download)` | `Content`: the file as bytes, with its type. Typed only |
| `attachment_text(message, attachment, TextLimit)` | `AttachmentText`: an attachment that is text, as text |
| `create_draft(DraftMessage)` | `Message`: the draft |
| `update_draft(message, DraftMessage)` | `Message` |
| `create_reply(message, ReplyContent)`, `create_reply_all(…)`, `create_forward(…)` | `Message`: the draft |
| `send(SendMail)` | nothing |
| `send_draft(message)` | nothing |
| `reply(message, ReplyContent)` | nothing |
| `update(message, UpdateMessage)` | `Message` |
| `move_to(message, folder)` | `Message`, under a new id |
| `delete(message)` | nothing |

Reading needs `Mail.Read`; drafts and changes need `Mail.ReadWrite`; `send`, `send_draft` and `reply` need `Mail.Send`.

**What a `Message` carries:** `id`, `conversationId`, `subject`, `from`, `sender`, `toRecipients`, `ccRecipients`, `bccRecipients`, `replyTo`, `receivedDateTime`, `sentDateTime`, `bodyPreview`, `body`, `isRead`, `isDraft`, `hasAttachments`, `webLink`, `importance`, `categories`, `flag`, `parentFolderId` and `internetMessageId`.

**Listing.** `ListMessages` has `folder`, `filter`, `search`, `orderBy`, `bodyType`, `limit` and `cursor`. `folder` is a folder id or a well-known name: `inbox`, `drafts`, `sentitems`, `deleteditems`, `archive`, `junkemail` and the rest of Graph's list. Without it the whole mailbox is listed, Deleted Items included. Graph returns 10 messages a page unless `limit` says otherwise, up to 1000.

- `filter` is Graph's `$filter` (`isRead eq false`) and `orderBy` its `$orderby` (`receivedDateTime desc`). Used together, what is sorted by has to come first in the filter, or Graph answers `InefficientFilter`, which arrives as `InvalidInput` with Graph's words.
- `search` is Graph's `$search`: plain words, or properties such as `from:grace subject:plan`. Socket sends it as one quoted phrase and escapes any quote inside it. Graph returns at most 1,000 results, sorted by when they were sent. A search cannot be given with `filter` or `orderBy`, and is refused if it is: Graph does not filter or sort a search further, and may answer with results that ignore what was asked.

**Bodies are plain text on reads.** `list`, `get` and `conversation` send `Prefer: outlook.body-content-type="text"`, so `body.content` is text a program can read. Set `bodyType` to `html` on `list` or `get` for the HTML. The methods that write, mark or move a message return it without that preference, so its body is in Graph's own default format, which is HTML. `body.contentType` says which came back.

**A conversation.** `conversation` takes a message's `conversationId` and returns the whole thread oldest first, from every folder: what was received and what was sent.

**Attachments.** `attachments_list` returns each attachment's `id`, `name`, `contentType`, `size` and `isInline`, and whether it is a file, another item or a link (`@odata.type`), without any file's content. `attachment_get` describes one in the same way. Neither carries a file: the fields are asked for by name, so Graph leaves the content out.

**The file itself** comes two ways.

- `attachment_content` returns the bytes exactly as Graph serves them, with the type Graph states, as a `Content` (`bytes`, `content_type`, `len()`). An attached message, event or contact arrives as Graph writes it out: in MIME, as an iCalendar file, as a vCard. A link to a file kept elsewhere has no content, and Graph refuses it. Ten megabytes are read and thirty seconds allowed unless `Download` says otherwise (`maxBytes`, `timeoutSecs`); a larger file is the error `too_large`, never a shorter file. This is a typed method only.
- `attachment_text`, which is also the operation `mail.attachment_text`, returns `contentType` and `text` for an attachment Graph serves as text: `text/…`, JSON or XML, in UTF-8. Anything else, text in another encoding included, is the error `unsupported`, whose message gives the size and type and nothing of the content. An attachment that says it is UTF-8 text and is not is the error `decode`. One megabyte is read unless `maxBytes` asks for more, and it can ask for at most ten.

**A file fetched by name comes back as text or not at all.** This is the rule for every operation that returns a file's content, and it leaves the JSON every other operation returns as it is: a program that calls by name, an agent above all, is never handed bytes, in base64 or any other form, and never more text than it asked for. Bytes are for a program that calls the typed method and has said how many it will hold.

**Drafts.** `create_draft` saves a message in Drafts; `create_reply`, `create_reply_all` and `create_forward` save an answer to an existing message. None of them sends anything, so a person can read the draft first. `update_draft` replaces the fields you set. A draft can be filled in over several steps, so nothing in `DraftMessage` is required: `subject`, `body`, `toRecipients`, `ccRecipients`, `bccRecipients`, `replyTo`, `importance`.

**Replying.** `ReplyContent` is a `comment`, a few words above the quoted message, or the fields of the reply itself, such as a whole `body` or more recipients. Give `comment` or `body`, not both: Graph refuses the two together, and so does Socket. A forward takes the people it goes to from `toRecipients` and is refused without any.

**Sending cannot be taken back.** `send` sends a message at once; it needs at least one recipient, and a `subject` or a `body`. `send_draft` sends a draft as it stands. `reply` answers the sender at once, and needs a `comment` or a `body`. A draft may be empty; what is sent at once may not. Graph answers "accepted", which means it took the message, not that it was delivered. A copy is kept in Sent Items unless `saveToSentItems` is `false`.

**Marking.** `update` sets `isRead`, `categories` (the whole list) and `flag`, whose `flagStatus` is `notFlagged`, `flagged` or `complete`.

**Moving changes the id.** `move_to` puts a message in another folder, `deleteditems` included, and returns it under a new id. The id you passed no longer finds it, so use the one that comes back.

**Recipients.** A recipient is `{ "emailAddress": { "address": "grace@contoso.example", "name": "Grace Hopper" } }`; the name is optional. One without an address is refused.

**Other people's mailboxes.** Everything here is the signed-in person's own mailbox. Shared and delegated mailboxes are not built; the method arguments leave room for them.

### `microsoft.mail_folders(&connection)`

| Method | Returns |
| --- | --- |
| `list(ListFolders)` | `Page<MailFolder>`: the top of the mailbox, or the folders inside `parent` |
| `get(folder)` | `MailFolder` |

A `MailFolder` has `id`, `displayName`, `parentFolderId`, `childFolderCount`, `unreadItemCount`, `totalItemCount` and `isHidden`. Hidden folders are left out unless `includeHidden` is set. Needs `Mail.Read`.

### Teams: `teams`, `channels`, `channel_messages` and `chats`

```rust
use socketkit::microsoft::models::{Cursor, Paging, SendChatMessage};

let teams = microsoft.teams(&connection).list_joined(Cursor::default()).await?;
let channels = microsoft.channels(&connection).list(&teams.items[0].id, Cursor::default()).await?;
let messages = microsoft.channel_messages(&connection)
    .list(&teams.items[0].id, &channels.items[0].id, Paging::default()).await?;
for message in &messages.items {
    println!("{}", message.text); // plain text, with mentions as @names
}
microsoft.chats(&connection).send(chat_id, SendChatMessage::text("On my way.")).await?;
```

| Group | Method | Returns |
| --- | --- | --- |
| `teams` | `list_joined(Cursor)` | `Page<Team>` |
| `teams` | `get(team)` | `Team` |
| `teams` | `members(team, Paging)` | `Page<ConversationMember>` |
| `channels` | `list(team, Cursor)` | `Page<Channel>` |
| `channels` | `get(team, channel)` | `Channel` |
| `channels` | `members(team, channel, Paging)` | `Page<ConversationMember>` |
| `channel_messages` | `list(team, channel, Paging)` | `Page<ChatMessage>`: the messages that start a conversation |
| `channel_messages` | `get(team, channel, message)` | `ChatMessage` |
| `channel_messages` | `replies(team, channel, message, Paging)` | `Page<ChatMessage>` |
| `channel_messages` | `send(team, channel, SendChatMessage)` | `ChatMessage` |
| `channel_messages` | `reply(team, channel, message, SendChatMessage)` | `ChatMessage` |
| `chats` | `list(Paging)` | `Page<Chat>` |
| `chats` | `get(chat)` | `Chat` |
| `chats` | `members(chat, Cursor)` | `Page<ConversationMember>` |
| `chats` | `messages(chat, Paging)` | `Page<ChatMessage>`, the most recently changed first |
| `chats` | `message_get(chat, message)` | `ChatMessage` |
| `chats` | `send(chat, SendChatMessage)` | `ChatMessage` |
| `chats` | `create(CreateChat)` | `Chat` |

**A message as plain text.** Teams writes a message as HTML, with an `<at>` tag for each mention and an `<attachment>` tag where a file or a card sits. A `ChatMessage` keeps that in `body`, and also carries `text`: the same message as a person would read it. A mention is `@` and the name, an attachment is `[attachment: name]` on a line of its own, an emoji or a picture is what it stands for, a symbol written by one of about 140 common names (`&mdash;`, `&hellip;`, `&eacute;`) is the symbol, and a name outside those is left as it was written, a link is its words with its address after them, the cells of a table are kept apart with `|`, and paragraphs are lines. What a tag holds in its attributes, such as a tooltip, is not text, and neither is a comment. Socket writes `text` from `body`; Graph does not send it.

**What a `ChatMessage` carries:** `id`, `replyToId`, `messageType`, `from`, `createdDateTime`, `lastModifiedDateTime`, `lastEditedDateTime` (set when it was edited), `deletedDateTime` (set when it was deleted), `body`, `text`, `attachments`, `mentions`, `reactions`, and also `subject`, `importance`, `webUrl`, `chatId`, `channelIdentity` and `eventDetail`.

- **Who sent it** is `from.user` for a person, or `from.application` for an application; a bot is an application whose `applicationIdentityType` is `bot`.
- **What Teams itself noted**, such as a member being added, has `messageType: "systemEventMessage"`, no sender and no text. What happened is in `eventDetail`, kept as Graph sent it; its `@odata.type` says which kind of event it is. Socket asks Graph to name every kind (`Prefer: include-unknown-enum-members`); without that Graph writes these as `unknownFutureValue`.
- **A message id is unique only inside its channel, its chat or its conversation.**

**Page sizes.** A page of messages or chats is at most 50, and a page of members at most 999; a larger `limit` is refused. Joined teams, a team's channels and a chat's members take no page size, so those take a `Cursor` and not a `Paging`.

**Sending.** `SendChatMessage` has `body` (`{ "contentType": "text" or "html", "content": "…" }`), and optionally `subject` (a new channel message only), `importance` and `mentions`. A message with nothing in it is refused. To mention someone so that they are notified, write `<at id="0">Name</at>` in an HTML body and name who it is in `mentions`, as Graph's own documentation shows. A message is posted as the signed-in person.

**Creating a chat.** `CreateChat` has `chatType` (`oneOnOne` or `group`), `members` (each person's directory id or sign-in name, a guest's `…#EXT#@…` name included) and, for a group, a `topic`. Everyone in the chat is named, the account that creates it included: two for a one-to-one chat. When a one-to-one chat between the two already exists, Graph returns it.

**Permissions.** Three of the reads need an administrator's consent: `teams.members` (`TeamMember.Read.All`), `channels.members` (`ChannelMember.Read.All`) and everything that reads channel messages (`ChannelMessage.Read.All`). The rest do not: `Team.ReadBasic.All`, `Channel.ReadBasic.All`, `Chat.ReadBasic` for chats and their members, `Chat.Read` for chat messages, `ChannelMessage.Send`, `ChatMessage.Send` and `Chat.Create`.

**Throttling.** Graph allows about one read and one post a second for each channel and each chat, and says a conversation should be polled for changes no more than once a day. A throttled call arrives as `RateLimited` with the wait Graph asked for.

### Teams meetings: `online_meetings`, `transcripts`, `recordings` and `attendance`

```rust
// From the join link on a calendar event to what was said.
let event = microsoft.events(&connection).get(event_id).await?;
let join_url = event.online_meeting.and_then(|m| m.join_url).expect("an online meeting");
let meeting = microsoft.online_meetings(&connection).find_by_join_url(&join_url).await?;
let transcripts = microsoft.transcripts(&connection).list(&meeting.id, Paging::default()).await?;
let content = microsoft.transcripts(&connection).content(&meeting.id, &transcripts.items[0].id).await?;
for entry in &content.entries {
    println!("{}: {}", entry.speaker.as_deref().unwrap_or("?"), entry.text);
}
```

| Group | Method | Returns |
| --- | --- | --- |
| `online_meetings` | `get(meeting)` | `OnlineMeeting` |
| `online_meetings` | `find_by_join_url(join_url)` | `OnlineMeeting` |
| `transcripts` | `list(meeting, Paging)` | `Page<Transcript>` |
| `transcripts` | `get(meeting, transcript)` | `Transcript` |
| `transcripts` | `content(meeting, transcript)` | `TranscriptContent`: the text, and one entry for each thing said |
| `recordings` | `list(meeting, Paging)` | `Page<Recording>` |
| `recordings` | `get(meeting, recording)` | `Recording`, with the address its video is at |
| `recordings` | `content(meeting, recording, Download)` | `Content`: the video as bytes. Typed only |
| `attendance` | `reports(meeting, Paging)` | `Page<AttendanceReport>`, one for each time the meeting was held |
| `attendance` | `records(meeting, report, Paging)` | `Page<AttendanceRecord>`: who joined, in what role, when and for how long |

**Finding the meeting.** A calendar event carries the join link (`onlineMeeting.joinUrl`). Give it to `find_by_join_url` exactly as the event has it, and the meeting that comes back has the `id` everything else here is asked for by. A link that matches no meeting the account can read is `NotFound`.

**What was said.** `content` asks for WebVTT and returns `text`, the transcript exactly as Microsoft wrote it, and `entries`: for each thing said, the `speaker`, `startMs` and `endMs` in milliseconds from the start of the transcript, and the `text` without markup. A start can be negative: transcription began while people were already talking.

- **An arrow that was said is kept.** Inside a cue, `-->` begins a new cue only on a line that starts with a time; "A --> B" is part of what was said.
- **The speaker is the voice tag that opens a cue**, which is where Teams writes it. Anything later in a cue that looks like a voice tag is kept as text and not believed, so that words cannot be made to read as another person's. A name is whatever the person joined under.
- **A transcript that cannot be read whole is an error**, never a shorter transcript: a cue whose timing cannot be read, or text that is not a cue. Only what WebVTT itself sets apart is skipped: the header, a `NOTE`, a `STYLE` and a `REGION`. The error does not repeat what was said.

**Limits Microsoft sets, which callers will meet:**

- **A transcript or a recording exists only if transcription or recording was switched on** during the meeting; otherwise the list is empty.
- **The meeting has to be on a calendar.** Transcripts and recordings are not offered for a meeting created through the API with no calendar event, for a live event, or for a meeting that has expired, 60 days after it ended.
- **Who may read.** A person can read the transcripts and the details of recordings of meetings they organised or were invited to. Attendance reports are the organiser's alone, and exist once the meeting has ended.
- **An organisation can switch these off.** Graph then answers 403, which arrives as `AccessDenied` with Graph's reason. One such setting withholds who spoke; Microsoft offers a second format without speakers for that case, which Socket does not ask for yet, and a refused transcript says that this may be why.
- **Permissions.** `OnlineMeetings.Read` for the meeting, `OnlineMeetingTranscript.Read.All` and `OnlineMeetingRecording.Read.All` for transcripts and recordings, both of which need an administrator's consent, and `OnlineMeetingArtifact.Read.All` for attendance.

**The recording itself.** `recordings.content` returns the video as the bytes Graph serves, with the type it states (`video/mp4`). A recording is far larger than the ten megabytes and thirty seconds a fetch is given by default, so say what the program is ready for:

```rust
use socketkit::microsoft::models::Download;

let limits = Download { max_bytes: Some(512 * 1024 * 1024), timeout_secs: Some(600) };
let video = microsoft.recordings(&connection).content(&meeting.id, &recording.id, limits).await?;
std::fs::write("review.mp4", &video.bytes)?;
```

All of it is held in memory before it is returned; a recording over the limit is the error `too_large`, and nothing is returned. Graph gives the video only to the meeting's organiser. There is no operation by this name, because an operation called by name never returns bytes.

## Page through a list

A list returns a `Page` with `items` and `next_cursor`. Pass the cursor back for the next page; `None` means the last page.

```rust
let mut paging = Paging { limit: Some(100), ..Default::default() };
loop {
    let page = microsoft.events(&connection).list_between(start, end, None, paging.clone()).await?;
    for event in &page.items { /* … */ }
    match page.next_cursor {
        Some(cursor) => paging.cursor = Some(cursor),
        None => break,
    }
}
```

The cursor is the address Graph gave for the next page (`@odata.nextLink`), and its query carries the rest of the request: the range, the page size, and Graph's own position marker. Pass it back unchanged, with the same arguments it came from.

A cursor comes back from the caller, so Socket does not trust it to be what Graph sent. **Nothing in a cursor is used as a host or a path.** The next page is requested at the address Socket builds from the method's own arguments, and the cursor supplies only the query. So a cursor cannot send the token to another host, the sign-in host included, and cannot make a listing read anything but its own list, whatever address it names. A cursor from another host, or with no query, is refused before any request is made. The cursor's path is not looked at: Graph writes the same list in more than one way, and a cursor from a different list simply continues this one at that cursor's position.

The query is used whole. Microsoft does not say which parameters a next page carries, so Socket takes none out. A cursor written by hand can therefore filter, sort or skip within the list it is given to, which a caller could also do by other means, and nothing more.

`limit` is Graph's `$top` and applies to the first page; later pages keep it. It is from 1 to 1000, or to 50 for messages and chats and 999 for members, and anything else is refused. A list Graph takes no page size for has only a `cursor`.

## Call an operation by name

Every method is also an operation an agent, an MCP server or another language can call with JSON. The plain arguments and the options sit side by side in one object.

```rust
let output = socket
    .invoke(key, "microsoft.events.create".into(), serde_json::json!({
        "subject": "Design review",
        "start": { "dateTime": "2026-10-12T16:00:00", "timeZone": "UTC" },
        "end": { "dateTime": "2026-10-12T17:00:00", "timeZone": "UTC" },
        "attendees": [{ "emailAddress": { "address": "grace@contoso.example" } }],
        "isOnlineMeeting": true
    }))
    .await?;
```

`socket.operations()` returns each operation's name, description, input schema, output schema, effect and scope. The effect lets a host ask a person before a change:

- **read** changes nothing.
- **write** adds something, or changes a mark that can be set back: creating an event, saving a draft, marking a message read, posting to a channel or a chat, creating a chat.
- **destructive** deletes, removes or overwrites what was there, or cannot be taken back: changing an event (which replaces its fields, and removes any attendee left out of a new list), answering an invitation (the organiser is told at once), cancelling a meeting, deleting an event; sending mail, changing a draft, moving a message (which can be to Deleted Items), deleting a message.

| Operation | Effect | Scope | What it does |
| --- | --- | --- | --- |
| `microsoft.identity.get` | read | User.Read | Return the account this connection is authorised as, confirming the token still works. |
| `microsoft.resource.resolve` | read | Files.ReadWrite | Confirm that a resource exists and the account can reach it. Accepts a OneDrive or SharePoint sharing link. |
| `microsoft.calendars.list` | read | Calendars.Read | List the account's calendars, its own and those shared with it. |
| `microsoft.calendars.get` | read | Calendars.Read | Get one calendar. |
| `microsoft.events.list_between` | read | Calendars.Read | List the events between two times, with each occurrence of a repeating event as its own event. Times are in UTC. |
| `microsoft.events.get` | read | Calendars.Read | Get one event, with its attendees, their answers and the link to join it. Times are in UTC. |
| `microsoft.events.instances` | read | Calendars.Read | List the occurrences of a repeating event between two times. |
| `microsoft.events.find_meeting_times` | read | Calendars.Read.Shared | Suggest times when a meeting could be held, from the attendees' calendars. Changes nothing. Work and school accounts only. |
| `microsoft.events.schedule` | read | Calendars.Read | Read when people, distribution lists and rooms are free and busy. Changes nothing. Work and school accounts only. |
| `microsoft.events.create` | write | Calendars.ReadWrite | Create an event and invite its attendees. Set isOnlineMeeting for a Teams meeting; the event returned carries the link to join it. |
| `microsoft.events.update` | destructive | Calendars.ReadWrite | Change an event, replacing the fields given and leaving the rest. Attendees left out of a new list are removed, and attendees are told of the change. |
| `microsoft.events.respond` | destructive | Calendars.ReadWrite | Answer an invitation: accept it, accept it tentatively, or decline it. The organiser is told, and the answer cannot be taken back. |
| `microsoft.events.cancel` | destructive | Calendars.ReadWrite | Cancel a meeting the account organised, and tell its attendees. |
| `microsoft.events.delete` | destructive | Calendars.ReadWrite | Delete an event from the account's calendar. Deleting a meeting the account organised cancels it for its attendees. |
| `microsoft.mail.list` | read | Mail.Read | List the messages of one folder or of the whole mailbox, with a filter, a search or a sort. Bodies are plain text unless HTML is asked for. |
| `microsoft.mail.get` | read | Mail.Read | Get one message, with its body as plain text unless HTML is asked for. |
| `microsoft.mail.conversation` | read | Mail.Read | List every message of one conversation, oldest first. |
| `microsoft.mail.attachments_list` | read | Mail.Read | List what is attached to a message: names, types and sizes, without the files. |
| `microsoft.mail.attachment_get` | read | Mail.Read | Describe one attachment: its name, type and size, without the file. |
| `microsoft.mail.attachment_text` | read | Mail.Read | Read an attachment that is text, such as a CSV file or a calendar invitation. One megabyte unless maxBytes allows more, up to ten. Anything that is not text is refused: bytes are not returned. |
| `microsoft.mail.create_draft` | write | Mail.ReadWrite | Save a new message in Drafts. Nothing is sent. |
| `microsoft.mail.update_draft` | destructive | Mail.ReadWrite | Change a draft, replacing the fields given and leaving the rest. |
| `microsoft.mail.create_reply` | write | Mail.ReadWrite | Save a reply to the sender of a message as a draft. Nothing is sent. |
| `microsoft.mail.create_reply_all` | write | Mail.ReadWrite | Save a reply to everyone on a message as a draft. Nothing is sent. |
| `microsoft.mail.create_forward` | write | Mail.ReadWrite | Save a forward of a message as a draft, addressed to toRecipients. Nothing is sent. |
| `microsoft.mail.send` | destructive | Mail.Send | Send a message at once, from the account's own address. It cannot be taken back. |
| `microsoft.mail.send_draft` | destructive | Mail.Send | Send a draft as it stands. It cannot be taken back. |
| `microsoft.mail.reply` | destructive | Mail.Send | Reply to the sender of a message and send the reply at once. It cannot be taken back. |
| `microsoft.mail.update` | write | Mail.ReadWrite | Mark a message: read or unread, its categories, its follow-up flag. |
| `microsoft.mail.move_to` | destructive | Mail.ReadWrite | Move a message to another folder, Deleted Items included. It comes back with a new id; the old one stops working. |
| `microsoft.mail.delete` | destructive | Mail.ReadWrite | Delete a message. |
| `microsoft.mail_folders.list` | read | Mail.Read | List the folders at the top of the mailbox, or those inside one folder. |
| `microsoft.mail_folders.get` | read | Mail.Read | Get one folder, by its id or by a well-known name such as inbox. |
| `microsoft.teams.list_joined` | read | Team.ReadBasic.All | List the teams the account is a member of. |
| `microsoft.teams.get` | read | Team.ReadBasic.All | Get one team. |
| `microsoft.teams.members` | read | TeamMember.Read.All | List a team's members and owners. Needs an administrator's consent. |
| `microsoft.channels.list` | read | Channel.ReadBasic.All | List a team's channels. |
| `microsoft.channels.get` | read | Channel.ReadBasic.All | Get one channel of a team. |
| `microsoft.channels.members` | read | ChannelMember.Read.All | List a channel's members and owners. Needs an administrator's consent. |
| `microsoft.channel_messages.list` | read | ChannelMessage.Read.All | List the messages that start a conversation in a channel, without their replies, each also as plain text. Needs an administrator's consent. |
| `microsoft.channel_messages.get` | read | ChannelMessage.Read.All | Get one message of a channel, also as plain text. Needs an administrator's consent. |
| `microsoft.channel_messages.replies` | read | ChannelMessage.Read.All | List the replies to a message of a channel, each also as plain text. Needs an administrator's consent. |
| `microsoft.channel_messages.send` | write | ChannelMessage.Send | Post a new message to a channel, as the signed-in person. Everyone in the channel sees it. |
| `microsoft.channel_messages.reply` | write | ChannelMessage.Send | Post a reply under a message of a channel, as the signed-in person. |
| `microsoft.chats.list` | read | Chat.ReadBasic | List the chats the account is in: one-to-one, group and meeting chats. |
| `microsoft.chats.get` | read | Chat.ReadBasic | Get one chat. |
| `microsoft.chats.members` | read | Chat.ReadBasic | List who is in a chat. |
| `microsoft.chats.messages` | read | Chat.Read | List a chat's messages, the most recently changed first, each also as plain text. |
| `microsoft.chats.message_get` | read | Chat.Read | Get one message of a chat, also as plain text. |
| `microsoft.chats.send` | write | ChatMessage.Send | Send a message to a chat, as the signed-in person. |
| `microsoft.chats.create` | write | Chat.Create | Create a chat between two people or among several. Returns the chat that already exists between two people, when there is one. |
| `microsoft.online_meetings.get` | read | OnlineMeetings.Read | Get one Teams online meeting by its id. |
| `microsoft.online_meetings.find_by_join_url` | read | OnlineMeetings.Read | Find the Teams online meeting behind a join link from a calendar event. Returns its id, which transcripts, recordings and attendance are asked for by. |
| `microsoft.transcripts.list` | read | OnlineMeetingTranscript.Read.All | List a meeting's transcripts. Empty when transcription was never switched on. Needs an administrator's consent. |
| `microsoft.transcripts.get` | read | OnlineMeetingTranscript.Read.All | Get one transcript's details: when it was made, and by whose meeting. Needs an administrator's consent. |
| `microsoft.transcripts.content` | read | OnlineMeetingTranscript.Read.All | Read what was said in a meeting: the transcript's text, and one entry for each thing said with the speaker, the start and the end. Needs an administrator's consent. |
| `microsoft.recordings.list` | read | OnlineMeetingRecording.Read.All | List a meeting's recordings. Empty when the meeting was not recorded. Needs an administrator's consent. |
| `microsoft.recordings.get` | read | OnlineMeetingRecording.Read.All | Get one recording's details, with the address its video is at. Needs an administrator's consent. |
| `microsoft.attendance.reports` | read | OnlineMeetingArtifact.Read.All | List a meeting's attendance reports, one for each time it was held. |
| `microsoft.attendance.records` | read | OnlineMeetingArtifact.Read.All | List who joined a meeting, in what role, when, and for how long. |

`find_meeting_times` and `schedule` are reads that Graph offers only as POST. They are marked `read` because they change nothing, which is what the effect is for.

`update` and `respond` are marked `destructive`, not `write`, by the same rule the Slack and GitHub operations follow: an update overwrites, and an answer cannot be taken back. A host that asks a person only before a destructive operation therefore asks before both.

The same rule marks four mail operations `destructive` that could be read as writes. `send`, `send_draft` and `reply` put mail in other people's inboxes, which cannot be undone. `update_draft` overwrites a draft's text. `move_to` takes a message from where it was, to Deleted Items if asked, and its id stops working. Saving a draft, and marking a message, stay `write`: a draft is the person's own until it is sent, and a mark can be set back.

Posting to a channel or a chat, and creating a chat, are `write`, as posting a message is for Slack: a message that was posted can be deleted by whoever posted it. A host that lets writes run without asking should know that a post is seen by everyone in the channel at once.

## Handle errors

| Kind | What it means for Microsoft | What to do |
| --- | --- | --- |
| `ReconnectRequired` | Graph answered 401 or called the token invalid, or the refresh token is no longer accepted | Socket renews the token once by itself; if that fails, connect again |
| `AccessDenied` | A permission is missing or not yet approved, or the account may not open the item. The message carries Graph's own reason | Add the permission, or have an administrator approve it |
| `NotFound` | No such item, or a sharing link that no longer exists | Check the id or the link |
| `InvalidInput` | Graph refused the request; the message carries its reason | Fix the input |
| `RateLimited` | Graph is throttling; `retry()` says how long to wait | Wait and try again |
| `Unexpected` | Graph failed. On a 503 that states a wait, `retry()` carries it | Try again later |
| `Decode` | Graph answered success without what was asked for, or with something that could not be read | Report it; this should not happen |

An input error never repeats the value you sent. It names the field when a required one is missing or a plain argument has the wrong type; for a wrong type inside the options it says only that a field has the wrong type.

**A field the operation does not know is refused**, at any depth, and named: `body.content_type`, `toRecipients[1].emailAddres`. Other providers ignore such a field; here it is refused, because a dropped field takes what it said with it: a subject, a time zone, the people in copy, "keep no copy". Each operation's input schema says the same, with `additionalProperties: false`. This applies to an operation called by name; the typed methods take structs, where a misspelt field does not compile. An answer that cannot be read is reported the same way: the error says where the unreadable value was, and neither its message nor its cause carries anything from the calendar.

A request sent as GET is retried on a throttle or a server error. Creating, changing, answering and cancelling are sent again in only two cases, both of which mean Graph did not carry them out: Graph throttled the request, or Graph rejected the access token and Socket renewed it to a different one. **If one of them fails with a server error, check before sending it again**; when creating, a `transactionId` of your own makes a second try safe, because Graph does not create a second event for one it has seen. The two reads sent as POST are not retried after a server error either.

**Deleting is the exception today.** The transport still repeats a DELETE after a server error. If the first try did delete the event or the message, the second is answered "not found", and the call reports `NotFound` for a delete that worked. Treat `NotFound` from `delete` as "it is gone".

Mail that fails with a server error may have been sent. Socket never sends it a second time; look in Sent Items before trying again.

## Confirmed against Microsoft's documentation, and not

Everything here was read from learn.microsoft.com in October 2026. Nothing was run against a live tenant.

Confirmed:

- The authorise and token addresses, the four kinds of tenant, space-separated scopes, and that `offline_access` is what yields a refresh token.
- PKCE with `S256` is recommended for web applications and shown beside the client secret.
- A refresh returns a new refresh token, which replaces the old one.
- `login_hint`, and the four `prompt` values.
- The client secret is sent in the form body.
- The numbered codes `AADSTS65001`, `90008`, `90094` and `90095`, with the meanings above.
- Graph's error shape `{ "error": { "code", "message" } }`, and `Retry-After` in seconds on 429 and on 503. A wait written as a date is read as well.
- `GET /me` and its fields, with `User.Read`.
- `GET /shares/{encoded link}/driveItem`, the encoding (`u!`, then the link in base64url without padding), and `Files.ReadWrite` as its least permission.
- Paging: the whole of `@odata.nextLink` is requested as it is, and nothing is taken out of it.
- Every calendar endpoint above, with its verb, body and status: `GET /me/calendars` and `/me/calendars/{id}`; `GET /me/calendarView` and `/me/calendars/{id}/calendarView` with `startDateTime` and `endDateTime`; `GET /me/events/{id}` and `/me/events/{id}/instances`; `POST /me/findMeetingTimes`; `POST /me/calendar/getSchedule`; `POST /me/events` and `/me/calendars/{id}/events` (201); `PATCH /me/events/{id}`; `POST /me/events/{id}/accept`, `/tentativelyAccept`, `/decline` and `/cancel` (202, no body); `DELETE /me/events/{id}` (204).
- The fields of a calendar and of an event, with their spelling, and the values of `type`, `showAs`, `response` and `onlineMeetingProvider` given beside those fields in `models`.
- `isOnlineMeeting` with `onlineMeetingProvider: "teamsForBusiness"` returns `onlineMeeting.joinUrl`.
- `Prefer: outlook.timezone`, that Graph answers in UTC without it, and `originalStartTimeZone` and `originalEndTimeZone`.
- That an offset in `startDateTime` is honoured and a time without one is UTC; `$top` from 1 to 1000 on a calendar view.
- The limits of `getSchedule` (20 schedules, under 62 days, an interval of 5 to 1440 minutes), and that it and `findMeetingTimes` do not support personal accounts.
- Every mail endpoint above, with its verb, body and status: `GET /me/messages`, `/me/mailFolders/{id}/messages` and `/me/messages/{id}`; `GET /me/messages/{id}/attachments`, `/attachments/{id}` and `/attachments/{id}/$value`; `GET /me/mailFolders`, `/me/mailFolders/{id}` and `/me/mailFolders/{id}/childFolders` with `includeHiddenFolders`; `POST /me/messages`; `PATCH /me/messages/{id}`; `POST /me/messages/{id}/createReply`, `/createReplyAll` and `/createForward`; `POST /me/sendMail`, `/me/messages/{id}/send` and `/me/messages/{id}/reply` (202, no body); `POST /me/messages/{id}/move` with `destinationId`; `DELETE /me/messages/{id}` (204).
- The fields of a message, an attachment and a folder, with their spelling, and the seventeen well-known folder names.
- `Prefer: outlook.body-content-type` with `text` and `html`, and that HTML is the default.
- `$search` in double quotes, its searchable properties, the cap of 1,000 results and the sort by sent time; `$top` from 1 to 1000 and a page of 10 by default.
- The three rules for a filter with a sort, and the error `InefficientFilter`.
- That `comment` with a `body` is refused on a reply, and that a forward needs its recipients in exactly one place.
- Which fields of a message can be changed only on a draft, and that `isRead`, `categories` and `flag` can be changed on any message.
- That a moved message is a new copy under a new id.
- The permissions: `Mail.Read` for attachments, `Mail.ReadWrite` for drafts, changes, moving and deleting, `Mail.Send` to send.
- That `/attachments/{id}/$value` answers 200 with the attachment itself, a file in its own type and an attached item in MIME, and refuses a link to a file with 405; and that naming the fields with `$select` leaves `contentBytes` out of one attachment.
- Every Teams endpoint above, with its verb, body and status: `GET /me/joinedTeams`, `/teams/{id}` and `/teams/{id}/members`; `GET /teams/{id}/channels`, `/channels/{id}` and `/channels/{id}/members`; `GET …/channels/{id}/messages`, `/messages/{id}` and `/messages/{id}/replies`; `POST …/messages` and `…/messages/{id}/replies` (201); `GET /me/chats`, `/chats/{id}`, `/chats/{id}/members`, `/chats/{id}/messages` and `/messages/{id}`; `POST /chats/{id}/messages` and `POST /chats` (201).
- The fields of a team, a channel, a member, a chat and a message, with their spelling; the `<at id>` and `<attachment id>` tags of a body; that a system message has no sender and the body `<systemEventMessage/>`; and that `Prefer: include-unknown-enum-members` is what names it.
- The page sizes: 50 for channel messages, replies, chats and chat messages, 999 for members; that joined teams, channels and chat members take none.
- Creating a chat: the body, that every member is named with the caller among them, and that an existing one-to-one chat is returned.
- Which permissions need an administrator's consent, and the limits of about one request a second for each channel and chat.
- Every meeting endpoint above: `GET /me/onlineMeetings/{id}` and `?$filter=JoinWebUrl eq '…'` with the link encoded; `…/transcripts`, `/transcripts/{id}` and `/transcripts/{id}/content` with `Accept: text/vtt`; `…/recordings`, `/recordings/{id}` and `/recordings/{id}/content`, which answers with the video itself as `video/mp4`; `…/attendanceReports` and `/attendanceReports/{id}/attendanceRecords`.
- The WebVTT a transcript is written in, with the speaker in a voice tag at the start of each cue, and that a time can be negative.
- That transcripts and recordings are not offered for a meeting with no calendar event, a live event or an expired meeting; that attendance is the organiser's alone, and so is the video of a recording.

Not confirmed:

- **`InvalidAuthenticationToken`** is not in Microsoft's error reference; it is treated as "renew the token" because the issue that asked for this provider says Graph returns it. A 401 is handled the same way with or without that code.
- **Which `error` value accompanies each numbered code** at the token endpoint. Socket goes by the number, and by `consent_required` when there is no number. Microsoft warns that the numbers may change over time.
- **What Graph answers when a sharing link cannot be opened.** Socket treats a 403 as a refusal and a 404 as not found, and cannot tell a missing permission from an item the account may not see.
- **Whether a sharing link's item always carries `parentReference.driveId`.** When it does not, the resource's id is `shares/{encoded link}/driveItem`, which addresses the same item.
- **`UTC` as the value of `Prefer: outlook.timezone`.** Microsoft's examples use other zones. `UTC` is in its list of zone names, and is also what Graph answers in when the header is absent or not understood.
- **How an all-day event's times read in UTC.**
- **Permissions, where the issue and the documentation differ.** The documentation gives `Calendars.ReadBasic` as the least permission for reading, which leaves out an event's body. The operations name `Calendars.Read`, which returns everything listed here. For `findMeetingTimes` the documentation gives `Calendars.Read.Shared`, and that is what the operation names.
- **`/me/calendarView`.** The documentation's request list writes `/me/calendar/calendarView`, and its example and text write `/me/calendarView`, which is what Socket calls.
- **`timeSlots`.** The `findMeetingTimes` page spells it so, and so does Socket; the page for the type itself spells it `timeslots`.
- **The `comment` of a cancellation.** The parameter is listed as `comment`, which Socket sends; the documentation's example sends `Comment`.
- **`$` in a parameter name written as `%24`**, as in `%24top`. This is ordinary URL encoding, not confirmed for Graph specifically.
- **An id that contains `/`.** Socket percent-encodes an id as one path segment. Whether Graph issues such ids on v1.0, and how it reads an encoded one, was not confirmed.
- **The default page size** of a calendar view. Pass `limit` to choose one.
- **That the query of a next page's address is all that is needed.** Microsoft's guidance is to request the whole address as it stands. Socket requests its own address for the list with that address's query, which is the same request as long as the two addresses name the same list, however Graph writes the path (`me/events('…')`, `users('…')/…`). If Graph ever keeps part of its place in the path, paging would return the wrong page; nothing in the documentation suggests it does.
- **What Graph does with an attendee's `status` in a request.** Socket leaves it out.
- **The query behind `conversation`.** No page of Microsoft's shows how to read one conversation in order. A filter on `conversationId` sorted by `receivedDateTime` breaks the documented rules, so Socket filters on `receivedDateTime ge 1900-01-01T00:00:00Z and conversationId eq '…'`, which follows them. That it works was not confirmed.
- **Combining `search` with `filter` or `orderBy`.** Graph's documentation does not say it can be done, and an older one says it cannot, so Socket refuses it. How a search pages, and whether a search works inside one folder, were not confirmed either.
- **That `$select` leaves the content out of a list of attachments.** Microsoft's guide to large attachments shows it for one attachment, which is how `attachment_get` asks; no page shows it for the list. If it does not apply there, a message with large attachments makes the list too large to read.
- **The type Graph states for an attached message, event or contact.** For a file, Microsoft says the type of `$value` follows the file's own. For an attached item it says only that the content is MIME, so `attachment_text` may refuse one as not text.
- **Whether a list of messages honours the text preference.** One page says a list returns HTML only, and lists the header all the same. `body.contentType` says what came back.
- **Where a deleted message goes.** The page for deleting does not say. To be sure it lands in Deleted Items, use `move_to` with `deleteditems`.
- **The body of `send_draft`.** Microsoft asks for an empty request with `Content-Length: 0`. Socket's transport sends no length for a request with no body, which a server may refuse, so Socket sends an empty JSON object instead. That Graph accepts it was not confirmed.
- **The status of a new draft.** The text says 201 and some examples show 200. Socket accepts either.
- **The casing of `importance` and `contentType`.** Pages differ (`low` and `Low`, `text` and `Text`). Socket passes on what it is given and returns what Graph sends.
- **That a deleted message's body is empty.** No page shows one. Socket returns whatever Graph sends, and `deletedDateTime` says it was deleted.
- **Whether joined teams, members and replies are paged.** Their pages do not say. Socket passes on a next page when Graph gives one.
- **That `+` is not read as a space in a query.** Graph's examples write `%20`, and so does Socket, for every query it sends.
- **Whether `find_by_join_url` finds a channel meeting**, and whether transcripts can be read for a meeting in a private channel. One Microsoft page says channel meetings are covered, another that they are in beta only.
- **That a transcript exists only when transcription was on.** Microsoft does not state it as a rule; it is what an empty list means.
- **How large a recording or a transcript can be.** A transcript over 10 MB cannot be read. A recording is read up to the limit its caller sets.
- **That a recording is always served by Graph itself.** Microsoft's example shows the bytes in Graph's own answer. Were Graph to send a reader to another host, the fetch would stop with "redirected … to an address Socket does not follow" until that host is declared as a content host of the provider.
- **What a mention must carry to notify someone.** Socket sends `mentions` as given; Microsoft's example includes `userIdentityType`.

## Not supported yet

- **Application-only access** (client credentials), where no person signs in.
- **The national clouds** (US Government, China), which use other hosts.
- **Certificates** in place of a client secret.
- **Files**: OneDrive and SharePoint beyond looking up a sharing link.
- **A recording as a stream.** The whole video is held in memory. And the transcript format without speakers.
- **Editing and deleting a Teams message, reactions, tabs and apps**, and **change notifications**.
- **Attaching a file to a Teams message.**
- **A date range for chat messages.** Graph takes one only with a matching sort; Socket lists the most recently changed first.
- **Shared and delegated mailboxes** (`/users/{id}/…`, `Mail.Read.Shared`).
- **Adding an attachment** to a draft.
- **`replyAll` and `forward` sent at once.** Make the draft with `create_reply_all` or `create_forward`, then `send_draft`.
- **Ids that survive a move** (`Prefer: IdType="ImmutableId"`), **MIME content**, **message rules**, **focused inbox overrides**, and **creating or deleting folders**.
- **Other people's calendars** (`/users/{id}/…`) and **calendar groups**. Everything here is the signed-in account's.
- **Creating, renaming and deleting calendars.**
- **Change tracking** (`/delta`), and **receiving change notifications**.
- **Attachments, reminders, categories and forwarding** of an event.
- **Choosing the format of a body.** Graph returns HTML unless asked for text.

Anything Graph offers that has no method here can still be called through the generic request, with the token, retries and error handling applied:

```rust
let response = socket.request(key, RawRequest::get("me/messages").with_query("$top", "5")).await?;
```
