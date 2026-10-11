//! Tasks: what Attio returns for them, and the content used to create and change them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Actor;
use super::nullable::{given, nullable};

/// Something to be done, with the records it is about and who is to do it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Task {
    pub id: TaskId,
    #[serde(deserialize_with = "nullable")]
    pub content_plaintext: String,
    pub deadline_at: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub is_completed: bool,
    pub completed_at: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub linked_records: Vec<LinkedRecord>,
    #[serde(deserialize_with = "nullable")]
    pub assignees: Vec<Assignee>,
    pub created_by_actor: Option<Actor>,
    pub created_at: Option<String>,
}

/// A task's id, with the workspace it belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct TaskId {
    #[serde(deserialize_with = "nullable")]
    pub workspace_id: String,
    #[serde(deserialize_with = "nullable")]
    pub task_id: String,
}

/// A record a task is about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct LinkedRecord {
    /// The id of the object the record belongs to. A slug is also taken when writing.
    #[serde(deserialize_with = "nullable")]
    pub target_object_id: String,
    #[serde(deserialize_with = "nullable")]
    pub target_record_id: String,
}

/// Someone a task is assigned to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Assignee {
    /// `workspace-member`: a task can be assigned to nobody else.
    #[serde(deserialize_with = "nullable")]
    pub referenced_actor_type: String,
    /// The workspace member's id.
    #[serde(deserialize_with = "nullable")]
    pub referenced_actor_id: String,
}

/// The order tasks are listed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum TaskSort {
    #[serde(rename = "created_at:asc")]
    CreatedAtAsc,
    #[serde(rename = "created_at:desc")]
    CreatedAtDesc,
    #[serde(rename = "completed_at:asc")]
    CompletedAtAsc,
    #[serde(rename = "completed_at:desc")]
    CompletedAtDesc,
}

impl TaskSort {
    /// The name Attio knows the order by.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CreatedAtAsc => "created_at:asc",
            Self::CreatedAtDesc => "created_at:desc",
            Self::CompletedAtAsc => "completed_at:asc",
            Self::CompletedAtDesc => "completed_at:desc",
        }
    }
}

/// Which tasks to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListTasks {
    /// The slug or id of an object, to list the tasks about one of its
    /// records. Given together with `linked_record_id`.
    pub linked_object: Option<String>,
    /// The id of the record whose tasks to list. Given together with `linked_object`.
    pub linked_record_id: Option<String>,
    /// The id or the email address of a workspace member, to list the tasks
    /// assigned to them, or the word `null` for the tasks assigned to nobody.
    pub assignee: Option<String>,
    /// `true` for the tasks that are done, `false` for those that are not.
    pub is_completed: Option<bool>,
    /// The order. Oldest first when not given.
    pub sort: Option<TaskSort>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most tasks to return, from 1 to 500. 50 when not given.
    pub limit: Option<u32>,
}

/// A new task.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateTask {
    /// What is to be done, as plain text of at most 2000 characters.
    pub content: String,
    /// When it is due, in ISO 8601. No deadline when not given.
    pub deadline_at: Option<String>,
    /// Whether it is already done. Not done when not given.
    pub is_completed: Option<bool>,
    /// The records the task is about, in the forms Attio takes: `{
    /// "target_object": "people", "target_record_id": "…" }`, or an email
    /// address or a company's domain as a string.
    pub linked_records: Option<Vec<Value>>,
    /// Who is to do it, in the forms Attio takes: `{
    /// "referenced_actor_type": "workspace-member", "referenced_actor_id":
    /// "…" }`, or `{ "workspace_member_email_address": "…" }`.
    pub assignees: Option<Vec<Value>>,
}

impl CreateTask {
    /// A task with only its text set.
    pub fn saying(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            ..Self::default()
        }
    }
}

/// What to change on a task. What is left unset is left as it is. Attio does
/// not let a task's text be changed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateTask {
    /// When it is due, in ISO 8601. `null` takes the deadline away; leaving
    /// this out leaves it as it is.
    #[serde(default, deserialize_with = "given")]
    pub deadline_at: Option<Option<String>>,
    pub is_completed: Option<bool>,
    /// The whole list of records the task is about: one left out is unlinked.
    pub linked_records: Option<Vec<Value>>,
    /// The whole list of people it is assigned to: one left out is unassigned.
    pub assignees: Option<Vec<Value>>,
}
