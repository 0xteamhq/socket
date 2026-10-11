//! Every typed method as a named operation.
//!
//! An operation's input is a JSON object with the method's plain arguments
//! and its options side by side: `{ "object_type": "contacts", "limit": 20 }`.
//! Both schemas are generated from the same types the typed methods use, so
//! the two ways of calling cannot drift apart.

use std::future::Future;
use std::sync::OnceLock;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use socketkit_core::{Connection, Effect, Error, ErrorKind, OperationInfo, Page, Result, schema_of};

use crate::HubSpot;
use crate::models::{
    Association, AssociationCreated, BatchCreate, BatchRead, BatchResult, BatchUpdate, CreateAssociation, CreateObject,
    GetObject, GetOwner, ListObjects, ListOwners, ListProperties, Object, Owner, Paging, Pipeline, Property,
    PropertySummary, Search, SearchResults, UpdateObject,
};

type Running = std::pin::Pin<Box<dyn Future<Output = Result<Value>> + Send>>;

/// One named operation: what it says about itself, and how to run it.
pub(crate) struct Operation {
    pub(crate) info: OperationInfo,
    run: Box<dyn Fn(HubSpot, Connection, Value) -> Running + Send + Sync>,
}

impl Operation {
    pub(crate) fn run(&self, hubspot: HubSpot, connection: Connection, input: Value) -> Running {
        (self.run)(hubspot, connection, input)
    }
}

fn invalid(message: String) -> Error {
    Error::new(ErrorKind::InvalidInput, message)
}

/// `schema` with every object in it closed: a field it does not list is not
/// allowed. The schema then says what [`unknown_field`] enforces.
///
/// The walk follows the schema's own structure and not every object in it.
/// A record's content is given under a field called `properties`, which is
/// also the word a schema lists its fields under, and the two must not be
/// taken for one another. A field that is a map, as that one is, lists no
/// fields and stays open.
fn closed(mut schema: Value) -> Value {
    fn close(node: &mut Value) {
        let Value::Object(schema) = node else { return };
        if let Some(Value::Object(fields)) = schema.get_mut("properties") {
            fields.values_mut().for_each(close);
            schema.insert("additionalProperties".to_owned(), Value::Bool(false));
        } else if let Some(values) = schema.get_mut("additionalProperties") {
            close(values);
        }
        for holding_one in ["items", "not"] {
            if let Some(inner) = schema.get_mut(holding_one) {
                close(inner);
            }
        }
        for holding_several in ["anyOf", "oneOf", "allOf", "prefixItems"] {
            if let Some(Value::Array(inner)) = schema.get_mut(holding_several) {
                inner.iter_mut().for_each(close);
            }
        }
        if let Some(Value::Object(defined)) = schema.get_mut("$defs") {
            defined.values_mut().for_each(close);
        }
    }
    close(&mut schema);
    schema
}

/// Where in `input` the first field is that the schema at `node` does not
/// list, and that field's name.
///
/// A field that is not known would be dropped in silence, with what it said:
/// a filter, a sort, the properties to return. The input types cannot refuse
/// one themselves, because their options are flattened into one object. A
/// field that is a map, such as a record's `properties`, lists no fields, so
/// whatever it holds is passed on to HubSpot, which knows the account's own.
fn unknown_field(root: &Value, node: &Value, input: &Value) -> Option<(String, String)> {
    // The schema of an object or a list, behind a reference or beside `null`.
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

/// Where in `input` the first value is that the schema at `node` does not
/// take: a number where text belongs, a word that is not one of a list.
/// `Some("")` is the value at `node` itself; `None` is nothing found.
///
/// serde says where a value could not be read, but loses the place inside
/// options that are flattened into the input, which here is nearly all of
/// them. A caller told only that "a field" is wrong has to guess which; an
/// amount sent as a number, where HubSpot takes text, is the likeliest.
fn wrong_kind(root: &Value, node: &Value, input: &Value) -> Option<String> {
    let mut node = node;
    for _ in 0..8 {
        match node["$ref"].as_str().and_then(|name| name.strip_prefix("#/$defs/")) {
            Some(name) => node = &root["$defs"][name],
            None => break,
        }
    }
    // One of several shapes: wrong only when it is wrong for every one of
    // them, and then the place is the deepest any of them reached.
    if let Some(arms) = node["anyOf"].as_array().or_else(|| node["oneOf"].as_array()) {
        let places: Option<Vec<String>> = arms.iter().map(|arm| wrong_kind(root, arm, input)).collect();
        return places.map(|places| places.into_iter().max_by_key(String::len).unwrap_or_default());
    }
    let kind = match input {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_f64() => "number",
        Value::Number(_) => "integer",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    let takes = |allowed: &str| allowed == kind || (allowed == "number" && kind == "integer");
    let fits = match &node["type"] {
        Value::String(allowed) => takes(allowed),
        Value::Array(allowed) => allowed.iter().filter_map(Value::as_str).any(takes),
        _ => true,
    };
    let listed = node["enum"].as_array().is_none_or(|values| values.contains(input));
    let exact = node.get("const").is_none_or(|value| value == input);
    if !(fits && listed && exact) {
        return Some(String::new());
    }
    let within = |place: String, inner: String| {
        let joint = if inner.is_empty() || inner.starts_with('[') {
            ""
        } else {
            "."
        };
        format!("{place}{joint}{inner}")
    };
    match input {
        Value::Object(fields) => fields.iter().find_map(|(name, value)| {
            // A field the schema names, or any field of a map.
            let schema = node["properties"]
                .get(name)
                .or_else(|| node.get("additionalProperties").filter(|values| values.is_object()))?;
            wrong_kind(root, schema, value).map(|inner| within(name.clone(), inner))
        }),
        Value::Array(items) => {
            let schema = node.get("items")?;
            items
                .iter()
                .enumerate()
                .find_map(|(at, item)| wrong_kind(root, schema, item).map(|inner| within(format!("[{at}]"), inner)))
        }
        _ => None,
    }
}

/// The refusal for a value of the wrong kind. The place may hold the name
/// of a property, which is the caller's own text, so it is repeated only
/// when it looks like a place.
fn not_that_kind(place: Option<String>) -> String {
    let shaped = |place: &String| {
        (1..=80).contains(&place.len())
            && place
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '@' | '.' | '-' | '[' | ']'))
    };
    match place.filter(shaped) {
        Some(place) => format!("`{place}` has the wrong type"),
        None => "the input has a field of the wrong type".to_owned(),
    }
}

/// Builds an operation from a typed handler. The input type gives the input
/// schema and the parsing; the output type gives the output schema.
fn operation<I, O, F, Fut>(name: &str, description: &str, effect: Effect, scopes: &[&str], handler: F) -> Operation
where
    I: DeserializeOwned + JsonSchema + Send + 'static,
    O: Serialize + JsonSchema + 'static,
    F: Fn(HubSpot, Connection, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<O>> + Send + 'static,
{
    let info = OperationInfo {
        name: format!("hubspot.{name}"),
        description: description.to_owned(),
        input_schema: closed(schema_of::<I>()),
        output_schema: schema_of::<O>(),
        effect,
        required_scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
    };
    let schema = info.input_schema.clone();
    let run = move |hubspot: HubSpot, connection: Connection, input: Value| -> Running {
        let provider = connection.provider().id.clone();
        if let Some(found) = unknown_field(&schema, &schema, &input) {
            return Box::pin(std::future::ready(Err(not_a_field(found).with_provider(provider))));
        }
        match serde_path_to_error::deserialize::<_, I>(&input) {
            Err(e) => {
                // serde's own message quotes the offending value, which may be
                // what a company keeps about a customer. Only the place of
                // the field goes into the error.
                let path = e.path().to_string();
                let inner = e.inner().to_string();
                let message = if inner.starts_with("missing field") {
                    inner
                } else if path == "." {
                    not_that_kind(wrong_kind(&schema, &schema, &input))
                } else {
                    not_that_kind(Some(path))
                };
                Box::pin(std::future::ready(Err(invalid(message).with_provider(provider))))
            }
            Ok(input) => {
                let output = handler(hubspot, connection, input);
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
    InType {
        /// The object type: `contacts`, `companies`, `deals`, `tickets`, `notes`, `calls`, `meetings`, `emails`, `tasks`, or a custom object's type id such as `2-3465404`.
        object_type: String
    } + ListObjects
);
input!(
    OneRecord {
        /// The object type, such as `contacts`.
        object_type: String,
        /// The record's id. With `idProperty`, the value of that property.
        id: String
    } + GetObject
);
input!(
    Records {
        /// The object type, such as `contacts`.
        object_type: String
    } + BatchRead
);
input!(
    Find {
        /// The object type, such as `contacts`.
        object_type: String
    } + Search
);
input!(
    New {
        /// The object type, such as `notes`.
        object_type: String
    } + CreateObject
);
input!(
    Change {
        /// The object type, such as `deals`.
        object_type: String,
        /// The record's id. With `idProperty`, the value of that property.
        id: String
    } + UpdateObject
);
input!(
    NewRecords {
        /// The object type, such as `contacts`.
        object_type: String
    } + BatchCreate
);
input!(
    Changes {
        /// The object type, such as `contacts`.
        object_type: String
    } + BatchUpdate
);
input!(ThisRecord {
    /// The object type, such as `contacts`.
    object_type: String,
    /// The record's id.
    id: String
});
input!(
    Associated {
        /// The object type of the record whose associations are listed, such as `contacts`.
        from_object_type: String,
        /// That record's id.
        from_id: String,
        /// The object type of the associated records to list, such as `companies`.
        to_object_type: String
    } + Paging
);
input!(
    Associate {
        /// The object type of the first record, such as `contacts`.
        from_object_type: String,
        /// The first record's id.
        from_id: String,
        /// The object type of the second record, such as `companies`.
        to_object_type: String,
        /// The second record's id.
        to_id: String
    } + CreateAssociation
);
input!(Pair {
    /// The object type of the first record, such as `contacts`.
    from_object_type: String,
    /// The first record's id.
    from_id: String,
    /// The object type of the second record, such as `companies`.
    to_object_type: String,
    /// The second record's id.
    to_id: String
});
input!(
    Fields {
        /// The object type whose properties to list, such as `contacts`.
        object_type: String
    } + ListProperties
);
input!(OneField {
    /// The object type, such as `contacts`.
    object_type: String,
    /// The property's internal name, such as `lifecyclestage`.
    name: String
});
input!(OfType {
    /// The object type: `deals` or `tickets`.
    object_type: String
});
input!(OnePipeline {
    /// The object type: `deals` or `tickets`.
    object_type: String,
    /// The pipeline's id, such as `default`.
    pipeline: String
});
input!(People {} + ListOwners);
input!(
    OneOwner {
        /// The owner's id. With `idProperty` set to `userId`, the id of the user behind the owner.
        owner: String
    } + GetOwner
);

// HubSpot grants access object by object, and these operations take the
// object type as an argument. So each lists the scopes for the four kinds
// of record every account has, which also cover the engagements: a notes,
// calls, meetings or tasks call goes by the contacts scope. An application
// that uses one object type can ask for less: `object_scopes` says what.

/// Reading contacts, companies, deals and tickets.
pub(crate) const READ: &[&str] = &[
    "crm.objects.contacts.read",
    "crm.objects.companies.read",
    "crm.objects.deals.read",
    "crm.objects.tickets.read",
];
/// Changing contacts, companies, deals and tickets.
const WRITE: &[&str] = &[
    "crm.objects.contacts.write",
    "crm.objects.companies.write",
    "crm.objects.deals.write",
    "crm.objects.tickets.write",
];
/// Reading how their properties are defined. HubSpot lists no schema scope
/// for tickets; the scope that reads tickets covers theirs.
const FIELDS: &[&str] = &[
    "crm.schemas.contacts.read",
    "crm.schemas.companies.read",
    "crm.schemas.deals.read",
    "crm.objects.tickets.read",
];
/// Reading the pipelines of the two object types that have them.
const STAGES: &[&str] = &["crm.objects.deals.read", "crm.objects.tickets.read"];
const OWNERS: &[&str] = &["crm.objects.owners.read"];

// `Write` adds something or changes fields that can be set back. `Destructive`
// is what takes something away: a record sent to the recycling bin, a link
// between two records removed. A host uses it to ask a person first.
use Effect::{Destructive, Read, Write};

/// Every HubSpot operation except identity and resource lookup, which the
/// integration adds itself.
pub(crate) fn all() -> &'static [Operation] {
    static ALL: OnceLock<Vec<Operation>> = OnceLock::new();
    ALL.get_or_init(build)
}

#[rustfmt::skip]
fn build() -> Vec<Operation> {
    vec![
        // ── objects ──
        operation("objects.list", "List the records of an object type: contacts, companies, deals, tickets, notes, calls, meetings, emails, tasks, or a custom object. Each record carries only the properties asked for; hubspot.properties.list names the ones there are.", Read, READ,
            |h: HubSpot, c: Connection, i: InType| async move { h.objects(&c).list(&i.object_type, i.options).await as Result<Page<Object>> }),
        operation("objects.get", "Get one record by its id, or by the value of a unique property such as a contact's email, with the properties, the history and the associated record ids asked for.", Read, READ,
            |h: HubSpot, c: Connection, i: OneRecord| async move { h.objects(&c).get(&i.object_type, &i.id, i.options).await as Result<Object> }),
        // HubSpot offers these two only as POST. They read records and change nothing.
        operation("objects.batch_read", "Read up to 100 records by their ids, or by the values of a unique property. Changes nothing. The records that do not exist are named in the result's errors.", Read, READ,
            |h: HubSpot, c: Connection, i: Records| async move { h.objects(&c).batch_read(&i.object_type, i.options).await as Result<BatchResult> }),
        operation("objects.search", "Search the records of an object type by words, by conditions on their properties, or both, with a sort. Changes nothing. Returns the first 10,000 matches at most, and HubSpot allows five searches a second.", Read, READ,
            |h: HubSpot, c: Connection, i: Find| async move { h.objects(&c).search(&i.object_type, i.options).await as Result<SearchResults> }),
        operation("objects.create", "Create a record: a contact, a deal, a ticket, or an engagement such as a note or a task, associated with the records it belongs to.", Write, WRITE,
            |h: HubSpot, c: Connection, i: New| async move { h.objects(&c).create(&i.object_type, i.options).await as Result<Object> }),
        operation("objects.update", "Change the properties given on a record and leave the rest: a deal's stage, a ticket's status, a contact's owner.", Write, WRITE,
            |h: HubSpot, c: Connection, i: Change| async move { h.objects(&c).update(&i.object_type, &i.id, i.options).await as Result<Object> }),
        operation("objects.batch_create", "Create up to 100 records of one object type in one call.", Write, WRITE,
            |h: HubSpot, c: Connection, i: NewRecords| async move { h.objects(&c).batch_create(&i.object_type, i.options).await as Result<BatchResult> }),
        operation("objects.batch_update", "Change up to 100 records of one object type in one call, each by the properties given for it.", Write, WRITE,
            |h: HubSpot, c: Connection, i: Changes| async move { h.objects(&c).batch_update(&i.object_type, i.options).await as Result<BatchResult> }),
        operation("objects.archive", "Move a record to HubSpot's recycling bin, where a person can restore it for a time. The record leaves every list and search.", Destructive, WRITE,
            |h: HubSpot, c: Connection, i: ThisRecord| async move { h.objects(&c).archive(&i.object_type, &i.id).await as Result<()> }),

        // ── associations ──
        operation("associations.list", "List the records of one object type that a record is associated with, such as a contact's companies or a deal's notes, with the labels of each association.", Read, READ,
            |h: HubSpot, c: Connection, i: Associated| async move { h.associations(&c).list(&i.from_object_type, &i.from_id, &i.to_object_type, i.options).await as Result<Page<Association>> }),
        operation("associations.create", "Associate two records. Without types it is the plain association between the two object types; with them, the labels given replace those the association has.", Write, WRITE,
            |h: HubSpot, c: Connection, i: Associate| async move { h.associations(&c).create(&i.from_object_type, &i.from_id, &i.to_object_type, &i.to_id, i.options).await as Result<AssociationCreated> }),
        operation("associations.remove", "Remove every association between two records, labelled or not. The records themselves stay.", Destructive, WRITE,
            |h: HubSpot, c: Connection, i: Pair| async move { h.associations(&c).remove(&i.from_object_type, &i.from_id, &i.to_object_type, &i.to_id).await as Result<()> }),

        // ── properties ──
        operation("properties.list", "List every property of an object type in this account: its internal name, its label and the kind of value it holds. A record returns only the properties it is asked for, and this is where their names are found.", Read, FIELDS,
            |h: HubSpot, c: Connection, i: Fields| async move { h.properties(&c).list(&i.object_type, i.options).await as Result<Vec<PropertySummary>> }),
        operation("properties.get", "Get one property of an object type, with its description and the values it can take.", Read, FIELDS,
            |h: HubSpot, c: Connection, i: OneField| async move { h.properties(&c).get(&i.object_type, &i.name).await as Result<Property> }),

        // ── pipelines ──
        operation("pipelines.list", "List the pipelines of deals or of tickets, each with its stages. A stage's id is what a deal's dealstage, or a ticket's hs_pipeline_stage, is set to.", Read, STAGES,
            |h: HubSpot, c: Connection, i: OfType| async move { h.pipelines(&c).list(&i.object_type).await as Result<Vec<Pipeline>> }),
        operation("pipelines.get", "Get one pipeline of deals or of tickets, with its stages.", Read, STAGES,
            |h: HubSpot, c: Connection, i: OnePipeline| async move { h.pipelines(&c).get(&i.object_type, &i.pipeline).await as Result<Pipeline> }),

        // ── owners ──
        operation("owners.list", "List the people and queues records can be assigned to, or find the one with an email address. An owner's id is what a record's hubspot_owner_id is set to.", Read, OWNERS,
            |h: HubSpot, c: Connection, i: People| async move { h.owners(&c).list(i.options).await as Result<Page<Owner>> }),
        operation("owners.get", "Get one owner, by the owner's id or by the id of the user behind it.", Read, OWNERS,
            |h: HubSpot, c: Connection, i: OneOwner| async move { h.owners(&c).get(&i.owner, i.options).await as Result<Owner> }),
    ]
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn schema() -> Value {
        closed(schema_of::<Find>())
    }

    #[test]
    fn closing_a_schema_leaves_a_map_of_properties_open_and_adds_no_field() {
        let schema = closed(schema_of::<New>());
        assert_eq!(schema["additionalProperties"], false);
        let fields = schema["properties"].as_object().unwrap();
        let mut names: Vec<&str> = fields.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, ["associations", "object_type", "properties"]);
        // The record's own fields: a map, whose values are text.
        assert_eq!(
            fields["properties"]["additionalProperties"],
            json!({ "type": "string" })
        );
        // What is nested is closed too.
        assert_eq!(schema["$defs"]["NewAssociation"]["additionalProperties"], false);
        assert_eq!(schema["$defs"]["ObjectId"]["additionalProperties"], false);
    }

    #[test]
    fn a_value_of_the_wrong_kind_is_found_wherever_it_is() {
        let at = |input: Value| wrong_kind(&schema(), &schema(), &input);
        assert_eq!(at(json!({ "object_type": "deals", "limit": 20, "query": "x" })), None);
        assert_eq!(
            at(json!({ "object_type": "deals", "limit": "ten" })).as_deref(),
            Some("limit")
        );
        assert_eq!(
            at(json!({ "object_type": "deals", "limit": 2.5 })).as_deref(),
            Some("limit")
        );
        assert_eq!(
            at(json!({ "object_type": "deals", "properties": "name" })).as_deref(),
            Some("properties")
        );
        assert_eq!(
            at(json!({ "object_type": "deals", "properties": ["name", 7] })).as_deref(),
            Some("properties[1]")
        );
        let filtered = |filter: Value| at(json!({ "object_type": "deals", "filterGroups": [{ "filters": [filter] }] }));
        assert_eq!(
            filtered(json!({ "propertyName": "amount", "operator": "GT", "value": "1" })),
            None
        );
        assert_eq!(
            filtered(json!({ "propertyName": "amount", "operator": "LIKE" })).as_deref(),
            Some("filterGroups[0].filters[0].operator")
        );
        assert_eq!(
            filtered(json!({ "propertyName": "amount", "operator": "GT", "value": 1 })).as_deref(),
            Some("filterGroups[0].filters[0].value")
        );
        assert_eq!(
            filtered(json!({ "propertyName": "amount", "operator": "IN", "values": "a,b" })).as_deref(),
            Some("filterGroups[0].filters[0].values")
        );
        // What may be left out may also be null.
        assert_eq!(
            at(json!({ "object_type": "deals", "query": null, "sorts": null })),
            None
        );
        assert_eq!(at(json!("deals")).as_deref(), Some(""));

        let record = closed(schema_of::<New>());
        let amount = json!({ "object_type": "deals", "properties": { "dealname": "Renewal", "amount": 4800 } });
        assert_eq!(
            wrong_kind(&record, &record, &amount).as_deref(),
            Some("properties.amount")
        );
    }

    #[test]
    fn a_place_is_repeated_only_when_it_looks_like_one() {
        assert_eq!(
            not_that_kind(Some("properties.amount".into())),
            "`properties.amount` has the wrong type"
        );
        assert_eq!(
            not_that_kind(Some("sorts[0].direction".into())),
            "`sorts[0].direction` has the wrong type"
        );
        for unsafe_to_repeat in ["", "properties.ada@example.com wrote: hello", "properties.`x`"] {
            assert_eq!(
                not_that_kind(Some(unsafe_to_repeat.into())),
                "the input has a field of the wrong type",
                "{unsafe_to_repeat:?}"
            );
        }
        assert_eq!(not_that_kind(None), "the input has a field of the wrong type");
        assert_eq!(
            not_that_kind(Some("a".repeat(81))),
            "the input has a field of the wrong type"
        );
    }

    #[test]
    fn an_unknown_field_is_found_and_a_property_of_a_record_is_not_one() {
        let record = closed(schema_of::<New>());
        let found = |input: Value| unknown_field(&record, &record, &input);
        assert_eq!(
            found(json!({ "object_type": "notes", "properties": { "anything": "x", "properties": "y" } })),
            None
        );
        assert_eq!(
            found(json!({ "object_type": "notes", "properties": {}, "assocations": [] })),
            Some((String::new(), "assocations".into()))
        );
        assert_eq!(
            found(
                json!({ "object_type": "notes", "properties": {}, "associations": [{ "to": { "id": "1", "kind": "x" }, "types": [] }] })
            ),
            Some(("associations[0].to".into(), "kind".into()))
        );
    }
}
