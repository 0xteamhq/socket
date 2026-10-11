//! The people of a workspace.

use socketkit_core::{RawRequest, Result};

use super::Api;
use crate::models::WorkspaceMember;

/// The people who have access to the workspace. Their ids are what a task
/// is assigned to and what a comment is written as.
#[derive(Debug, Clone, Copy)]
pub struct WorkspaceMembers<'a>(pub(crate) Api<'a>);

impl WorkspaceMembers<'_> {
    /// Lists every member of the workspace.
    pub async fn list(&self) -> Result<Vec<WorkspaceMember>> {
        self.0
            .all(RawRequest::get("workspace_members"), "workspace members")
            .await
    }

    /// Gets one member.
    pub async fn get(&self, member: &str) -> Result<WorkspaceMember> {
        let member = self.0.segment("a workspace member id", member)?;
        let request = RawRequest::get(format!("workspace_members/{member}"));
        self.0.one(request, "a workspace member").await
    }
}
