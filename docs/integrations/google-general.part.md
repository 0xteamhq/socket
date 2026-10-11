# Google

**Status:** built and tested against a local server that answers as Google's documentation says. Not yet run against the real Google. [What was and was not confirmed](#what-was-confirmed-and-what-was-not) is listed below.

One provider covers Google's products, because they share one sign-in. Today Socket's Google integration gives a program identity, Drive file lookup, and Google Calendar: 10 typed methods and the same 10 as operations callable by name with JSON. This page shows how to connect, lists everything that is supported, and says what is not.

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

The Calendar scopes are **not** among the defaults (`drive.readonly`, `documents.readonly`). An application that uses Calendar names them:

```rust
use socketkit::google::{CALENDAR_EVENTS_SCOPE, CALENDAR_READONLY_SCOPE, Google, GoogleOAuth};
use socketkit::{OAuthClient, SecretString};

let google = Google::with_oauth(GoogleOAuth {
    client: OAuthClient {
        client_id: config.google_client_id,
        client_secret: SecretString::new(config.google_client_secret),
        redirect_uri: "https://yourapp.example/oauth/google/callback".parse()?,
    },
    scopes: Some(vec![
        // Leave out whichever the application does not use.
        CALENDAR_READONLY_SCOPE.into(), // every Calendar read
        CALENDAR_EVENTS_SCOPE.into(),   // create, change, answer and delete events
        "https://www.googleapis.com/auth/drive.readonly".into(), // identity and Drive lookup, see below
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

| Scope | Constant | Covers |
| --- | --- | --- |
| `https://www.googleapis.com/auth/calendar.readonly` | `CALENDAR_READONLY_SCOPE` | `calendar_list.*`, `calendar_events.list`, `.get`, `.instances`, `calendar_freebusy.query` |
| `https://www.googleapis.com/auth/calendar.events` | `CALENDAR_EVENTS_SCOPE` | `calendar_events.insert`, `.patch`, `.respond`, `.delete` |

`calendar.events` also lets a token read events, but not the calendar list and not free/busy, so an application that does both asks for both.

**Identity needs a Drive scope.** `google.identity.get` reads the account from Drive, and `google.resource.resolve` looks up a Drive file. A connection with only the Calendar scopes can use every Calendar operation, but those two are refused with `AccessDenied`. Keep a Drive scope if the application calls them.

## Use the typed methods

Methods are grouped the way Google groups its Calendar API. Each group is reached from the integration with a connection:

```rust
use socketkit::google::models::{EventTime, FreeBusyQuery, InsertEvent, ListEvents, PatchEvent, Respond};

// This week's meetings, each occurrence of a recurring one by itself, in order.
let week = ListEvents {
    time_min: Some("2026-10-12T00:00:00Z".into()),
    time_max: Some("2026-10-19T00:00:00Z".into()),
    single_events: Some(true),
    order_by: Some("startTime".into()),
    ..Default::default()
};
let events = google.calendar_events(&connection).list("primary", week).await?;

// Schedule a meeting with a Google Meet link, and email the invitation.
let mut meeting = InsertEvent::new(
    EventTime::at("2026-10-14T09:00:00-07:00"),
    EventTime::at("2026-10-14T09:30:00-07:00"),
)
.summary("Design review")
.invite("grace@example.test")
.with_meet_link();
meeting.send_updates = Some("all".into());
let created = google.calendar_events(&connection).insert("primary", meeting).await?;
println!("{:?}", created.hangout_link);

// Move it, answer an invitation, check who is free.
let changes = PatchEvent { location: Some("Room 5".into()), ..Default::default() };
google.calendar_events(&connection).patch("primary", &created.id, changes).await?;
google.calendar_events(&connection).respond("primary", "another-event-id", Respond::accept()).await?;
let busy = google.calendar_freebusy(&connection)
    .query(
        &["primary".into(), "grace@example.test".into()],
        FreeBusyQuery::between("2026-10-14T00:00:00Z", "2026-10-15T00:00:00Z"),
    )
    .await?;
```

Two rules hold for every method:

- **What identifies the thing is a plain argument**: a calendar id, an event id. `primary` names the signed-in person's own calendar; a person's calendar id is otherwise their email address.
- **Content and options are a struct** from `socketkit::google::models`. A field left unset is not sent, so Google applies its own default.

| Group | Reached with | Methods |
| --- | --- | --- |
| Calendar list | `google.calendar_list(&connection)` | `list`, `get` |
| Events | `google.calendar_events(&connection)` | `list`, `get`, `instances`, `insert`, `patch`, `respond`, `delete` |
| Free/busy | `google.calendar_freebusy(&connection)` | `query` |

Things worth knowing:

- **Names are Google's.** In JSON, fields are spelled as Google spells them: `timeMin`, `singleEvents`, `hangoutLink`, `conferenceData`. In Rust the same fields are `time_min`, `single_events`, `hangout_link`, `conference_data`.
- **Times.** `timeMin`, `timeMax` and `updatedMin` are RFC 3339 with the offset: `2026-10-12T00:00:00Z` or `2026-10-12T09:00:00-07:00`. An event's `start` and `end` each hold either `dateTime` or, for an all-day event, `date`. Ends are exclusive: an all-day event on the 12th ends on the 13th.
- **Recurring events.** Without `singleEvents`, a recurring event comes back once, as the series, with its `recurrence` rules. With it, every occurrence in the window comes back as its own event carrying `recurringEventId`. `orderBy: "startTime"` only works with `singleEvents`. `instances` lists the occurrences of one series.
- **From a meeting to its recording.** An event keeps `hangoutLink`, `conferenceData` (the Meet code in `conferenceId`, and every way to join in `entryPoints`) and `attachments`. Google Meet attaches a meeting's recording, transcript and notes to the event as Drive files, so `attachments[].fileId` is where to look for them afterwards.
- **Meet links.** `createMeetLink` on `insert` or `patch` asks Google to create one. Google may still be creating it when it answers: then `conferenceData.createRequest.status.statusCode` is `pending` and `hangoutLink` is unset. Read the event again.
- **Who is emailed.** `sendUpdates` is `all`, `externalOnly` or `none`. Left unset, Google emails nobody.
- **Changing an event.** `patch` changes only the fields given. `attendees`, when given, replaces the whole guest list: anyone left out is uninvited, so read the event first and send everyone who stays, each with the `responseStatus` they already gave. Moving an event between all-day and timed works; Socket clears the form that is no longer used.
- **Answering an invitation.** Google has no call for an answer alone: it is a change to the event's guest list, and a change replaces the list. `respond` therefore reads the event, writes the answer into the entry Google marks as the calendar's own (`self`), and sends the list back with everyone else untouched. It names the version it read, so if someone changed the event in between, Google refuses the write instead of losing their change, and the error says to try again. A calendar whose owner is not on the guest list is refused before anything is written.
- **Whose answer it is.** `self` marks the calendar the event was read from, not whoever is signed in. On `primary` the two are the same person. On a calendar the signed-in person manages for someone else, `respond` answers for that calendar's owner. A host that asks a person to approve the call should show them the `calendarId`.
- **Free/busy.** A calendar Google could not answer for, such as one the account may not see, comes back with `errors` set and no busy periods. That is not the same as free: check `errors` before reading `busy`.
- **Calendar ids** may contain `@` and `#`. They are encoded for the URL.

### Identity and lookup

```rust
let me = google.identity(&connection).await?;                  // id, name, email
let doc = google.resolve(&connection, "https://docs.google.com/document/d/1AbC…/edit").await?;
```

`resolve` accepts a Drive, Docs or Sheets URL, or a file id, and confirms the account can open it. To require accounts from one Google Workspace domain, set `hosted_domain` on `GoogleOAuth`.

## Page through a list

A list returns `Page { items, next_cursor }`. Pass `next_cursor` back as `page_token` (`pageToken` in JSON), with the same filters, until it is `None`.

```rust
use socketkit::google::models::ListEvents;

let mut page_token = None;
loop {
    let options = ListEvents { time_min: Some("2026-10-01T00:00:00Z".into()), page_token, ..Default::default() };
    let events = google.calendar_events(&connection).list("primary", options).await?;
    for event in &events.items { /* … */ }
    match events.next_cursor {
        Some(next) => page_token = Some(next),
        None => break,
    }
}
```

A page may hold fewer events than `maxResults`, or none, and still be followed by another. Only a missing `next_cursor` means the end.

## Call an operation by name

Every typed method is also an operation, for an agent, an MCP server, or a program in another language. The input is one JSON object with the plain arguments and the options side by side.

```rust
let created = socket.invoke(
    key,
    "google.calendar_events.insert".into(),
    json!({
        "calendarId": "primary",
        "summary": "Design review",
        "start": { "dateTime": "2026-10-14T09:00:00-07:00" },
        "end": { "dateTime": "2026-10-14T09:30:00-07:00" },
        "attendees": [{ "email": "grace@example.test" }],
        "createMeetLink": true,
        "sendUpdates": "all"
    }),
).await?;
```

`socket.operations()` returns each operation's input and output JSON Schema, its effect and its scopes. The effect is what a host goes by: a `read` changes nothing and can run unasked; a `write` creates or changes something, and here may email people; a `destructive` one cannot be taken back.

| Operation | Effect | Scope | What it does |
| --- | --- | --- | --- |
| `google.identity.get` | read |  | Return the account this connection is authorised as, confirming the token still works. |
| `google.resource.resolve` | read |  | Confirm that a resource exists and the account can reach it. Accepts a Google Drive, Docs or Sheets URL, or a file id. |
| `google.calendar_list.list` | read | calendar.readonly | List the calendars on the signed-in person's calendar list. |
| `google.calendar_list.get` | read | calendar.readonly | Get one calendar from the signed-in person's calendar list. |
| `google.calendar_events.list` | read | calendar.readonly | List a calendar's events: inside a time window, matching free text, with recurring events expanded when singleEvents is set. |
| `google.calendar_events.get` | read | calendar.readonly | Get one event, with its attendees, its meeting link and its attachments. |
| `google.calendar_events.instances` | read | calendar.readonly | List the occurrences of a recurring event. |
| `google.calendar_events.insert` | write | calendar.events | Create an event, with a Google Meet link when createMeetLink is set. |
| `google.calendar_events.patch` | write | calendar.events | Change an event. Only the fields given are changed; attendees, when given, replace the whole guest list. |
| `google.calendar_events.respond` | write | calendar.events | Answer an invitation on a calendar: accepted, declined, tentative, or needsAction to take an answer back. On primary this is the signed-in person's own answer. |
| `google.calendar_events.delete` | destructive | calendar.events | Delete an event. |
| `google.calendar_freebusy.query` | read | calendar.readonly | Find when calendars are busy inside a time window. |

Scopes are shown by their last part; each is `https://www.googleapis.com/auth/` followed by it.

`google.calendar_freebusy.query` is a read that Google only takes as a `POST`. It is marked `read` because it changes nothing. The one consequence is in the next section.

## Handle errors

Every failure is an `Error` with a `kind()` a program can act on.

| Kind | When Google causes it | What to do |
| --- | --- | --- |
| `ReconnectRequired` | 401: the token is revoked or expired and could not be refreshed | Ask the user to connect again |
| `AccessDenied` | 403: the token lacks the scope, the account may not change this calendar, or Google's abuse limit (`quotaExceeded`) was reached | Tell the user; Google's reason is in the message |
| `NotFound` | 404: no such calendar or event, or the account cannot see it. 410: it was deleted, which is also what deleting an event twice gives | Check the id; treat a deleted event as gone |
| `InvalidInput` | 400 with Google's reason, such as an empty time range. 410 when `updatedMin` is further back than Google keeps changes. 412 from `respond` when the event changed while it was being answered, with `retry()` saying to try again. Also anything Socket refuses before sending | Fix the argument the message names, or call `respond` again |
| `RateLimited` | 429, or 403 with the reason `rateLimitExceeded` or `userRateLimitExceeded` | Wait for `error.retry()` |
| `Decode` | Google answered success without the result, or `respond` read an event it could not safely write back | Report it; nothing should be assumed done |

A read is tried again after a server error. A change is never sent twice after a failure that may have been processed: an event whose creation timed out is reported, not created again. A request Google throttled was not carried out, so it is tried again whatever it is. `calendar_freebusy.query` is not tried again after a server error, because the transport goes by the verb and it is a `POST`; call it again yourself.

## What was confirmed, and what was not

Confirmed against Google's Calendar API reference and guides on 2026-10-10: the verb, path, parameters and accepted scopes of each of the ten methods; the fields of the event, calendar list and free/busy resources used here; how `createRequest` and `conferenceDataVersion` create a Meet link; that `patch` merges, replaces arrays whole and clears a field set to `null`; and the error statuses and reasons in the table above.

Not confirmed, because the documentation does not say or only a real account can show it:

- **Nothing here has been run against a real Google account.**
- **`respond`.** That an attendee who is not the organiser may change their own answer through `events.patch` with the `calendar.events` scope, and what Google does with the other guests in the list such an attendee sends back. That `events.patch` honours `If-Match`: Google documents the 412 for a stale ETag in its error guide, not on the method's page.
- **Replacing a guest list.** Whether Google keeps the answer of a guest who stays on the list when `patch` sends them without a `responseStatus`. The reference says only that the new array replaces the old one, so send the answer.
- **Clearing `date` or `dateTime` in a patch.** It follows from the documented patch rules; Google does not document it for `start` and `end` specifically.
- **`sendUpdates` left unset.** The reference gives the default as `false`, which is not one of the parameter's values. It is taken to mean `none`.
- **Free/busy keys.** That each calendar comes back under the id it was asked about by, `primary` included.
- **A token without the Calendar scopes.** Handled as any 403, so `AccessDenied` with Google's message; the exact reason Google gives was not checked.
- **Where Meet puts a recording.** That Google Meet attaches a meeting's recording, transcript and notes to its event is how the product is known to behave; the Calendar reference only says an event has `attachments`.
- **Limits.** The page sizes are from the reference (250 calendars, 2500 events). Google's request quotas were not looked into.

## Not supported yet

- **Incremental sync** with sync tokens, and **push notifications** (watch channels).
- **Moving or importing an event**, `quickAdd`, and replacing an event whole (`events.update`).
- **Creating, changing or deleting calendars**, sharing them (ACLs), settings and colours.
- **Adding attachments** to an event. Attachments are returned, not written.
- **Reminders, extended properties, working locations, focus time and out-of-office details** on an event.
- **Expanding a group** in a free/busy query.
- **Gmail, Meet, Drive, Docs and Sheets** beyond the Drive file lookup. Each is its own piece of work.
- **Identity without a Drive scope**, as said under Connect.

For anything in this list that is a plain REST call, the authenticated request still works:

```rust
use socketkit::RawRequest;
let colors = socket.request(key, RawRequest::get("calendar/v3/colors")).await?;
```
