//! The people who work in the workspace.

use socketkit_core::{RawRequest, Result};

use super::Api;
use crate::models::WorkspaceMember;

/// The people who work in the workspace.
#[derive(Debug, Clone, Copy)]
pub struct WorkspaceMembers<'a>(pub(crate) Api<'a>);

impl WorkspaceMembers<'_> {
    /// Lists every member of the workspace, suspended ones included. Attio
    /// returns them all at once.
    pub async fn list(&self) -> Result<Vec<WorkspaceMember>> {
        self.0
            .one(RawRequest::get("workspace_members"), "workspace members")
            .await
    }

    /// Gets one member by id.
    pub async fn get(&self, member: &str) -> Result<WorkspaceMember> {
        let member = self.0.id("a workspace member id", member)?;
        let request = RawRequest::get(format!("workspace_members/{member}"));
        let member: WorkspaceMember = self.0.one(request, "a workspace member").await?;
        if member.id.workspace_member_id.is_empty() {
            return Err(self.0.missing("a workspace member"));
        }
        Ok(member)
    }
}
