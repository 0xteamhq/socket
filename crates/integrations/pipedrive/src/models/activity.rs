//! Activities: calls, meetings, tasks and emails logged against a record.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;
use super::{Address, SortDirection};

/// An activity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Activity {
    pub id: u64,
    pub subject: Option<String>,
    /// The kind of activity: `call`, `meeting`, `task`, `email`, `deadline`,
    /// `lunch`, or one the company added.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub done: Option<bool>,
    /// The day it is due, as `YYYY-MM-DD`.
    pub due_date: Option<String>,
    /// The time it is due, as `HH:MM`, in UTC.
    pub due_time: Option<String>,
    /// How long it takes, as `HH:MM`.
    pub duration: Option<String>,
    /// The user it is assigned to.
    pub owner_id: Option<u64>,
    pub creator_user_id: Option<u64>,
    pub deal_id: Option<u64>,
    pub lead_id: Option<String>,
    /// The first of the persons taking part.
    pub person_id: Option<u64>,
    pub org_id: Option<u64>,
    pub project_id: Option<u64>,
    /// Whether it shows the user as busy in their calendar.
    pub busy: Option<bool>,
    pub marked_as_done_time: Option<String>,
    pub location: Option<Address>,
    /// The persons taking part.
    #[serde(default, deserialize_with = "nullable")]
    pub participants: Vec<Participant>,
    /// Who was invited by email. Returned by `get` only: a row of a list
    /// does not carry the field at all, which is not "nobody was invited".
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Vec::is_empty")]
    pub attendees: Vec<Attendee>,
    pub conference_meeting_url: Option<String>,
    /// One of the choices of the `priority` field of activities, by its id.
    pub priority: Option<u64>,
    /// What was written about the activity, as HTML. Returned by `get`
    /// only: a row of a list does not carry the field at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// What invited people see about it. Returned by `get` only: a row of a
    /// list does not carry the field at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_description: Option<String>,
    pub add_time: Option<String>,
    pub update_time: Option<String>,
    pub is_deleted: Option<bool>,
}

/// A person taking part in an activity.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Participant {
    pub person_id: u64,
    /// Whether this is the person the activity is mainly with.
    pub primary: Option<bool>,
}

/// Someone invited to an activity by email.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Attendee {
    pub email: Option<String>,
    pub name: Option<String>,
    /// Their answer to the invitation.
    pub status: Option<String>,
    pub is_organizer: Option<bool>,
    /// The person they are in Pipedrive, when they are one.
    pub person_id: Option<u64>,
    /// The user they are in Pipedrive, when they are one.
    pub user_id: Option<u64>,
}

/// Which activities to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListActivities {
    /// Only the activities assigned to this user.
    pub owner_id: Option<u64>,
    /// Only the activities of this deal.
    pub deal_id: Option<u64>,
    /// Only the activities of this lead.
    pub lead_id: Option<String>,
    /// Only the activities mainly with this person.
    pub person_id: Option<u64>,
    /// Only the activities of this organisation.
    pub org_id: Option<u64>,
    /// Only those that are done, or only those that are not.
    pub done: Option<bool>,
    /// Only the activities a saved filter matches. Pipedrive then ignores the other filters.
    pub filter_id: Option<u64>,
    /// Only activities changed at or after this time, in RFC 3339.
    pub updated_since: Option<String>,
    /// Only activities changed before this time, in RFC 3339.
    pub updated_until: Option<String>,
    /// `id` (the default), `update_time`, `add_time` or `due_date`.
    pub sort_by: Option<String>,
    pub sort_direction: Option<SortDirection>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most activities to return in a page, from 1 to 500. Pipedrive returns 100 when not given.
    pub limit: Option<u32>,
}

/// An activity to create.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateActivity {
    pub subject: Option<String>,
    /// The kind of activity: `call`, `meeting`, `task`, `email`, `deadline`,
    /// `lunch`, or one the company added. Pipedrive's default when not given.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// The day it is due, as `YYYY-MM-DD`.
    pub due_date: Option<String>,
    /// The time it is due, as `HH:MM`, in UTC.
    pub due_time: Option<String>,
    /// How long it takes, as `HH:MM`.
    pub duration: Option<String>,
    /// Whether it is already done, as a logged call is.
    pub done: Option<bool>,
    /// The user it is assigned to. The signed-in user when not given.
    pub owner_id: Option<u64>,
    pub deal_id: Option<u64>,
    pub lead_id: Option<String>,
    pub org_id: Option<u64>,
    /// The persons taking part. The one marked `primary` is the activity's person.
    pub participants: Option<Vec<Participant>>,
    pub busy: Option<bool>,
    pub location: Option<Address>,
    pub priority: Option<u64>,
    /// What to write about the activity, as HTML.
    pub note: Option<String>,
    pub public_description: Option<String>,
}

/// What to change on an activity. A field that is not set is left as it is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateActivity {
    pub subject: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub due_date: Option<String>,
    pub due_time: Option<String>,
    pub duration: Option<String>,
    /// Marks the activity done, or not done.
    pub done: Option<bool>,
    pub owner_id: Option<u64>,
    pub deal_id: Option<u64>,
    pub lead_id: Option<String>,
    pub org_id: Option<u64>,
    /// The persons taking part. The list replaces the one that was there.
    pub participants: Option<Vec<Participant>>,
    pub busy: Option<bool>,
    pub location: Option<Address>,
    pub priority: Option<u64>,
    pub note: Option<String>,
    pub public_description: Option<String>,
}
