//! Salesforce's REST API as typed methods, one file per area of the API.
//!
//! Identifiers (an object type, a record id, a field name) are plain
//! arguments. Content and options are structs from [`crate::models`]. Reads
//! are sent as GET, so the transport may repeat them after a server error.
//! Everything else goes out with the verb Salesforce dictates. A POST or a
//! PATCH is never repeated unless Salesforce refused it outright; a DELETE
//! still is, after a server error, until the transport stops repeating
//! anything but a read.
//!
//! This file holds only what every area shares: the access to the API, and
//! the re-exports. Each area's methods are in the file named after it.

mod limits;
mod query;
mod records;
mod search;
mod sobjects;

use serde::de::DeserializeOwned;
use serde_json::Value;
use socketkit_core::{Connection, Error, ErrorKind, RawRequest, Result};
use url::Url;

pub use limits::Limits;
pub use query::Query;
pub use records::Records;
pub use search::Search;
pub use sobjects::SObjects;

use crate::models::Record;

/// One connection's access to Salesforce.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Api<'a> {
    pub(crate) connection: &'a Connection,
}

impl Api<'_> {
    pub(crate) fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message).with_provider(self.connection.provider().id.clone())
    }

    /// The address this connection's calls go to: the API of the
    /// organisation that authorised it.
    ///
    /// Every organisation has its own host, which Salesforce names when a
    /// person authorises. The definition has none to offer in its place: its
    /// own address is the sign-in host, which serves no data. So a
    /// connection that was stored without its address is refused here,
    /// before anything is sent, and its token is not sent to the sign-in
    /// host to be turned away.
    pub(crate) fn instance(&self) -> Result<Url> {
        let base = self.connection.api_base();
        if base.host_str().is_some_and(crate::is_sign_in_host) {
            return Err(self.error(
                ErrorKind::ReconnectRequired,
                "this salesforce connection does not say which organisation it belongs to; \
                 connect it again through OAuth, or give the instance URL beside the token",
            ));
        }
        Ok(base)
    }

    /// Sends `request` to this connection's organisation and returns what
    /// Salesforce answered. A request that answers with no content, as an
    /// update and a delete do, returns `null`.
    ///
    /// The query is written here and not by the transport, so that a query
    /// in SOQL reaches Salesforce with every character that is not a letter
    /// or a digit percent-encoded, which no server reads two ways.
    pub(super) async fn send(&self, mut request: RawRequest) -> Result<Value> {
        self.instance()?;
        if !request.query.is_empty() {
            let written: Vec<String> = request
                .query
                .drain(..)
                .map(|(name, value)| format!("{name}={}", encoded(&value)))
                .collect();
            request.path = format!("{}?{}", request.path, written.join("&"));
        }
        Ok(self.connection.request(request).await?.body)
    }

    /// Reads `path` on this connection's own host, outside the versioned
    /// data API: `/services/oauth2/userinfo`.
    pub(crate) async fn read_at_instance(&self, path: &str) -> Result<Value> {
        let address = self
            .instance()?
            .join(path)
            .map_err(|_| self.error(ErrorKind::Unexpected, "could not address the organisation's own host"))?;
        Ok(self.connection.request(RawRequest::get(address)).await?.body)
    }

    /// Reads a response as `T`. `what` names it in the error: "an object".
    ///
    /// serde's own message quotes the value it could not read, and here that
    /// may be the content of a customer's record. An error is logged and
    /// shown, so only the place of the value goes into it, and serde's error
    /// is not kept as its cause. The names in the place come from our own types.
    pub(super) fn decode<T: DeserializeOwned>(&self, body: Value, what: &str) -> Result<T> {
        serde_path_to_error::deserialize(body).map_err(|e| {
            let path = e.path().to_string();
            let place = if path == "." {
                String::new()
            } else {
                format!(", at `{path}`")
            };
            self.error(
                ErrorKind::Decode,
                format!("salesforce sent {what} that could not be read{place}"),
            )
        })
    }

    /// Reads one record. A success that carries none is an error, never a
    /// record with nothing in it.
    pub(crate) fn record(&self, body: Value) -> Result<Record> {
        Record::from_wire(body).ok_or_else(|| self.error(ErrorKind::Decode, "salesforce answered without a record"))
    }

    /// Reads the list of records `body` carries under `name`.
    pub(super) fn records(&self, body: &Value, name: &str) -> Result<Vec<Record>> {
        let listed = body
            .get(name)
            .and_then(Value::as_array)
            .ok_or_else(|| self.error(ErrorKind::Decode, "salesforce answered without records"))?;
        listed.iter().cloned().map(|record| self.record(record)).collect()
    }

    pub(super) fn required(&self, what: &str, value: &str) -> Result<()> {
        if value.trim().is_empty() {
            return Err(self.error(ErrorKind::InvalidInput, format!("{what} is required")));
        }
        Ok(())
    }

    /// An object type, as one segment of a path: `Account`, `Invoice__c`.
    ///
    /// An API name is letters, digits and underscores and starts with a
    /// letter, so one that holds anything else is not a name. It is refused
    /// and not encoded: nothing it says could be the object that was meant.
    pub(crate) fn object<'n>(&self, name: &'n str) -> Result<&'n str> {
        self.named("an object type", name.trim())
    }

    /// A field's API name, as one segment of a path.
    pub(super) fn field<'n>(&self, name: &'n str) -> Result<&'n str> {
        self.named("a field name", name.trim())
    }

    fn named<'n>(&self, what: &str, name: &'n str) -> Result<&'n str> {
        self.required(what, name)?;
        if !is_api_name(name) {
            // The value is not repeated: it is refused for what it holds.
            return Err(self.error(
                ErrorKind::InvalidInput,
                format!("{what} is letters, digits and underscores, and starts with a letter"),
            ));
        }
        Ok(name)
    }

    /// A record id, as one segment of a path: 15 or 18 letters and digits.
    pub(crate) fn id<'n>(&self, id: &'n str) -> Result<&'n str> {
        let id = id.trim();
        self.required("a record id", id)?;
        if !crate::is_record_id(id) {
            return Err(self.error(ErrorKind::InvalidInput, "a record id is 15 or 18 letters and digits"));
        }
        Ok(id)
    }

    /// Writes a value as one segment of a path, whatever it contains.
    ///
    /// The value of an external id is another system's text, and may hold
    /// `/`, `?` or a space. Everything outside the characters a URL leaves
    /// alone is percent-encoded, so it can never add a segment, a query or
    /// a fragment. A segment made only of dots would be resolved away and
    /// address something else, so it is refused.
    pub(super) fn segment(&self, what: &str, value: &str) -> Result<String> {
        self.required(what, value)?;
        if value == "." || value == ".." {
            return Err(self.error(ErrorKind::InvalidInput, format!("{what} is not valid")));
        }
        Ok(encoded(value))
    }

    /// The `fields` parameter for the fields that were named, if any were.
    ///
    /// A name may reach through a relationship, as `Owner.Name` does, so
    /// each part between dots is checked as a name.
    pub(super) fn field_list(&self, fields: Option<&[String]>) -> Result<Option<String>> {
        let Some(fields) = fields.filter(|fields| !fields.is_empty()) else {
            return Ok(None);
        };
        self.field_names(fields)?;
        let names: Vec<&str> = fields.iter().map(|field| field.trim()).collect();
        Ok(Some(names.join(",")))
    }

    /// Checks that every one of `fields` is a field name, or a path to one
    /// through a relationship.
    pub(super) fn field_names(&self, fields: &[String]) -> Result<()> {
        if fields.iter().all(|field| field.trim().split('.').all(is_api_name)) {
            return Ok(());
        }
        Err(self.error(
            ErrorKind::InvalidInput,
            "every one of `fields` is a field's API name: letters, digits and underscores, starting with a letter",
        ))
    }
}

/// True for the shape of an API name: letters, digits and underscores,
/// starting with a letter. Salesforce allows 40 characters, and a namespace
/// and a suffix lengthen that, so the limit here is only what keeps an
/// address an address.
fn is_api_name(name: &str) -> bool {
    (1..=255).contains(&name.len())
        && name.starts_with(|first: char| first.is_ascii_alphabetic())
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// `text` with everything percent-encoded but the characters a URL always
/// leaves alone: letters, digits and `-._~`.
pub(super) fn encoded(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_api_name_is_letters_digits_and_underscores_after_a_letter() {
        for name in [
            "Account",
            "Invoice__c",
            "acme__Invoice_Line__c",
            "Knowledge__kav",
            "A",
            "Order2",
        ] {
            assert!(is_api_name(name), "{name}");
        }
        for bad in [
            "",
            "_Account",
            "2Fast",
            "Account/describe",
            "Account/../User",
            "..",
            "Account?x=1",
            "Account#",
            "Account%2F",
            "Account Name",
            "Account.Name",
            "Compte\u{00e9}",
            "Account\n",
        ] {
            assert!(!is_api_name(bad), "{bad:?}");
        }
        assert!(is_api_name(&"a".repeat(255)));
        assert!(!is_api_name(&"a".repeat(256)));
    }

    #[test]
    fn a_value_is_encoded_into_one_segment() {
        assert_eq!(encoded("A-17"), "A-17");
        assert_eq!(encoded("a/b?c#d e"), "a%2Fb%3Fc%23d%20e");
        assert_eq!(encoded("../x"), "..%2Fx");
        assert_eq!(encoded("zoë@acme.example"), "zo%C3%AB%40acme.example");
        assert_eq!(encoded("100%"), "100%25");
    }
}
