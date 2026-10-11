//! Who joined a Teams online meeting, when, and for how long.

use socketkit_core::{Page, RawRequest, Result};

use super::Api;
use crate::models::{AttendanceRecord, AttendanceReport, Paging};

/// Who joined a Teams online meeting, when, and for how long.
#[derive(Debug, Clone, Copy)]
pub struct Attendance<'a>(pub(crate) Api<'a>);

impl Attendance<'_> {
    /// Lists a meeting's attendance reports, one for each time it was held.
    pub async fn reports(&self, meeting: &str, paging: Paging) -> Result<Page<AttendanceReport>> {
        let meeting = self.0.segment("a meeting id", meeting)?;
        let request = RawRequest::get(format!("me/onlineMeetings/{meeting}/attendanceReports"));
        self.0.page(request, &paging, "attendance reports").await
    }

    /// Lists who is in one attendance report: each person, their role, and
    /// every time they joined and left.
    pub async fn records(&self, meeting: &str, report: &str, paging: Paging) -> Result<Page<AttendanceRecord>> {
        let meeting = self.0.segment("a meeting id", meeting)?;
        let report = self.0.segment("an attendance report id", report)?;
        let path = format!("me/onlineMeetings/{meeting}/attendanceReports/{report}/attendanceRecords");
        self.0.page(RawRequest::get(path), &paging, "attendance records").await
    }
}
