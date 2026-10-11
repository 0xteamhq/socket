//! Searching the text of records.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, RawRequest, Result};

use super::Api;
use crate::escape_sosl;
use crate::models::{Find, FindIn, SearchResult};

/// Searching the text of records, across object types.
#[derive(Debug, Clone, Copy)]
pub struct Search<'a>(pub(crate) Api<'a>);

impl Search<'_> {
    /// Runs a search written in SOSL, Salesforce's search language:
    /// `FIND {Acme} IN NAME FIELDS RETURNING Account(Id, Name), Contact(Id)`.
    ///
    /// Text placed between the braces has to be written with
    /// [`crate::escape_sosl`], or it can change what the search asks.
    pub async fn run(&self, sosl: &str) -> Result<SearchResult> {
        self.0.required("a search", sosl)?;
        let body = self
            .0
            .send(RawRequest::get("search").with_query("q", sosl.trim()))
            .await?;
        self.found(&body)
    }

    /// Searches for some text, saying where to look and what to return as
    /// options and not as SOSL.
    ///
    /// Salesforce takes this search as a POST. It changes nothing.
    pub async fn find(&self, find: Find) -> Result<SearchResult> {
        let invalid = |message: &str| self.0.error(ErrorKind::InvalidInput, message);
        self.0.required("`text`", &find.text)?;
        let within_bounds = |limit: Option<u32>| limit.is_none_or(|limit| (1..=2000).contains(&limit));
        if !within_bounds(find.limit) {
            return Err(invalid("`limit` is from 1 to 2000"));
        }
        if !within_bounds(find.overall_limit) {
            return Err(invalid("`overall_limit` is from 1 to 2000"));
        }
        let objects = find.objects.as_deref().unwrap_or_default();
        if objects.is_empty() && find.fields.as_ref().is_some_and(|fields| !fields.is_empty()) {
            return Err(invalid(
                "`fields` needs `objects`: a field is returned of the object types that are named",
            ));
        }
        // The text is the person's own words, and is searched for as it
        // stands. Unescaped, a `-` in a name would mean "not", and a brace
        // would be an error.
        let mut body = json!({ "q": escape_sosl(find.text.trim()) });
        if let Some(fields) = find.fields.as_deref().filter(|fields| !fields.is_empty()) {
            self.0.field_names(fields)?;
            body["fields"] = json!(trimmed(fields));
        }
        if !objects.is_empty() {
            let listed: Result<Vec<Value>> = objects.iter().map(|object| self.searched(object)).collect();
            body["sobjects"] = Value::Array(listed?);
        }
        if let Some(scope) = find.within {
            body["in"] = json!(scope.as_str());
        }
        if let Some(limit) = find.limit {
            body["defaultLimit"] = json!(limit);
        }
        if let Some(limit) = find.overall_limit {
            body["overallLimit"] = json!(limit);
        }
        let answer = self.0.send(RawRequest::post("parameterizedSearch", body)).await?;
        self.found(&answer)
    }

    /// One object type to search, as Salesforce takes it.
    fn searched(&self, object: &FindIn) -> Result<Value> {
        let mut searched = json!({ "name": self.0.object(&object.name)? });
        if let Some(fields) = object.fields.as_deref().filter(|fields| !fields.is_empty()) {
            self.0.field_names(fields)?;
            searched["fields"] = json!(trimmed(fields));
        }
        if let Some(limit) = object.limit {
            if !(1..=2000).contains(&limit) {
                return Err(self
                    .0
                    .error(ErrorKind::InvalidInput, "an object's `limit` is from 1 to 2000"));
            }
            searched["limit"] = json!(limit);
        }
        Ok(searched)
    }

    fn found(&self, body: &Value) -> Result<SearchResult> {
        Ok(SearchResult {
            records: self.0.records(body, "searchRecords")?,
        })
    }
}

fn trimmed(fields: &[String]) -> Vec<&str> {
    fields.iter().map(|field| field.trim()).collect()
}
