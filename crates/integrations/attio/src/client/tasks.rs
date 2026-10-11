//! Tasks.

use serde_json::{Map, Value, json};
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{A_PAGE, Api, MOST};
use crate::models::{AssignTo, CreateTask, LinkRecord, ListTasks, Paging, Task, UpdateTask};

/// Tasks: things to do, who is to do them, and the records they concern.
#[derive(Debug, Clone, Copy)]
pub struct Tasks<'a>(pub(crate) Api<'a>);

impl Tasks<'_> {
    /// Lists tasks: all of them, those about one record, one person's,
    /// those done or those still to do.
    pub async fn list(&self, options: ListTasks) -> Result<Page<Task>> {
        let object = self
            .0
            .optional_id("a linked object", options.linked_object.as_deref())?;
        let record = self
            .0
            .optional_id("a linked record id", options.linked_record_id.as_deref())?;
        let mut request = RawRequest::get("tasks");
        match (object, record) {
            (Some(object), Some(record)) => {
                request = request
                    .with_query("linked_object", object)
                    .with_query("linked_record_id", record);
            }
            (None, None) => {}
            _ => {
                return Err(self.0.error(
                    ErrorKind::InvalidInput,
                    "`linked_object` and `linked_record_id` are given together or not at all",
                ));
            }
        }
        if let Some(assignee) = options.assignee.as_deref().map(str::trim) {
            // Attio reads an empty `assignee` as "assigned to nobody". A
            // filter left blank by mistake must not turn into that question,
            // so only the word asks it.
            if assignee.is_empty() {
                return Err(self.0.error(
                    ErrorKind::InvalidInput,
                    "`assignee` is a member's id or email address, or `null` for tasks assigned to nobody",
                ));
            }
            request = request.with_query("assignee", assignee);
        }
        if let Some(done) = options.is_completed {
            request = request.with_query("is_completed", done.to_string());
        }
        if let Some(sort) = options.sort {
            request = request.with_query("sort", sort.as_str());
        }
        let paging = Paging {
            cursor: options.cursor,
            limit: options.limit,
        };
        self.0.page(request, &paging, (A_PAGE, MOST), "tasks").await
    }

    /// Gets one task.
    pub async fn get(&self, task: &str) -> Result<Task> {
        let task = self.0.id("a task id", task)?;
        self.task(RawRequest::get(format!("tasks/{task}"))).await
    }

    /// Creates a task. Its text is plain: Attio takes no formatting and no
    /// links to records in it.
    pub async fn create(&self, task: CreateTask) -> Result<Task> {
        let length = task.content.chars().count();
        if task.content.trim().is_empty() || length > 2000 {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "a task needs `content`, of at most 2000 characters",
            ));
        }
        if task
            .deadline_at
            .as_deref()
            .is_some_and(|deadline| deadline.trim().is_empty())
        {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "`deadline_at` is a time in ISO 8601"));
        }
        // Attio asks for every one of these, set or not.
        let body = json!({ "data": {
            "content": task.content,
            "format": "plaintext",
            "deadline_at": task.deadline_at,
            "is_completed": task.is_completed.unwrap_or(false),
            "linked_records": self.linked(task.linked_records.as_deref().unwrap_or_default())?,
            "assignees": self.assigned(task.assignees.as_deref().unwrap_or_default())?,
        } });
        self.task(RawRequest::post("tasks", body)).await
    }

    /// Changes a task's deadline, whether it is done, the records it
    /// concerns or who it is assigned to. Its text cannot be changed.
    pub async fn update(&self, task: &str, changes: UpdateTask) -> Result<Task> {
        let task = self.0.id("a task id", task)?;
        let mut data = Map::new();
        match changes.deadline_at {
            // `null` removes the deadline, and is sent as it is.
            // A blank is neither: it is not sent for Attio to guess at.
            Some(deadline @ Value::Null) => {
                data.insert("deadline_at".to_owned(), deadline);
            }
            Some(Value::String(deadline)) if !deadline.trim().is_empty() => {
                data.insert("deadline_at".to_owned(), Value::String(deadline));
            }
            Some(_) => {
                return Err(self
                    .0
                    .error(ErrorKind::InvalidInput, "`deadline_at` is a time in ISO 8601, or null"));
            }
            None => {}
        }
        if let Some(done) = changes.is_completed {
            data.insert("is_completed".to_owned(), json!(done));
        }
        if let Some(records) = &changes.linked_records {
            data.insert("linked_records".to_owned(), self.linked(records)?);
        }
        if let Some(people) = &changes.assignees {
            data.insert("assignees".to_owned(), self.assigned(people)?);
        }
        if data.is_empty() {
            return Err(self.0.error(ErrorKind::InvalidInput, "nothing to change was given"));
        }
        let body = json!({ "data": data });
        self.task(RawRequest::new("PATCH", format!("tasks/{task}")).with_body(body))
            .await
    }

    /// Deletes a task.
    pub async fn delete(&self, task: &str) -> Result<()> {
        let task = self.0.id("a task id", task)?;
        self.0.send(RawRequest::new("DELETE", format!("tasks/{task}"))).await?;
        Ok(())
    }

    /// The records a task concerns, as Attio takes them.
    fn linked(&self, records: &[LinkRecord]) -> Result<Value> {
        let records: Result<Vec<Value>> = records
            .iter()
            .map(|record| {
                Ok(json!({
                    "target_object": self.0.id("the object of a linked record", &record.target_object)?,
                    "target_record_id": self.0.id("the id of a linked record", &record.target_record_id)?,
                }))
            })
            .collect();
        Ok(Value::Array(records?))
    }

    /// The people a task is assigned to, as Attio takes them: each by a
    /// member id or by an email address.
    fn assigned(&self, people: &[AssignTo]) -> Result<Value> {
        let people: Result<Vec<Value>> = people
            .iter()
            .map(
                |person| match (filled(&person.referenced_actor_id), filled(&person.workspace_member_email_address)) {
                    (Some(id), None) => Ok(json!({
                        "referenced_actor_type": "workspace-member",
                        "referenced_actor_id": self.0.id("an assignee's id", id)?,
                    })),
                    (None, Some(email)) => Ok(json!({ "workspace_member_email_address": email })),
                    _ => Err(self.0.error(
                        ErrorKind::InvalidInput,
                        "an assignee is named by `referenced_actor_id` or by `workspace_member_email_address`, and not by both",
                    )),
                },
            )
            .collect();
        Ok(Value::Array(people?))
    }

    async fn task(&self, request: RawRequest) -> Result<Task> {
        let task: Task = self.0.one(request, "a task").await?;
        if task.id.task_id.is_empty() {
            return Err(self.0.missing("a task"));
        }
        Ok(task)
    }
}

/// The text of an optional field, when it has any.
fn filled(text: &Option<String>) -> Option<&str> {
    text.as_deref().map(str::trim).filter(|text| !text.is_empty())
}
