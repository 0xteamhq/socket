//! Tasks.

use serde_json::{Map, Value, json};
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{Api, asking, filled, set};
use crate::models::{CreateTask, ListTasks, Task, UpdateTask};

/// The longest text Attio takes for a task.
const MAX_CONTENT: usize = 2000;

/// Tasks, with the records they are about and the people they are assigned to.
#[derive(Debug, Clone, Copy)]
pub struct Tasks<'a>(pub(crate) Api<'a>);

impl Tasks<'_> {
    /// Lists tasks: all of them, or those about one record, assigned to one
    /// person, done or not done.
    pub async fn list(&self, tasks: ListTasks) -> Result<Page<Task>> {
        let (object, record) = (tasks.linked_object.as_deref(), tasks.linked_record_id.as_deref());
        self.0
            .together(("linked_object", object), ("linked_record_id", record))?;
        // To Attio a blank assignee means "assigned to nobody". Left out, it
        // would list every task; sent, it would surprise whoever meant no
        // filter. So it is refused, and the other way to say it is named.
        if tasks
            .assignee
            .as_deref()
            .is_some_and(|assignee| assignee.trim().is_empty())
        {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "`assignee` is a member's id or email address, or the word `null` for tasks assigned to nobody",
            ));
        }
        // Attio returns 500 unless told otherwise. 50 is asked for when the
        // caller names no limit.
        let window = self.0.window(tasks.cursor.as_deref(), tasks.limit, 50, 500)?;
        let request = RawRequest::get("tasks")
            .with_query("limit", window.limit.to_string())
            .with_query("offset", window.offset.to_string());
        let request = asking(request, "sort", tasks.sort.map(|sort| sort.as_str()));
        let request = asking(request, "linked_object", filled(object));
        let request = asking(request, "linked_record_id", filled(record));
        let request = asking(request, "assignee", filled(tasks.assignee.as_deref()));
        let request = asking(request, "is_completed", tasks.is_completed);
        Ok(window.page(self.0.all(request, "tasks").await?))
    }

    /// Gets one task.
    pub async fn get(&self, task: &str) -> Result<Task> {
        let task = self.0.segment("a task id", task)?;
        self.0.one(RawRequest::get(format!("tasks/{task}")), "a task").await
    }

    /// Creates a task. The people it is assigned to are told.
    pub async fn create(&self, task: CreateTask) -> Result<Task> {
        let invalid = |message: String| self.0.error(ErrorKind::InvalidInput, message);
        if task.content.trim().is_empty() {
            return Err(invalid("a task needs `content`".to_owned()));
        }
        if task.content.chars().count() > MAX_CONTENT {
            return Err(invalid(format!("`content` is at most {MAX_CONTENT} characters")));
        }
        // Attio asks for every one of these, set or not.
        let body = json!({ "data": {
            "content": task.content,
            "format": "plaintext",
            "deadline_at": task.deadline_at,
            "is_completed": task.is_completed.unwrap_or(false),
            "linked_records": task.linked_records.unwrap_or_default(),
            "assignees": task.assignees.unwrap_or_default(),
        } });
        self.0.one(RawRequest::post("tasks", body), "a task").await
    }

    /// Changes a task: its deadline, whether it is done, the records it is
    /// about and who it is assigned to. Only what is set is touched.
    pub async fn update(&self, task: &str, changes: UpdateTask) -> Result<Task> {
        let task = self.0.segment("a task id", task)?;
        let mut data = Map::new();
        // `null` is said on purpose here: it is how a deadline is taken away.
        if let Some(deadline) = changes.deadline_at {
            data.insert("deadline_at".to_owned(), deadline.map_or(Value::Null, Value::from));
        }
        set(&mut data, "is_completed", changes.is_completed);
        set(&mut data, "linked_records", changes.linked_records);
        set(&mut data, "assignees", changes.assignees);
        if data.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "an update needs at least one field to change"));
        }
        let body = json!({ "data": Value::Object(data) });
        let request = RawRequest::new("PATCH", format!("tasks/{task}")).with_body(body);
        self.0.one(request, "a task").await
    }

    /// Deletes a task.
    pub async fn delete(&self, task: &str) -> Result<()> {
        let task = self.0.segment("a task id", task)?;
        self.0.done(RawRequest::new("DELETE", format!("tasks/{task}"))).await
    }
}
