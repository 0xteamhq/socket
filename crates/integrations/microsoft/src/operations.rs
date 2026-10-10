//! Every typed method as a named operation.
//!
//! An operation's input is a JSON object with the method's plain arguments
//! and its options side by side: `{ "meeting": "MSpk…", "top": 10 }`. Both
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

use crate::Microsoft;
use crate::models::{
    AttendanceRecord, AttendanceReport, OnlineMeeting, Paging, Recording, Transcript, TranscriptContent,
};

type Running = std::pin::Pin<Box<dyn Future<Output = Result<Value>> + Send>>;

/// One named operation: what it says about itself, and how to run it.
pub(crate) struct Operation {
    pub(crate) info: OperationInfo,
    run: Box<dyn Fn(Microsoft, Connection, Value) -> Running + Send + Sync>,
}

impl Operation {
    pub(crate) fn run(&self, microsoft: Microsoft, connection: Connection, input: Value) -> Running {
        (self.run)(microsoft, connection, input)
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
    F: Fn(Microsoft, Connection, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<O>> + Send + 'static,
{
    let info = OperationInfo {
        name: format!("microsoft.{name}"),
        description: description.to_owned(),
        input_schema: schema_of::<I>(),
        output_schema: schema_of::<O>(),
        effect,
        required_scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
    };
    let run = move |microsoft: Microsoft, connection: Connection, input: Value| -> Running {
        let provider = connection.provider().id.clone();
        match serde_path_to_error::deserialize::<_, I>(input) {
            Err(e) => {
                // serde's own message quotes the offending value, which may be
                // a join link or a credential. Only the field's name, which
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
                let output = handler(microsoft, connection, input);
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

input!(OneMeeting {
    /// The id of an online meeting, as `online_meetings.find_by_join_url` returns it.
    meeting: String
});
input!(JoinUrl {
    /// The meeting's join link, exactly as the calendar event has it.
    join_url: String
});
input!(
    InMeeting {
        /// The id of an online meeting.
        meeting: String
    } + Paging
);
input!(OneTranscript {
    /// The id of an online meeting.
    meeting: String,
    /// The id of one of the meeting's transcripts.
    transcript: String
});
input!(OneRecording {
    /// The id of an online meeting.
    meeting: String,
    /// The id of one of the meeting's recordings.
    recording: String
});
input!(
    InReport {
        /// The id of an online meeting.
        meeting: String,
        /// The id of one of the meeting's attendance reports.
        report: String
    } + Paging
);

// Everything here reads. A host lets a read run without asking a person, so
// nothing that changes Microsoft's data may be added with this effect.
use Effect::Read;

const MEETINGS: &[&str] = &["OnlineMeetings.Read"];
const TRANSCRIPTS: &[&str] = &["OnlineMeetingTranscript.Read.All"];
const RECORDINGS: &[&str] = &["OnlineMeetingRecording.Read.All"];
const ATTENDANCE: &[&str] = &["OnlineMeetingArtifact.Read.All"];

/// Every Microsoft operation except identity, which the integration adds itself.
pub(crate) fn all() -> &'static [Operation] {
    static ALL: OnceLock<Vec<Operation>> = OnceLock::new();
    ALL.get_or_init(build)
}

#[rustfmt::skip]
fn build() -> Vec<Operation> {
    vec![
        // ── online meetings ──
        operation("online_meetings.get", "Get one Teams online meeting by its id.", Read, MEETINGS,
            |m: Microsoft, c: Connection, i: OneMeeting| async move { m.online_meetings(&c).get(&i.meeting).await as Result<OnlineMeeting> }),
        operation("online_meetings.find_by_join_url", "Find the Teams online meeting behind a join link from a calendar event. Returns its id, which transcripts, recordings and attendance are asked for by.", Read, MEETINGS,
            |m: Microsoft, c: Connection, i: JoinUrl| async move { m.online_meetings(&c).find_by_join_url(&i.join_url).await as Result<OnlineMeeting> }),

        // ── transcripts ──
        operation("transcripts.list", "List a meeting's transcripts. Empty when transcription was never switched on.", Read, TRANSCRIPTS,
            |m: Microsoft, c: Connection, i: InMeeting| async move { m.transcripts(&c).list(&i.meeting, i.options).await as Result<Page<Transcript>> }),
        operation("transcripts.get", "Get one transcript's details: when it was made, and by whose meeting.", Read, TRANSCRIPTS,
            |m: Microsoft, c: Connection, i: OneTranscript| async move { m.transcripts(&c).get(&i.meeting, &i.transcript).await as Result<Transcript> }),
        operation("transcripts.content", "Read what was said in a meeting: the transcript's text, and one entry for each thing said with the speaker, the start and the end.", Read, TRANSCRIPTS,
            |m: Microsoft, c: Connection, i: OneTranscript| async move { m.transcripts(&c).content(&i.meeting, &i.transcript).await as Result<TranscriptContent> }),

        // ── recordings ──
        operation("recordings.list", "List a meeting's recordings. Empty when the meeting was not recorded.", Read, RECORDINGS,
            |m: Microsoft, c: Connection, i: InMeeting| async move { m.recordings(&c).list(&i.meeting, i.options).await as Result<Page<Recording>> }),
        operation("recordings.get", "Get one recording's details, with the address its video is at.", Read, RECORDINGS,
            |m: Microsoft, c: Connection, i: OneRecording| async move { m.recordings(&c).get(&i.meeting, &i.recording).await as Result<Recording> }),

        // ── attendance ──
        operation("attendance.reports", "List a meeting's attendance reports, one for each time it was held.", Read, ATTENDANCE,
            |m: Microsoft, c: Connection, i: InMeeting| async move { m.attendance(&c).reports(&i.meeting, i.options).await as Result<Page<AttendanceReport>> }),
        operation("attendance.records", "List who joined a meeting, in what role, when, and for how long.", Read, ATTENDANCE,
            |m: Microsoft, c: Connection, i: InReport| async move { m.attendance(&c).records(&i.meeting, &i.report, i.options).await as Result<Page<AttendanceRecord>> }),
    ]
}
