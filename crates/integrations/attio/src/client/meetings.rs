//! Meetings.

use socketkit_core::{Page, RawRequest, Result};

use super::{Api, asking, filled};
use crate::models::{ListMeetings, Meeting};

/// The meetings Attio knows of, from calendars and integrations. Attio
/// marks this part of its API as beta.
#[derive(Debug, Clone, Copy)]
pub struct Meetings<'a>(pub(crate) Api<'a>);

impl Meetings<'_> {
    /// Lists meetings: all of them, or those linked to one record, with
    /// given people, or within a span of time.
    ///
    /// Attio answers this only for a token that acts for the whole workspace.
    pub async fn list(&self, meetings: ListMeetings) -> Result<Page<Meeting>> {
        let (object, record) = (meetings.linked_object.as_deref(), meetings.linked_record_id.as_deref());
        self.0
            .together(("linked_object", object), ("linked_record_id", record))?;
        // Attio returns 50 unless told otherwise, and at most 200.
        let request = self.0.after(
            RawRequest::get("meetings"),
            meetings.cursor.as_deref(),
            meetings.limit,
            200,
        )?;
        let participants: Vec<&str> = meetings
            .participants
            .iter()
            .flatten()
            .filter_map(|address| filled(Some(address)))
            .collect();
        let request = asking(request, "linked_object", filled(object));
        let request = asking(request, "linked_record_id", filled(record));
        let request = asking(
            request,
            "participants",
            (!participants.is_empty()).then(|| participants.join(",")),
        );
        let request = asking(request, "sort", meetings.sort.map(|sort| sort.as_str()));
        let request = asking(request, "ends_from", filled(meetings.ends_from.as_deref()));
        let request = asking(request, "starts_before", filled(meetings.starts_before.as_deref()));
        let request = asking(request, "timezone", filled(meetings.timezone.as_deref()));
        self.0.cursor_page(request, "meetings").await
    }

    /// Gets one meeting.
    pub async fn get(&self, meeting: &str) -> Result<Meeting> {
        let meeting = self.0.segment("a meeting id", meeting)?;
        self.0
            .one(RawRequest::get(format!("meetings/{meeting}")), "a meeting")
            .await
    }
}
