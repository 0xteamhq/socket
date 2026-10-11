//! What is left of an organisation's allowances.

use std::collections::BTreeMap;

use serde_json::Value;
use socketkit_core::{ErrorKind, RawRequest, Result};

use super::Api;
use crate::models::{Limit, Limits as OrgLimits};

/// What is left of an organisation's allowances.
#[derive(Debug, Clone, Copy)]
pub struct Limits<'a>(pub(crate) Api<'a>);

impl Limits<'_> {
    /// Reports each of the organisation's allowances and how much of it is
    /// left: above all its API requests for the last 24 hours, which every
    /// call through this integration counts against.
    pub async fn get(&self) -> Result<OrgLimits> {
        let body = self.0.send(RawRequest::get("limits")).await?;
        let mut all: BTreeMap<String, Limit> = body
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(name, limit)| Some((name.clone(), allowance(limit)?)))
            .collect();
        let Some(daily_api_requests) = all.remove("DailyApiRequests") else {
            return Err(self.0.error(
                ErrorKind::Decode,
                "salesforce answered without the allowance of API requests",
            ));
        };
        Ok(OrgLimits {
            daily_api_requests,
            others: all,
        })
    }
}

/// One allowance, when `limit` is one. Salesforce nests the share of each
/// connected application inside some of them, and those are not passed on.
fn allowance(limit: &Value) -> Option<Limit> {
    Some(Limit {
        max: limit["Max"].as_i64()?,
        remaining: limit["Remaining"].as_i64()?,
    })
}
