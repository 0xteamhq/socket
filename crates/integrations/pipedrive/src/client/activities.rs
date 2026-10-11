//! Activities. Version 2 of Pipedrive's API throughout.

use socketkit_core::{Page, RawRequest, Result};

use super::{Api, body, filtered};
use crate::models::{Activity, CreateActivity, ListActivities, Paging, UpdateActivity};

/// Activities: calls, meetings, tasks and emails logged against a deal, a
/// lead, a person or an organisation.
#[derive(Debug, Clone, Copy)]
pub struct Activities<'a>(pub(crate) Api<'a>);

impl Activities<'_> {
    /// Lists activities: all that are not deleted, or those an owner, a
    /// deal, a lead, a person, an organisation or being done selects.
    ///
    /// A row leaves out what was written about the activity (`note`), what
    /// invited people see (`public_description`) and who was invited
    /// (`attendees`). Those fields are absent from a row, not empty; `get`
    /// returns all three.
    pub async fn list(&self, options: ListActivities) -> Result<Page<Activity>> {
        let paging = Paging {
            cursor: options.cursor.clone(),
            limit: options.limit,
        };
        let request = filtered(RawRequest::get("v2/activities"), &options);
        let mut page: Page<Activity> = self.0.page(request, &paging, "activities").await?;
        for activity in &mut page.items {
            activity.note = None;
            activity.public_description = None;
            activity.attendees.clear();
        }
        Ok(page)
    }

    /// Gets one activity, with its note, its public description and who was invited.
    pub async fn get(&self, activity: u64) -> Result<Activity> {
        // Pipedrive leaves the invited people out unless they are asked for.
        let request = RawRequest::get(format!("v2/activities/{activity}")).with_query("include_fields", "attendees");
        self.0.one(request, "an activity").await
    }

    /// Creates an activity: a task to do, or a call or a meeting that was held.
    pub async fn create(&self, activity: CreateActivity) -> Result<Activity> {
        let request = RawRequest::post("v2/activities", body(&activity));
        self.0.one(request, "an activity").await
    }

    /// Changes an activity, or marks it done. What is not set is left as it is.
    pub async fn update(&self, activity: u64, change: UpdateActivity) -> Result<Activity> {
        let request = self.0.change(format!("v2/activities/{activity}"), &change)?;
        self.0.one(request, "an activity").await
    }

    /// Deletes an activity. Pipedrive keeps it for 30 days and then removes it for good.
    pub async fn delete(&self, activity: u64) -> Result<()> {
        let request = RawRequest::new("DELETE", format!("v2/activities/{activity}"));
        self.0.done(request, "a deleted activity").await
    }
}
