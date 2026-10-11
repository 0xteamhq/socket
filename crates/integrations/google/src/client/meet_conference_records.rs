//! Conference records: the meetings that were held.
//!
//! Google takes one `filter` string in a syntax of its own. The options
//! here are typed, and the filter is built from them, so that nothing a
//! caller gives can end a quoted value early or add a clause.

use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::meet::{RECORD, SPACE, without_link};
use super::{Api, MEET};
use crate::models::{ConferenceRecord, MeetListConferenceRecords, Paging, meet_millis};

/// The most conference records Google returns in one page.
const MOST: u32 = 100;

/// The longest a meeting code is, by Google's account of it.
const LONGEST_CODE: usize = 128;

/// An option that was given. Blank text is an option not given.
fn set(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|value| !value.is_empty())
}

/// The meetings that were held in Meet.
#[derive(Debug, Clone, Copy)]
pub struct MeetConferenceRecords<'a>(pub(crate) Api<'a>);

impl MeetConferenceRecords<'_> {
    /// Lists the meetings the account organised, newest first, optionally
    /// only those of one meeting code or space, or that began between two
    /// times. Google keeps a record for 30 days after the meeting ended.
    pub async fn list(&self, options: MeetListConferenceRecords) -> Result<Page<ConferenceRecord>> {
        let filter = self.filter(&options)?;
        let paging = Paging {
            cursor: options.cursor,
            limit: options.limit,
        };
        let request = RawRequest::get(self.0.on(MEET, "v2/conferenceRecords"));
        let request = match filter {
            Some(filter) => request.with_query("filter", filter),
            None => request,
        };
        let request = self.0.paged(request, &paging, "pageSize", MOST)?;
        self.0
            .page(self.0.send(request).await?, "conferenceRecords", "conference records")
    }

    /// Gets one conference record: when the meeting began and ended, and
    /// the space it was held in.
    pub async fn get(&self, record: &str) -> Result<ConferenceRecord> {
        let body = self.0.send(RawRequest::get(self.0.meet(&[(&RECORD, record)])?)).await?;
        let record: ConferenceRecord = self.0.decode(body, "a conference record")?;
        if record.name.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "google answered without a conference record"));
        }
        Ok(record)
    }

    /// Google's filter for `options`, or `None` when they ask for everything.
    ///
    /// The clauses are written as Google's reference writes them, and joined
    /// with `AND`. Every value is checked to be what its field holds before
    /// it is quoted: a meeting code is letters, digits and hyphens, a time
    /// is an RFC 3339 timestamp, and a space id has no quote, backslash or
    /// control character in it. So a value stays inside its quotes.
    fn filter(&self, options: &MeetListConferenceRecords) -> Result<Option<String>> {
        let invalid = |message: &str| self.0.error(ErrorKind::InvalidInput, message);
        let mut clauses = Vec::new();

        let (code, space) = (set(&options.meeting_code), set(&options.space));
        if code.is_some() && space.is_some() {
            return Err(invalid("`meetingCode` and `space` cannot be given together"));
        }
        if let Some(code) = code.map(without_link) {
            let typed = |c: char| c.is_ascii_alphanumeric() || c == '-';
            if code.is_empty() || code.len() > LONGEST_CODE || !code.chars().all(typed) {
                return Err(invalid(
                    "`meetingCode` is a code such as `abc-mnop-xyz`, or the link that ends in it",
                ));
            }
            // A code is typed in any case; Google writes it in lower case.
            clauses.push(format!("space.meeting_code = \"{}\"", code.to_ascii_lowercase()));
        }
        if let Some(space) = space {
            let id = self.0.meet_ids(&[(&SPACE, space)])?[0];
            if id.chars().any(|c| c == '"' || c == '\\' || c.is_control()) {
                return Err(invalid("`space` is not a space's name or id"));
            }
            clauses.push(format!("space.name = \"spaces/{id}\""));
        }

        let earliest = self.time(&options.start_time_min, "startTimeMin")?;
        let latest = self.time(&options.start_time_max, "startTimeMax")?;
        if matches!((earliest, latest), (Some((_, earliest)), Some((_, latest))) if earliest > latest) {
            return Err(invalid("`startTimeMin` is after `startTimeMax`"));
        }
        if let Some((text, _)) = earliest {
            clauses.push(format!("start_time>=\"{text}\""));
        }
        if let Some((text, _)) = latest {
            clauses.push(format!("start_time<=\"{text}\""));
        }

        Ok((!clauses.is_empty()).then(|| clauses.join(" AND ")))
    }

    /// A time a caller gave as a bound: as it was written, and as a number
    /// to compare it by. Only a timestamp is let through, so what goes into
    /// the filter is digits and the marks a timestamp is written with.
    fn time<'t>(&self, value: &'t Option<String>, field: &str) -> Result<Option<(&'t str, i64)>> {
        let Some(text) = set(value) else {
            return Ok(None);
        };
        match meet_millis(text) {
            Some(at) => Ok(Some((text, at))),
            None => Err(self.0.error(
                ErrorKind::InvalidInput,
                format!("`{field}` is a time in RFC 3339, such as `2026-10-01T00:00:00Z`"),
            )),
        }
    }
}
