# Microsoft

**Status:** built and tested against a local server that answers as Microsoft's documentation says. Not yet run against a real Microsoft 365 organisation.

One provider, `microsoft`, for everything behind Microsoft Graph, because Outlook, Teams, OneDrive and SharePoint share one sign-in. What is built so far is the provider itself and **Teams meetings**: finding a meeting, its transcripts and what was said in them, its recordings, and who attended. That is 9 typed methods and the same 9 as operations callable by name with JSON, plus identity. All of them read; none changes anything in Microsoft 365.

This page shows how to connect, lists what is supported, states the limits a caller will meet, and says what was checked against Microsoft's reference and what was not.

## Connect

Add the crate with the Microsoft feature:

```sh
cargo add socketkit --features microsoft
```

### With your own app registration, to connect your users

Register an application in Microsoft Entra, give it a client secret and a redirect URI, and add the delegated Graph permissions you will ask for.

```rust
use std::sync::Arc;
use socketkit::microsoft::{Microsoft, MicrosoftOAuth};
use socketkit::{ConnectionKey, OAuthClient, ProviderId, SecretString, Socket};

let microsoft = Microsoft::with_oauth(MicrosoftOAuth {
    // `None` asks for the defaults: offline_access and User.Read.
    scopes: Some(vec![
        "offline_access".into(),
        "User.Read".into(),
        "OnlineMeetings.Read".into(),
        "OnlineMeetingTranscript.Read.All".into(),
    ]),
    // `None` is `common`: any work, school or personal account.
    tenant: Some("organizations".into()),
    login_hint: None,
    prompt: None,
    ..OAuthClient {
        client_id: config.microsoft_client_id,
        client_secret: SecretString::new(config.microsoft_client_secret),
        redirect_uri: "https://yourapp.example/oauth/microsoft/callback".parse()?,
    }
    .into()
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
| `scopes` | The Graph permissions to ask for. **Keep `offline_access` in the list**: without it Microsoft issues no refresh token and the connection stops working when the access token expires, about an hour later. |
| `tenant` | Who may sign in: `common` (default), `organizations` (work and school accounts), `consumers` (personal accounts), or one organisation's tenant id or domain. It is part of the sign-in address, so a value that is not a tenant is refused when the `Socket` is built. Meetings need a work or school account, so `organizations` or a tenant id is the usual choice. |
| `login_hint` | Prefills the sign-in page with an account. |
| `prompt` | `login`, `none`, `consent` or `select_account`. |

Microsoft replaces the refresh token on each refresh. Socket saves the new one every time; nothing needs doing.

PKCE is switched on. Microsoft's sign-in reference recommends it for every kind of application, confidential clients included.

### With a token you already hold

```rust
let microsoft = Microsoft::with_token("eyJ0eXAi…");
let socket = Socket::in_memory().integration(Arc::new(microsoft.clone())).build()?;
let connection = socket.connection(ConnectionKey::new(ProviderId::new("microsoft")?, "me")).await?;
```

Every call uses that token. A Graph access token lasts about an hour and this way has no refresh, so it suits a script or a test, not a product.

## Permissions

All delegated: the calls act as the person who connected, and reach what that person may reach.

| Methods | Permission | Needs an administrator's consent |
| --- | --- | --- |
| `identity` | `User.Read` | No |
| `online_meetings` | `OnlineMeetings.Read` | No |
| `transcripts` | `OnlineMeetingTranscript.Read.All` | **Yes** |
| `recordings` | `OnlineMeetingRecording.Read.All` | **Yes** |
| `attendance` | `OnlineMeetingArtifact.Read.All` | No |

"Needs an administrator's consent" means a person cannot approve the permission for themselves: an administrator of their organisation approves it once for everyone. Until then Microsoft's sign-in page tells that organisation's people that approval is needed, and they cannot connect. None of these permissions exists for personal Microsoft accounts.

Each operation lists its permission in `required_scopes`.

## Use the typed methods

Each group is reached through the `Microsoft` value and a connection. A meeting's id, a transcript's id and so on are plain arguments.

```rust
use socketkit::microsoft::models::Paging;

// A calendar event carries the join link; the link finds the meeting.
let meeting = microsoft.online_meetings(&connection).find_by_join_url(&event_join_url).await?;

let transcripts = microsoft.transcripts(&connection).list(&meeting.id, Paging::default()).await?;
for transcript in &transcripts.items {
    let content = microsoft.transcripts(&connection).content(&meeting.id, &transcript.id).await?;
    for entry in &content.entries {
        println!("[{} ms] {}: {}", entry.start_ms, entry.speaker.as_deref().unwrap_or("?"), entry.text);
    }
}
```

| Group | Method | What it returns |
| --- | --- | --- |
| `online_meetings` | `get(meeting)` | The meeting: subject, start and end, join link, chat, organiser and attendees |
| | `find_by_join_url(join_url)` | The meeting behind a join link; `NotFound` when no meeting the account can read has it |
| `transcripts` | `list(meeting, paging)` | The meeting's transcripts, one for each time transcription ran |
| | `get(meeting, transcript)` | One transcript's details |
| | `content(meeting, transcript)` | `text`, the transcript as Microsoft wrote it (WebVTT), and `entries`: `speaker`, `start_ms`, `end_ms`, `text` |
| `recordings` | `list(meeting, paging)` | The meeting's recordings |
| | `get(meeting, recording)` | One recording's details, with `recordingContentUrl` |
| `attendance` | `reports(meeting, paging)` | One report for each time the meeting was held |
| | `records(meeting, report, paging)` | Who is in a report: email, role, total seconds, and every join and leave |

`microsoft.identity(&connection)` returns the account that connected: its id, its display name, and `mail` or, for an account without a mailbox, the sign-in name.

Models are in `socketkit::microsoft::models`. As JSON their fields carry Graph's own names (`joinWebUrl`, `createdDateTime`); dates are the ISO 8601 text Graph sends, in UTC.

**A transcript's entries.** `start_ms` and `end_ms` are milliseconds from the start of the transcript. They are negative when transcription was switched on while people were already talking. `speaker` is absent when the transcript does not name one. `TranscriptContent::from_vtt` is public, for WebVTT text obtained another way. A cue whose timing cannot be read, or text that is not a transcript at all, is an error, never a shorter or an empty transcript.

## Page through a list

A list returns `items` and `next_cursor`. Pass the cursor back to get the next page; `None` means the last page.

```rust
let mut paging = Paging { top: Some(20), ..Paging::default() };
loop {
    let page = microsoft.attendance(&connection).records(&meeting.id, &report.id, paging.clone()).await?;
    // use page.items
    let Some(cursor) = page.next_cursor else { break };
    paging.cursor = Some(cursor);
}
```

The cursor is the link Graph gives for the next page. It already carries the page size, so `top` is used only for the first page. A cursor that does not point into Graph's API is refused before anything is sent.

## Call an operation by name

Every method is an operation with the method's arguments and its options side by side.

| Operation | Input |
| --- | --- |
| `microsoft.identity.get` | `{}` |
| `microsoft.online_meetings.get` | `{ "meeting": "MSpk…" }` |
| `microsoft.online_meetings.find_by_join_url` | `{ "join_url": "https://teams.microsoft.com/l/meetup-join/…" }` |
| `microsoft.transcripts.list` | `{ "meeting": "MSpk…", "top": 10, "cursor": "…" }` |
| `microsoft.transcripts.get` | `{ "meeting": "MSpk…", "transcript": "MSMj…" }` |
| `microsoft.transcripts.content` | `{ "meeting": "MSpk…", "transcript": "MSMj…" }` |
| `microsoft.recordings.list` | `{ "meeting": "MSpk…" }` |
| `microsoft.recordings.get` | `{ "meeting": "MSpk…", "recording": "7e31…" }` |
| `microsoft.attendance.reports` | `{ "meeting": "MSpk…" }` |
| `microsoft.attendance.records` | `{ "meeting": "MSpk…", "report": "c9b6…" }` |

All ten have the effect `read`. `top` and `cursor` are optional wherever they appear.

`microsoft.transcripts.content` returns the whole transcript inline, as text, and again as entries. Socket refuses a body over 10 MB.

## Limits you will meet

- **A transcript exists only if transcription was switched on during the meeting.** Otherwise `transcripts.list` is empty. The same holds for recordings.
- **Who can read them.** Microsoft's reference says the transcript and recording APIs are available to the people on the meeting's calendar invitation, for private chat meetings and for channel meetings, and only until the meeting expires.
- **An organisation can switch this off.** Its administrator can disable reading transcripts through the API altogether, or disable speaker names. Both come back as `AccessDenied` with a message that says so. With speaker names disabled, `transcripts.content` is refused, because it asks for the format that names speakers; Microsoft offers the transcript without names in another format, which is not built here.
- **Meetings that are not on a calendar.** Transcripts and recordings are not available for a meeting made with Graph's "create onlineMeeting" call that has no calendar event. Live events are not covered either.
- **The join link comes from the calendar event** (`onlineMeeting.joinUrl`). Give it to `find_by_join_url` unchanged, percent signs and all.
- **Attendance reports:** Microsoft returns the fifty most recent for a meeting.
- **Personal Microsoft accounts** cannot use any of this.

## Handle errors

| Kind | What it means for Microsoft | What to do |
| --- | --- | --- |
| `ReconnectRequired` | The token expired or was revoked and could not be renewed | Connect again |
| `AccessDenied` | A permission was not granted, the person may not read this meeting, or the organisation switched the feature off. The message carries Microsoft's reason | Add the permission, ask an administrator, or leave this meeting out |
| `NotFound` | No such meeting, transcript, recording or report, or no meeting with that join link | Check the id or the link |
| `InvalidInput` | An argument is wrong; the message names the field | Fix the input |
| `RateLimited` | Graph is throttling; `retry()` says how long to wait | Wait and try again |
| `Unexpected` | Graph is unavailable; `retry()` carries the wait when Graph gave one | Try again later |
| `Decode` | Graph answered success without what was asked for, or with a transcript that could not be read | Report it |

An input error names the field and never repeats the value you sent. A transcript's words never appear in an error.

Every method here is a read, sent as `GET`, and is tried again after a throttle or a server error.

## What was checked, and what was not

Checked against Microsoft Graph's v1.0 reference and the Microsoft identity platform's sign-in reference in October 2026:

- Every path used, the `$filter` form of the join-link lookup, and the response shapes.
- The permission for each call, and which need an administrator's consent.
- `text/vtt` as the transcript format, selected with the `Accept` header, and the shape of a cue.
- The two administrator settings and their inner error codes, which Microsoft says take effect at the end of July 2026.
- `$top` on the transcript and recording lists.
- PKCE, `offline_access`, the tenant in the sign-in address, `login_hint` and `prompt`.

Not confirmed, because the reference does not say or because it needs a real organisation:

- **Which meetings `find_by_join_url` finds for someone who did not organise them**, and whether it finds channel meetings. The reference documents the lookup and does not state this.
- **Whether attendance records can be listed for a channel meeting.** Microsoft's "get one attendance report" call does not support channel meetings; the two calls used here do not carry that warning, and were not tried.
- **Whether these lists ever return a next-page link**, and `$top` on the attendance lists, which the reference covers only by saying the usual query options are supported.
- **Nothing has been run against a real Microsoft 365 organisation.**

## Not supported yet

- **Downloading a recording.** The video is bytes, and Socket's transport reads text and JSON only. `recordings.get` returns the address; fetching it waits for the core to carry bytes.
- **A transcript without speaker names**, for organisations that switched speaker attribution off. It can be read through the generic request, shown below.
- **Resource lookup** (`microsoft.resource.resolve`, a OneDrive or SharePoint sharing link), and telling a person plainly when sign-in failed because an administrator's consent is missing.
- **Outlook mail and calendar, Teams chats and channels, OneDrive and SharePoint.**
- **Access as the application itself** (client credentials), and Microsoft's national clouds, which use other hosts.
- Copilot's meeting insights, and the organisation-wide transcript listing.

Anything Graph offers that has no method here can still be called through the generic request, with the token, retries and error handling applied. Add `.as_text()` when the answer is text and not JSON; the body is then a JSON string. An id put into a path this way must be percent-encoded by the caller.

```rust
// The transcript without speaker names, in Microsoft's other format.
let request = RawRequest::get(format!("me/onlineMeetings/{meeting}/transcripts/{transcript}/content"))
    .with_header("Accept", "application/vnd.microsoft.graph.transcript+text")
    .as_text();
let text = socket.request(key, request).await?.body;
```
