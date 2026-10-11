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

- **It stops at `maxEntries`**, 1,000 unless another number from 1 to 10,000 is given, and sets `truncated` when the transcript has more. Ask again with a larger number, or page through `entries`. It never reads more than twice the pages that many entries would fill, plus two, nor more than 40 pages of participants.
- **The speaker's name is the participant's display name**: a signed-in person's first and last name, the name a guest typed when joining without signing in, or a caller's partly hidden phone number. A guest's name is whatever they typed; nobody checked it. Someone who left and came back is one participant with several sessions, and has one name.
- **`speaker` is absent** when Meet withholds the name ("for privacy reasons, profile information might not be available for all participants"), when the entry names no participant, or when the participant is not in the list.
- **A name is written on one line, and so is each entry in `text`**, so that a line break in a name or in speech cannot pass for another person's line. `entries[].text` is as Meet sent it.
- **A time that cannot be read is an error**, never a shorter transcript: the transcript's `startTime`, or an entry's `startTime` or `endTime`. The error names the field and the entry's place, and does not repeat what was said.
- **The entries may differ from the Doc.** Google says the entries "might not match the transcription found in the Docs transcript file", when the Doc was changed after it was written.

**The Doc and the file.** A transcript's `docsDestination` has `document`, the Google Doc's id, and `exportUri`, the address that opens it. A recording's `driveDestination` has `file`, the Drive file's id of an MP4, and `exportUri`, the address that plays it. Google has them once the file has been written, which `state` says with `FILE_GENERATED`; before that they may be absent. Reading the Doc or the video is a Drive or Docs read, with its own scope.

**Limits Google sets, which callers will meet:**

- **A transcript or a recording exists only if it was switched on** before the meeting ended; otherwise the list is empty. A transcript does not need a recording.
- **Transcripts are part of some Google Workspace editions only.** Google's help page lists Business Standard and Plus, Enterprise Starter, Standard and Plus, Teaching and Learning Upgrade, Education Plus, and Workspace Individual, and eight languages.
- **Entries are kept for 30 days.** Google deletes a transcript's entries 30 days after the meeting ended, and the conference record itself at its `expireTime`, also 30 days after the end. The Google Doc and the recording stay in the organiser's Drive under Drive's own rules, so after that the Doc is the only transcript there is.
- **Who may read.** A meeting's organiser and its participants can read its conference record, participants, transcripts and recordings. But `meet_conference_records.list` returns only the meetings the account organised; for a meeting someone else organised, start from its space or its record's name. The Doc and the file belong to the organiser, and Drive decides who else may open them.
- **Errors.** A record that has expired, or a meeting the account was not in, is `NotFound` or `AccessDenied`, as Google answers. Without the scope, or with the Meet API not enabled in the Google Cloud project, every call is `AccessDenied` with Google's reason.
- **Page sizes.** `limit` is from 1 to 100, or to 250 for participants and sessions. Google would lower a larger number by itself; Socket refuses it, so that a page is never smaller than was asked for without saying so.

#### Confirmed against Google's documentation, and not

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

#### Not supported yet

- **Gemini's notes** (`conferenceRecords.smartNotes`). Until then they are reached as the Doc attached to the calendar event, through Calendar and Drive.
- **Creating, changing and ending spaces**, and a space's members. Out of scope: this integration only reads Meet.
- **Reading the Doc of a transcript or the video of a recording.** Meet gives the ids; Drive and Docs read them.
- **One participant session, or one transcript entry, by name** (`participantSessions.get`, `entries.get`). The lists return the same fields.
- **Filtering participants and sessions** (`latest_end_time IS NULL` for who is there now), and conference records still going on (`end_time IS NULL`).
- **A space's phone numbers, PINs and SIP addresses** (`phoneAccess`, `gatewaySipAccess`), and its moderation restrictions. They are not read into `MeetSpace`.
- **Continuing `read` past 10,000 entries.** Page through `entries` for a transcript that long.
- **Events** when a meeting starts or a transcript is ready (the Workspace Events API).
