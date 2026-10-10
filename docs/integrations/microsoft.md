# Microsoft

**Status:** built and tested against a local server that answers as Microsoft's documentation says. Not yet run against the real Microsoft Graph.

Socket's Microsoft integration is one provider for everything behind Microsoft Graph: Outlook, Teams, OneDrive, SharePoint and Entra ID share one sign-in. Today it gives a program the Outlook calendar as 12 typed methods, and the same 12 as operations callable by name with JSON, plus identity and lookup of a sharing link. This page shows how to connect, lists everything that is supported, and says what was and was not confirmed against Microsoft's documentation.

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

A tenant that is not one of the keywords, an id or a domain is reported when the `Socket` is built, because the tenant is part of the address of the sign-in page.

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

**Changing an event.** `update` sends only the fields you set. `attendees` is the whole list: anyone left out is removed. To add one person, read the event, add them to its `attendees`, and send that list back; each attendee's `status` is Graph's to fill in and is left out of what is sent. Changing the `body` of an online meeting must keep the part that holds the join details, and a `body` must carry its `content`. An update with nothing set is refused.

**Answering an invitation.** `EventResponse` is `Accept`, `TentativelyAccept` or `Decline` (`accept`, `tentatively_accept`, `decline` in JSON). `RespondToEvent` has `comment`, `sendResponse` and `proposedNewTime`; another time can be proposed only with a decline or a tentative acceptance.

**Cancel or delete.** `cancel` is for a meeting the account organised: it tells the attendees and moves the event to Deleted Items; an attendee who calls it is refused by Graph. `delete` removes the event from the account's own calendar, and for a meeting the account organised it also sends a cancellation.

**Availability.** `find_meeting_times` and `schedule` read other people's calendars and change nothing. Both are for work and school accounts only. `schedule` reads at most 20 people, lists and rooms at once, over less than 62 days; an address Graph could not read comes back with an `error` in its place, not as a failure of the call.

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

`limit` is Graph's `$top` and applies to the first page; later pages keep it. A calendar view takes from 1 to 1000.

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
- **write** adds something: creating an event.
- **destructive** deletes, removes or overwrites what was there, or cannot be taken back: changing an event (which replaces its fields, and removes any attendee left out of a new list), answering an invitation (the organiser is told at once), cancelling a meeting, deleting an event.

| Operation | Effect | Scope | What it does |
| --- | --- | --- | --- |
| `microsoft.identity.get` | read |  | Return the account this connection is authorised as, confirming the token still works. |
| `microsoft.resource.resolve` | read |  | Confirm that a resource exists and the account can reach it. Accepts a OneDrive or SharePoint sharing link. |
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

`find_meeting_times` and `schedule` are reads that Graph offers only as POST. They are marked `read` because they change nothing, which is what the effect is for.

`update` and `respond` are marked `destructive`, not `write`, by the same rule the Slack and GitHub operations follow: an update overwrites, and an answer cannot be taken back. A host that asks a person only before a destructive operation therefore asks before both.

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

An input error never repeats the value you sent. It names the field when a required one is missing or a plain argument has the wrong type; for a wrong type inside the options it says only that a field has the wrong type. Keys that are not known are ignored, as they are for every provider, so check the spelling of an optional field that seems to have no effect. An answer that cannot be read is reported the same way: the error says where the unreadable value was, and neither its message nor its cause carries anything from the calendar.

A request sent as GET is retried on a throttle or a server error. Creating, changing, answering and cancelling are sent again in only two cases, both of which mean Graph did not carry them out: Graph throttled the request, or Graph rejected the access token and Socket renewed it to a different one. **If one of them fails with a server error, check before sending it again**; when creating, a `transactionId` of your own makes a second try safe, because Graph does not create a second event for one it has seen. The two reads sent as POST are not retried after a server error either.

**Deleting is the exception today.** The transport still repeats a DELETE after a server error. If the first try did delete the event, the second is answered "not found", and the call reports `NotFound` for a delete that worked. Treat `NotFound` from `delete` as "it is gone".

## Confirmed against Microsoft's documentation, and not

Everything here was read from learn.microsoft.com in October 2026. Nothing was run against a live tenant.

Confirmed:

- The authorise and token addresses, the four kinds of tenant, space-separated scopes, and that `offline_access` is what yields a refresh token.
- PKCE with `S256` is recommended for web applications and shown beside the client secret.
- A refresh returns a new refresh token, which replaces the old one.
- `login_hint`, and the four `prompt` values.
- The client secret is sent in the form body.
- The numbered codes `AADSTS65001`, `90008`, `90094` and `90095`, with the meanings above.
- Graph's error shape `{ "error": { "code", "message" } }`, and `Retry-After` in seconds on 429 and on 503.
- `GET /me` and its fields, with `User.Read`.
- `GET /shares/{encoded link}/driveItem`, the encoding (`u!`, then the link in base64url without padding), and `Files.ReadWrite` as its least permission.
- Paging: the whole of `@odata.nextLink` is requested as it is, and nothing is taken out of it.
- Every calendar endpoint above, with its verb, body and status: `GET /me/calendars` and `/me/calendars/{id}`; `GET /me/calendarView` and `/me/calendars/{id}/calendarView` with `startDateTime` and `endDateTime`; `GET /me/events/{id}` and `/me/events/{id}/instances`; `POST /me/findMeetingTimes`; `POST /me/calendar/getSchedule`; `POST /me/events` and `/me/calendars/{id}/events` (201); `PATCH /me/events/{id}`; `POST /me/events/{id}/accept`, `/tentativelyAccept`, `/decline` and `/cancel` (202, no body); `DELETE /me/events/{id}` (204).
- The fields of a calendar and of an event, with their spelling, and the values of `type`, `showAs`, `response` and `onlineMeetingProvider` given beside those fields in `models`.
- `isOnlineMeeting` with `onlineMeetingProvider: "teamsForBusiness"` returns `onlineMeeting.joinUrl`.
- `Prefer: outlook.timezone`, that Graph answers in UTC without it, and `originalStartTimeZone` and `originalEndTimeZone`.
- That an offset in `startDateTime` is honoured and a time without one is UTC; `$top` from 1 to 1000 on a calendar view.
- The limits of `getSchedule` (20 schedules, under 62 days, an interval of 5 to 1440 minutes), and that it and `findMeetingTimes` do not support personal accounts.

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

## Not supported yet

- **Application-only access** (client credentials), where no person signs in.
- **The national clouds** (US Government, China), which use other hosts.
- **Certificates** in place of a client secret.
- **The other products**: mail, Teams, meeting transcripts, files. Each is its own piece of work on top of this provider.
- **Other people's calendars** (`/users/{id}/…`) and **calendar groups**. Everything here is the signed-in account's.
- **Creating, renaming and deleting calendars.**
- **Change tracking** (`/delta`), and **receiving change notifications**.
- **Attachments, reminders, categories and forwarding** of an event.
- **Choosing the format of a body.** Graph returns HTML unless asked for text.

Anything Graph offers that has no method here can still be called through the generic request, with the token, retries and error handling applied:

```rust
let response = socket.request(key, RawRequest::get("me/messages").with_query("$top", "5")).await?;
```
