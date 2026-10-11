//! Every typed method as a named operation.
//!
//! An operation's input is a JSON object with the method's plain arguments
//! and its options side by side: `{ "object": "Account", "id": "001…",
//! "fields": ["Name"] }`. Both schemas are generated from the same types the
//! typed methods use, so the two ways of calling cannot drift apart.

use std::future::Future;
use std::sync::OnceLock;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use socketkit_core::{Connection, Effect, Error, ErrorKind, OperationInfo, Result, schema_of};

use crate::Salesforce;
use crate::models::{
    Describe, Find, GetRecord, Limits, ListSObjects, QueryOptions, QueryResult, Record, RecordFields, SObjectSummary,
    Saved, SearchResult,
};

type Running = std::pin::Pin<Box<dyn Future<Output = Result<Value>> + Send>>;

/// One named operation: what it says about itself, and how to run it.
pub(crate) struct Operation {
    pub(crate) info: OperationInfo,
    run: Box<dyn Fn(Salesforce, Connection, Value) -> Running + Send + Sync>,
}

impl Operation {
    pub(crate) fn run(&self, salesforce: Salesforce, connection: Connection, input: Value) -> Running {
        (self.run)(salesforce, connection, input)
    }
}

fn invalid(message: String) -> Error {
    Error::new(ErrorKind::InvalidInput, message)
}

/// `schema` with every object in it closed: a field it does not list is not
/// allowed. The schema then says what [`unknown_field`] enforces.
///
/// The input itself is closed even when it takes nothing, so an operation
/// without arguments says so and refuses one.
fn closed(mut schema: Value) -> Value {
    fn close(node: &mut Value) {
        match node {
            Value::Object(fields) => {
                if fields.contains_key("properties") {
                    fields.insert("additionalProperties".to_owned(), Value::Bool(false));
                }
                fields.values_mut().for_each(close);
            }
            Value::Array(items) => items.iter_mut().for_each(close),
            _ => {}
        }
    }
    if let Value::Object(fields) = &mut schema {
        fields.entry("properties").or_insert_with(|| Value::Object(Map::new()));
    }
    close(&mut schema);
    schema
}

/// The schema of a value, behind a reference or beside `null`.
fn behind<'s>(root: &'s Value, node: &'s Value) -> &'s Value {
    let mut node = node;
    for _ in 0..8 {
        let defined = node["$ref"].as_str().and_then(|name| name.strip_prefix("#/$defs/"));
        let optional = node["anyOf"]
            .as_array()
            .and_then(|arms| arms.iter().find(|arm| arm["type"] != "null"));
        match (defined, optional) {
            (Some(name), _) => node = &root["$defs"][name],
            (None, Some(arm)) => node = arm,
            (None, None) => break,
        }
    }
    node
}

/// The first field of `input` whose value is not of the kind the schema
/// gives it: a number where text is asked for, a word that is not one of a
/// choice.
///
/// serde reports such a value without its place when the input's options
/// are flattened into one object, as they are here. The schema knows the
/// place, so the caller is told which field to correct. Only the top of the
/// input is looked at; what lies deeper is left to serde.
fn mistyped(root: &Value, input: &Value) -> Option<String> {
    let fits = |kind: &str, value: &Value| match kind {
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "null" => value.is_null(),
        _ => true,
    };
    let known = root["properties"].as_object()?;
    input.as_object()?.iter().find_map(|(name, value)| {
        let stated = known.get(name)?;
        let optional = stated["anyOf"]
            .as_array()
            .is_some_and(|arms| arms.iter().any(|arm| arm["type"] == "null"));
        if value.is_null() && optional {
            return None;
        }
        let schema = behind(root, stated);
        let of_kind = match &schema["type"] {
            Value::String(kind) => fits(kind, value),
            Value::Array(kinds) => kinds.iter().filter_map(Value::as_str).any(|kind| fits(kind, value)),
            _ => true,
        };
        let choices: Vec<&Value> = schema["enum"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(
                schema["oneOf"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|arm| arm.get("const")),
            )
            .collect();
        let chosen = choices.is_empty() || choices.contains(&value);
        // A count cannot be negative, which the schema says as a minimum.
        let in_range = match (schema["minimum"].as_f64(), value.as_f64()) {
            (Some(least), Some(given)) => given >= least,
            _ => true,
        };
        (!of_kind || !chosen || !in_range).then(|| name.clone())
    })
}

/// Where in `input` the first field is that the schema at `node` does not
/// list, and that field's name.
///
/// A field that is not known would be dropped in silence, with what it said:
/// the fields to return, the size of a batch, a limit. The input types
/// cannot refuse one themselves, because their options are flattened into
/// one object. A record's own `fields` are a map the schema leaves open, and
/// are not looked into.
fn unknown_field(root: &Value, node: &Value, input: &Value) -> Option<(String, String)> {
    let node = behind(root, node);
    let within = |place: String, (inner, name): (String, String)| {
        let joint = if inner.is_empty() || inner.starts_with('[') {
            ""
        } else {
            "."
        };
        (format!("{place}{joint}{inner}"), name)
    };
    match input {
        Value::Object(fields) => {
            let known = node["properties"].as_object()?;
            fields.iter().find_map(|(name, value)| match known.get(name) {
                None => Some((String::new(), name.clone())),
                Some(schema) => unknown_field(root, schema, value).map(|found| within(name.clone(), found)),
            })
        }
        Value::Array(items) => {
            let schema = node.get("items")?;
            items
                .iter()
                .enumerate()
                .find_map(|(at, item)| unknown_field(root, schema, item).map(|found| within(format!("[{at}]"), found)))
        }
        _ => None,
    }
}

/// The refusal for a field that is not known. Its name is the caller's own
/// text, so it is repeated only when it looks like a name.
fn not_a_field((place, name): (String, String)) -> Error {
    let named = (1..=40).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '@' | '.' | '-'));
    let joint = if place.is_empty() { "" } else { "." };
    invalid(match (named, place.is_empty()) {
        (true, _) => format!("`{place}{joint}{name}` is not a field of this operation; check its spelling"),
        (false, true) => "the input has a field this operation does not know".to_owned(),
        (false, false) => format!("`{place}` has a field this operation does not know"),
    })
}

/// Builds an operation from a typed handler. The input type gives the input
/// schema and the parsing; the output type gives the output schema.
fn operation<I, O, F, Fut>(name: &str, description: &str, effect: Effect, scopes: &[&str], handler: F) -> Operation
where
    I: DeserializeOwned + JsonSchema + Send + 'static,
    O: Serialize + JsonSchema + 'static,
    F: Fn(Salesforce, Connection, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<O>> + Send + 'static,
{
    let info = OperationInfo {
        name: format!("salesforce.{name}"),
        description: description.to_owned(),
        input_schema: closed(schema_of::<I>()),
        output_schema: schema_of::<O>(),
        effect,
        required_scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
    };
    let schema = info.input_schema.clone();
    let run = move |salesforce: Salesforce, connection: Connection, input: Value| -> Running {
        let provider = connection.provider().id.clone();
        if let Some(found) = unknown_field(&schema, &schema, &input) {
            return Box::pin(std::future::ready(Err(not_a_field(found).with_provider(provider))));
        }
        let misplaced = mistyped(&schema, &input);
        match serde_path_to_error::deserialize::<_, I>(input) {
            Err(e) => {
                // serde's own message quotes the offending value, which may be
                // a customer's record or a credential. Only the field's name,
                // which comes from our own types, goes into the error.
                let path = e.path().to_string();
                let inner = e.inner().to_string();
                let message = match (inner.starts_with("missing field"), path.as_str(), misplaced) {
                    (true, _, _) => inner,
                    (false, ".", Some(field)) => format!("`{field}` has the wrong type"),
                    (false, ".", None) => "the input has a field of the wrong type".to_owned(),
                    (false, path, _) => format!("`{path}` has the wrong type"),
                };
                Box::pin(std::future::ready(Err(invalid(message).with_provider(provider))))
            }
            Ok(input) => {
                let output = handler(salesforce, connection, input);
                Box::pin(async move {
                    serde_json::to_value(output.await?).map_err(|e| {
                        Error::new(ErrorKind::Unexpected, "could not encode the result")
                            .with_provider(provider)
                            .with_source(e)
                    })
                })
            }
        }
    };
    Operation {
        info,
        run: Box::new(run),
    }
}

/// Defines an operation's input: its plain arguments, and optionally one
/// options struct whose fields sit beside them.
macro_rules! input {
    ($name:ident { $($(#[$doc:meta])* $field:ident : $kind:ty),* $(,)? }) => {
        #[derive(Debug, Deserialize, JsonSchema)]
        struct $name { $($(#[$doc])* $field: $kind,)* }
    };
    ($name:ident { $($(#[$doc:meta])* $field:ident : $kind:ty),* $(,)? } + $options:ty) => {
        #[derive(Debug, Deserialize, JsonSchema)]
        struct $name {
            $($(#[$doc])* $field: $kind,)*
            #[serde(flatten)]
            options: $options,
        }
    };
}

input!(
    RunQuery {
        /// The query, in SOQL: `SELECT Id, Name FROM Account WHERE Industry = 'Energy' LIMIT 20`.
        /// Needed for the first batch. With a `cursor` it is not read and can be left out.
        soql: Option<String>
    } + QueryOptions
);
input!(RunSearch {
    /// The search, in SOSL: `FIND {Acme} IN NAME FIELDS RETURNING Account(Id, Name), Contact(Id, Name)`.
    sosl: String
});
input!(FindText {} + Find);
input!(ObjectTypes {} + ListSObjects);
input!(OneObject {
    /// An object type's API name, such as `Account` or `Invoice__c`.
    object: String
});
input!(
    OneRecord {
        /// The record's object type, such as `Account` or `Invoice__c`.
        object: String,
        /// The record's id: 15 or 18 letters and digits.
        id: String
    } + GetRecord
);
input!(
    ByExternalId {
        /// The record's object type.
        object: String,
        /// The API name of the external id field, such as `ERP_Id__c`.
        field: String,
        /// The value of that field on the record that is meant.
        value: String
    } + GetRecord
);
input!(
    NewRecord {
        /// The object type to create a record of.
        object: String
    } + RecordFields
);
input!(
    ChangeRecord {
        /// The record's object type.
        object: String,
        /// The id of the record to change.
        id: String
    } + RecordFields
);
input!(
    UpsertRecord {
        /// The record's object type.
        object: String,
        /// The API name of the external id field, such as `ERP_Id__c`.
        field: String,
        /// The value of that field on the record to create or change. It is
        /// given here and not among `fields`.
        value: String
    } + RecordFields
);
input!(ThisRecord {
    /// The record's object type.
    object: String,
    /// The id of the record.
    id: String
});
input!(Nothing {});

/// Every operation, built once.
pub(crate) fn all() -> &'static [Operation] {
    static ALL: OnceLock<Vec<Operation>> = OnceLock::new();
    ALL.get_or_init(build)
}

#[rustfmt::skip]
fn build() -> Vec<Operation> {
    use Effect::{Destructive, Read, Write};
    vec![
        // ── query ──
        operation("query.run", "Run a query in SOQL and return the records it matches, a batch at a time. A query only reads. Select the fields that are needed and end with LIMIT: a batch holds up to 2000 records. Use sobjects.describe to learn an object's fields.", Read, &["api"],
            |s: Salesforce, c: Connection, i: RunQuery| async move { s.query(&c).run(i.soql.as_deref().unwrap_or_default(), i.options).await as Result<QueryResult> }),
        operation("query.run_all", "Run a query in SOQL as query.run does, including records that were deleted and are still in the recycle bin, and archived tasks and events.", Read, &["api"],
            |s: Salesforce, c: Connection, i: RunQuery| async move { s.query(&c).run_all(i.soql.as_deref().unwrap_or_default(), i.options).await as Result<QueryResult> }),

        // ── search ──
        operation("search.run", "Run a text search written in SOSL across object types. Use search.find to search without writing SOSL.", Read, &["api"],
            |s: Salesforce, c: Connection, i: RunSearch| async move { s.search(&c).run(&i.sosl).await as Result<SearchResult> }),
        operation("search.find", "Search records for some text. Name the object types to look in and the fields to return of each; without them only the ids of what is found come back.", Read, &["api"],
            |s: Salesforce, c: Connection, i: FindText| async move { s.search(&c).find(i.options).await as Result<SearchResult> }),

        // ── sobjects ──
        operation("sobjects.list", "List the object types of the organisation, Salesforce's own and those it defined, each with its API name and label. There are hundreds: give `contains` to find one.", Read, &["api"],
            |s: Salesforce, c: Connection, i: ObjectTypes| async move { s.sobjects(&c).list(i.options).await as Result<Vec<SObjectSummary>> }),
        operation("sobjects.describe", "Describe one object type: its fields with their types, picklist values and what they refer to, the object types that refer to it, and its record types. This is how to learn what a query may select and a record may hold.", Read, &["api"],
            |s: Salesforce, c: Connection, i: OneObject| async move { s.sobjects(&c).describe(&i.object).await as Result<Describe> }),

        // ── records ──
        operation("records.get", "Get one record by its id. Name the fields to return; without them every field comes back.", Read, &["api"],
            |s: Salesforce, c: Connection, i: OneRecord| async move { s.records(&c).get(&i.object, &i.id, i.options).await as Result<Record> }),
        operation("records.get_by_external_id", "Get one record by the id another system knows it by: the value of an external id field.", Read, &["api"],
            |s: Salesforce, c: Connection, i: ByExternalId| async move { s.records(&c).get_by_external_id(&i.object, &i.field, &i.value, i.options).await as Result<Record> }),
        operation("records.create", "Create a record of an object type from the fields given, and return its id. The organisation's own rules and automation run as they do for a person.", Write, &["api"],
            |s: Salesforce, c: Connection, i: NewRecord| async move { s.records(&c).create(&i.object, i.options).await as Result<Saved> }),
        operation("records.update", "Change the fields given on one record, leaving the others as they are. What a field held before is overwritten.", Write, &["api"],
            |s: Salesforce, c: Connection, i: ChangeRecord| async move { s.records(&c).update(&i.object, &i.id, i.options).await as Result<()> }),
        operation("records.upsert", "Create the record that holds a value in an external id field, or change it when it exists. Says which of the two it did.", Write, &["api"],
            |s: Salesforce, c: Connection, i: UpsertRecord| async move { s.records(&c).upsert(&i.object, &i.field, &i.value, i.options).await as Result<Saved> }),
        operation("records.delete", "Delete one record. It goes to the recycle bin, and records that depend on it may be deleted with it.", Destructive, &["api"],
            |s: Salesforce, c: Connection, i: ThisRecord| async move { s.records(&c).delete(&i.object, &i.id).await as Result<()> }),

        // ── limits ──
        operation("limits.get", "Report what is left of the organisation's allowances, above all its API requests for the last 24 hours, which every call here counts against.", Read, &["api"],
            |s: Salesforce, c: Connection, _: Nothing| async move { s.limits(&c).get().await as Result<Limits> }),
    ]
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn an_input_that_takes_nothing_is_closed_too() {
        let schema = closed(schema_of::<Nothing>());
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(
            unknown_field(&schema, &schema, &json!({ "verbose": true })),
            Some((String::new(), "verbose".to_owned()))
        );
        assert_eq!(unknown_field(&schema, &schema, &json!({})), None);
    }

    #[test]
    fn a_records_own_fields_are_left_open_and_everything_beside_them_is_closed() {
        let schema = closed(schema_of::<NewRecord>());
        let record = json!({ "object": "Account", "fields": { "Name": "Acme", "Anything__c": { "nested": [1, 2] } } });
        assert_eq!(unknown_field(&schema, &schema, &record), None);
        let misspelt = json!({ "object": "Account", "field": { "Name": "Acme" } });
        assert_eq!(
            unknown_field(&schema, &schema, &misspelt),
            Some((String::new(), "field".to_owned()))
        );
    }

    #[test]
    fn a_field_inside_a_list_of_options_is_placed() {
        let schema = closed(schema_of::<FindText>());
        let input =
            json!({ "text": "Acme", "objects": [{ "name": "Account" }, { "name": "Contact", "feilds": ["Name"] }] });
        assert_eq!(
            unknown_field(&schema, &schema, &input),
            Some(("objects[1]".to_owned(), "feilds".to_owned()))
        );
    }

    #[test]
    fn a_value_of_the_wrong_kind_is_placed_by_the_schema() {
        let query = closed(schema_of::<RunQuery>());
        let wrong = |input: Value| mistyped(&query, &input);
        assert_eq!(
            wrong(json!({ "soql": "SELECT Id FROM Account", "batch_size": 200 })),
            None
        );
        assert_eq!(
            wrong(json!({ "soql": "SELECT Id FROM Account", "cursor": null, "batch_size": null })),
            None
        );
        assert_eq!(wrong(json!({ "soql": 7 })).as_deref(), Some("soql"));
        // The query may be left out, for a batch that a cursor names.
        assert_eq!(wrong(json!({ "soql": null, "cursor": "x" })), None);
        assert_eq!(wrong(json!({ "cursor": "x" })), None);
        assert_eq!(
            wrong(json!({ "object": null })).as_deref(),
            None,
            "a field this input does not have"
        );
        assert_eq!(
            wrong(json!({ "soql": "x", "batch_size": "200" })).as_deref(),
            Some("batch_size")
        );
        assert_eq!(
            wrong(json!({ "soql": "x", "batch_size": 200.5 })).as_deref(),
            Some("batch_size")
        );
        assert_eq!(
            wrong(json!({ "soql": "x", "batch_size": -1 })).as_deref(),
            Some("batch_size")
        );
        assert_eq!(
            wrong(json!({ "soql": "x", "cursor": ["a"] })).as_deref(),
            Some("cursor")
        );

        let find = closed(schema_of::<FindText>());
        assert_eq!(mistyped(&find, &json!({ "text": "Acme", "within": "name" })), None);
        assert_eq!(mistyped(&find, &json!({ "text": "Acme", "within": null })), None);
        assert_eq!(
            mistyped(&find, &json!({ "text": "Acme", "within": "everywhere" })).as_deref(),
            Some("within")
        );
        assert_eq!(
            mistyped(&find, &json!({ "text": "Acme", "objects": "Account" })).as_deref(),
            Some("objects")
        );

        let record = closed(schema_of::<NewRecord>());
        assert_eq!(
            mistyped(&record, &json!({ "object": null, "fields": {} })).as_deref(),
            Some("object")
        );
        assert_eq!(
            mistyped(&record, &json!({ "object": "Account", "fields": ["Name"] })).as_deref(),
            Some("fields")
        );
        assert_eq!(
            mistyped(&record, &json!({ "object": "Account", "fields": { "Name": 7 } })),
            None
        );
    }

    #[test]
    fn a_name_that_does_not_look_like_one_is_not_repeated() {
        let refusal = not_a_field((String::new(), "my password is hunter2".to_owned()));
        assert!(!refusal.message().contains("hunter2"), "{}", refusal.message());
        let named = not_a_field(("objects[1]".to_owned(), "feilds".to_owned()));
        assert!(named.message().contains("`objects[1].feilds`"), "{}", named.message());
    }
}
