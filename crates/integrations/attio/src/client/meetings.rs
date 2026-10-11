//! Meetings.

use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{A_PAGE, Api};
use crate::models::{ListMeetings, Meeting, Paging};

/// Meetings: calendar events Attio knows of. Attio marks this part of its
/// API as beta.
#[derive(Debug, Clone, Copy)]
pub struct Meetings<'a>(pub(crate) Api<'a>);

impl Meetings<'_> {
    /// Lists meetings, earliest first unless asked otherwise: all of them,
    /// those in a range of time, those that concern a record, or those with
    /// certain people in them. At most 200 a page.
    pub async fn list(&self, options: ListMeetings) -> Result<Page<Meeting>> {
        let invalid = |message: &str| self.0.error(ErrorKind::InvalidInput, message);
        let object = self
            .0
            .optional_id("a linked object", options.linked_object.as_deref())?;
        let record = self
            .0
            .optional_id("a linked record id", options.linked_record_id.as_deref())?;
        let mut request = RawRequest::get("meetings");
        match (object, record) {
            (Some(object), Some(record)) => {
                request = request
                    .with_query("linked_object", object)
                    .with_query("linked_record_id", record);
            }
            (None, None) => {}
            _ => {
                return Err(invalid(
                    "`linked_object` and `linked_record_id` are given together or not at all",
                ));
            }
        }
        if let Some(people) = &options.participants {
            // Attio takes the addresses in one parameter, with commas between them.
            let people: Vec<&str> = people.iter().map(|person| person.trim()).collect();
            if people.iter().any(|person| person.is_empty() || person.contains(',')) {
                return Err(invalid("every one of `participants` is an email address"));
            }
            if !people.is_empty() {
                request = request.with_query("participants", people.join(","));
            }
        }
        if let Some(sort) = options.sort {
            request = request.with_query("sort", sort.as_str());
        }
        for (name, value) in [
            ("ends_from", &options.ends_from),
            ("starts_before", &options.starts_before),
            ("timezone", &options.timezone),
        ] {
            if let Some(value) = value.as_deref().map(str::trim) {
                // A bound left blank would be dropped, and the list would
                // then be of every meeting and not of a range.
                if value.is_empty() {
                    return Err(invalid(&format!("`{name}` was given and is blank")));
                }
                request = request.with_query(name, value);
            }
        }
        let paging = Paging {
            cursor: options.cursor,
            limit: options.limit,
        };
        self.0.cursor_page(request, &paging, (A_PAGE, 200), "meetings").await
    }

    /// Gets one meeting.
    pub async fn get(&self, meeting: &str) -> Result<Meeting> {
        let meeting = self.0.id("a meeting id", meeting)?;
        let meeting: Meeting = self
            .0
            .one(RawRequest::get(format!("meetings/{meeting}")), "a meeting")
            .await?;
        if meeting.id.meeting_id.is_empty() {
            return Err(self.0.missing("a meeting"));
        }
        Ok(meeting)
    }
}
