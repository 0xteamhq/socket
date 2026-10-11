### Gmail: `gmail_messages`, `gmail_threads`, `gmail_labels`, `gmail_drafts` and `gmail_profile`

Reading, searching, labelling, drafting and sending mail in the mailbox of the account that connected. A message comes back decoded: Gmail returns a tree of MIME parts in base64, and Socket returns headers, text, HTML and a list of attachments. A message that is sent is written by Socket from structured content, so no caller builds a MIME message or encodes one.

None of the Gmail scopes is a default of the provider. Name the ones you need in `GoogleOAuth.scopes`, from `socketkit::google::scopes`.

| Method | What it does | Effect | Scope |
| --- | --- | --- | --- |
| `gmail_messages.list(GmailListMessages, Paging)` | `Page<GmailMessageRef>`: the ids of the messages a search finds | read | `GMAIL_READONLY` |
| `gmail_messages.get(message, GmailGetMessage)` | `GmailMessage`: headers, text, HTML, attachments without their content | read | `GMAIL_READONLY` |
| `gmail_messages.attachment_get(message, attachment)` | `GmailAttachmentBody`: one attachment's content in URL-safe base64, and its size | read | `GMAIL_READONLY` |
| `gmail_messages.send(GmailSendMessage)` | `GmailMessageRef`: sends at once | destructive | `GMAIL_SEND` |
| `gmail_messages.reply(message, GmailReply)` | `GmailMessageRef`: answers in the same thread, at once | destructive | `GMAIL_READONLY` and `GMAIL_SEND` |
| `gmail_messages.send_draft(draft)` | `GmailMessageRef`: sends a draft as it stands | destructive | `GMAIL_COMPOSE` |
| `gmail_messages.modify(message, GmailModifyMessage)` | `GmailMessageRef`: adds and removes labels | write | `GMAIL_MODIFY` |
| `gmail_messages.trash(message)`, `untrash(message)` | `GmailMessageRef`: to the bin, and back | write | `GMAIL_MODIFY` |
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

Each is also a named operation: `google.gmail_messages.list`, `google.gmail_drafts.create`, and so on. In an operation's input the plain arguments are `message`, `attachment`, `thread`, `label` and `draft`, the options sit beside them under Gmail's own names, and paging is `cursor` and `limit`.

```rust
use socketkit::google::models::{GmailGetMessage, GmailListMessages, GmailModifyMessage, GmailReply, Paging};

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

    // Answer in the same thread, then archive and mark as read.
    messages.reply(&row.id, GmailReply {
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
- `text` is every `text/plain` part that is not a file, and `html` every `text/html` part. Of the alternatives of one message (`multipart/alternative`) one text and one HTML are kept. A body is read in the character set its part names; UTF-8, ASCII, ISO-8859-1 and Windows-1252 are read exactly, and anything else is read as UTF-8 with the replacement character where it is not.
- `attachments` lists every part that is a file: `attachmentId`, `filename`, `mimeType`, `size`, `inline`, `contentId` and `partId`. A text file that was attached is listed here and is not part of `text`. `inline` is `true` for a part the sender marked to be shown in the body, such as a picture in a signature, which the HTML refers to as `cid:` and its `contentId`.
- `format` chooses how much comes back: `full` (the default), `metadata` (headers, no body, no attachments) or `minimal` (ids and labels). Gmail's `raw` format is not offered, since it is the undecoded message.

**Attachments.** `attachment_get` returns `data` as Gmail sends it, in base64 with the URL-safe alphabet (`-` and `_`), and the `size` of the file in bytes. Socket reads an answer of at most 10 MB, so a file over about 7 MB cannot be fetched this way yet and fails with `Decode`.

**Sending.** `GmailSendMessage` is `to`, `cc`, `bcc`, `subject`, `text` and `html`. Each person is `{ "email": "grace@example.test", "name": "Grace Hopper" }`, with the name optional. Socket writes the message as mail travels (RFC 5322 and MIME) and sends it to Gmail in `raw`:

- `send` needs at least one person in `to`, `cc` or `bcc`, and a `subject`, `text` or `html`.
- With `text` and `html` the message is `multipart/alternative`, and each reader sees the one its mail program prefers. With one of them it is that alone.
- Bodies are sent in base64 as UTF-8, so any text arrives as written. Line ends become CR LF, as mail requires.
- A subject or a name outside ASCII, or too long for one line, is sent as encoded words and reads normally in every mail program.
- The message goes out from the account's own address. There is no `from`; Gmail writes it, with the date and the message's id.
- People in `bcc` are sent to Gmail with the rest. Gmail delivers to them and leaves them out of what the others receive.

**What is refused, and why.** A line break or any other control character in `subject` or in a `name` is refused with `InvalidInput`, and so is an `email` that is not exactly one mailbox in ASCII (`grace@example.test`): no list, no `Name <address>`, no quoted part. A line break in a header would end it and begin another, which is how a subject becomes a hidden `Bcc`. Nothing is sent to Google when a message is refused, and the error names the field (`to[1]`) without repeating it. In a named operation a field that is not one of the six, such as `from` or `raw`, is refused too and not dropped.

**Replying.** `reply(message, GmailReply)` reads the original's headers (one more request, without its body) and sends an answer that stays in the thread: it carries the original's `threadId`, names the original in `In-Reply-To`, and lists the thread so far in `References`.

- It goes to the address the original asks replies to go to (`Reply-To`), or else to its sender (`From`). Nobody else is added: this is "reply", not "reply all". Set `to`, `cc` and `bcc` to choose the recipients yourself. Replying to a message the account itself sent therefore addresses the account, unless `to` says otherwise.
- The subject is the original's with `Re: ` before it, and an original that already starts with `Re:` is not marked twice. Gmail keeps a reply in its thread only while the subjects match, so setting `subject` can start a new thread.
- A reply needs `text` or `html`. The original is not quoted beneath it.
- An original with no `Message-ID` cannot be answered in its thread, and `reply` fails with `Decode` and sends nothing. So does one that names no mailbox to answer, with `InvalidInput`, unless `to` is given.

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

Up to 100 labels can be added and 100 removed in one call. `SENT` and `DRAFT` are Gmail's to set and cannot be added. `gmail_labels.list` gives every label's `id`, `name` and `type` (`system` or `user`); the counts (`messagesTotal`, `messagesUnread`, `threadsTotal`, `threadsUnread`) come with `gmail_labels.get`.

**The bin.** `trash` moves a message to the bin and `untrash` brings it back, so both are `write`. Nothing here deletes a message for good.

**Errors.** A message, thread, label or draft that does not exist is `NotFound`. A scope the connection was not given is `AccessDenied`. Gmail's limit on how fast one mailbox is used arrives as `RateLimited`, whether Google sends it as a 429 or as a 403. An answer Socket cannot read is `Decode`, and names the place (`payload.parts[1].body.data`) without repeating what was there.

#### Confirmed against Google's documentation, and not

Everything here was read from developers.google.com in October 2026. Nothing was run against a live account.

Confirmed:

- `GET users/me/messages`, with `q`, `labelIds` (repeated), `includeSpamTrash`, `maxResults` (100 by default, 500 at most) and `pageToken`; that each row is only an `id` and a `threadId`; and its scopes. ([users.messages/list](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/list))
- `GET users/me/messages/{id}` with `format` and `metadataHeaders`, and the formats `full`, `metadata`, `minimal` and `raw`. ([users.messages/get](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/get), [Format](https://developers.google.com/workspace/gmail/api/reference/rest/v1/Format))
- The fields of a message, of a part (`partId`, `mimeType`, `filename`, `headers`, `body`, `parts`), of a header and of a part's body (`attachmentId`, `size`, `data` in base64url). ([users.messages](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages), [users.messages.attachments](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages.attachments))
- `GET users/me/messages/{messageId}/attachments/{id}`. ([users.messages.attachments/get](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages.attachments/get))
- `POST users/me/messages/send` with a message whose `raw` is the RFC 2822 message in base64url, that it sends to the people in `To`, `Cc` and `Bcc`, and that `gmail.send` is among its scopes. ([users.messages/send](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/send), [sending](https://developers.google.com/workspace/gmail/api/guides/sending))
- What keeps a message in a thread: the `threadId` on the request, `References` and `In-Reply-To` by RFC 2822, and matching subjects. ([threads](https://developers.google.com/workspace/gmail/api/guides/threads))
- `POST users/me/messages/{id}/modify` with `addLabelIds` and `removeLabelIds`, 100 of each at most, under `gmail.modify`. ([users.messages/modify](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/modify))
- `POST users/me/messages/{id}/trash` and `/untrash`, which return the message, under `gmail.modify`. ([trash](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/trash), [untrash](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/untrash))
- `GET users/me/threads` with the same parameters as messages, that a row carries no messages, `GET users/me/threads/{id}` with `format`, and the fields of a thread. ([users.threads/list](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.threads/list), [get](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.threads/get), [users.threads](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.threads))
- `GET users/me/labels`, which takes no paging and returns each label's id, name, visibility and type only; `GET users/me/labels/{id}`; the fields of a label and the values of `type` and of the two visibilities. ([users.labels/list](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.labels/list), [get](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.labels/get), [users.labels](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.labels))
- Which of Gmail's own labels can be applied by hand, and that `SENT` and `DRAFT` cannot. ([labels](https://developers.google.com/workspace/gmail/api/guides/labels))
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
- **Whether a long body ever arrives as an `attachmentId` and not as `data`.** If it does, it is listed in `attachments` with an empty `filename`, and `attachment_get` returns it.
- **How Gmail compares subjects** when it decides whether a reply belongs to a thread, beyond "the subjects must match".
- **The word Gmail gives for each refusal** (`notFound`, `insufficientPermissions`, `userRateLimitExceeded`). Socket goes by the status, and by Google's general rule for a throttling 403.
- **Gmail's own limits**: the size of a message, the number of recipients, how much can be sent in a day. None was read.

#### Not supported yet

- **Deleting a message for good** (`messages.delete`), on purpose. `trash` can be undone, and that is enough.
- **Change tracking** (`history.list`) and **push notifications** (`users.watch`). `historyId` is returned everywhere so that they can follow.
- **Attachments on a message that is sent or drafted.** `GmailSendMessage` is text and HTML.
- **Attachments over about 7 MB**, until the transport can return content as it is (issue #6).
- **A draft that is a reply.** `reply` sends at once; `gmail_drafts.create` starts a new thread.
- **Reply all, and forwarding.** Give `reply` the `to` and `cc` you want.
- **Sending as another address** (`From`, send-as aliases), and **`Reply-To`** on what is sent.
- **Addresses outside ASCII** (`grüße@example.test`), which need a mail server that accepts them.
- **Character sets other than UTF-8, ISO-8859-1 and Windows-1252** on reading. Such a body or header is returned, with what could not be read replaced or left encoded.
- **Changing the labels of a whole thread** (`threads.modify`), **creating and deleting labels**, and **changing many messages at once** (`batchModify`).
- **The message as it was sent** (`format=raw`), and **choosing which headers** come back (`metadataHeaders`).
- **Other people's mailboxes.** Every path is `users/me`.
- **Settings**: filters, forwarding, signatures, vacation replies.
