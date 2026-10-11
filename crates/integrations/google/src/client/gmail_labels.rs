//! Gmail labels: Gmail's own, and the ones a person made.

use socketkit_core::{ErrorKind, RawRequest, Result};

use super::{Api, GMAIL};
use crate::models::GmailLabel;

const LABELS: &str = "gmail/v1/users/me/labels";

/// The labels of a Gmail mailbox.
#[derive(Debug, Clone, Copy)]
pub struct GmailLabels<'a>(pub(crate) Api<'a>);

impl GmailLabels<'_> {
    /// Lists every label of the mailbox: Gmail's own and the person's. Gmail
    /// returns them all at once, with each label's id, name and kind and
    /// without its counts.
    pub async fn list(&self) -> Result<Vec<GmailLabel>> {
        let body = self.0.send(RawRequest::get(self.0.on(GMAIL, LABELS))).await?;
        let labels = self.0.page::<GmailLabel>(body, "labels", "labels")?.items;
        if labels.iter().any(|label| label.id.is_empty()) {
            return Err(self.0.error(ErrorKind::Decode, "google answered without a label"));
        }
        Ok(labels)
    }

    /// Gets one label, with how many messages and threads carry it and how
    /// many of them are unread.
    pub async fn get(&self, label: &str) -> Result<GmailLabel> {
        let label = self.0.segment("a label id", label)?;
        let path = self.0.on(GMAIL, &format!("{LABELS}/{label}"));
        let label: GmailLabel = self.0.decode(self.0.send(RawRequest::get(path)).await?, "a label")?;
        if label.id.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "google answered without a label"));
        }
        Ok(label)
    }
}
