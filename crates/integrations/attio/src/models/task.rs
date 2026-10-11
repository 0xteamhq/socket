//! Tasks: things to do, with who is to do them and which records they concern.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Actor;
use super::nullable::nullable;

/// The id of a task.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct TaskId {
    pub workspace_id: String,
    pub task_id: String,
}

/// A task.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Task {
    pub id: TaskId,
    /// What is to be done. A record the text links to reads as `@` and its name.
    pub content_plaintext: Option<String>,
    pub deadline_at: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub is_completed: bool,
    pub completed_at: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub linked_records: Vec<TaskRecord>,
    #[serde(deserialize_with = "nullable")]
    pub assignees: Vec<TaskAssignee>,
    pub created_by_actor: Option<Actor>,
    pub created_at: Option<String>,
}

/// A record a task concerns, as Attio returns it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct TaskRecord {
    pub target_object_id: Option<String>,
    pub target_record_id: Option<String>,
}

/// Someone a task is assigned to, as Attio returns them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct TaskAssignee {
    /// `workspace-member`.
    pub referenced_actor_type: Option<String>,
    pub referenced_actor_id: Option<String>,
}

/// The order of a list of tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum TaskSort {
    /// Oldest first. Attio's order when none is asked for.
    #[serde(rename = "created_at:asc")]
    CreatedAsc,
    #[serde(rename = "created_at:desc")]
    CreatedDesc,
    /// Tasks still to do, then those done, oldest first.
    #[serde(rename = "completed_at:asc")]
    CompletedAsc,
    /// Tasks done, most recent first, then those still to do.
    #[serde(rename = "completed_at:desc")]
    CompletedDesc,
}

impl TaskSort {
    /// The value Attio takes for it.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::CreatedAsc => "created_at:asc",
            Self::CreatedDesc => "created_at:desc",
            Self::CompletedAsc => "completed_at:asc",
            Self::CompletedDesc => "completed_at:desc",
        }
    }
}

/// Which tasks to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListTasks {
    /// The object of the record whose tasks are wanted. It needs `linked_record_id` beside it.
    pub linked_object: Option<String>,
    /// The record whose tasks are wanted. It needs `linked_object` beside it.
    pub linked_record_id: Option<String>,
    /// The member whose tasks are wanted, by id or email address. `null`,
    /// written as that word, asks for tasks assigned to nobody. A blank is refused.
    pub assignee: Option<String>,
    /// Only tasks that are done, or only those that are not. Both when not given.
    pub is_completed: Option<bool>,
    pub sort: Option<TaskSort>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most tasks to return in one page, from 1 to 500. 50 when not given.
    pub limit: Option<u32>,
}

/// A record for a task to concern.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LinkRecord {
    /// The object the record belongs to, by its slug or id.
    pub target_object: String,
    pub target_record_id: String,
}

/// Someone to assign a task to: by member id, or by email address, and not by both.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AssignTo {
    /// The id of a workspace member.
    pub referenced_actor_id: Option<String>,
    /// The email address of a workspace member.
    pub workspace_member_email_address: Option<String>,
}

/// A task to create.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateTask {
    /// What is to be done, as plain text of at most 2000 characters. Attio
    /// takes no formatting and no links to records here.
    pub content: String,
    /// When it is due, in ISO 8601. No deadline when not given.
    pub deadline_at: Option<String>,
    /// Not done when not given.
    pub is_completed: Option<bool>,
    pub linked_records: Option<Vec<LinkRecord>>,
    pub assignees: Option<Vec<AssignTo>>,
}

/// What to change about a task. Attio does not let its text be changed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateTask {
    /// When it is due, in ISO 8601. `null` removes the deadline; leaving
    /// this out leaves it as it is.
    #[serde(default, deserialize_with = "given")]
    pub deadline_at: Option<Value>,
    pub is_completed: Option<bool>,
    /// The records the task concerns, as the whole list.
    pub linked_records: Option<Vec<LinkRecord>>,
    /// The people it is assigned to, as the whole list.
    pub assignees: Option<Vec<AssignTo>>,
}

/// Reads a field that was given as `Some`, a `null` included, so that
/// "set this to nothing" can be told from "leave this as it is".
fn given<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}
