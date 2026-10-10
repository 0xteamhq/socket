//! Who joined a Teams online meeting, when, and for how long.

use socketkit_core::{Page, Result};

use super::Api;
use crate::models::{AttendanceRecord, AttendanceReport, Paging};

/// Who joined a Teams online meeting, when, and for how long.
#[derive(Debug, Clone, Copy)]
pub struct Attendance<'a>(pub(crate) Api<'a>);

impl Attendance<'_> {
    /// Lists a meeting's attendance reports, one for each time it was held.
    /// Microsoft returns the fifty most recent at most.
    pub async fn reports(&self, meeting: &str, paging: Paging) -> Result<Page<AttendanceReport>> {
        let meeting = self.0.segment("a meeting", meeting)?;
        let path = format!("me/onlineMeetings/{meeting}/attendanceReports");
        self.0.list(&path, &paging, "attendance reports").await
    }

    /// Lists who is in one attendance report: each person, their role, and
    /// every time they joined and left.
    pub async fn records(&self, meeting: &str, report: &str, paging: Paging) -> Result<Page<AttendanceRecord>> {
        let meeting = self.0.segment("a meeting", meeting)?;
        let report = self.0.segment("an attendance report", report)?;
        let path = format!("me/onlineMeetings/{meeting}/attendanceReports/{report}/attendanceRecords");
        self.0.list(&path, &paging, "attendance records").await
    }
}
