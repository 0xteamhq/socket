//! Queries in SOQL.

use socketkit_core::{ErrorKind, RawRequest, Result};

use super::Api;
use crate::models::{QueryOptions, QueryResult};

/// Queries in SOQL, Salesforce's query language.
#[derive(Debug, Clone, Copy)]
pub struct Query<'a>(pub(crate) Api<'a>);

impl Query<'_> {
    /// Runs a query and returns the first batch of what it matches, or with
    /// a `cursor` the batch after the one that gave it. With a cursor `soql`
    /// is not read, and may be empty.
    ///
    /// SOQL only reads: there is no statement in it that changes a record.
    /// A value placed in the query has to be written with
    /// [`crate::escape_soql`], or it can change what the query asks.
    pub async fn run(&self, soql: &str, options: QueryOptions) -> Result<QueryResult> {
        self.batch("query", soql, &options).await
    }

    /// [`Query::run`], including records that were deleted and are still in
    /// the recycle bin, and tasks and events that were archived. A deleted
    /// record says so in its `IsDeleted` field, when the query asks for it.
    pub async fn run_all(&self, soql: &str, options: QueryOptions) -> Result<QueryResult> {
        self.batch("queryAll", soql, &options).await
    }

    async fn batch(&self, resource: &str, soql: &str, options: &QueryOptions) -> Result<QueryResult> {
        if options.batch_size.is_some_and(|size| !(200..=2000).contains(&size)) {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "`batch_size` is from 200 to 2000"));
        }
        let cursor = options.cursor.as_deref().map(str::trim).filter(|c| !c.is_empty());
        let request = match (cursor, options.batch_size) {
            // The batch size was fixed by the first request, and the query
            // with it: neither is sent again.
            (Some(cursor), _) => RawRequest::get(self.next_batch(cursor)?),
            (None, size) => {
                if soql.trim().is_empty() {
                    return Err(self.0.error(
                        ErrorKind::InvalidInput,
                        "a query is required: give `soql`, or the `cursor` of the batch before",
                    ));
                }
                let first = RawRequest::get(resource).with_query("q", soql.trim());
                match size {
                    Some(size) => first.with_header("Sforce-Query-Options", format!("batchSize={size}")),
                    None => first,
                }
            }
        };
        let body = self.0.send(request).await?;
        let total_size = body["totalSize"]
            .as_u64()
            .ok_or_else(|| self.0.error(ErrorKind::Decode, "salesforce answered without a result"))?;
        let next_cursor = body["nextRecordsUrl"]
            .as_str()
            .filter(|next| !next.is_empty())
            .map(str::to_owned);
        Ok(QueryResult {
            total_size,
            done: body["done"].as_bool().unwrap_or(next_cursor.is_none()),
            records: self.0.records(&body, "records")?,
            next_cursor,
        })
    }

    /// The request path for the batch a cursor points to.
    ///
    /// Salesforce gives the address of the next batch as a path on its own
    /// host, `/services/data/v67.0/query/01gxx0000004RpzAAE-2000`, and that
    /// is the cursor. It comes back from the caller, so it is not trusted to
    /// be what Salesforce sent. Used as it stood, it would let a query read
    /// any address that answers a GET. So nothing of it is used as an
    /// address: only the locator at its end is taken, and only when it is
    /// what a locator looks like. The batch is then asked for at the place
    /// this crate builds for it, under the connection's own API.
    fn next_batch(&self, cursor: &str) -> Result<String> {
        locator(cursor)
            .map(|(resource, locator)| format!("{resource}/{locator}"))
            .ok_or_else(|| {
                self.0.error(
                    ErrorKind::InvalidInput,
                    "`cursor` is not the address of a next batch; pass back `next_cursor` unchanged",
                )
            })
    }
}

/// The resource and the locator a cursor names, when it is the address of a
/// next batch: `/services/data/v{version}/query/{locator}`.
///
/// Salesforce writes `query` there also for the results of `queryAll`. Both
/// names are read, and nothing else is. A locator is a record id, a hyphen
/// and a number, so one that holds anything but letters, digits, `-` and `_`
/// is not a locator.
fn locator(cursor: &str) -> Option<(&'static str, &str)> {
    let (version, rest) = cursor.strip_prefix("/services/data/v")?.split_once('/')?;
    let numbered = !version.is_empty() && version.bytes().all(|byte| byte.is_ascii_digit() || byte == b'.');
    let (resource, locator) = rest.split_once('/')?;
    let resource = match resource {
        "query" => "query",
        "queryAll" => "queryAll",
        _ => return None,
    };
    let well_formed = (1..=128).contains(&locator.len())
        && locator
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    (numbered && well_formed).then_some((resource, locator))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cursor_is_the_address_of_a_next_batch_and_only_its_locator_is_used() {
        assert_eq!(
            locator("/services/data/v67.0/query/01gxx0000004RpzAAE-2000"),
            Some(("query", "01gxx0000004RpzAAE-2000"))
        );
        // Whatever version the cursor names, the request goes to the connection's own.
        assert_eq!(
            locator("/services/data/v20.0/query/01gD0000002HU6KIAW-200"),
            Some(("query", "01gD0000002HU6KIAW-200"))
        );
        assert_eq!(
            locator("/services/data/v67.0/queryAll/01gxx0000004RpzAAE-4000"),
            Some(("queryAll", "01gxx0000004RpzAAE-4000"))
        );
    }

    #[test]
    fn anything_else_is_not_a_cursor() {
        for forged in [
            "",
            "01gxx0000004RpzAAE-2000",
            "https://evil.example/services/data/v67.0/query/01gxx0000004RpzAAE-2000",
            "https://acme.my.salesforce.com/services/data/v67.0/query/01gxx0000004RpzAAE-2000",
            "//evil.example/services/data/v67.0/query/01gxx0000004RpzAAE-2000",
            "/services/data/v67.0/sobjects/User/005xx000001SvogAAC",
            "/services/data/v67.0/query/",
            "/services/data/v67.0/query",
            "/services/data/v67.0/query/../sobjects/User",
            "/services/data/v67.0/query/01g/../../sobjects/User",
            "/services/data/v67.0/query/01gxx0000004RpzAAE-2000?q=SELECT+Id+FROM+User",
            "/services/data/v67.0/query/01gxx0000004RpzAAE-2000#x",
            "/services/data/v67.0/query/01gxx%2F..%2Fsobjects",
            "/services/data/v67.0/query/01gxx 2000",
            "/services/data/v67.0/tooling/query/01gxx0000004RpzAAE-2000",
            "/services/data/v/query/01gxx0000004RpzAAE-2000",
            "/services/data/v67.0/../v67.0/query/01gxx0000004RpzAAE-2000",
            "/services/data/vX/query/01gxx0000004RpzAAE-2000",
            "/services/oauth2/revoke",
            "services/data/v67.0/query/01gxx0000004RpzAAE-2000",
        ] {
            assert_eq!(locator(forged), None, "{forged:?}");
        }
        assert_eq!(
            locator(&format!("/services/data/v67.0/query/{}", "a".repeat(129))),
            None
        );
    }
}
