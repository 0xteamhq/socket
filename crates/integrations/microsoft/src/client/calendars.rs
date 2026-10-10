//! The account's calendars.

use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::Api;
use crate::models::{Calendar, Paging};

/// The account's calendars.
#[derive(Debug, Clone, Copy)]
pub struct Calendars<'a>(pub(crate) Api<'a>);

impl Calendars<'_> {
    /// Lists the account's calendars, its own and those shared with it.
    pub async fn list(&self, paging: Paging) -> Result<Page<Calendar>> {
        self.0.page(RawRequest::get("me/calendars"), &paging, "calendars").await
    }

    /// Gets one calendar.
    pub async fn get(&self, calendar: &str) -> Result<Calendar> {
        let calendar = self.0.segment("a calendar id", calendar)?;
        let body = self.0.send(RawRequest::get(format!("me/calendars/{calendar}"))).await?;
        let calendar: Calendar = self.0.decode(body, "a calendar")?;
        if calendar.id.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "microsoft answered without a calendar"));
        }
        Ok(calendar)
    }
}
