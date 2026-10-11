//! What the Meet groups share: how a caller names a thing, and where it is.
//!
//! Meet addresses everything by a resource name: `conferenceRecords/{id}`,
//! `conferenceRecords/{id}/transcripts/{id}`, `spaces/{id}`. A caller usually
//! holds the name, because that is what Google returned a moment ago, and
//! sometimes only the id. So every identifier is taken either way: the bare
//! id, or the whole name of that thing. Anything else is refused.
//!
//! This is not a group. It stands to the Meet groups as `mod.rs` stands to
//! all of them.

use socketkit_core::{ErrorKind, Result};

use super::{Api, MEET};

/// One level of a resource name: the collection Google files a thing under,
/// and what an error calls the thing.
pub(super) struct Level {
    collection: &'static str,
    what: &'static str,
}

const fn level(collection: &'static str, what: &'static str) -> Level {
    Level { collection, what }
}

pub(super) const RECORD: Level = level("conferenceRecords", "a conference record");
pub(super) const PARTICIPANT: Level = level("participants", "a participant");
pub(super) const TRANSCRIPT: Level = level("transcripts", "a transcript");
pub(super) const RECORDING: Level = level("recordings", "a recording");
pub(super) const SPACE: Level = level("spaces", "a space");

/// The address people join a meeting at. What follows it is the meeting code.
const JOIN: &str = "https://meet.google.com/";

/// What a caller gave for a meeting code or a space, without the join link
/// around it: `https://meet.google.com/abc-mnop-xyz?authuser=0` is
/// `abc-mnop-xyz`. Anything else is returned as it is, trimmed.
pub(super) fn without_link(given: &str) -> &str {
    let given = given.trim();
    match given.strip_prefix(JOIN) {
        Some(code) => code.split(['?', '#']).next().unwrap_or(code),
        None => given,
    }
}

impl Api<'_> {
    /// The ids of a thing and of what it lies in, outermost first, from
    /// what the caller gave for each level.
    ///
    /// Each is a bare id, or the whole name of that level: for a transcript,
    /// `conferenceRecords/{id}/transcripts/{id}`. A name of another
    /// collection, or of another depth, is refused, and so is a name that
    /// lies in something other than the levels given before it: a transcript
    /// of one conference record is not read as another's.
    pub(super) fn meet_ids<'g>(&self, given: &[(&Level, &'g str)]) -> Result<Vec<&'g str>> {
        let mut ids: Vec<&str> = Vec::with_capacity(given.len());
        for (depth, (level, value)) in given.iter().enumerate() {
            self.required(level.what, value)?;
            let value = value.trim();
            let id = if value.contains('/') {
                let parts: Vec<&str> = value.split('/').collect();
                let named = parts.len() == 2 * (depth + 1)
                    && parts
                        .chunks(2)
                        .zip(given)
                        .all(|(part, (level, _))| part[0] == level.collection && !part[1].is_empty());
                if !named {
                    return Err(self.error(
                        ErrorKind::InvalidInput,
                        format!("{} is its id, or its name as Google returned it", level.what),
                    ));
                }
                // The ids of the levels above it, which come at every other place.
                if parts
                    .iter()
                    .skip(1)
                    .step_by(2)
                    .zip(&ids)
                    .any(|(inner, outer)| inner != outer)
                {
                    return Err(self.error(
                        ErrorKind::InvalidInput,
                        format!(
                            "{} was given a name that lies outside what was given with it",
                            level.what
                        ),
                    ));
                }
                parts[parts.len() - 1]
            } else {
                value
            };
            ids.push(id);
        }
        Ok(ids)
    }

    /// The address of a thing in Meet, from what the caller gave for each
    /// level of its name. Each id is written as one segment of the path.
    pub(super) fn meet(&self, given: &[(&Level, &str)]) -> Result<String> {
        let mut path = String::from("v2");
        for ((level, _), id) in given.iter().zip(self.meet_ids(given)?) {
            path.push('/');
            path.push_str(level.collection);
            path.push('/');
            path.push_str(&self.segment(level.what, id)?);
        }
        Ok(self.on(MEET, &path))
    }
}
