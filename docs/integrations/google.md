# Google

**Status:** built and tested against a local server that answers as Google's documentation says. Not yet run against the real Google. [What was confirmed against that documentation and what was not](#confirmed-against-googles-documentation-and-not) is listed below.

One provider covers Google's products, because they share one sign-in. Today Socket's Google integration gives a program identity, Drive file lookup, and 63 typed methods: Gmail (20), Calendar (10), Meet (12), Drive (12), and Docs and Sheets (9). Each is also an operation callable by name with JSON, except the two that return a file's bytes, `gmail_messages.attachment_content` and `drive_files.download`: an operation called by name returns text, never bytes, so those are typed only and each has a named counterpart that reads text. This page shows how to connect, lists everything that is supported, and says what is not.

## Connect

```sh
cargo add socketkit --features google
```

### With a token you already hold

```rust
use std::sync::Arc;
use socketkit::google::Google;
use socketkit::{ConnectionKey, ProviderId, Socket};

let google = Google::with_token("ya29.your-access-token");
let socket = Socket::in_memory().integration(Arc::new(google.clone())).build()?;
let connection = socket.connection(ConnectionKey::new(ProviderId::new("google")?, "me")).await?;
```

Every call uses that token. The tenant name (`"me"`) is only a label. A Google access token lasts about an hour and this way has no refresh token, so it suits a script more than a service.

### With your own OAuth app, to connect your users

Only two scopes are asked for by default: `drive.readonly` and `documents.readonly`. Everything else is named by the application, so a person is asked for exactly what it uses. The constants are in `socketkit::google::scopes`.

```rust
use socketkit::google::{Google, GoogleOAuth, scopes};
use socketkit::{OAuthClient, SecretString};

let google = Google::with_oauth(GoogleOAuth {
    client: OAuthClient {
        client_id: config.google_client_id,
        client_secret: SecretString::new(config.google_client_secret),
        redirect_uri: "https://yourapp.example/oauth/google/callback".parse()?,
    },
    scopes: Some(vec![
        // Leave out whichever the application does not use.
        scopes::DRIVE_READONLY.into(),          // identity, Drive lookup, and every Drive read
        scopes::GMAIL_READONLY.into(),          // read mail
        scopes::CALENDAR_READONLY.into(),       // read calendars
        scopes::MEETINGS_SPACE_READONLY.into(), // read what happened in a meeting
    ]),
    hosted_domain: None,
    login_hint: None,
});
let socket = Socket::builder(Arc::new(my_token_store)).integration(Arc::new(google.clone())).build()?;

// When a user clicks "Connect Google":
let key = ConnectionKey::new(ProviderId::new("google")?, user.id);
let authorization = socket.begin_authorization(key.clone(), None)?;
// Keep `authorization.pending` in that user's session, then redirect them to `authorization.url`.

// At your callback route:
socket.complete_authorization(pending, code, state).await?;
let connection = socket.connection(key).await?;
```

Setting `scopes` replaces the defaults, so name the Drive and Docs scopes again if the application still uses them. Each scope is `https://www.googleapis.com/auth/` followed by the name below.

| Scope | Constant | Covers |
| --- | --- | --- |
| `drive.readonly` (default) | `DRIVE_READONLY` | `identity.get`, `resource.resolve`, `drive_files.list`, `.get`, `.export`, `.download_text`, `.permissions`, `drive_shared_drives.list`, and the typed `download` |
| `drive.file` | `DRIVE_FILE` | `drive_files.create_folder`, `.copy`, `.move_to`, `.rename`, `.trash`, on the files this application created or was given |
| `documents.readonly` (default) | `DOCUMENTS_READONLY` | `docs_documents.get`, `.read` |
| `documents` | `DOCUMENTS` | `docs_documents.create`, `.append_text` |
| `spreadsheets.readonly` | `SPREADSHEETS_READONLY` | `sheets_spreadsheets.get`, `.values_get`, `.values_batch_get` |
| `spreadsheets` | `SPREADSHEETS` | `sheets_spreadsheets.values_update`, `.values_append` |
| `gmail.readonly` | `GMAIL_READONLY` | every Gmail read, and the read a reply makes of the message it answers |
| `gmail.compose` | `GMAIL_COMPOSE` | `gmail_drafts.create`, `.update`, `.delete`, `gmail_messages.send_draft` |
| `gmail.send` | `GMAIL_SEND` | `gmail_messages.send`, `.reply` |
| `gmail.modify` | `GMAIL_MODIFY` | `gmail_messages.modify`, `.trash`, `.untrash` |
| `calendar.readonly` | `CALENDAR_READONLY` | `calendar_list.*`, `calendar_events.list`, `.get`, `.instances`, `calendar_freebusy.query` |
| `calendar.events` | `CALENDAR_EVENTS` | `calendar_events.insert`, `.patch`, `.respond`, `.delete` |
| `meetings.space.readonly` | `MEETINGS_SPACE_READONLY` | every Meet method |

Three things surprise people:

- **Identity needs a Drive scope.** `google.identity.get` reads the account from Drive, and `google.resource.resolve` looks up a Drive file. A connection with only Gmail or Calendar scopes can use every Gmail or Calendar operation, but those two are refused with `AccessDenied`. Keep `drive.readonly` if the application calls them; `gmail_profile.get` gives the address of a Gmail-only connection.
- **Sheets is not covered by the defaults.** The defaults read Drive and Docs. Reading a Sheet's cells needs `spreadsheets.readonly`.
- **Google treats the Gmail scopes as restricted**, and `drive.readonly` too. An application that asks for them goes through Google's verification, which can include a security assessment, before it can be used outside its own organisation. That is the application's work, not Socket's, but it is best known before a release is planned.

## Identity and lookup

```rust
let me = google.identity(&connection).await?;                  // id, name, email
let doc = google.resolve(&connection, "https://docs.google.com/document/d/1AbC…/edit").await?;
```

`resolve` accepts a Drive, Docs or Sheets URL, or a file id, and confirms the account can open it. Its `id` is what the Drive, Docs and Sheets methods take. To require accounts from one Google Workspace domain, set `hosted_domain` on `GoogleOAuth`.

## Use the typed methods

Methods are grouped the way Google groups its own APIs. Each group is reached from the integration with a connection:

| Product | Reached with | Methods |
| --- | --- | --- |
| Gmail | `google.gmail_messages(&connection)` | `list`, `get`, `attachment_content`, `attachment_text`, `send`, `reply`, `send_draft`, `modify`, `trash`, `untrash` |
| | `google.gmail_threads(&connection)` | `list`, `get` |
| | `google.gmail_labels(&connection)` | `list`, `get` |
| | `google.gmail_drafts(&connection)` | `list`, `get`, `create`, `update`, `delete` |
| | `google.gmail_profile(&connection)` | `get` |
| Calendar | `google.calendar_list(&connection)` | `list`, `get` |
| | `google.calendar_events(&connection)` | `list`, `get`, `instances`, `insert`, `patch`, `respond`, `delete` |
| | `google.calendar_freebusy(&connection)` | `query` |
| Meet | `google.meet_conference_records(&connection)` | `list`, `get` |
| | `google.meet_participants(&connection)` | `list`, `get`, `sessions` |
| | `google.meet_transcripts(&connection)` | `list`, `get`, `entries`, `read` |
| | `google.meet_recordings(&connection)` | `list`, `get` |
| | `google.meet_spaces(&connection)` | `get` |
| Drive | `google.drive_files(&connection)` | `list`, `get`, `export`, `download` (typed only), `download_text`, `permissions`, `create_folder`, `copy`, `move_to`, `rename`, `trash` |
| | `google.drive_shared_drives(&connection)` | `list` |
| Docs | `google.docs_documents(&connection)` | `get`, `read`, `create`, `append_text` |
| Sheets | `google.sheets_spreadsheets(&connection)` | `get`, `values_get`, `values_batch_get`, `values_update`, `values_append` |

Three rules hold for every method:

- **What identifies the thing is a plain argument**: a message id, a calendar id, a file id. Every id is written into the address as one segment, whatever it contains.
- **Content and options are a struct** from `socketkit::google::models`. A field left unset is not sent, so Google applies its own default.
- **Names are Google's.** In JSON, fields are spelled as Google spells them: `threadId`, `timeMin`, `hangoutLink`, `mimeType`. In Rust the same fields are `thread_id`, `time_min`, `hangout_link`, `mime_type`.

From a meeting to what was said in it takes three of the products. A calendar event keeps `hangoutLink` and `conferenceData` (the Meet code is `conferenceData.conferenceId`). `meet_conference_records.list` finds the meetings held under that code, and `meet_transcripts.read` returns one as entries of speaker, start, end and text, the same four fields a Teams transcript has in the Microsoft integration. A transcript's `docsDestination.document` and a recording's `driveDestination.file` are ids for `docs_documents.read` and `drive_files.get`.

### Gmail: `gmail_messages`, `gmail_threads`, `gmail_labels`, `gmail_drafts` and `gmail_profile`

Reading, searching, labelling, drafting and sending mail in the mailbox of the account that connected. A message comes back decoded: Gmail returns a tree of MIME parts in base64, and Socket returns headers, text, HTML and a list of attachments. A message that is sent is written by Socket from structured content, so no caller builds a MIME message or encodes one.

None of the Gmail scopes is a default of the provider. Name the ones you need in `GoogleOAuth.scopes`, from `socketkit::google::scopes`.

| Method | What it does | Effect | Scope |
| --- | --- | --- | --- |
| `gmail_messages.list(GmailListMessages, Paging)` | `Page<GmailMessageRef>`: the ids of the messages a search finds | read | `GMAIL_READONLY` |
| `gmail_messages.get(message, GmailGetMessage)` | `GmailMessage`: headers, text, HTML, attachments without their content | read | `GMAIL_READONLY` |
| `gmail_messages.attachment_content(message, attachment, Download)` | `Content`: one attachment's bytes, unchanged. Typed only, not a named operation | read | `GMAIL_READONLY` |
| `gmail_messages.attachment_text(message, attachment, TextLimit)` | `GmailAttachmentText`: an attachment that is text, as text, and its size | read | `GMAIL_READONLY` |
| `gmail_messages.send(GmailSendMessage)` | `GmailMessageRef`: sends at once | destructive | `GMAIL_SEND` |
| `gmail_messages.reply(message, GmailReply)` | `GmailMessageRef`: answers in the same thread, at once, to the people named in `to` | destructive | `GMAIL_READONLY` and `GMAIL_SEND` |
| `gmail_messages.send_draft(draft)` | `GmailMessageRef`: sends a draft as it stands | destructive | `GMAIL_COMPOSE` |
| `gmail_messages.modify(message, GmailModifyMessage)` | `GmailMessageRef`: adds and removes labels | write | `GMAIL_MODIFY` |
| `gmail_messages.trash(message)` | `GmailMessageRef`: to the bin | destructive | `GMAIL_MODIFY` |
| `gmail_messages.untrash(message)` | `GmailMessageRef`: back from the bin | write | `GMAIL_MODIFY` |
| `gmail_threads.list(GmailListThreads, Paging)` | `Page<GmailThread>`: ids and snippets, without messages | read | `GMAIL_READONLY` |
| `gmail_threads.get(thread, GmailGetThread)` | `GmailThread`: every message of one thread, decoded | read | `GMAIL_READONLY` |
| `gmail_labels.list()` | `Vec<GmailLabel>`: every label, without counts | read | `GMAIL_READONLY` |
| `gmail_labels.get(label)` | `GmailLabel`, with its counts | read | `GMAIL_READONLY` |
| `gmail_profile.get()` | `GmailProfile`: the address, the totals, the current history id | read | `GMAIL_READONLY` |
| `gmail_drafts.list(GmailListDrafts, Paging)` | `Page<GmailDraftRef>`: ids only | read | `GMAIL_READONLY` |
| `gmail_drafts.get(draft, GmailGetMessage)` | `GmailDraft`, with its message decoded | read | `GMAIL_READONLY` |
| `gmail_drafts.create(GmailSendMessage)` | `GmailDraftRef`: saves a draft, sends nothing | write | `GMAIL_COMPOSE` |
| `gmail_drafts.update(draft, GmailSendMessage)` | `GmailDraftRef`: replaces the whole draft | destructive | `GMAIL_COMPOSE` |
| `gmail_drafts.delete(draft)` | nothing: deletes the draft for good | destructive | `GMAIL_COMPOSE` |

Each but `attachment_content` is also a named operation: `google.gmail_messages.list`, `google.gmail_drafts.create`, and so on. An operation called by name never returns a file's bytes, so `attachment_content` is a typed method only, and `attachment_text` is the one that can be called by name. In an operation's input the plain arguments are `message`, `attachment`, `thread`, `label` and `draft`, the options sit beside them under Gmail's own names, and paging is `cursor` and `limit`.

```rust
use socketkit::google::models::{
    GmailAddress, GmailGetMessage, GmailListMessages, GmailModifyMessage, GmailReply, Paging,
};

let messages = google.gmail_messages(&connection);

// Ids only: one request, however many messages match.
let unread = messages.list(
    GmailListMessages {
        q: Some("from:grace@example.test newer_than:7d".into()),
        label_ids: Some(vec!["INBOX".into(), "UNREAD".into()]),
        ..Default::default()
    },
    Paging { cursor: None, limit: Some(20) },
).await?;

for row in &unread.items {
    let message = messages.get(&row.id, GmailGetMessage::default()).await?;
    println!("{:?}: {:?}", message.from, message.subject);
    let said = message.text.or(message.html).unwrap_or_default();

    // Answer in the same thread, then archive and mark as read. Who the
    // answer goes to is said here: the message's own `from` is its sender's
    // text, and is never used unasked.
    messages.reply(&row.id, GmailReply {
        to: vec![GmailAddress::new("grace@example.test")],
        text: Some(format!("Thanks, I have read all {} characters.", said.len())),
        ..Default::default()
    }).await?;
    messages.modify(&row.id, GmailModifyMessage {
        remove_label_ids: Some(vec!["INBOX".into(), "UNREAD".into()]),
        ..Default::default()
    }).await?;
}
```

**Google treats these scopes as restricted.** `gmail.readonly`, `gmail.compose` and `gmail.modify` are restricted scopes, and `gmail.send` is a sensitive one. An application that asks for a restricted scope goes through Google's verification before anyone outside its own organisation can use it, and through a security assessment if it keeps or passes on what it reads. That is the application's work with Google, not something Socket does, and it is better known before the first user is asked to connect than after.

**Whose mailbox is this?** `google.identity.get` reads Drive's `about`, so it needs a Drive scope even in an application that is otherwise only Gmail's. With Gmail scopes alone, call `gmail_profile.get`: `emailAddress` is the account.

**A list is light.** `gmail_messages.list` returns what Gmail returns: each message's `id` and `threadId`, and nothing else. Socket does not fetch each message to fill the row in, so a list of 500 is one request. Read the ones you need with `get`. The same holds for `gmail_threads.list` (id, `snippet`, `historyId`, no messages) and `gmail_drafts.list` (the draft's id and its message's ids). `limit` is from 1 to 500; Gmail returns 100 when it is not given.

**Searching.** `q` is the search box of Gmail, word for word: `from:grace is:unread`, `subject:(q3 plan) newer_than:7d`, `has:attachment filename:pdf`. `labelIds` keeps only what carries every label named. Spam and the bin are left out unless `includeSpamTrash` is `true`. Send the same filters with a `cursor` as with the first page.

**What a `GmailMessage` carries.** `id`, `threadId`, `labelIds`, `snippet`, `historyId`, `internalDate` (milliseconds since 1970, as a string), `sizeEstimate`; the headers `from`, `to`, `cc`, `bcc`, `replyTo`, `subject`, `date`, `messageId`, `inReplyTo` and `references`, each as the sender wrote it; `text` and `html`; and `attachments`.

- Header names are matched in any case. A header mail allows once (`subject`, `from`, `date`) is its first occurrence. A list of people written as several headers is joined with commas.
- Header values are returned with RFC 2047 encoded words read (`=?UTF-8?B?…?=`), in UTF-8, ISO-8859-1 and Windows-1252. A word in another character set is left as it was sent.
- A header, a subject and a file name are a stranger's text, so what a person would not see in them is returned as a space: control characters, zero-width characters and joiners, soft hyphens, line separators, and the marks that turn the direction of writing around (which can make `exe.fdp` read as `pdf.exe`, or a name read as an address). An emoji joined from several is returned as its parts.
- `from`, `to`, `cc`, `bcc` and `replyTo` are written so that the mailbox cannot be mistaken: it is the address in angle brackets, or the one that stands alone. A name beside it is bare only when it is plain words (letters, digits, spaces, `.`, `-`, `'`, `_`), and in quotes otherwise, so a name written to look like an address, with `@` and `<` or with the full-width signs that resemble them, stays a quoted name. What is not one plain mailbox is returned as text in quotes and never as an address: a group, an address with a quoted part or an encoded word in it, and a whole header that ends inside a comment or a quoted name. These fields are for a person or a program to read and decide on. Nothing Socket does is decided by them.
- `text` is every `text/plain` part that is not a file, and `html` every `text/html` part. Of the alternatives of one message (`multipart/alternative`) one text and one HTML are kept. A body is read in the character set its part names; UTF-8, ASCII, ISO-8859-1 and Windows-1252 are read exactly, and anything else is read as UTF-8 with the replacement character where it is not.
- `attachments` lists every part that is a file: `attachmentId`, `filename`, `mimeType`, `size`, `inline`, `contentId` and `partId`. A text file that was attached is listed here and is not part of `text`. `inline` is `true` for a part the sender marked to be shown in the body, such as a picture in a signature, which the HTML refers to as `cid:` and its `contentId`.
- `format` chooses how much comes back: `full` (the default), `metadata` (headers, no body, no attachments) or `minimal` (ids and labels). Gmail's `raw` format is not offered, since it is the undecoded message.

**Attachments.** A message lists its files without their content. Each entry's `attachmentId` is what the two methods that read a file take, with the message's id.

- **`attachment_content(message, attachment, Download)`** returns the file as a `Content`: its bytes, unchanged. Gmail hands a file over in base64 inside JSON, and Socket takes it out. Gmail does not say what the file is, so `content_type` is always `None`; the type and the name are on the message, in the `mimeType` and `filename` of the entry the id came from. It is a typed method only. No operation called by name returns bytes.
- **`attachment_text(message, attachment, TextLimit)`**, which is `google.gmail_messages.attachment_text`, returns a file that is text: `{ "size": …, "text": … }`, with `size` the file's bytes. Since Gmail states no type, the file has to be text by its own bytes: UTF-8, with no control character but a tab, a line feed, a carriage return and a form feed. Anything else is refused with `Unsupported` and nothing of it is returned: a PDF, a picture, a spreadsheet, text in another encoding (UTF-16, Windows-1252), and a file that reads as UTF-8 and holds a NUL or an escape character, which is what a small binary file looks like and how text takes over the screen it is shown on. A byte order mark at the start is left out of `text`.
- **Limits.** `Download.max_bytes` is the most bytes of the file to accept: ten megabytes unless set, and as much as the caller sets. `TextLimit.max_bytes` (`maxBytes` in an operation's input) is one megabyte unless set, and at most ten; asking for more is refused with `InvalidInput` before Google is called. A file over the limit is refused whole with `TooLarge` (`too_large`), never cut short, and the error names the limit that was asked for. The limit is on the file. What Socket fetches is larger, by the third that base64 adds and by the JSON around it, and the fetch is limited to that, so a file far over the limit is not read to its end. `Download.timeout_secs` is the longest to wait, thirty seconds unless set.
- An answer that holds no file, or data that is not base64, is `Decode`, with nothing of it in the error. An empty file is a file: zero bytes, or the text `""`.

**Sending.** `GmailSendMessage` is `to`, `cc`, `bcc`, `subject`, `text` and `html`. Each person is `{ "email": "grace@example.test", "name": "Grace Hopper" }`, with the name optional. Socket writes the message as mail travels (RFC 5322 and MIME) and sends it to Gmail in `raw`:

- `send` needs at least one person in `to`, `cc` or `bcc`, and a `subject`, `text` or `html`.
- With `text` and `html` the message is `multipart/alternative`, and each reader sees the one its mail program prefers. With one of them it is that alone.
- Bodies are sent in base64 as UTF-8, so any text arrives as written. Line ends become CR LF, as mail requires.
- A subject or a name outside ASCII, or too long for one line, is sent as encoded words and reads normally in every mail program.
- The message goes out from the account's own address. There is no `from`; Gmail writes it, with the date and the message's id.
- People in `bcc` are sent to Gmail with the rest. Gmail delivers to them and leaves them out of what the others receive.

**What is refused, and why.** A line break or any other control character in `subject` or in a `name` is refused with `InvalidInput`, and so is an `email` that is not exactly one mailbox in ASCII (`grace@example.test`): no list, no `Name <address>`, no quoted part, no encoded word (`=?`). A line break in a header would end it and begin another, which is how a subject becomes a hidden `Bcc`. Nothing is sent to Google when a message is refused, and the error names the field (`to[1]`) without repeating it. In a named operation a field that is not one of the six, such as `from` or `raw`, is refused too and not dropped.

**Replying.** `reply(message, GmailReply)` sends an answer to the people named in `to`, and keeps it in the thread of the message it answers. It reads the original's headers first (one more request, without its body): the reply carries the original's `threadId`, names the original in `In-Reply-To`, and lists the thread so far in `References`.

- **`to` is required, and nothing in the original decides who a reply goes to.** A person approves a reply by the input they are shown, so the recipients are in that input. What a message says about where its answers should go is its sender's own text: `from` can be written to look like someone else, and `replyTo` can be any address at all, so mail that seems to come from a colleague can ask for its answers to go to a stranger. Read the message's `from` and `replyTo` with `get`, decide, and name the people in `to`. A reply without `to`, or with an empty one, fails with `InvalidInput` before anything is read or sent.
- It goes to the people in `to`, `cc` and `bcc` and to nobody else. The original's sender and the people it had in copy are not added.
- The subject is the original's with `Re: ` before it, and an original that already starts with `Re:` is not marked twice. Gmail keeps a reply in its thread only while the subjects match, so setting `subject` can start a new thread.
- A reply needs `text` or `html`. The original is not quoted beneath it.
- An original that carries no `Message-ID` cannot be answered in its thread: `reply` fails with `InvalidInput`, says that the message carries none, and sends nothing. When a message has the header more than once, the one a reply names is the one `get` returns as `messageId`.

**Sending cannot be taken back.** `send`, `reply` and `send_draft` are `destructive` for that reason, and a host that asks a person before a destructive operation asks before each. They are sent once: if Google answers with a server error, Socket does not try again, because the message may have gone.

**Drafts.** `create` and `update` take the same `GmailSendMessage`, and nothing in it is required, so a draft can be begun empty. A draft has two ids: its own (`id`), which `get`, `update`, `delete` and `send_draft` take, and its message's (`message.id`), which changes every time the draft is saved. `update` replaces the whole draft: what is not given again is gone, which is why it is `destructive`. `delete` removes a draft for good, without the bin. `send_draft` sends it as it stands; Gmail then deletes the draft and returns the sent message under a new id. `send_draft` is Gmail's `drafts.send`, which Google allows under `gmail.compose` and not under `gmail.send`.

**Labels.** Gmail has no folders, only labels. `modify` adds and removes them, and a call that names none is refused:

| To | Send |
| --- | --- |
| archive | `removeLabelIds: ["INBOX"]` |
| mark as read | `removeLabelIds: ["UNREAD"]` |
| mark as unread | `addLabelIds: ["UNREAD"]` |
| star | `addLabelIds: ["STARRED"]` |
| file under a label of the person's | `addLabelIds: ["Label_12"]` |

Up to 100 labels can be added and 100 removed in one call. `SENT` and `DRAFT` are Gmail's to set and cannot be added. `TRASH` and `SPAM` are refused in `addLabelIds` with `InvalidInput`, before Google is called: adding `TRASH` is moving a message to the bin, which is `trash` and is `destructive`, and marking a message as spam is not offered; both can be removed. `gmail_labels.list` gives every label's `id`, `name` and `type` (`system` or `user`); the counts (`messagesTotal`, `messagesUnread`, `threadsTotal`, `threadsUnread`) come with `gmail_labels.get`.

**The bin.** `trash` moves a message to the bin and `untrash` brings it back. Gmail empties the bin by itself, and what was in it is then gone for good, so `trash` is `destructive` and a host asks a person first; `untrash` is `write`. Nothing here deletes a message at once.

**Errors.** A message, thread, label or draft that does not exist is `NotFound`. A scope the connection was not given is `AccessDenied`. Gmail's limit on how fast one mailbox is used arrives as `RateLimited`, whether Google sends it as a 429 or as a 403. An answer Socket cannot read is `Decode`, and names the place (`payload.parts[1].body.data`) without repeating what was there.

### Calendar: `calendar_list`, `calendar_events` and `calendar_freebusy`

Google Calendar, grouped the way Google groups its own API: the calendars a person has, the events on one of them, and when calendars are busy. Each group is reached from the integration with a connection, such as `google.calendar_events(&connection)`.

Neither Calendar scope is among the provider's defaults. An application that uses Calendar names them in `GoogleOAuth::scopes`: `scopes::CALENDAR_READONLY` for everything that reads, `scopes::CALENDAR_EVENTS` for everything that changes an event.

| Method | What it does | Effect | Scope |
| --- | --- | --- | --- |
| `calendar_list.list(CalendarListFilter, Paging)` | `Page<CalendarListEntry>`: the person's own calendars and those shared with or subscribed to by them | read | `calendar.readonly` |
| `calendar_list.get(calendar)` | `CalendarListEntry` | read | `calendar.readonly` |
| `calendar_events.list(calendar, EventFilter, Paging)` | `Page<CalendarEvent>`: inside a time window, matching free text, with recurring events expanded when `singleEvents` is set | read | `calendar.readonly` |
| `calendar_events.get(calendar, event)` | `CalendarEvent` | read | `calendar.readonly` |
| `calendar_events.instances(calendar, event, EventInstancesFilter, Paging)` | `Page<CalendarEvent>`: the occurrences of one recurring event | read | `calendar.readonly` |
| `calendar_freebusy.query(calendars, FreeBusyQuery)` | `FreeBusy`: when each calendar is busy inside a window | read | `calendar.readonly` |
| `calendar_events.insert(calendar, EventInsert)` | `CalendarEvent`: the new event, with a Google Meet link when asked for | write | `calendar.events` |
| `calendar_events.patch(calendar, event, EventPatch)` | `CalendarEvent`: the event with the fields given replaced | destructive | `calendar.events` |
| `calendar_events.respond(calendar, event, EventResponse)` | `CalendarEvent`: the event with the calendar owner's answer set | destructive | `calendar.events` |
| `calendar_events.delete(calendar, event, EventDelete)` | nothing | destructive | `calendar.events` |

Scopes are shown by their last part; each is `https://www.googleapis.com/auth/` followed by it. `calendar.events` also lets a token read events, but not the calendar list and not free/busy, so an application that does both asks for both.

```rust
use socketkit::google::models::{EventFilter, EventInsert, EventPatch, EventResponse, EventTime, FreeBusyQuery, Paging};

let events = google.calendar_events(&connection);

// This week's meetings, each occurrence of a recurring one by itself, in order.
let week = EventFilter {
    time_min: Some("2026-10-12T00:00:00Z".into()),
    time_max: Some("2026-10-19T00:00:00Z".into()),
    single_events: Some(true),
    order_by: Some("startTime".into()),
    ..Default::default()
};
let page = events.list("primary", week, Paging::default()).await?;

// Schedule a meeting with a Google Meet link, and email the invitation.
let mut meeting = EventInsert::between(
    EventTime::at("2026-10-14T09:00:00-07:00"),
    EventTime::at("2026-10-14T09:30:00-07:00"),
)
.summary("Design review")
.invite("grace@example.test")
.with_meet_link();
meeting.send_updates = Some("all".into());
let created = events.insert("primary", meeting).await?;
let join = created.hangout_link;

// Move it, answer another invitation, check who is free.
let changes = EventPatch { location: Some("Room 5".into()), ..Default::default() };
events.patch("primary", &created.id, changes).await?;
events.respond("primary", "another-event-id", EventResponse::accept()).await?;
let busy = google.calendar_freebusy(&connection)
    .query(
        &["primary".into(), "grace@example.test".into()],
        FreeBusyQuery::between("2026-10-14T00:00:00Z", "2026-10-15T00:00:00Z"),
    )
    .await?;
```

By name, the same calls take one JSON object: `calendar` and `event` are the ids, `calendars` the list a free/busy query asks about, `cursor` and `limit` page a list, and everything else is under Google's own name (`timeMin`, `singleEvents`, `q`, `sendUpdates`). A field an operation does not have, such as Google's `calendarId` or `pageToken`, is refused and not dropped.

**Calendar ids.** `primary` names the signed-in person's own calendar; a person's calendar id is otherwise their email address. An id may contain `@` and `#`. It is encoded as one segment of the URL whatever it contains.

**What a `CalendarEvent` carries:** `id`, `status`, `htmlLink`, `summary`, `description`, `location`, `start` and `end`, `creator`, `organizer`, `attendees` with each one's `responseStatus`, `attendeesOmitted`, `guestsCanSeeOtherGuests`, `hangoutLink`, `conferenceData`, `attachments`, `recurrence`, `recurringEventId`, `originalStartTime`, `iCalUID`, `eventType`, `transparency`, `visibility`, `created` and `updated`.

**From a meeting to its recording.** An event keeps `hangoutLink`, `conferenceData` (the Meet code in `conferenceId`, and every way to join in `entryPoints`) and `attachments`. Google Meet attaches a meeting's recording, transcript and notes to the event as Drive files, so `attachments[].fileId` is where to look for them afterwards.

**Times.** `timeMin`, `timeMax` and `updatedMin` are RFC 3339 with the offset: `2026-10-12T00:00:00Z` or `2026-10-12T09:00:00-07:00`. Google refuses a time without one, so Socket refuses it first and names the field. A time is sent exactly as given, so space around it, or in place of the `T`, is refused too. An event's `start` and `end` each hold either `dateTime` or, for an all-day event, `date`, never both. A `dateTime` carries its offset too, unless a `timeZone` is given beside it (`EventTime::in_zone("2026-10-12T09:00:00", "Europe/Zurich")`), which is the form a recurring event needs. Ends are exclusive: an all-day event on the 12th ends on the 13th.

**Listing.** `EventFilter` has `timeMin`, `timeMax`, `q`, `singleEvents`, `orderBy`, `updatedMin`, `showDeleted` and `timeZone`. `q` is free text, matched against titles, descriptions, locations and the people on an event. `orderBy` is `startTime` or `updated`, both oldest first. `startTime` works only with `singleEvents: true`, and is refused without it before Google is called. Google returns 250 events a page unless `limit` says otherwise, up to 2500; calendars come 100 a page, up to 250. A limit outside that range is refused. A page may hold fewer events than `limit`, or none, and still be followed by another: only a missing `next_cursor` means the end.

**Recurring events.** Without `singleEvents`, a recurring event comes back once, as the series, with its `recurrence` rules. With it, every occurrence in the window comes back as its own event carrying `recurringEventId`. `instances` lists the occurrences of one series. An occurrence that was removed from its series arrives with little more than `id`, `status: "cancelled"`, `recurringEventId` and `originalStartTime`.

**Meet links.** `createMeetLink` on `insert` or `patch` asks Google to create one. Socket sends `conferenceData.createRequest` with a request id of its own, 128 random bits, new on every call, and `conferenceDataVersion=1`, without which Google ignores the request. Google may still be creating the link when it answers: then `conferenceData.createRequest.status.statusCode` is `pending` and `hangoutLink` is unset. Read the event again.

**Who is emailed.** `sendUpdates` is `all`, `externalOnly` or `none`, on `insert`, `patch`, `respond` and `delete`. Left unset, it is not sent, and Google emails nobody.

**Changing an event.** `patch` changes only the fields given, and a patch with nothing in it is refused. It is marked destructive because what it changes is overwritten: `attendees`, when given, replaces the whole guest list, and anyone left out is uninvited. To add one person, send everyone who stays and the newcomer, each by `email`. A patch that names guests reads the event first, which is one more request: a guest who stays is sent back as Google has them, with the answer they gave, their note and whoever they are bringing, changed only by what the patch says of them. It also names the version it read (`If-Match`), so a guest someone else added in between is not uninvited by a list written before they were on it: Google refuses the change, it arrives as `InvalidInput`, and reading again and repeating it works. A guest list that is hidden or cut short on this calendar cannot be replaced from it, and is refused. Moving an event between all-day and timed works; Socket clears the form that is no longer used.

**Answering an invitation.** Google has no call for an answer alone: it is a change to the event's guest list, and a change replaces the list. `respond` therefore reads the event, writes the answer into the entry Google marks as the calendar's own (`self`), and sends the list back with everyone else exactly as they were, including fields Socket does not describe. It names the version it read (`If-Match`), so if someone changed the event in between, Google refuses the write instead of losing their change. That arrives as `InvalidInput` and is not tried again by itself; call `respond` again. A calendar whose owner is not on the guest list is refused before anything is written. `responseStatus` is `accepted`, `declined`, `tentative` or `needsAction`. An answer can be changed again, but the organiser sees it at once and a notice sent with `sendUpdates` cannot be unsent, so `respond` is `destructive`, as answering an invitation is in the Microsoft integration.

**A hidden guest list.** When the organiser hid the guests from each other (`guestsCanSeeOtherGuests: false`), or Google cut the list short (`attendeesOmitted: true`), the list that was read is not the whole of it, and sending it back as the list would uninvite everyone it leaves out. `respond` then sends only the calendar's own entry with `attendeesOmitted: true`, which is how Google is told that the list is partial and only the answer is to change.

**Whose answer it is.** `self` marks the calendar the event was read from, not whoever is signed in. On `primary` the two are the same person. On a calendar the signed-in person manages for someone else, `respond` answers for that calendar's owner. A host that asks a person to approve the call should show them `calendar`.

**Deleting.** `delete` removes the event, and for an event the account organised it cancels it for the guests. An event that is already gone is answered with 410, which arrives as `NotFound`: treat it as deleted. The transport repeats a `DELETE` after a server error, so a delete that Google carried out but failed to confirm can come back as `NotFound` from the second attempt. That too means the event is gone.

**Free/busy.** `calendars` are calendar ids, or `primary`; at least one, and none of them blank. A calendar Google could not answer for, such as one the account may not see, comes back with `errors` set and no busy periods. That is not the same as free: check `errors` before reading `busy`. An answer that names no calendar at all is an error, not an empty day. Google takes this read only as a `POST`, so unlike the other reads it is not tried again after a server error; call it again yourself.

**What an error means.** A 403 with the reason `rateLimitExceeded` or `userRateLimitExceeded` is a throttle, `RateLimited`, and is tried again. Any other 403 is `AccessDenied` with Google's reason: the token lacks the scope, the account may not change this calendar, a guest tried to change what only the organiser may (`forbiddenForNonOrganizer`), or Google's usage limit was reached (`quotaExceeded`). A 410 with `updatedMinTooLongAgo` is `InvalidInput`: `updatedMin` is further back than Google keeps changes. An answer that says success without the event, the list or the busy times is `Decode`, and nothing should be assumed done.

### Meet: `meet_conference_records`, `meet_participants`, `meet_transcripts`, `meet_recordings` and `meet_spaces`

What happened in a Google Meet meeting: when it was held, who was in it, what was said, and where the recording and the transcript were saved. Everything here reads; nothing creates, changes or ends a meeting. All of it needs one scope, `https://www.googleapis.com/auth/meetings.space.readonly` (`scopes::MEETINGS_SPACE_READONLY`), which is not among the provider's defaults: name it in `GoogleOAuth::scopes`.

```rust
// From the link on a calendar event to what was said.
let space = google.meet_spaces(&connection).get("https://meet.google.com/abc-mnop-xyz").await?;
let held = google
    .meet_conference_records(&connection)
    .list(MeetListConferenceRecords { space: Some(space.name), ..Default::default() })
    .await?;
let record = &held.items[0].name; // the newest meeting in that space

let transcripts = google.meet_transcripts(&connection);
let made = transcripts.list(record, Paging::default()).await?;
let content = transcripts.read(record, &made.items[0].name, MeetReadTranscript::default()).await?;
for entry in &content.entries {
    println!("{}: {}", entry.speaker.as_deref().unwrap_or("?"), entry.text);
}
// The Google Doc of the same transcript, for the Docs and Drive methods.
let document = content.transcript.docs_destination.and_then(|doc| doc.document);
```

| Group | Method | What it does | Effect | Scope |
| --- | --- | --- | --- | --- |
| `meet_conference_records` | `list(MeetListConferenceRecords)` | The meetings the account organised, newest first: `Page<ConferenceRecord>`. Narrowed by meeting code, space, or when the meeting began. | read | `meetings.space.readonly` |
| `meet_conference_records` | `get(record)` | One `ConferenceRecord`: start, end, the space, and when Google deletes it. | read | `meetings.space.readonly` |
| `meet_participants` | `list(record, Paging)` | Who was in the meeting: `Page<MeetParticipant>`. | read | `meetings.space.readonly` |
| `meet_participants` | `get(record, participant)` | One `MeetParticipant`, with the name they were shown under. | read | `meetings.space.readonly` |
| `meet_participants` | `sessions(record, participant, Paging)` | Each time that participant was connected: `Page<MeetParticipantSession>`. | read | `meetings.space.readonly` |
| `meet_transcripts` | `list(record, Paging)` | The meeting's transcripts: `Page<MeetTranscript>`, each with the Google Doc it was saved to. | read | `meetings.space.readonly` |
| `meet_transcripts` | `get(record, transcript)` | One `MeetTranscript`. | read | `meetings.space.readonly` |
| `meet_transcripts` | `entries(record, transcript, Paging)` | What was said, as Meet returns it: `Page<MeetTranscriptEntry>`, the speaker being a reference to a participant. | read | `meetings.space.readonly` |
| `meet_transcripts` | `read(record, transcript, MeetReadTranscript)` | The whole transcript with each speaker named: `MeetTranscriptContent`. | read | `meetings.space.readonly` |
| `meet_recordings` | `list(record, Paging)` | The meeting's recordings: `Page<MeetRecording>`, each with the Drive file it was saved to. | read | `meetings.space.readonly` |
| `meet_recordings` | `get(record, recording)` | One `MeetRecording`. | read | `meetings.space.readonly` |
| `meet_spaces` | `get(space)` | The `MeetSpace` behind a name, an id, a meeting code or a join link, with the meeting going on in it now. | read | `meetings.space.readonly` |

Every request is a GET to `meet.googleapis.com/v2`.

**Naming a thing.** Meet addresses everything by a resource name: `conferenceRecords/{id}`, `conferenceRecords/{id}/transcripts/{id}`, `spaces/{id}`. Every answer carries its `name`, and every identifier argument takes either that name, exactly as Google returned it, or the bare id. So the `name` of one answer is what the next call is given: a record's `name` as `record`, a transcript's `name` as `transcript`, the `participant` of a transcript entry as `participant`, a record's `space` as `space`. Anything else is refused before Google is called: a name of another collection (`spaces/x` as a conference record), a name cut short or too long, and a name that lies in another conference record than the one given beside it. Each id is written as one segment of the path, so nothing in it can add a segment, a query or a fragment.

**Finding the meeting.** `meet_spaces.get` takes a space's name or id, a meeting code (`abc-mnop-xyz`) or the join link that ends in one, as a calendar event has it in `hangoutLink`. Google takes a meeting code in the place of the id (`spaces/abc-mnop-xyz`). Keep the space's `name` and not the code: Google says a code can come to mean another space, generally 365 days after it was last used. A space is where meetings are held; each time one is held there is a conference record, and `activeConference` names the one going on now.

**Listing conference records.** Google takes one `filter` string in its own syntax. Socket offers typed options and writes the filter itself:

| Option | Clause sent |
| --- | --- |
| `meetingCode` (a code, or a join link) | `space.meeting_code = "abc-mnop-xyz"` |
| `space` (a name or an id) | `space.name = "spaces/jQCFfuBOdN5z"` |
| `startTimeMin` | `start_time>="2026-10-01T00:00:00Z"` |
| `startTimeMax` | `start_time<="2026-10-02T00:00:00Z"` |

Clauses are joined with `AND`. A value is checked to be what its field holds before it is quoted, so it cannot end the quotes or add a clause: a meeting code is letters, digits and hyphens (and is sent in lower case), a time is an RFC 3339 timestamp, and a space id with a quote, a backslash or a control character is refused. `meetingCode` and `space` cannot be given together, and `startTimeMin` cannot be after `startTimeMax`.

**What was said.** `entries` returns what Meet returns: for each thing said, the `text`, `startTime`, `endTime`, `languageCode`, and `participant`, which is a participant's name and not a person's. `read` puts the whole transcript together in the shape a Teams transcript has in the Microsoft integration, so both are read the same way:

- `text`: one line for each entry, `Ada Lovelace: Shall we begin?`. A line whose speaker is not known has only what was said.
- `entries`: for each, `speaker`, `startMs` and `endMs` in milliseconds from the transcript's own `startTime`, and `text`; and beside those Meet's own `startTime`, `endTime`, `languageCode` and `participant`.
- `truncated`, and `transcript`, the transcript's own details with the Google Doc it was saved to.

`read` makes several requests: the transcript, each page of its entries (100 to a page), and pages of the participants (250 to a page) until every speaker has been found.

- **It stops at `maxEntries`**, 1,000 unless another number from 1 to 10,000 is given, and sets `truncated` when Meet says more may follow. A transcript of exactly that many entries can be marked so too. Ask again with a larger number, or page through `entries`. It never reads more than twice the pages that many entries would fill, plus two, nor more than 40 pages of participants.
- **The speaker's name is the participant's display name**: a signed-in person's first and last name, the name a guest typed when joining without signing in, or a caller's partly hidden phone number. A guest's name is whatever they typed; nobody checked it. Someone who left and came back is one participant with several sessions, and has one name.
- **`speaker` is absent** when Meet withholds the name ("for privacy reasons, profile information might not be available for all participants"), when the entry names no participant, or when the participant is not in the list.
- **A name is written on one line, and so is each entry in `text`**, so that a line break in a name or in speech cannot pass for another person's line. `entries[].text` is as Meet sent it.
- **A time that cannot be read is an error**, never a shorter transcript: the transcript's `startTime`, or an entry's `startTime` or `endTime`. An entry with no `endTime` at all, as something still being said may be, ends where it began. The error names the field and the entry's place, and does not repeat what was said.
- **The entries may differ from the Doc.** Google says the entries "might not match the transcription found in the Docs transcript file", when the Doc was changed after it was written.

**The Doc and the file.** A transcript's `docsDestination` has `document`, the Google Doc's id, and `exportUri`, the address that opens it. A recording's `driveDestination` has `file`, the Drive file's id of an MP4, and `exportUri`, the address that plays it. Google has them once the file has been written, which `state` says with `FILE_GENERATED`; before that they may be absent. Reading the Doc or the video is a Drive or Docs read, with its own scope.

**Limits Google sets, which callers will meet:**

- **A transcript or a recording exists only if it was switched on** before the meeting ended; otherwise the list is empty. A transcript does not need a recording.
- **Transcripts are part of some Google Workspace editions only.** Google's help page lists Business Standard and Plus, Enterprise Starter, Standard and Plus, Teaching and Learning Upgrade, Education Plus, and Workspace Individual, and eight languages.
- **Entries are kept for 30 days.** Google deletes a transcript's entries 30 days after the meeting ended, and the conference record itself at its `expireTime`, also 30 days after the end. The Google Doc and the recording stay in the organiser's Drive under Drive's own rules, so after that the Doc is the only transcript there is.
- **Who may read.** A meeting's organiser and its participants can read its conference record, participants, transcripts and recordings. But `meet_conference_records.list` returns only the meetings the account organised; for a meeting someone else organised, start from its space or its record's name. The Doc and the file belong to the organiser, and Drive decides who else may open them.
- **Errors.** A record that has expired, or a meeting the account was not in, is `NotFound` or `AccessDenied`, as Google answers. Without the scope, or with the Meet API not enabled in the Google Cloud project, every call is `AccessDenied` with Google's reason.
- **Page sizes.** `limit` is from 1 to 100, or to 250 for participants and sessions. Google would lower a larger number by itself; Socket refuses it, so that a page is never smaller than was asked for without saying so.

### Drive: `drive_files` and `drive_shared_drives`

Find files and folders, read what describes one, read a Google document as text, download a file, see who can open a file, and file things: make a folder, copy, move, rename, and put in the bin. A Google Doc, a Sheet, a folder and a shortcut are all files in Drive; `mimeType` says which.

```rust
use socketkit::google::models::{DriveCreateFolder, DriveExportFormat, DriveListFiles, Paging, TextLimit};

let drive = google.drive_files(&connection);

// The Docs changed this month that mention the plan, newest first.
let found = drive.list(DriveListFiles {
    q: Some(
        "fullText contains 'quarterly plan' and mimeType = 'application/vnd.google-apps.document' \
         and modifiedTime > '2026-10-01T00:00:00' and trashed = false".into(),
    ),
    ..Default::default()
}).await?;

// Before showing a document to other people, see who was allowed to read it.
let doc = &found.items[0];
let who = drive.permissions(&doc.id, Paging::default()).await?;
let text = drive.export(&doc.id, DriveExportFormat::Markdown, TextLimit::default()).await?.text;

// File it away.
let archive = drive.create_folder(DriveCreateFolder {
    name: Some("Archive 2026".into()),
    ..Default::default()
}).await?;
drive.move_to(&doc.id, &archive.id).await?;
```

| Group | Method | What it does | Effect | Scope |
| --- | --- | --- | --- | --- |
| `drive_files` | `list(DriveListFiles)` | `Page<DriveFile>`: what matches a search, or everything the account can see | read | `drive.readonly` |
| `drive_files` | `get(file)` | `DriveFile`: what describes one file or folder, not its content | read | `drive.readonly` |
| `drive_files` | `export(file, DriveExportFormat, TextLimit)` | `DriveExport`: a Google document as text | read | `drive.readonly` |
| `drive_files` | `download(file, Download)` | `Content`: the file as bytes, with its type. Typed only | read | `drive.readonly` |
| `drive_files` | `download_text(file, TextLimit)` | `DriveFileText`: a file that is text, as text | read | `drive.readonly` |
| `drive_files` | `permissions(file, Paging)` | `Page<DrivePermission>`: who can see a file, and in what role | read | `drive.readonly` |
| `drive_files` | `create_folder(DriveCreateFolder)` | `DriveFile`: the new folder | write | `drive.file` |
| `drive_files` | `copy(file, DriveCopyFile)` | `DriveFile`: the copy | write | `drive.file` |
| `drive_files` | `move_to(file, folder)` | `DriveFile`, in its new folder | destructive | `drive.file` |
| `drive_files` | `rename(file, name)` | `DriveFile`, under its new name | write | `drive.file` |
| `drive_files` | `trash(file)` | `DriveFile`, in the bin | destructive | `drive.file` |
| `drive_shared_drives` | `list(Paging)` | `Page<SharedDrive>`: the shared drives the account is a member of | read | `drive.readonly` |

`drive.readonly` is one of the provider's default scopes. `drive.file` is not: name it in `GoogleOAuth::scopes` to use the writes. It is the narrowest scope Google offers for them, and it reaches only the files the application created or the person opened with it, through Google's file picker for one. A write to any other file is refused with 403, which arrives as `AccessDenied` with Google's words ("The user has not granted the app … access to the file …"). To change every file the account can, ask for the full `https://www.googleapis.com/auth/drive` scope instead; the operations work with it unchanged.

**An id, or a link.** Every `file` and `folder` is a Drive id. `google.resource.resolve` turns a link someone pasted (`https://docs.google.com/document/d/…/edit`, `https://drive.google.com/drive/folders/…`) into the id, and confirms the account can open it. Where Google takes a folder's id it also takes `root`, the top of the account's own My Drive: `get("root")`, `'root' in parents`, `move_to(file, "root")`.

**What a `DriveFile` carries:** `id`, `name`, `mimeType`, `parents`, `createdTime`, `modifiedTime`, `size`, `owners` (each with `displayName`, `emailAddress`, `permissionId`, `me`), `webViewLink`, `trashed`, `driveId` and `shortcutDetails` (`targetId`, `targetMimeType`, `targetResourceKey`). Drive returns only the fields a request names, so every request names exactly these and no other field of a file can come back. A test compares what is asked for with what the types hold, for each operation.

- `mimeType` is `application/vnd.google-apps.folder` for a folder, `…document` for a Doc, `…spreadsheet` for a Sheet, `…presentation` for Slides and `…shortcut` for a shortcut. Anything else is a file with content of its own, such as `application/pdf`.
- `parents` holds the one folder a file is in. It is empty when the account cannot see that folder, as with a file someone shared from their own Drive.
- `size` is a number in a string, as Google writes it. A folder and a shortcut have none.
- `owners` is empty for a file in a shared drive, which belongs to the drive; `driveId` names the drive.
- A shortcut is a file of its own that points at another. Read the other by `shortcutDetails.targetId`.

**Shared drives are always in reach.** Every request that names a file or lists files says `supportsAllDrives=true`, and `list` also says `includeItemsFromAllDrives=true`. Without them Google answers as if what is in a shared drive did not exist, with no error. There is no option to turn this off.

**Listing.** `DriveListFiles` has `q`, `orderBy`, `driveId`, `limit` and `cursor`. With nothing set, the list is of everything the account can see, what is in the bin included, in no particular order.

- `q` is Drive's own query language and goes to Google exactly as you wrote it. Socket does not read it, so a mistake in it is Google's to report: a 400, which arrives as `InvalidInput` with Google's words.
- `orderBy` is Google's sort keys with commas between them, each followed by ` desc` to reverse it: `folder,modifiedTime desc`. The keys are `createdTime`, `folder`, `modifiedByMeTime`, `modifiedTime`, `name`, `name_natural`, `quotaBytesUsed`, `recency`, `sharedWithMeTime`, `starred` and `viewedByMeTime`. Google refuses a sort together with a `fullText` search, whose results are always ordered by relevance.
- `driveId` lists what is in one shared drive. Socket sends `corpora=drive` with it, which is what makes Google search that drive. Get the id from `drive_shared_drives.list`.
- A page holds at most 1000 files (`limit`), and Google may return fewer than asked for. Pass `next_cursor` back as `cursor` with the same `q` and `orderBy`.

Common searches:

| To find | `q` |
| --- | --- |
| A file by its exact name | `name = 'Q4 plan'` |
| Names that contain a word | `name contains 'budget'` |
| Words anywhere in the content | `fullText contains 'quarterly plan'` |
| An exact phrase in the content | `fullText contains '"quarterly plan"'` |
| One type of file | `mimeType = 'application/vnd.google-apps.spreadsheet'` |
| Everything but folders | `mimeType != 'application/vnd.google-apps.folder'` |
| What is in a folder | `'FOLDER_ID' in parents` |
| Changed since a time | `modifiedTime > '2026-10-01T00:00:00'` |
| Not in the bin | `trashed = false` |
| Shared with the account | `sharedWithMe` |
| Owned by someone | `'ada@example.test' in owners` |

Join them with `and`, `or` and `not`: `'FOLDER_ID' in parents and trashed = false`. A time is RFC 3339 and is read as UTC unless it carries an offset. `contains` on a `name` matches a prefix only: `name contains 'Hello'` finds "HelloWorld" and `name contains 'World'` does not. On `fullText` it matches whole words.

**Quoting a value.** A value goes in single quotes. Inside it, write a single quote as `\'` and a backslash as `\\`: a file named `quinn's paper\essay` is found with `name contains 'quinn\'s paper\\essay'`. When the value comes from a person, do that replacement before putting it in `q`, backslashes first: Socket cannot do it for you, because it does not read the query.

**Exporting a Google document.** `export` returns a Doc, a Sheet or a Slides presentation as text, in `text`. `DriveExportFormat` is the whole choice:

| Format | In JSON | For |
| --- | --- | --- |
| `Text` | `text/plain` | A Doc, or a Slides presentation |
| `Markdown` | `text/markdown` | A Doc, with its headings, lists, links and tables |
| `Csv` | `text/csv` | A Sheet. **Only its first sheet is exported.** For the others, read the spreadsheet through Sheets |

- Any other format is refused before Google is called. An export is text or nothing: PDF, Word, Excel and the rest are bytes, and are not offered.
- **An export is one megabyte unless you ask for more.** `TextLimit` has `maxBytes`: the most text to read, one megabyte when not set and at most ten (10,485,760 bytes). A larger `maxBytes` is refused before Google is called. An export over the limit is the error `too_large`, whose message gives the limit and says that `maxBytes` can be raised; it is never cut short.
- **Google itself exports at most 10 MB.** It refuses a larger one, and the call fails with `InvalidInput` and the message "this file is too large to export: the limit is 10 MB of exported content". There is no way around it here; a document that large has to be read in parts through Docs or Sheets.
- **Only a Google document can be exported.** A PDF, an image or a Word file has content of its own and nothing to export. Google refuses it, and the call fails with `InvalidInput` and "this file is not a Google document, so there is nothing to export". Check `mimeType` first: it starts with `application/vnd.google-apps.` for the files that can be. The others are read with `download` or `download_text`.
- A format that does not suit the file, such as `Csv` for a Doc, is Google's to refuse: `InvalidInput` with Google's words.
- An empty document is an empty `text`, not an error. The byte order mark Google puts before a Doc's plain text is left out; line endings are as Google wrote them. What Google serves that is not UTF-8 text is refused, never mended.

**Downloading a file.** A file that is not a Google document has content of its own: a PDF, an image, a CSV file someone uploaded. Two methods read it, both with `GET drive/v3/files/{id}?alt=media`.

```rust
use socketkit::google::models::{Download, TextLimit};

// The bytes, for a program: up to 50 MB, and two minutes to arrive.
let pdf = drive.download(pdf_id, Download { max_bytes: Some(50 * 1024 * 1024), timeout_secs: Some(120) }).await?;
std::fs::write("plan.pdf", &pdf.bytes)?;

// The text, for a model: one megabyte unless more is asked for.
let csv = drive.download_text(csv_id, TextLimit::default()).await?.text;
```

- `download` returns the bytes exactly as Google serves them, with the type Google states, as a `Content` (`bytes`, `content_type`, `len()`). Ten megabytes are read and thirty seconds allowed unless `Download` says otherwise (`maxBytes`, `timeoutSecs`). All of it is held in memory before it is returned; a file over the limit is the error `too_large`, and nothing of it is returned. **There is no operation by this name**, because an operation called by name never returns bytes.
- `download_text`, which is also the operation `drive_files.download_text`, returns `contentType` and `text` for a file Google serves as text: `text/…`, JSON or XML, in UTF-8. Anything else, text in another encoding included, is the error `unsupported`, whose message gives the size and type and nothing of the content. A file that says it is UTF-8 text and is not is the error `decode`. One megabyte is read unless `maxBytes` allows more, up to ten, exactly as for `export`. The text is the file's own, from its first byte: a byte order mark at its start is kept.
- **The type is the one the file was stored with**, which is whatever uploaded it said. A Markdown or a log file stored as `application/octet-stream` is not text to `download_text`; read it with `download`.
- **A Google Doc, Sheet or Slides presentation has no content of its own.** Google refuses to download one, and both methods fail with `InvalidInput` and "this file has no content of its own to download: a Google Doc, Sheet or Slides presentation is read with `export`".
- **A file Google has flagged as malware or spam** is given only to its owner, and only when the request says the risk is accepted (`acknowledgeAbuse`). Socket never says so on a person's behalf, so such a file is refused: `AccessDenied` with Google's words.
- **A download that Google answers from another host is refused.** Socket sends the token only to the hosts the provider declares, and follows a redirect only to a declared host. Google's download hosts are many and are named by pattern, which a provider cannot declare yet, so none is declared. If Google answers a download with a redirect to one of them, the call fails with `Unexpected` and "google redirected the request … to an address Socket does not follow", and nothing is fetched. Google's own example of this request follows redirects and does not say when one is sent, so how often this happens is not known.
- Both say `supportsAllDrives=true`, so a file in a shared drive is read like any other. An owner or an organiser can restrict who may download a file; Google then refuses, which arrives as `AccessDenied`.

**Who can see a file.** `permissions` lists every grant on a file or a folder. Each `DrivePermission` has `id`, `type` (`user`, `group`, `domain` or `anyone`), `role` (`owner`, `organizer`, `fileOrganizer`, `writer`, `commenter` or `reader`), `emailAddress` for a person or a group, `domain` for a domain, `displayName`, `deleted` (the account it was granted to no longer exists), `allowFileDiscovery`, `expirationTime` and `permissionDetails`.

- `type: "anyone"` with `allowFileDiscovery: false` is "anyone with the link". With `true`, the file can also be found by searching.
- `permissionDetails` says where each grant comes from: `permissionType` is `file` for a grant on a file or folder and `member` for membership of a shared drive, `inherited` says whether it comes from above, and `inheritedFrom` names the folder or drive it comes from.
- A page holds at most 100 permissions. An account that may not see who a file is shared with is refused by Google, which arrives as `AccessDenied`.

**Making a folder.** `DriveCreateFolder` has `name`, which is required, and `parents`: a list of the one folder to create it in. Without `parents` the folder is made at the top of the account's My Drive. To make one in a shared drive, give the drive's id or a folder inside it. Drive allows two folders of the same name in one place, so calling this twice makes two.

**Copying.** `DriveCopyFile` has `name` and `parents`, both optional: without them Google names the copy "Copy of …" and puts it beside the original. Google does not copy a folder. A copy is a new file with a new id.

**`parents` is a list of exactly one.** That is how Google writes it, and a file has one parent. A list of none or of several is refused before Google is called.

**Moving.** `move_to(file, folder)` puts a file or a folder into another folder and takes it out of the one it was in. It keeps its id. **Who can see a file follows its folder**: moved, it is shown to everyone the new folder is shared with and taken from those who had it through the old one, and a file moved into a shared drive belongs to that drive. That is why a move is `destructive` and a host asks a person first. `folder` is one id; a comma in it is refused, since Google would read a list. Google moves a file by being told which parent to add and which to remove, so Socket reads the file first and then sends the change: two requests.

- **Already there.** A file that is in the folder, and nowhere else, is returned as it is. No change is sent.
- **A file whose folder the account cannot see**, such as one shared from someone else's Drive, names no parent. Socket then only adds the new one. Google moves the file if the account may, and refuses with 403 if it may not; nothing is changed by the refusal.
- **A file in several folders**, which only files from before 2020 can be, ends up in the one folder it was moved to.
- **`root`** is looked up first, with one more read, because a file names its parent by id and never as `root`.
- A file is not moved into itself; that is refused before Google is called.
- If someone else moves the file between the read and the change, Google is asked to remove a parent the file no longer has. What Google does then was not confirmed; read the file again and repeat the move.
- Moving into or between shared drives has rules of its own (who may move, and that a folder cannot always follow). Google's refusal arrives as `AccessDenied` with its reason.

**Renaming.** `rename(file, name)` changes the name and nothing else. A blank name is refused.

**The bin.** `trash` puts a file in the bin, and a folder with everything in it. Nothing is deleted: it can be taken out again for 30 days, after which Google empties it. Only a file's owner can bin it, or in a shared drive someone whose role allows it; anyone else is refused with `AccessDenied`. Taking a file out of the bin, and deleting one for good, are not offered.

**A change is sent once.** A POST or a PATCH that fails with a server error may have been made, so Socket does not send it again. After such a failure on `create_folder` or `copy`, list the folder before trying again, or there may be two.

**Shared drives.** `drive_shared_drives.list` returns each drive's `id`, `name`, `createdTime` and `hidden`. The `id` is what `DriveListFiles.driveId` takes, and is also the id of the drive's top folder, so it can be given as a parent. A page holds 10 drives unless `limit` says otherwise, up to 100.

### Docs and Sheets: `docs_documents` and `sheets_spreadsheets`

A Google Doc is read as plain text, tabs included, and can be created and added to. A Google Sheet is described by its sheets and their sizes, and its cells are read and written by range. Both take the id from the file's address, the part after `/d/`; `google.resource.resolve` turns a pasted Docs or Sheets link into that id and confirms the account can open the file.

| Group | Method | What it does | Effect | Scope |
| --- | --- | --- | --- | --- |
| `docs_documents` | `get(document)` | `Document`: title, revision and tabs, without what is written in it | read | `documents.readonly` |
| `docs_documents` | `read(document)` | `DocumentText`: the whole document as plain text, and each tab's own | read | `documents.readonly` |
| `docs_documents` | `create(DocsCreateDocument)` | `Document`: a new blank document with a title | write | `documents` |
| `docs_documents` | `append_text(document, DocsAppendText)` | `DocumentUpdate`: adds text at the end of the document, or of one tab | write | `documents` |
| `sheets_spreadsheets` | `get(spreadsheet)` | `Spreadsheet`: title, locale, time zone, and each sheet's id, name, position and size | read | `spreadsheets.readonly` |
| `sheets_spreadsheets` | `values_get(spreadsheet, range, SheetsGetValues)` | `ValueRange`: the values of one range | read | `spreadsheets.readonly` |
| `sheets_spreadsheets` | `values_batch_get(spreadsheet, ranges, SheetsGetValues)` | `SheetsValueRanges`: the values of several ranges, in the order asked for | read | `spreadsheets.readonly` |
| `sheets_spreadsheets` | `values_update(spreadsheet, range, SheetsUpdateValues)` | `SheetsUpdatedValues`: writes values over the cells of a range | destructive | `spreadsheets` |
| `sheets_spreadsheets` | `values_append(spreadsheet, range, SheetsAppendValues)` | `SheetsAppendedValues`: adds rows under a table | write | `spreadsheets` |

```rust
use serde_json::json;
use socketkit::google::models::{DocsAppendText, SheetsAppendValues, SheetsGetValues};

// A meeting's notes as text, and a line added under them.
let docs = google.docs_documents(&connection);
let notes = docs.read(document_id).await?;
println!("{}", notes.text);
docs.append_text(document_id, DocsAppendText {
    text: "\nDecision: open in Paris first.".into(),
    tab_id: None,
}).await?;

// The sheets of a spreadsheet, the cells of one, and a row added to it.
let sheets = google.sheets_spreadsheets(&connection);
let stock = sheets.get(spreadsheet_id).await?;
let first = &stock.sheets[0].properties.title;
let cells = sheets.values_get(spreadsheet_id, &format!("'{first}'!A1:C100"), SheetsGetValues::default()).await?;
for row in &cells.values {
    println!("{row:?}");
}
sheets.values_append(spreadsheet_id, first, SheetsAppendValues::raw(vec![vec![json!("Washers"), json!(12)]])).await?;
```

**Scopes.** The provider's default scopes cover reading Docs (`documents.readonly`) and not Sheets: an application that reads spreadsheets has to ask for `spreadsheets.readonly` in `GoogleOAuth::scopes`, and one that writes has to ask for `documents` or `spreadsheets`. A token without the scope is refused by Google with a 403, which arrives as `AccessDenied` with Google's words, "Request had insufficient authentication scopes." The same error with "The caller does not have permission" means the account may not open that file.

**`get` is light.** Both `get` methods name the fields they want, so a long document or a large spreadsheet answers in a few hundred bytes. A document's tabs come back as one list in the order a person sees them, a child tab after the tab it is inside, each with `tabId`, `title`, `index`, `nestingLevel` and `parentTabId`. A spreadsheet's sheets each carry `sheetId`, `title`, `index`, `sheetType`, `hidden` and `gridProperties` (`rowCount`, `columnCount`, `frozenRowCount`, `frozenColumnCount`); a sheet that holds a chart has no `gridProperties`. `revisionId` is sent only to an account that may edit the document.

**A document as text.** `read` asks Google for every tab's content and writes it as the text a person reads. `tabs` has each tab with its own `text`, in the order a person sees them, a child tab after the tab it is inside. A named operation returns exactly that, each tab's text once. A caller in Rust also has `DocumentText::text`, the whole document as one string: with one tab it is that tab's text, and with more each tab's text follows a line that names it, `[tab: Plan > Budget]` for a tab called Budget inside one called Plan. Socket writes the text; Google does not send it.

- **A paragraph is a line**, and a line break inside a paragraph stays one. Empty paragraphs are kept between lines and dropped at the two ends.
- **A heading is a Markdown heading**: `#` for a title and for a first heading, `##` to `######` for the five below. A subtitle is an ordinary line.
- **A list item keeps its marker and its depth**: `-` for a bullet, the item's number and a full stop for a numbered item, behind two spaces for each list it is inside. Letters and Roman numerals are written as numbers. A checklist item is written as a bullet; Google does not say whether it is ticked.
- **A table has one line for each row**, its cells between `|`. What a cell holds is put on one line, a `|` in a cell is written `\|`, and a `\` as `\\`, so that nothing written in a cell can pass for the edge of one. In a link, `[` and `]` in the words and `(`, `)` and spaces in the address are escaped for the same reason: the words cannot end their link and seem to lead somewhere else.
- **Linked words** are `[words](address)`. A smart chip for a file or a page is written the same way from its title and address, a person's chip is the name (or the address when there is no name), a date's chip is the date as the document shows it, and a dropdown is the option chosen.
- **A picture** is `[image: its title or description]`, or `[image]` when it has none; a drawing is `[object]`. A footnote is `[^1]` where it is referred to and `[^1]: …` after the text. A rule across the page is `---`, an equation is `[equation]`.
- **Breaks hold no words.** A page break or a column break leaves the line it is on as it was, and a section break leaves an empty line.
- **Headers, footers and page numbers are left out.**
- **An element of a kind Socket does not know is skipped**, and the rest of the document is read. A document is never refused for holding one.

**Suggestions.** A document is read as it stands: text someone has only suggested adding is left out, and text someone has only suggested deleting is kept. Google's own default depends on the account, with suggestions shown to an account that may edit and hidden from one that may only read, so `read` always asks for `PREVIEW_WITHOUT_SUGGESTIONS`. Comments are not read.

**Creating and adding.** `create` takes a `title` and nothing else: Google makes the document blank and ignores any content sent with it. `append_text` puts `text` at the end of the body of the first tab, or of the tab named in `tabId` (a tab's `tabId` from `get`). The text is sent exactly as given. Google puts it before the newline that ends the document, so it carries on the last paragraph: start it with `\n` to begin a new one. A new paragraph takes the look of the one before it, so text added after a heading or a list item is a heading or a list item too. Empty text and an empty title are refused before Google is called. A tab the document does not have is Google's to refuse, as `InvalidInput`.

**Ranges.** A range is in A1 notation: `Sheet1!A1:C10`, `Sheet1!A:A`, `A1:B2` for the first visible sheet, or a sheet's name alone for all of it. A name with a space or a symbol goes between single quotes: `'Q3 plan/final'!A1:B2`. For `values_get`, `values_update` and `values_append` the range is part of the address, and Socket writes it as one segment whatever it holds, so a `/`, a `?` or a `#` in a sheet's name cannot change where the request goes. A range Google cannot read, such as a sheet that is not there, arrives as `InvalidInput` with Google's words.

**Values.** `values` is a list of rows, each a list of cells, and a cell is whatever JSON Google sent: a string, a number or a boolean. Google leaves out the empty rows and columns at the end of a range and the empty cells at the end of a row, so rows differ in length and an empty range has no rows; Socket pads nothing. `SheetsGetValues` has three options, none sent unless set:

- `valueRenderOption`: `FORMATTED_VALUE` (the default: every cell as the string it shows, `$1.23`), `UNFORMATTED_VALUE` (numbers as numbers) or `FORMULA` (what was typed, `=A1`).
- `dateTimeRenderOption`: `SERIAL_NUMBER` (the default: days since 30 December 1899) or `FORMATTED_STRING`. Google ignores it when values are formatted.
- `majorDimension`: `ROWS` (the default) or `COLUMNS`, for a list of columns.

**Writing values.** `valueInputOption` is required and has no default, because neither value is safe for every caller. `USER_ENTERED` reads each value as if a person had typed it: text that starts with `=` becomes a formula, and text that looks like a number or a date becomes one. Text taken from an email or a form must not be written that way. `RAW` stores every value as it is given, so `2026-10-12` and `=SUM(A1:A9)` stay text. In Rust, `SheetsUpdateValues::raw(values)` and `::user_entered(values)` name the choice, and the same two exist on `SheetsAppendValues`. In `values`, `null` leaves a cell as it is and an empty string empties it. A write with no cell in it, or with nothing but `null` in its cells, would write nothing and is refused before Google is called; so is a list or an object where a cell should be.

**`values_update` overwrites.** What was in the cells is gone, so it is `destructive` and a host asks a person first. It reports what Google wrote: `updatedRange`, `updatedRows`, `updatedColumns` and `updatedCells`. It is sent as a PUT, and Socket's transport still repeats a PUT after a server error; repeating it writes the same values over the same cells, which changes nothing further.

**`values_append` adds rows.** Google looks in `range` for a table and writes after its last row, from its first column. It reports `tableRange`, the table as it was, and `updates`, where the rows went. New rows are always inserted for what is added (`insertDataOption=INSERT_ROWS`), and whatever lay under the table moves down. Google's own default, `OVERWRITE`, would write the rows over the cells there; that is not offered, so that an append only adds. To write over cells, use `values_update`. An append is a POST and is never repeated after a server error, so a failed append may or may not have added its rows; read the range before trying again.

**Limits Google sets.** Docs allows 300 reads and 60 writes a minute for each user of an application; Sheets allows 60 of each. Past that Google answers 429, which arrives as `RateLimited`. Google recommends keeping a Sheets request under 2 MB, and stops one it has worked on for 180 seconds. Socket reads an answer of at most 10 MB: a document whose structure is larger than that is refused as `Decode`, and a large range is better read in parts.

## Page through a list

A list returns `Page { items, next_cursor }`. Pass `next_cursor` back as the `cursor` of `Paging`, with the same filters, until it is `None`. `limit` is the most items in one page; each list says how many it takes.

```rust
use socketkit::google::models::{EventFilter, Paging};

let mut cursor = None;
loop {
    let filter = EventFilter { time_min: Some("2026-10-01T00:00:00Z".into()), ..Default::default() };
    let paging = Paging { cursor, limit: Some(250) };
    let events = google.calendar_events(&connection).list("primary", filter, paging).await?;
    for event in &events.items { /* … */ }
    match events.next_cursor {
        Some(next) => cursor = Some(next),
        None => break,
    }
}
```

A page may hold fewer items than `limit`, or none, and still be followed by another. Only a missing `next_cursor` means the end. The cursor is Google's page token and belongs to the request that produced it, so send it with the same filters.

## Call an operation by name

Every typed method is also an operation, for an agent, an MCP server, or a program in another language. The input is one JSON object with the plain arguments and the options side by side. Identifiers are short nouns (`message`, `calendar`, `event`, `file`, `document`, `spreadsheet`, `record`), paging is `cursor` and `limit`, and everything else is under Google's own name.

```rust
let created = socket.invoke(
    key,
    "google.calendar_events.insert".into(),
    json!({
        "calendar": "primary",
        "summary": "Design review",
        "start": { "dateTime": "2026-10-14T09:00:00-07:00" },
        "end": { "dateTime": "2026-10-14T09:30:00-07:00" },
        "attendees": [{ "email": "grace@example.test" }],
        "createMeetLink": true,
        "sendUpdates": "all"
    }),
).await?;
```

A field an operation does not know is refused and named, not dropped, and input that is not a JSON object is refused: a list is never read as the arguments in their order. `socket.operations()` returns each operation's input and output JSON Schema, its effect and its scopes.

**The effect is what a host goes by.** A `read` changes nothing and can run unasked. A `write` creates something, or makes a change that can be set back. A `destructive` one deletes, overwrites what was there, or cannot be taken back: mail that was sent, an answer to an invitation, a file taken from everyone who could see it. A host should ask a person before either of the last two, and show them the input. Of the 63 operations, 41 are `read`, 10 `write` and 12 `destructive`.

| Operation | Effect | Scopes | What it does |
| --- | --- | --- | --- |
| `google.identity.get` | read | `drive.readonly` | Return the account this connection is authorised as, confirming the token still works. |
| `google.resource.resolve` | read | `drive.readonly` | Confirm that a resource exists and the account can reach it. Accepts a Google Drive, Docs or Sheets URL, or a file id. |
| `google.gmail_messages.list` | read | `gmail.readonly` | List the messages a Gmail search finds. Returns ids only: each message's id and its thread's id. Read one with gmail_messages.get. |
| `google.gmail_messages.get` | read | `gmail.readonly` | Get one Gmail message, decoded: its headers, its body as plain text and as HTML, and its attachments without their content. Read an attachment that is text with gmail_messages.attachment_text. |
| `google.gmail_messages.attachment_text` | read | `gmail.readonly` | Read an attachment of a Gmail message that is text, such as a CSV file, a text file or a calendar invitation. One megabyte unless maxBytes allows more, up to ten. A file that is not UTF-8 text, such as a PDF or a picture, is refused, and nothing of it is returned. |
| `google.gmail_messages.send` | destructive | `gmail.send` | Send a message at once from the Gmail account's own address. It cannot be taken back. |
| `google.gmail_messages.reply` | destructive | `gmail.readonly`, `gmail.send` | Answer a Gmail message in its thread and send the answer at once, to the people named in `to` and nobody else. `to` is required: nothing in the message answered decides who a reply goes to, so read its `from` and `replyTo` and name them. It cannot be taken back. |
| `google.gmail_messages.send_draft` | destructive | `gmail.compose` | Send a Gmail draft as it stands. It cannot be taken back, and the draft is gone once it is sent. |
| `google.gmail_messages.modify` | write | `gmail.modify` | Add labels to a Gmail message and remove others: remove INBOX to archive, remove UNREAD to mark as read, add STARRED to star. TRASH and SPAM cannot be added: gmail_messages.trash moves a message to the bin. |
| `google.gmail_messages.trash` | destructive | `gmail.modify` | Move a Gmail message to the bin. It can be brought back with gmail_messages.untrash until Gmail empties the bin, after which it is gone for good. |
| `google.gmail_messages.untrash` | write | `gmail.modify` | Take a Gmail message out of the bin. |
| `google.gmail_threads.list` | read | `gmail.readonly` | List the Gmail threads a search finds. Each is its id and a snippet, without its messages. Read one with gmail_threads.get. |
| `google.gmail_threads.get` | read | `gmail.readonly` | Get one Gmail thread with its messages, each decoded. Ask for the metadata format to leave the bodies out of a long thread. |
| `google.gmail_labels.list` | read | `gmail.readonly` | List every label of the Gmail mailbox, Gmail's own and the person's, without their counts. |
| `google.gmail_labels.get` | read | `gmail.readonly` | Get one Gmail label, with how many messages and threads carry it and how many are unread. |
| `google.gmail_profile.get` | read | `gmail.readonly` | Get the Gmail account's address, how many messages and threads it holds, and its current history id. |
| `google.gmail_drafts.list` | read | `gmail.readonly` | List the Gmail drafts. Returns ids only: each draft's id and the ids of the message it holds. Read one with gmail_drafts.get. |
| `google.gmail_drafts.get` | read | `gmail.readonly` | Get one Gmail draft, with its message decoded: headers, text, HTML and attachments. |
| `google.gmail_drafts.create` | write | `gmail.compose` | Save a new Gmail draft. Nothing is sent. |
| `google.gmail_drafts.update` | destructive | `gmail.compose` | Replace everything a Gmail draft says. What is not given again is gone. |
| `google.gmail_drafts.delete` | destructive | `gmail.compose` | Delete a Gmail draft for good. It does not go to the bin. |
| `google.calendar_list.list` | read | `calendar.readonly` | List the calendars on the signed-in person's calendar list. |
| `google.calendar_list.get` | read | `calendar.readonly` | Get one calendar from the signed-in person's calendar list. |
| `google.calendar_events.list` | read | `calendar.readonly` | List a calendar's events: inside a time window, matching free text, with recurring events expanded when singleEvents is set. |
| `google.calendar_events.get` | read | `calendar.readonly` | Get one event, with its attendees and their answers, its meeting link and its attachments. |
| `google.calendar_events.instances` | read | `calendar.readonly` | List the occurrences of a recurring event. |
| `google.calendar_events.insert` | write | `calendar.events` | Create an event and invite its attendees, with a Google Meet link when createMeetLink is set. With sendUpdates, Google emails the invitation, which cannot be taken back. |
| `google.calendar_events.patch` | destructive | `calendar.events` | Change an event, replacing the fields given and leaving the rest. Attendees, when given, replace the whole guest list: anyone left out is uninvited, and a guest who stays keeps the answer they gave. |
| `google.calendar_events.respond` | destructive | `calendar.events` | Answer an invitation on a calendar: accepted, declined, tentative, or needsAction. On primary this is the signed-in person's own answer. The organiser sees it at once, and a notice sent with sendUpdates cannot be taken back. Nobody else on the guest list is changed. |
| `google.calendar_events.delete` | destructive | `calendar.events` | Delete an event. Deleting an event the account organised cancels it for its attendees. |
| `google.calendar_freebusy.query` | read | `calendar.readonly` | Read when calendars are busy inside a time window. Changes nothing. |
| `google.meet_conference_records.list` | read | `meetings.space.readonly` | List the Meet meetings the account organised, newest first: one conference record for each time a meeting was held. Can be narrowed to one meeting code or space, or to meetings that began between two times. Google keeps a record for 30 days after the meeting ended. At most 100 in a page. |
| `google.meet_conference_records.get` | read | `meetings.space.readonly` | Get one Meet conference record: when the meeting began and ended, and the space it was held in. |
| `google.meet_participants.list` | read | `meetings.space.readonly` | List who was in a Meet meeting, with the name each was shown under. Someone who left and came back is listed once. At most 250 in a page. |
| `google.meet_participants.get` | read | `meetings.space.readonly` | Get one participant of a Meet meeting: the name they were shown under, and when they first joined and last left. |
| `google.meet_participants.sessions` | read | `meetings.space.readonly` | List each time a participant was connected to a Meet meeting: one session for every time they joined, from every device. At most 250 in a page. |
| `google.meet_transcripts.list` | read | `meetings.space.readonly` | List the transcripts of a Meet meeting. Empty when transcription was never switched on. Each names the Google Doc it was saved to. At most 100 in a page. |
| `google.meet_transcripts.get` | read | `meetings.space.readonly` | Get one Meet transcript's details: when it was made, whether its Google Doc has been written, and which Doc that is. |
| `google.meet_transcripts.entries` | read | `meetings.space.readonly` | List a Meet transcript's entries as Meet returns them: what was said, when, in what language, and the speaker as a reference to a participant, not a name. Google deletes them 30 days after the meeting ended. At most 100 in a page. Use meet_transcripts.read for the whole transcript with names. |
| `google.meet_transcripts.read` | read | `meetings.space.readonly` | Read what was said in a Meet meeting: the transcript as lines of `Speaker: what was said`, and one entry for each thing said with the speaker's name, the start and the end. Makes several requests. Stops at maxEntries (1000 unless given) and sets truncated when the transcript has more. |
| `google.meet_recordings.list` | read | `meetings.space.readonly` | List the recordings of a Meet meeting. Empty when the meeting was not recorded. Each names the Drive file it was saved to. At most 100 in a page. |
| `google.meet_recordings.get` | read | `meetings.space.readonly` | Get one Meet recording: whether its file is ready, and the Drive file it was saved to. |
| `google.meet_spaces.get` | read | `meetings.space.readonly` | Get a Meet space from its name, its id, a meeting code or the link people join by. Returns its name, its link and code, how it is set up, and the meeting going on in it now if there is one. |
| `google.drive_files.list` | read | `drive.readonly` | List the files and folders that match a search in Drive's query language (name, full text, type, parent folder, modified time), or everything the account can see. What is in shared drives is included. |
| `google.drive_files.get` | read | `drive.readonly` | Get what describes one file or folder: its name, type, folder, owners, size and link. Not its content. |
| `google.drive_files.export` | read | `drive.readonly` | Return a Google document as text: a Doc as plain text or Markdown, a Sheet as CSV (its first sheet only). One megabyte unless maxBytes allows more, up to ten. A file that is not a Google document cannot be exported; read it with download_text. |
| `google.drive_files.download_text` | read | `drive.readonly` | Read a file that is text, such as a CSV, a text or a JSON file. One megabyte unless maxBytes allows more, up to ten. Anything that is not text is refused, and nothing of it is returned. A Google Doc or Sheet has no content of its own; read it with export. |
| `google.drive_files.permissions` | read | `drive.readonly` | List who can see a file or folder and in what role: people, groups, whole domains, and anyone with the link. |
| `google.drive_files.create_folder` | write | `drive.file` | Create a folder, at the top of the account's My Drive or inside another folder. |
| `google.drive_files.copy` | write | `drive.file` | Make a copy of a file, beside it or in another folder, under a new name if one is given. A folder cannot be copied. |
| `google.drive_files.move_to` | destructive | `drive.file` | Move a file or folder into another folder, out of the one it is in. Who can see it changes with it: the people the new folder is shared with gain it, those who had it through the old folder lose it, and a file moved into a shared drive belongs to that drive. |
| `google.drive_files.rename` | write | `drive.file` | Give a file or folder another name. It stays where it is, under the same id. |
| `google.drive_files.trash` | destructive | `drive.file` | Put a file or folder in the bin, with everything inside a folder. Everyone who could see it loses it. It can be restored for 30 days, after which Google deletes it for good. |
| `google.drive_shared_drives.list` | read | `drive.readonly` | List the shared drives the account is a member of. |
| `google.docs_documents.get` | read | `documents.readonly` | Get a Google Doc's title, revision and tabs, without what is written in it. |
| `google.docs_documents.read` | read | `documents.readonly` | Read a Google Doc as plain text, each of its tabs with its own text. Headings are Markdown headings, list items keep their markers, and a table has a line for each row. |
| `google.docs_documents.create` | write | `documents` | Create a blank Google Doc with a title. |
| `google.docs_documents.append_text` | write | `documents` | Add text at the end of a Google Doc, or of one of its tabs. Nothing that was there is changed. |
| `google.sheets_spreadsheets.get` | read | `spreadsheets.readonly` | Get a Google Sheet's title, locale and time zone, and its sheets with their names and sizes. No cell is read. |
| `google.sheets_spreadsheets.values_get` | read | `spreadsheets.readonly` | Read the values of one range of a Google Sheet, in A1 notation. |
| `google.sheets_spreadsheets.values_batch_get` | read | `spreadsheets.readonly` | Read the values of several ranges of a Google Sheet in one call. They come back in the order asked for. |
| `google.sheets_spreadsheets.values_update` | destructive | `spreadsheets` | Write values over the cells of a range of a Google Sheet. What was in those cells is replaced. |
| `google.sheets_spreadsheets.values_append` | write | `spreadsheets` | Add rows under a table in a Google Sheet. Google finds the table in the range and writes after its last row. Rows are inserted for them, so nothing under the table is written over. |

`google.calendar_freebusy.query` is a read that Google only takes as a `POST`. It is marked `read` because it changes nothing. The one consequence is in the next section.

## Handle errors

Every failure is an `Error` with a `kind()` a program can act on. An error never repeats what was in a message, an event or a document, and where it says which part of Google's answer could not be read it names fields only: a key that is Google's data, such as the calendar id that busy times are listed under, is written as `*`.

| Kind | When Google causes it | What to do |
| --- | --- | --- |
| `ReconnectRequired` | 401: the token is revoked or expired and could not be refreshed | Ask the user to connect again |
| `AccessDenied` | 403: the token lacks the scope, the API is not enabled for the project, or the account may not open or change the thing | Tell the user; Google's reason is in the message |
| `NotFound` | 404: no such thing, or the account cannot see it. 410: it was deleted, which is also what deleting an event twice gives | Check the id; treat a deleted event as gone |
| `InvalidInput` | 400 with Google's reason. 410 when a list asks for changes further back than Google keeps them. 412 when what a change was made against has changed since, as when an event is edited while it is being answered. A 403 for an export over Drive's 10 MB limit. Also anything Socket refuses before sending | Fix the argument the message names; after a 412, read again and repeat the call |
| `RateLimited` | 429, or 403 with the reason `rateLimitExceeded`, `userRateLimitExceeded` or `RESOURCE_EXHAUSTED` | Wait for `error.retry()` |
| `Decode` | Google answered success without the result, or with something that could not be read | Report it; nothing should be assumed done |

**What is sent again.** A read is tried again after a server error. A change sent as `POST` or `PATCH` is never sent twice after a failure that may have been processed: mail whose sending timed out is reported, not sent again. A request Google throttled was not carried out, so it is tried again whatever it is. Two things follow from the transport going by the verb, and both are noted on the roadmap:

- `calendar_freebusy.query` is a `POST`, so it is not tried again after a server error; call it again yourself.
- A `PUT` and a `DELETE` are still tried again after a server error. For `sheets_spreadsheets.values_update` and `gmail_drafts.update` that writes the same content twice, which does no harm. For `calendar_events.delete` and `gmail_drafts.delete` it means a delete that worked, but whose answer was lost, can come back as `NotFound`: treat that as deleted.

## Confirmed against Google's documentation, and not

**Nothing here has been run against a real Google account.** Each method was built from its reference page on developers.google.com, read on 2026-10-10 and 2026-10-11. What each product's pages confirmed, and what they do not say, is below.

### Gmail

Everything here was read from developers.google.com in October 2026. Nothing was run against a live account.

Confirmed:

- `GET users/me/messages`, with `q`, `labelIds` (repeated), `includeSpamTrash`, `maxResults` (100 by default, 500 at most) and `pageToken`; that each row is only an `id` and a `threadId`; and its scopes. ([users.messages/list](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/list))
- `GET users/me/messages/{id}` with `format` and `metadataHeaders`, and the formats `full`, `metadata`, `minimal` and `raw`. ([users.messages/get](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/get), [Format](https://developers.google.com/workspace/gmail/api/reference/rest/v1/Format))
- The fields of a message, of a part (`partId`, `mimeType`, `filename`, `headers`, `body`, `parts`), of a header and of a part's body (`attachmentId`, `size`, `data` in base64url). ([users.messages](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages), [users.messages.attachments](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages.attachments))
- `GET users/me/messages/{messageId}/attachments/{id}`, which answers with a part's body: the file in base64url in `data`, its `size`, and nothing that says what the file is. ([users.messages.attachments/get](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages.attachments/get))
- `POST users/me/messages/send` with a message whose `raw` is the RFC 2822 message in base64url, that it sends to the people in `To`, `Cc` and `Bcc`, and that `gmail.send` is among its scopes. ([users.messages/send](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/send), [sending](https://developers.google.com/workspace/gmail/api/guides/sending))
- What keeps a message in a thread: the `threadId` on the request, `References` and `In-Reply-To` by RFC 2822, and matching subjects. ([threads](https://developers.google.com/workspace/gmail/api/guides/threads))
- `POST users/me/messages/{id}/modify` with `addLabelIds` and `removeLabelIds`, 100 of each at most, under `gmail.modify`. ([users.messages/modify](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/modify))
- `POST users/me/messages/{id}/trash` and `/untrash`, which return the message, under `gmail.modify`. ([trash](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/trash), [untrash](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/untrash))
- `GET users/me/threads` with the same parameters as messages, that a row carries no messages, `GET users/me/threads/{id}` with `format`, and the fields of a thread. ([users.threads/list](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.threads/list), [get](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.threads/get), [users.threads](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.threads))
- `GET users/me/labels`, which takes no paging and returns each label's id, name, visibility and type only; `GET users/me/labels/{id}`; the fields of a label and the values of `type` and of the two visibilities. ([users.labels/list](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.labels/list), [get](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.labels/get), [users.labels](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.labels))
- Which of Gmail's own labels can be applied by hand, that `SENT` and `DRAFT` cannot, and that `TRASH` and `SPAM` can, which is why `modify` refuses to add them. ([labels](https://developers.google.com/workspace/gmail/api/guides/labels))
- `GET users/me/profile` and its four fields. ([users/getProfile](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users/getProfile))
- `GET users/me/drafts` with `q`, `includeSpamTrash`, `maxResults` and `pageToken`, each row a draft id and its message's ids; `GET users/me/drafts/{id}` with `format`; `POST users/me/drafts` and `PUT users/me/drafts/{id}` with `{ "message": { "raw": … } }`; that an update replaces the draft's message; `DELETE users/me/drafts/{id}`, which is permanent; `POST users/me/drafts/send` with the draft's `id`, after which the draft is deleted and the sent message has a new id. ([users.drafts](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.drafts), [list](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.drafts/list), [get](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.drafts/get), [create](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.drafts/create), [update](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.drafts/update), [delete](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.drafts/delete), [send](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.drafts/send), [drafts](https://developers.google.com/workspace/gmail/api/guides/drafts))
- The scopes each method accepts. `gmail.readonly` covers every read here, drafts and the profile included. `drafts.send` accepts `gmail.compose`, `gmail.modify` and full access, and not `gmail.send`.
- That `gmail.readonly`, `gmail.compose`, `gmail.modify` and `gmail.metadata` are restricted and `gmail.send` is sensitive, and what a restricted scope requires. ([scopes](https://developers.google.com/workspace/gmail/api/auth/scopes))

Not confirmed:

- **Whether header values arrive decoded.** The reference says only that a value is what follows the colon. Socket reads encoded words either way, which changes nothing in a value that has none.
- **That a body's bytes are in the character set its part names.** The reference says `data` is the body in base64url and nothing about its character set. Socket reads it in the set the part's `Content-Type` names.
- **That Gmail writes `From`, `Date` and `Message-ID`** on a message sent without them. Google's own samples set `From`. Socket leaves all three to Gmail.
- **That Gmail leaves the `Bcc` header out of what the others receive.** The reference says only that it sends to the people in `Bcc`.
- **An empty JSON object as the body of `trash` and `untrash`.** Google says the body must be empty. Socket's transport sends no length for a request with no body, which Google's servers may refuse, so Socket sends `{}`. That Gmail accepts it was not confirmed.
- **Padding in `raw`.** The reference says base64url. Google's samples pad, and so does Socket.
- **The words of a search.** The reference says `q` takes what Gmail's search box takes, and lists no operators. The ones shown here (`from:`, `subject:`, `is:unread`, `newer_than:`, `has:attachment`, `filename:`) are the search box's, and were not read from the reference.
- **The order of a list**, and of the messages of a thread. No page states either. In practice a list is newest first and a thread oldest first.
- **How long the bin keeps a message.** The reference for `trash` does not say.
- **Whether an attachment's id stays the same** from one read of a message to the next. `partId` and `filename` are steadier ways to recognise a part.
- **Whether a long body ever arrives as an `attachmentId` and not as `data`.** If it does, it is listed in `attachments` with an empty `filename`, and `attachment_text` returns it.
- **What an attachment's answer looks like for an empty file, and how much Gmail writes around the file.** Socket takes an answer with a `size` of 0 and no `data` as an empty file, and allows 8 KB for the JSON around the base64 (its field names, and an `attachmentId` if Gmail repeats one) when it works out how much to fetch for a given limit. An answer with more around it than that would make a file of exactly the limit fail with `TooLarge`.
- **That `size` in an attachment's answer is the length of the file.** Socket does not go by it: the file is what `data` decodes to.
- **How Gmail compares subjects** when it decides whether a reply belongs to a thread, beyond "the subjects must match".
- **The word Gmail gives for each refusal** (`notFound`, `insufficientPermissions`, `userRateLimitExceeded`). Socket goes by the status, and by Google's general rule for a throttling 403.
- **Gmail's own limits**: the size of a message, the number of recipients, how much can be sent in a day. None was read.

### Calendar

Read from developers.google.com on 2026-10-11. Nothing was run against a real Google account.

Confirmed:

- The verb, path, parameters and accepted scopes of each of the ten methods: [calendarList.list](https://developers.google.com/workspace/calendar/api/v3/reference/calendarList/list), [calendarList.get](https://developers.google.com/workspace/calendar/api/v3/reference/calendarList/get), [events.list](https://developers.google.com/workspace/calendar/api/v3/reference/events/list), [events.get](https://developers.google.com/workspace/calendar/api/v3/reference/events/get), [events.instances](https://developers.google.com/workspace/calendar/api/v3/reference/events/instances), [events.insert](https://developers.google.com/workspace/calendar/api/v3/reference/events/insert), [events.patch](https://developers.google.com/workspace/calendar/api/v3/reference/events/patch), [events.delete](https://developers.google.com/workspace/calendar/api/v3/reference/events/delete) and [freebusy.query](https://developers.google.com/workspace/calendar/api/v3/reference/freebusy/query).
- `calendar.readonly` is accepted by every read, `freebusy.query` included, and `calendar.events` by `insert`, `patch` and `delete`.
- `timeMin` and `timeMax` must be RFC 3339 with a mandatory offset; `orderBy=startTime` is only for `singleEvents=true`; the page sizes (events: 250 by default, 2500 at most; calendar list: 100 and 250).
- The answer of a list is `calendar#events` or `calendar#calendarList` with `items` and `nextPageToken`; of a free/busy query, `calendar#freeBusy` with `calendars`, each with `busy` and `errors`.
- The fields of an event in the [Events resource](https://developers.google.com/workspace/calendar/api/v3/reference/events): `start` and `end` as `date` or `dateTime` with `timeZone`, where an offset is required unless `timeZone` is set; `attendees[]` and the four values of `responseStatus`; `attendees[].self` as the calendar the copy is on; `hangoutLink`; `conferenceData` with `createRequest`, `entryPoints`, `conferenceSolution` and `conferenceId`, the Meet code; `attachments[]`; `recurrence`; `recurringEventId`; `originalStartTime`; `guestsCanSeeOtherGuests`.
- `conferenceData.createRequest` with a new `requestId` creates a conference, a request whose id repeats the one before is ignored, `conferenceDataVersion=1` is needed, and the status may be `pending` ([create events](https://developers.google.com/workspace/calendar/api/guides/create-events)).
- `patch` changes only the fields given, and an array given replaces the one that was there.
- `attendeesOmitted`, on an update, "can be used to only update the participant's response".
- `sendUpdates` is `all`, `externalOnly` or `none`.
- The errors in the [error guide](https://developers.google.com/workspace/calendar/api/guides/errors): 403 `rateLimitExceeded` and `userRateLimitExceeded` as throttles, 403 `quotaExceeded` and `forbiddenForNonOrganizer`, 410 `deleted` for an event already deleted, 410 `updatedMinTooLongAgo` and `fullSyncRequired`, and 412 `conditionNotMet` when the etag in `If-Match` is no longer current.

Not confirmed, because the documentation does not say or only a real account can show it:

- **`respond` as a whole.** That a guest who is not the organiser may change their own answer through `events.patch` with `calendar.events`. The error guide points guests to `patch`, and says nothing more.
- **`If-Match` on `events.patch`.** The 412 is documented in the error guide, not on the method's page.
- **A hidden guest list.** What exactly Google returns to a guest when `guestsCanSeeOtherGuests` is false (Socket takes it to be the guest's own entry alone), and that `attendeesOmitted: true` works on `patch` as the Events resource says it does "when updating an event".
- **Replacing a guest list.** That Google accepts a guest sent back as it returned them, read-only fields such as `self` and `organizer` included; `respond` rests on the same thing. The reference says only that the new array replaces the old one.
- **A Meet link on an event that already has one.** What `createMeetLink` on `patch` does then: whether Google ignores the request, refuses it, or replaces the link for every guest.
- **Clearing `date` or `dateTime` in a patch.** It follows from the patch rules; Google does not document it for `start` and `end`.
- **`sendUpdates` left unset.** The reference gives the default of `insert` as `false`, which is not one of the parameter's values, and gives none for `patch` and `delete`. It is taken to mean `none`.
- **The status of a delete.** The reference says the answer has no body, not which status it has. Any success is taken.
- **What a delete does to the guests.** That deleting an event the account organised cancels it for its guests is how the product is known to behave; the reference says only that the event is deleted.
- **Free/busy keys.** That each calendar comes back under the id it was asked about by, `primary` included.
- **Where Meet puts a recording.** That Google Meet attaches a meeting's recording, transcript and notes to its event is how the product is known to behave; the Calendar reference only says an event has `attachments`.
- **Limits.** Google's request quotas were not looked into. A free/busy query is documented to answer for at most 50 calendars; what it does with more was not checked, and Socket does not refuse them.

### Meet

Read from developers.google.com and support.google.com in October 2026, and from the API's own description at `https://meet.googleapis.com/$discovery/rest?version=v2` (revision 20261005). Nothing was run against a live account.

Confirmed:

- Every endpoint above, with its verb, path, parameters and response field: `GET /v2/conferenceRecords` (`filter`, `pageSize`, `pageToken`; `conferenceRecords`) and `/v2/conferenceRecords/{id}`; `…/participants` (`participants`), `/participants/{id}` and `/participants/{id}/participantSessions` (`participantSessions`); `…/transcripts` (`transcripts`), `/transcripts/{id}` and `/transcripts/{id}/entries` (`transcriptEntries`); `…/recordings` (`recordings`) and `/recordings/{id}`; `GET /v2/spaces/{id}`. ([reference](https://developers.google.com/workspace/meet/api/reference/rest/v2/conferenceRecords/list), and the pages beside it)
- `meetings.space.readonly` is accepted by every one of them, `spaces.get` included. ([spaces.get](https://developers.google.com/workspace/meet/api/reference/rest/v2/spaces/get))
- The filter on conference records: the fields `space.meeting_code`, `space.name`, `start_time` and `end_time`, and the examples `space.name = "spaces/NAME"`, `space.meeting_code = "abc-mnop-xyz"`, `start_time>="2024-01-01T00:00:00.000Z" AND start_time<="2024-01-02T00:00:00.000Z"` and `end_time IS NULL`. ([conferenceRecords.list](https://developers.google.com/workspace/meet/api/reference/rest/v2/conferenceRecords/list))
- Page sizes: 25 by default and 100 at most for conference records; 100 and 250 for participants and sessions; 10 and 100 for transcripts, entries and recordings.
- The fields of a conference record, a participant (`signedinUser`, `anonymousUser`, `phoneUser`, each with `displayName`), a session, a transcript, an entry, a recording and a space, with their spelling, and the states `STARTED`, `ENDED` and `FILE_GENERATED`.
- `spaces.get` takes `spaces/{space}` or `spaces/{meetingCode}`; a code is not case sensitive, is at most 128 characters, and generally expires 365 days after last use. ([meeting spaces](https://developers.google.com/workspace/meet/api/guides/meeting-spaces))
- Entries are deleted 30 days after the meeting ended; a conference record is deleted 30 days after it ended; recordings, transcripts and notes are saved to the organiser's Drive and kept under Drive's rules. ([artifacts](https://developers.google.com/workspace/meet/api/guides/artifacts))
- A meeting's owner and participants can read its records and artifacts, and `list` returns only the meetings the account organised. ([conferences](https://developers.google.com/workspace/meet/api/guides/conferences))
- A participant-device pair has a session for each time it joined, and a profile may be withheld. ([participants](https://developers.google.com/workspace/meet/api/guides/participants))
- The Workspace editions and languages that have transcripts, and that a transcript is also attached to the meeting's calendar event. ([help](https://support.google.com/meet/answer/12849897))

Not confirmed:

- **Clauses on different fields joined with `AND`.** Google's only example of `AND` joins two bounds on `start_time`. A meeting code or a space together with a time is written the same way; that Google accepts it was not confirmed.
- **How a quote is written inside a filter value.** The documentation calls the syntax EBNF and does not say. Socket refuses a value with a quote or a backslash and escapes nothing.
- **Whether a meeting code in a filter is matched in any case.** Socket sends it in lower case, as Google writes codes.
- **Whether `space.name` matches a meeting code** written as `spaces/abc-mnop-xyz`. Give a code as `meetingCode`.
- **`<` and `>` in a filter, and `end_time`.** Only `>=`, `<=` and `IS NULL` appear in the examples. Socket sends `>=` and `<=` on `start_time` only.
- **What an id may contain.** Google's ids look like letters, digits and hyphens. Socket percent-encodes an id as one path segment; how Meet reads an encoded one was not confirmed.
- **That an entry's `participant` is always the `name` of a participant in the list.** The guide says each entry "is connected to a participant name". Where it is not, `speaker` is absent.
- **What Google answers for an expired record**, and for a meeting the account was not in. Socket passes on the kind Google's status gives.
- **Whether `spaces.get` with only this scope returns a space the account did not create**, and which of its `config` it shows.
- **That the time fields always carry `Z`.** Google documents RFC 3339 with 0, 3, 6 or 9 fractional digits; Socket also reads an offset.

Where the issue and the documentation differ:

- **Gemini's notes are in this API now.** The issue says they are only a Doc on the calendar event. Google's v2 reference lists `conferenceRecords.smartNotes` (`list` and `get`), each with the `docsDestination` of the notes. It is not built here; see below.
- **Who sees a record.** The issue says a person sees the records of meetings they organised or attended. That holds for `get` and for everything under a record; `list` returns only the meetings the account organised.

### Drive

Everything here was read from developers.google.com in October 2026. Nothing was run against a live account.

Confirmed:

- `GET drive/v3/files` with `q`, `orderBy` and its eleven keys, `corpora`, `driveId`, `includeItemsFromAllDrives`, `supportsAllDrives`, `pageSize` (at most 1000; larger values are coerced) and `pageToken`; the answer's `kind: "drive#fileList"`, `files` and `nextPageToken`. <https://developers.google.com/workspace/drive/api/reference/rest/v3/files/list>
- That `corpora=drive` needs `driveId`, and that shared drive items are left out without `includeItemsFromAllDrives` and `supportsAllDrives`. <https://developers.google.com/workspace/drive/api/guides/enable-shareddrives>
- `GET drive/v3/files/{fileId}` with `supportsAllDrives`. <https://developers.google.com/workspace/drive/api/reference/rest/v3/files/get>
- The fields of a file with their spelling and types, that `size` is a string, that a file has one parent, and the fields of a user. <https://developers.google.com/workspace/drive/api/reference/rest/v3/files>, <https://developers.google.com/workspace/drive/api/reference/rest/v3/User>
- The `fields` parameter: commas, brackets for the fields of a list or an object, and that `files.list` returns only `kind`, `id`, `name` and `mimeType` without it. <https://developers.google.com/workspace/drive/api/guides/fields-parameter>
- The query language: the terms and their operators, single quotes, `\'` and `\\`, double quotes for a phrase, that `contains` matches a prefix of a name and whole words in content, RFC 3339 times in UTC, and every search in the table above. <https://developers.google.com/workspace/drive/api/guides/search-files>, <https://developers.google.com/workspace/drive/api/guides/ref-search-terms>
- That a sort with a `fullText` search is refused with 400 `badRequest`. <https://developers.google.com/workspace/drive/api/guides/handle-errors>
- `GET drive/v3/files/{fileId}/export` with `mimeType` as its only parameter, and that exported content is limited to 10 MB. <https://developers.google.com/workspace/drive/api/reference/rest/v3/files/export>
- The export formats: `text/plain` and `text/markdown` for a Doc, `text/plain` for Slides, `text/csv` for a Sheet with "first sheet only". <https://developers.google.com/workspace/drive/api/guides/ref-export-formats>
- That a file's content is fetched with `GET drive/v3/files/{fileId}?alt=media` on `www.googleapis.com` with the token in the `Authorization` header, that this is for files stored in Drive and a Google document is exported instead, and that `files.get` takes `supportsAllDrives`. <https://developers.google.com/workspace/drive/api/guides/manage-downloads>, <https://developers.google.com/workspace/drive/api/reference/rest/v3/files/get>
- That a download of a Google document is refused with 403, reason `fileNotDownloadable` and the message "Only files with binary content can be downloaded. Use Export with Docs Editors files." <https://developers.google.com/workspace/drive/api/guides/handle-errors>
- That a file flagged as abusive is given only to its owner and only with `acknowledgeAbuse=true`, and that downloading can be restricted (`capabilities.canDownload`). <https://developers.google.com/workspace/drive/api/guides/manage-downloads>
- That the newer `POST drive/v3/files/{fileId}/download` answers with a long-running operation and not with the content, and is the only way to download a Google Vids file. Socket does not use it. <https://developers.google.com/workspace/drive/api/reference/rest/v3/files/download>
- `GET drive/v3/files/{fileId}/permissions` with `supportsAllDrives`, `pageSize` (at most 100) and `pageToken`; `kind: "drive#permissionList"`; the fields of a permission and of `permissionDetails`, and the values of `type`, `role` and `permissionType`. <https://developers.google.com/workspace/drive/api/reference/rest/v3/permissions/list>, <https://developers.google.com/workspace/drive/api/reference/rest/v3/permissions>
- `POST drive/v3/files` for a file with no content, a folder's MIME type, and that a file without `parents` goes to the top of My Drive. <https://developers.google.com/workspace/drive/api/reference/rest/v3/files/create>, <https://developers.google.com/workspace/drive/api/guides/folder>
- `POST drive/v3/files/{fileId}/copy` with a file as its body. <https://developers.google.com/workspace/drive/api/reference/rest/v3/files/copy>
- `PATCH drive/v3/files/{fileId}` with `addParents`, `removeParents` and `supportsAllDrives`, that only the fields sent are changed, and that a move reads the file's parents first. <https://developers.google.com/workspace/drive/api/reference/rest/v3/files/update>, <https://developers.google.com/workspace/drive/api/guides/folder>
- That a file is binned by setting `trashed` to `true`, that the bin is emptied after 30 days, that only the owner can bin a file, and that a shared drive file needs `supportsAllDrives`. <https://developers.google.com/workspace/drive/api/guides/delete>
- `GET drive/v3/drives` with `pageSize` (10 by default, at most 100) and `pageToken`; `kind: "drive#driveList"`; the fields of a shared drive. <https://developers.google.com/workspace/drive/api/reference/rest/v3/drives/list>, <https://developers.google.com/workspace/drive/api/reference/rest/v3/drives>
- The scopes each method accepts: `drive.readonly` for every read here, `drive.file` for create, copy and update. The reads of files also accept `drive.file`, for the files that scope reaches; the list of shared drives does not.
- The errors `insufficientFilePermissions`, `appNotAuthorizedToFile`, `fileNotExportable` and the shared drive refusals, all 403. <https://developers.google.com/workspace/drive/api/guides/handle-errors>

Not confirmed:

- **How Google reports an export that is too large.** Its documentation states the 10 MB limit and not the error. Reports from people who met it give a 403 with the reason `exportSizeLimitExceeded` and the message "This file is too large to be exported." Socket recognises the reason, whatever the message.
- **How Google reports an export of a file that is not a Google document.** The documentation lists the reason `fileNotExportable` only with a message about Google Vids. The message Socket recognises, "Export only supports Docs Editors files.", is from experience of the API and not from a page.
- **That two refusals are told by their message.** A file that cannot be exported, and a Google document that cannot be downloaded, are recognised by Google's wording and not by the `reason` beside it. If Google rewords either, the call still fails, as `AccessDenied` with Google's own words, and not with the clearer message.
- **Whether Google answers a download with a redirect to another host, and when.** Its example follows redirects without saying why. Socket refuses such a redirect; see "Downloading a file".
- **The type Google states for a downloaded file.** It is taken to be the file's own `mimeType`; the pages do not say so. For an export it is taken to be the format asked for.
- **How Google refuses a flagged file** without `acknowledgeAbuse`. The page says the parameter is needed and not what is answered without it.
- **What Google answers for a download of a folder or a shortcut.** If it is the refusal it gives for a Google document, the message that points to `export` is given for those too.
- **That a Doc's plain text begins with a byte order mark.** It is what the API returns; no page says so. Socket removes one if it is there.
- **`fields` on `drives.list` and on the writes.** It is a parameter of every Google API, and the pages for these methods do not list it separately.
- **A PATCH with an empty object as its body**, which is what a move sends. Google's example sends no body at all. Socket's transport gives no length to a request without a body, which a server may refuse, and an empty object changes nothing.
- **What `files.list` returns by default for shared drives.** With no `driveId`, Google searches its `user` body of files with shared drive items included. Exactly which shared drive files that covers is not stated; to be sure of all of one drive, give its `driveId`.
- **That `export` reaches a file in a shared drive.** The method has no `supportsAllDrives` parameter, so none is sent.
- **Removing a parent the file no longer has**, and **adding a parent to a file whose own parent cannot be seen.** See "Moving".
- **That Google refuses to copy a folder**, and with what error. The page for `copy` does not mention folders.
- **Who may bin a file in a shared drive.** The table of roles did not load.
- **Which files `drive.file` reaches.** What is said above is Google's description of the scope as it is generally given; its page was not reread.
- **`root` as a value of `addParents`.** Socket does not rely on it: it looks up the id and sends that.

### Docs and Sheets

Read from developers.google.com in October 2026. Nothing was run against a live Google account.

Confirmed:

- `GET https://docs.googleapis.com/v1/documents/{documentId}`, its parameters `includeTabsContent` and `suggestionsViewMode`, and its scopes, `documents.readonly` among them ([documents.get](https://developers.google.com/workspace/docs/api/reference/rest/v1/documents/get)).
- That `suggestionsViewMode` defaults to `DEFAULT_FOR_CURRENT_ACCESS`, which shows suggestions inline to an account that may edit and hides them from one that may only view, and that `PREVIEW_WITHOUT_SUGGESTIONS` returns the document with every suggestion rejected ([documents](https://developers.google.com/workspace/docs/api/reference/rest/v1/documents)).
- That with `includeTabsContent` the content is in `tabs[].documentTab` and the fields at the top are empty, that without it they hold the first tab's content, and that child tabs are in `childTabs` ([tabs](https://developers.google.com/workspace/docs/api/how-tos/tabs)).
- The `fields` parameter and its syntax, with commas, dots and parentheses, and that a mask that names `tabs` is treated as `includeTabsContent` ([field masks](https://developers.google.com/workspace/docs/api/how-tos/field-masks), [documents.get](https://developers.google.com/workspace/docs/api/reference/rest/v1/documents/get)).
- The names Socket reads from a document: `tabProperties` (`tabId`, `title`, `parentTabId`, `index`, `nestingLevel`, `iconEmoji`), `body.content`, the four kinds of structural element (`paragraph`, `sectionBreak`, `table`, `tableOfContents`), the twelve kinds of paragraph element, `paragraphStyle.namedStyleType` and its values, `bullet.listId` and `nestingLevel`, `lists[].listProperties.nestingLevels[]` with `glyphType`, `glyphSymbol` and `startNumber`, `tableRows[].tableCells[].content`, `textStyle.link.url`, `personProperties`, `richLinkProperties`, `dateElementProperties.displayText`, `dropdownProperties.displayValue`, `inlineObjects[].inlineObjectProperties.embeddedObject` and `footnotes[].content` ([documents](https://developers.google.com/workspace/docs/api/reference/rest/v1/documents)).
- That `revisionId` is sent only to an account that may edit, and is good for 24 hours for that account.
- `POST https://docs.googleapis.com/v1/documents`, that it uses the title and ignores everything else, and its scopes ([documents.create](https://developers.google.com/workspace/docs/api/reference/rest/v1/documents/create)).
- `POST https://docs.googleapis.com/v1/documents/{documentId}:batchUpdate`, its body `requests`, its answer `documentId`, `replies` and `writeControl`, and its scopes ([documents.batchUpdate](https://developers.google.com/workspace/docs/api/reference/rest/v1/documents/batchUpdate)).
- `insertText` with `text` and `endOfSegmentLocation`, that an empty `segmentId` is the body, that `tabId` names the tab and the first tab is used without it, that the text goes immediately before the last newline of the segment, and that a newline in the text starts a paragraph in the style of the one before ([requests](https://developers.google.com/workspace/docs/api/reference/rest/v1/documents/request)).
- `GET https://sheets.googleapis.com/v4/spreadsheets/{spreadsheetId}`, that cell data is not returned by default, that `includeGridData` is ignored when a field mask is set, and its scopes ([spreadsheets.get](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets/get)).
- The fields of a spreadsheet, of `SpreadsheetProperties`, of `SheetProperties` and of `GridProperties`, and the three sheet types ([spreadsheets](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets), [sheets](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets/sheets)).
- `GET …/values/{range}` and `GET …/values:batchGet` with `ranges` repeated, their three options with their values and defaults, that the answers come in the order asked for, and their scopes ([values.get](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets.values/get), [values.batchGet](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets.values/batchGet)).
- `PUT …/values/{range}` and `POST …/values/{range}:append`, `valueInputOption` with `RAW` and `USER_ENTERED` and what each does, `insertDataOption` with `OVERWRITE` and `INSERT_ROWS`, the body as a `ValueRange`, the two answers with their fields, and their scopes ([values.update](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets.values/update), [values.append](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets.values/append), [UpdateValuesResponse](https://developers.google.com/workspace/sheets/api/reference/rest/v4/UpdateValuesResponse)).
- `ValueRange`: that trailing empty rows and columns are left out, that a `null` in what is written is skipped and an empty string empties a cell, and that a cell written is a boolean, a string or a number ([values](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets.values)).
- A1 notation, and that single quotes are required around a sheet name with spaces or special characters ([concepts](https://developers.google.com/workspace/sheets/api/guides/concepts)).
- The quotas above, the 429, the 2 MB recommendation and the 180 seconds ([Docs limits](https://developers.google.com/workspace/docs/api/limits), [Sheets limits](https://developers.google.com/workspace/sheets/api/limits)).

Not confirmed:

- **How deep tabs can nest.** No page gives a limit. A field mask cannot say "at every depth", so `get` names four: a tab and three inside one another under it. A tab deeper than that is missing from the list `get` returns. `read` uses no mask and returns every tab at any depth.
- **Whether `documents.create` answers with the new document's tabs.** Socket asks for them with the same field mask as `get`. If Google sends none, `create` returns a document with an empty `tabs`, and `get` returns them.
- **That a body opens with a section break**, and that the newline that ends a paragraph is in its last text run. Google's examples show both and no page states them. Socket skips a section break only where it is the first element, and removes one newline from the end of a paragraph.
- **That a line break inside a paragraph is a vertical tab** (`U+000B`) in a text run. No page says so. If Google writes it another way, the break is kept as whatever character Google sent.
- **Whether a checklist item's tick can be read.** No field for it was found, so a checklist reads as bullets.
- **An empty `endOfSegmentLocation`.** Socket sends `{}` for the end of the first tab's body. The page says an empty `segmentId` is the body and an omitted `tabId` is the first tab; it does not show the object with neither.
- **What `append_text` does to a document that ends in a table**, or in another element text cannot follow.
- **A range written with `%21` and `%3A`** for `!` and `:`. This is ordinary URL encoding of a path segment, not confirmed for Sheets specifically, and neither is a `/` in a sheet's name written as `%2F`.
- **How an apostrophe inside a sheet's name is written in A1 notation.** Google's own example, `'Jon's_Data'!A1:D5`, does not double it. Socket passes the range on as it is given.
- **That an empty cell between two values is an empty string**, and that the cells at the end of a row are left out. The page speaks only of trailing rows and columns. Socket returns what Google sends either way.
- **What Google answers for a write of only `null` values**, and whether `updatedCells` is then absent. An absent count reads as 0.
- **The words of Google's errors** used in the tests ("Unable to parse range", "insufficient authentication scopes"). Socket goes by the status, and passes the words on.
- **Whether the quotas count a `batchGet` as one read.**

## Not supported yet

Across all of Google: incoming events (push notifications and watch channels), incremental sync, service accounts and domain-wide delegation, and Slides, Chat, Tasks, People and Admin. `google.identity.get` without a Drive scope, as said under Connect.

### Gmail

- **Deleting a message for good** (`messages.delete`), on purpose. `trash` can be undone, and that is enough.
- **Change tracking** (`history.list`) and **push notifications** (`users.watch`). `historyId` is returned everywhere so that they can follow.
- **Attachments on a message that is sent or drafted.** `GmailSendMessage` is text and HTML.
- **An attachment's bytes from an operation called by name.** `attachment_content` is a typed method only; by name, `attachment_text` returns a file that is UTF-8 text and refuses any other.
- **Attachments that are text in another encoding** (UTF-16, Windows-1252), read as text. `attachment_content` returns their bytes.
- **Reading part of an attachment.** A file is fetched whole or refused; Gmail offers no range of one.
- **A draft that is a reply.** `reply` sends at once; `gmail_drafts.create` starts a new thread.
- **Reply all, and forwarding.** Give `reply` the `to` and `cc` you want.
- **Marking a message as spam.** `modify` refuses `SPAM` in `addLabelIds`. Removing it is not refused.
- **Sending as another address** (`From`, send-as aliases), and **`Reply-To`** on what is sent.
- **Addresses outside ASCII** (`grüße@example.test`), which need a mail server that accepts them.
- **Character sets other than UTF-8, ISO-8859-1 and Windows-1252** on reading. Such a body or header is returned, with what could not be read replaced or left encoded.
- **Changing the labels of a whole thread** (`threads.modify`), **creating and deleting labels**, and **changing many messages at once** (`batchModify`).
- **The message as it was sent** (`format=raw`), and **choosing which headers** come back (`metadataHeaders`).
- **Other people's mailboxes.** Every path is `users/me`.
- **Settings**: filters, forwarding, signatures, vacation replies.

### Calendar

- **Incremental sync** with sync tokens (`syncToken`, `nextSyncToken`), which the issue leaves for later, and **push notifications** (watch channels). To follow changes today, list with `updatedMin`.
- **Moving or importing an event**, `quickAdd`, and replacing an event whole (`events.update`).
- **Creating, changing or deleting calendars**, sharing them (ACLs), settings and colours.
- **Adding attachments** to an event. Attachments are returned, not written.
- **Reminders, extended properties, working locations, focus time and out-of-office details** on an event, and the list filters that go with them (`eventTypes`, `iCalUID`, `privateExtendedProperty`, `sharedExtendedProperty`, `showHiddenInvitations`, `maxAttendees`).
- **Expanding a group** in a free/busy query (`groupExpansionMax`, `calendarExpansionMax`).
- **A conference other than Google Meet**, and copying an existing conference onto another event.
- **Narrower scopes.** Google also accepts `calendar.events.readonly`, `calendar.calendarlist.readonly`, `calendar.freebusy` and `calendar.events.owned` for parts of this. The operations name `calendar.readonly` and `calendar.events`.

### Meet

- **Gemini's notes** (`conferenceRecords.smartNotes`). Until then they are reached as the Doc attached to the calendar event, through Calendar and Drive.
- **Creating, changing and ending spaces**, and a space's members. Out of scope: this integration only reads Meet.
- **Reading the Doc of a transcript or the video of a recording.** Meet gives the ids; Drive and Docs read them.
- **One participant session, or one transcript entry, by name** (`participantSessions.get`, `entries.get`). The lists return the same fields.
- **Filtering participants and sessions** (`latest_end_time IS NULL` for who is there now), and conference records still going on (`end_time IS NULL`).
- **A space's phone numbers, PINs and SIP addresses** (`phoneAccess`, `gatewaySipAccess`), and its moderation restrictions. They are not read into `MeetSpace`.
- **Continuing `read` past 10,000 entries.** Page through `entries` for a transcript that long.
- **Events** when a meeting starts or a transcript is ready (the Workspace Events API).

### Drive

- **Exporting to a format that is not text** (PDF, Word, Excel, PowerPoint, images). An export returns text; the bytes of such a format are not offered, by name or typed.
- **An export over 10 MB**, which Google does not make, and **text of more than ten megabytes** by name.
- **A download Google redirects to another host.** Google's download hosts are named by pattern, which a provider cannot declare yet.
- **Downloading a file Google has flagged** (`acknowledgeAbuse`), **part of a file** (`Range`), **an earlier revision**, and **Google Vids**, which only the long-running `files.download` gives.
- **Reading as text a file that is stored under a type that is not text**, or in an encoding other than UTF-8. `download` returns its bytes.
- **Other sheets of a spreadsheet as CSV.** Drive exports the first only; the rest are read through Sheets.
- **Searching every shared drive at once, or a whole domain** (`corpora` of `allDrives` or `domain`). Google may then search only part of what was asked and say so in `incompleteSearch`, which a page of results has no place for.
- **Searching for shared drives** (`q` on `drive_shared_drives.list`), and the lists an administrator sees (`useDomainAdminAccess`).
- **Permanent deletion**, left out on purpose, and **taking a file out of the bin**.
- **Sharing**: adding, changing or removing a permission.
- **Uploading a file, or changing a file's content.** `create_folder` and `copy` are the only ways to make a file here.
- **Creating a shortcut**, and following one: read the target by `shortcutDetails.targetId`.
- **Other fields of a file**, such as `description`, `starred`, `lastModifyingUser`, `capabilities`, `exportLinks` and labels, and **changing anything but a file's name, folder and bin**.
- **Comments, revisions, change tracking and notifications.**

### Docs and Sheets

- **Editing a document** beyond adding text at its end: inserting at a place, replacing, deleting, styling, tables, images, and writing as a suggestion. `batchUpdate` is used for the one request only.
- **Adding a tab, renaming one or deleting one.**
- **Comments and suggestions** in a document. Suggested text is not marked; it is left out.
- **A document's headers and footers**, and the tick of a checklist item.
- **Reading a document in a format other than text.** `drive_files.export` gives a Doc as Markdown; HTML, PDF and Word are not offered yet.
- **Reading part of a document.** `read` always returns every tab, and a document whose structure is over 10 MB cannot be read.
- **Creating a spreadsheet, and adding, renaming or deleting a sheet.**
- **Clearing a range** (`values.clear`), **writing several ranges at once** (`values.batchUpdate`), and returning the written values (`includeValuesInResponse`).
- **Formats, merges, notes, charts, named ranges, filters and protected ranges.** `spreadsheets.get` is asked only for what describes the sheets.
- **Ranges in R1C1 notation.** Google accepts them where it accepts A1 notation and Socket passes a range on as given, but nothing here was tested with one.

For anything in these lists that is a plain REST call returning JSON, the authenticated request still works:

```rust
use socketkit::RawRequest;
let colors = socket.request(key, RawRequest::get("calendar/v3/colors")).await?;
```
