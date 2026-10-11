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
| `calendar_events.respond(calendar, event, EventResponse)` | `CalendarEvent`: the event with the calendar owner's answer set | write | `calendar.events` |
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

**Times.** `timeMin`, `timeMax` and `updatedMin` are RFC 3339 with the offset: `2026-10-12T00:00:00Z` or `2026-10-12T09:00:00-07:00`. Google refuses a time without one, so Socket refuses it first and names the field. An event's `start` and `end` each hold either `dateTime` or, for an all-day event, `date`, never both. A `dateTime` carries its offset too, unless a `timeZone` is given beside it (`EventTime::in_zone("2026-10-12T09:00:00", "Europe/Zurich")`), which is the form a recurring event needs. Ends are exclusive: an all-day event on the 12th ends on the 13th.

**Listing.** `EventFilter` has `timeMin`, `timeMax`, `q`, `singleEvents`, `orderBy`, `updatedMin`, `showDeleted` and `timeZone`. `q` is free text, matched against titles, descriptions, locations and the people on an event. `orderBy` is `startTime` or `updated`, both oldest first. `startTime` works only with `singleEvents: true`, and is refused without it before Google is called. Google returns 250 events a page unless `limit` says otherwise, up to 2500; calendars come 100 a page, up to 250. A limit outside that range is refused. A page may hold fewer events than `limit`, or none, and still be followed by another: only a missing `next_cursor` means the end.

**Recurring events.** Without `singleEvents`, a recurring event comes back once, as the series, with its `recurrence` rules. With it, every occurrence in the window comes back as its own event carrying `recurringEventId`. `instances` lists the occurrences of one series. An occurrence that was removed from its series arrives with little more than `id`, `status: "cancelled"`, `recurringEventId` and `originalStartTime`.

**Meet links.** `createMeetLink` on `insert` or `patch` asks Google to create one. Socket sends `conferenceData.createRequest` with a request id of its own, 128 random bits, new on every call, and `conferenceDataVersion=1`, without which Google ignores the request. Google may still be creating the link when it answers: then `conferenceData.createRequest.status.statusCode` is `pending` and `hangoutLink` is unset. Read the event again.

**Who is emailed.** `sendUpdates` is `all`, `externalOnly` or `none`, on `insert`, `patch`, `respond` and `delete`. Left unset, it is not sent, and Google emails nobody.

**Changing an event.** `patch` changes only the fields given, and a patch with nothing in it is refused. It is marked destructive because what it changes is overwritten: `attendees`, when given, replaces the whole guest list, and anyone left out is uninvited. To add one person, read the event and send everyone who stays, each with the `responseStatus` they already gave. Moving an event between all-day and timed works; Socket clears the form that is no longer used.

**Answering an invitation.** Google has no call for an answer alone: it is a change to the event's guest list, and a change replaces the list. `respond` therefore reads the event, writes the answer into the entry Google marks as the calendar's own (`self`), and sends the list back with everyone else exactly as they were, including fields Socket does not describe. It names the version it read (`If-Match`), so if someone changed the event in between, Google refuses the write instead of losing their change. That arrives as `InvalidInput` and is not tried again by itself; call `respond` again. A calendar whose owner is not on the guest list is refused before anything is written. `responseStatus` is `accepted`, `declined`, `tentative`, or `needsAction` to take an answer back, which is why `respond` is a write and not destructive.

**A hidden guest list.** When the organiser hid the guests from each other (`guestsCanSeeOtherGuests: false`), or Google cut the list short (`attendeesOmitted: true`), the list that was read is not the whole of it, and sending it back as the list would uninvite everyone it leaves out. `respond` then sends only the calendar's own entry with `attendeesOmitted: true`, which is how Google is told that the list is partial and only the answer is to change.

**Whose answer it is.** `self` marks the calendar the event was read from, not whoever is signed in. On `primary` the two are the same person. On a calendar the signed-in person manages for someone else, `respond` answers for that calendar's owner. A host that asks a person to approve the call should show them `calendar`.

**Deleting.** `delete` removes the event, and for an event the account organised it cancels it for the guests. An event that is already gone is answered with 410, which arrives as `NotFound`: treat it as deleted. The transport repeats a `DELETE` after a server error, so a delete that Google carried out but failed to confirm can come back as `NotFound` from the second attempt. That too means the event is gone.

**Free/busy.** `calendars` are calendar ids, or `primary`; at least one, and none of them blank. A calendar Google could not answer for, such as one the account may not see, comes back with `errors` set and no busy periods. That is not the same as free: check `errors` before reading `busy`. An answer that names no calendar at all is an error, not an empty day. Google takes this read only as a `POST`, so unlike the other reads it is not tried again after a server error; call it again yourself.

**What an error means.** A 403 with the reason `rateLimitExceeded` or `userRateLimitExceeded` is a throttle, `RateLimited`, and is tried again. Any other 403 is `AccessDenied` with Google's reason: the token lacks the scope, the account may not change this calendar, a guest tried to change what only the organiser may (`forbiddenForNonOrganizer`), or Google's usage limit was reached (`quotaExceeded`). A 410 with `updatedMinTooLongAgo` is `InvalidInput`: `updatedMin` is further back than Google keeps changes. An answer that says success without the event, the list or the busy times is `Decode`, and nothing should be assumed done.

#### Confirmed against Google's documentation, and not

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
- **Replacing a guest list.** Whether Google keeps the answer of a guest who stays on the list when `patch` sends them without a `responseStatus`. The reference says only that the new array replaces the old one, so send the answer.
- **Clearing `date` or `dateTime` in a patch.** It follows from the patch rules; Google does not document it for `start` and `end`.
- **`sendUpdates` left unset.** The reference gives the default of `insert` as `false`, which is not one of the parameter's values, and gives none for `patch` and `delete`. It is taken to mean `none`.
- **The status of a delete.** The reference says the answer has no body, not which status it has. Any success is taken.
- **What a delete does to the guests.** That deleting an event the account organised cancels it for its guests is how the product is known to behave; the reference says only that the event is deleted.
- **Free/busy keys.** That each calendar comes back under the id it was asked about by, `primary` included.
- **Where Meet puts a recording.** That Google Meet attaches a meeting's recording, transcript and notes to its event is how the product is known to behave; the Calendar reference only says an event has `attachments`.
- **Limits.** Google's request quotas were not looked into. A free/busy query is documented to answer for at most 50 calendars; what it does with more was not checked, and Socket does not refuse them.

#### Not supported yet

- **Incremental sync** with sync tokens (`syncToken`, `nextSyncToken`), which the issue leaves for later, and **push notifications** (watch channels). To follow changes today, list with `updatedMin`.
- **Moving or importing an event**, `quickAdd`, and replacing an event whole (`events.update`).
- **Creating, changing or deleting calendars**, sharing them (ACLs), settings and colours.
- **Adding attachments** to an event. Attachments are returned, not written.
- **Reminders, extended properties, working locations, focus time and out-of-office details** on an event, and the list filters that go with them (`eventTypes`, `iCalUID`, `privateExtendedProperty`, `sharedExtendedProperty`, `showHiddenInvitations`, `maxAttendees`).
- **Expanding a group** in a free/busy query (`groupExpansionMax`, `calendarExpansionMax`).
- **A conference other than Google Meet**, and copying an existing conference onto another event.
- **Narrower scopes.** Google also accepts `calendar.events.readonly`, `calendar.calendarlist.readonly`, `calendar.freebusy` and `calendar.events.owned` for parts of this. The operations name `calendar.readonly` and `calendar.events`.
