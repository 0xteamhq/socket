//! The channels of a team.

use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{Api, named};
use crate::models::{Channel, ConversationMember, Cursor, Paging};

/// The channels of a team.
#[derive(Debug, Clone, Copy)]
pub struct Channels<'a>(pub(crate) Api<'a>);

impl Channels<'_> {
    /// Lists a team's channels. Private and shared channels the account is
    /// not in are left out.
    pub async fn list(&self, team: &str, place: Cursor) -> Result<Page<Channel>> {
        let team = self.0.segment("a team id", team)?;
        let request = named(RawRequest::get(format!("teams/{team}/channels")));
        self.0.page(request, &place.into(), "channels").await
    }

    /// Gets one channel.
    pub async fn get(&self, team: &str, channel: &str) -> Result<Channel> {
        let body = self.0.send(named(RawRequest::get(self.path(team, channel)?))).await?;
        let channel: Channel = self.0.decode(body, "a channel")?;
        if channel.id.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "microsoft answered without a channel"));
        }
        Ok(channel)
    }

    /// Lists a channel's members and owners.
    pub async fn members(&self, team: &str, channel: &str, paging: Paging) -> Result<Page<ConversationMember>> {
        let request = RawRequest::get(format!("{}/members", self.path(team, channel)?));
        self.0.page_up_to(999, request, &paging, "members").await
    }

    fn path(&self, team: &str, channel: &str) -> Result<String> {
        let team = self.0.segment("a team id", team)?;
        let channel = self.0.segment("a channel id", channel)?;
        Ok(format!("teams/{team}/channels/{channel}"))
    }
}
