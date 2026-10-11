//! The teams a person belongs to.

use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::Api;
use crate::models::{ConversationMember, Cursor, Paging, Team};

/// The teams a person belongs to.
#[derive(Debug, Clone, Copy)]
pub struct Teams<'a>(pub(crate) Api<'a>);

impl Teams<'_> {
    /// Lists the teams the account is a member of. Graph fills in each
    /// team's name, description and whether it is archived; `get` returns the rest.
    pub async fn list_joined(&self, place: Cursor) -> Result<Page<Team>> {
        self.0
            .page(RawRequest::get("me/joinedTeams"), &place.into(), "teams")
            .await
    }

    /// Gets one team.
    pub async fn get(&self, team: &str) -> Result<Team> {
        let team = self.0.segment("a team id", team)?;
        let body = self.0.send(RawRequest::get(format!("teams/{team}"))).await?;
        let team: Team = self.0.decode(body, "a team")?;
        if team.id.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "microsoft answered without a team"));
        }
        Ok(team)
    }

    /// Lists a team's members and owners.
    pub async fn members(&self, team: &str, paging: Paging) -> Result<Page<ConversationMember>> {
        let team = self.0.segment("a team id", team)?;
        let request = RawRequest::get(format!("teams/{team}/members"));
        self.0.page_up_to(999, request, &paging, "members").await
    }
}
