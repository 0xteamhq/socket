//! The Gmail account a connection reads and writes.

use socketkit_core::{ErrorKind, RawRequest, Result};

use super::{Api, GMAIL};
use crate::models::GmailProfile;

/// The Gmail account itself.
#[derive(Debug, Clone, Copy)]
pub struct GmailProfiles<'a>(pub(crate) Api<'a>);

impl GmailProfiles<'_> {
    /// Gets the mailbox's address, how many messages and threads it holds,
    /// and where its record of changes stands.
    ///
    /// This is how an application that asked only for Gmail learns whose
    /// mailbox it has: `google.identity.get` reads Drive.
    pub async fn get(&self) -> Result<GmailProfile> {
        let request = RawRequest::get(self.0.on(GMAIL, "gmail/v1/users/me/profile"));
        let profile: GmailProfile = self.0.decode(self.0.send(request).await?, "a profile")?;
        if profile.email_address.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "google answered without a profile"));
        }
        Ok(profile)
    }
}
