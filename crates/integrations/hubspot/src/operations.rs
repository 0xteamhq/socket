//! Every typed method as a named operation.
//!
//! An operation's input is a JSON object with the method's plain arguments
//! and its options side by side: `{ "object_type": "contacts", "limit": 10 }`.
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
    Associated, Association, BatchCreate, BatchRead, BatchResult, BatchUpdate, CreateAssociation, CreateObject,
    GetObject, GetOwner, ListObjects, ListOwners, ListProperties, Owner, Paging, Pipeline, Property, Record, Search,
    UpdateObject,
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
/// The schema is walked as a schema, and not as any JSON: a record's
/// `properties` is a field of that name, and a map of field names is not
/// itself an object to close. A map such as `properties`, which lists no
/// fields of its own, stays open to any name.
fn closed(mut schema: Value) -> Value {
    fn close(node: &mut Value) {
        let Value::Object(schema) = node else { return };
        if schema.get("properties").is_some_and(Value::is_object) {
            schema.insert("additionalProperties".to_owned(), Value::Bool(false));
        }
        for holder in ["properties", "$defs"] {
            if let Some(Value::Object(schemas)) = schema.get_mut(holder) {
                schemas.values_mut().for_each(close);
            }
        }
        for list in ["anyOf", "oneOf", "allOf"] {
            if let Some(Value::Array(schemas)) = schema.get_mut(list) {
                schemas.iter_mut().for_each(close);
            }
        }
        for one in ["items", "additionalProperties"] {
            if let Some(inner) = schema.get_mut(one) {
                close(inner);
            }
        }
    }
    close(&mut schema);
    schema
}

/// Where in `input` the first field is that the schema at `node` does not
/// list, and that field's name.
///
/// A field that is not known would be dropped in silence, with what it said:
/// a filter, a property to return, the record to associate. The input types
/// cannot refuse one themselves, because their options are flattened into
/// one object.
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
        match serde_path_to_error::deserialize::<_, I>(input) {
            Err(e) => {
                // serde's own message quotes the offending value, which may be
                // the content of a record or a credential. Only the field's
                // name, which comes from our own types, goes into the error.
                let path = e.path().to_string();
                let inner = e.inner().to_string();
                let message = if inner.starts_with("missing field") {
                    inner
                } else if path == "." {
                    "the input has a field of the wrong type".to_owned()
                } else {
                    format!("`{path}` has the wrong type")
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

input!(OfType {
    /// The object type: `contacts`, `companies`, `deals`, `tickets`, `notes`, `calls`, `meetings`, `emails`, `tasks`, or a custom object's type id such as `2-12345`.
    object_type: String
});
input!(
    Records {
        /// The object type: `contacts`, `companies`, `deals`, `tickets`, `notes`, `calls`, `meetings`, `emails`, `tasks`, or a custom object's type id such as `2-12345`.
        object_type: String
    } + ListObjects
);
input!(
    OneRecord {
        /// The object type, such as `contacts`.
        object_type: String,
        /// The record's id, or its value of `idProperty` when that is given.
        record: String
    } + GetObject
);
input!(
    ManyRecords {
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
    NewRecord {
        /// The object type, such as `contacts`.
        object_type: String
    } + CreateObject
);
input!(
    Change {
        /// The object type, such as `contacts`.
        object_type: String,
        /// The record's id, or its value of `idProperty` when that is given.
        record: String
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
    record: String
});

input!(
    Linked {
        /// The object type of the record whose associations are listed, such as `contacts`.
        object_type: String,
        /// That record's id.
        record: String,
        /// The object type of the associated records to list, such as `companies`.
        to_object_type: String
    } + Paging
);
input!(
    Link {
        /// The object type of the first record, such as `contacts`.
        object_type: String,
        /// The first record's id.
        record: String,
        /// The object type of the second record, such as `companies`.
        to_object_type: String,
        /// The second record's id.
        to_record: String
    } + CreateAssociation
);
input!(Unlink {
    /// The object type of the first record, such as `contacts`.
    object_type: String,
    /// The first record's id.
    record: String,
    /// The object type of the second record, such as `companies`.
    to_object_type: String,
    /// The second record's id.
    to_record: String
});

input!(
    Fields {
        /// The object type whose properties are listed, such as `contacts`.
        object_type: String
    } + ListProperties
);
input!(OneField {
    /// The object type, such as `contacts`.
    object_type: String,
    /// The property's internal name, such as `lifecyclestage`.
    property: String
});
input!(OnePipeline {
    /// The object type: `deals` or `tickets`.
    object_type: String,
    /// The pipeline's id.
    pipeline: String
});
input!(People {} + ListOwners);
input!(
    OneOwner {
        /// The owner's id, as a record's `hubspot_owner_id` holds it.
        owner: String
    } + GetOwner
);

// `Destructive` is anything that deletes or removes: archiving a record, and
// taking an association away. Setting a property is a `Write`. A host uses
// the effect to ask a person first.
use Effect::{Destructive, Read, Write};

/// Every HubSpot operation except identity and resource lookup, which the
/// integration adds itself.
pub(crate) fn all() -> &'static [Operation] {
    static ALL: OnceLock<Vec<Operation>> = OnceLock::new();
    ALL.get_or_init(build)
}

// The scope an operation needs depends on the object type it is given, which
// is an argument. So each description says how the scope is formed, and
// `required_scopes` holds only what is the same for every object type.
#[rustfmt::skip]
fn build() -> Vec<Operation> {
    vec![
        // ── objects ──
        operation("objects.list", "List the records of an object type, each with only the properties asked for. Needs the type's read scope: crm.objects.{type}.read.", Read, &[],
            |h: HubSpot, c: Connection, i: Records| async move { h.objects(&c).list(&i.object_type, i.options).await as Result<Page<Record>> }),
        operation("objects.get", "Get one record, with the properties and the ids of associated records asked for. Needs the type's read scope: crm.objects.{type}.read.", Read, &[],
            |h: HubSpot, c: Connection, i: OneRecord| async move { h.objects(&c).get(&i.object_type, &i.record, i.options).await as Result<Record> }),
        // HubSpot offers these two only as POST. They read records and change nothing.
        operation("objects.batch_read", "Read up to 100 records at once, by id or by a property with unique values. Changes nothing. A record that does not exist is reported under errors. Needs the type's read scope: crm.objects.{type}.read.", Read, &[],
            |h: HubSpot, c: Connection, i: ManyRecords| async move { h.objects(&c).batch_read(&i.object_type, i.options).await as Result<BatchResult> }),
        operation("objects.search", "Search the records of an object type by filters, by words, or both, with one sort. Changes nothing. Returns at most 10,000 results for one search. Needs the type's read scope: crm.objects.{type}.read.", Read, &[],
            |h: HubSpot, c: Connection, i: Find| async move { h.objects(&c).search(&i.object_type, i.options).await as Result<Page<Record>> }),
        operation("objects.create", "Create a record, and associate it with the records named. Needs the type's write scope: crm.objects.{type}.write.", Write, &[],
            |h: HubSpot, c: Connection, i: NewRecord| async move { h.objects(&c).create(&i.object_type, i.options).await as Result<Record> }),
        operation("objects.update", "Change a record, setting the properties given and leaving the rest. Needs the type's write scope: crm.objects.{type}.write.", Write, &[],
            |h: HubSpot, c: Connection, i: Change| async move { h.objects(&c).update(&i.object_type, &i.record, i.options).await as Result<Record> }),
        operation("objects.batch_create", "Create up to 100 records at once. Needs the type's write scope: crm.objects.{type}.write.", Write, &[],
            |h: HubSpot, c: Connection, i: NewRecords| async move { h.objects(&c).batch_create(&i.object_type, i.options).await as Result<BatchResult> }),
        operation("objects.batch_update", "Change up to 100 records at once, setting the properties given and leaving the rest. Needs the type's write scope: crm.objects.{type}.write.", Write, &[],
            |h: HubSpot, c: Connection, i: Changes| async move { h.objects(&c).batch_update(&i.object_type, i.options).await as Result<BatchResult> }),
        operation("objects.archive", "Move a record to the recycling bin, where HubSpot keeps it for 90 days. Needs the type's write scope: crm.objects.{type}.write.", Destructive, &[],
            |h: HubSpot, c: Connection, i: ThisRecord| async move { h.objects(&c).archive(&i.object_type, &i.record).await as Result<()> }),

        // ── associations ──
        operation("associations.list", "List the records of one type that a record is associated with, and the kinds of association between them. Needs the read scope of both object types.", Read, &[],
            |h: HubSpot, c: Connection, i: Linked| async move { h.associations(&c).list(&i.object_type, &i.record, &i.to_object_type, i.options).await as Result<Page<Association>> }),
        operation("associations.create", "Associate two records: with HubSpot's default association, or with the kinds given in types. The kinds given become all the labels between the two records, so a label that was there and is not given is removed. Making a default association that is already there changes nothing. Needs the write scope of both object types.", Write, &[],
            |h: HubSpot, c: Connection, i: Link| async move { h.associations(&c).create(&i.object_type, &i.record, &i.to_object_type, &i.to_record, i.options).await as Result<Associated> }),
        operation("associations.remove", "Remove every association between two records. The records themselves stay. Needs the write scope of both object types.", Destructive, &[],
            |h: HubSpot, c: Connection, i: Unlink| async move { h.associations(&c).remove(&i.object_type, &i.record, &i.to_object_type, &i.to_record).await as Result<()> }),

        // ── properties ──
        operation("properties.list", "List every property of an object type: its internal name, label, type and options. A record returns only the properties asked for, so this is how to learn their names. Needs the type's schema scope, crm.schemas.{type}.read, or its read scope.", Read, &[],
            |h: HubSpot, c: Connection, i: Fields| async move { h.properties(&c).list(&i.object_type, i.options).await as Result<Vec<Property>> }),
        operation("properties.get", "Get one property of an object type by its internal name, with its type and options. Needs the type's schema scope, crm.schemas.{type}.read, or its read scope.", Read, &[],
            |h: HubSpot, c: Connection, i: OneField| async move { h.properties(&c).get(&i.object_type, &i.property).await as Result<Property> }),

        // ── pipelines ──
        operation("pipelines.list", "List the pipelines of an object type, deals or tickets, each with its stages. Needs the type's read scope: crm.objects.{type}.read.", Read, &[],
            |h: HubSpot, c: Connection, i: OfType| async move { h.pipelines(&c).list(&i.object_type).await as Result<Vec<Pipeline>> }),
        operation("pipelines.get", "Get one pipeline of deals or tickets, with its stages. Needs the type's read scope: crm.objects.{type}.read.", Read, &[],
            |h: HubSpot, c: Connection, i: OnePipeline| async move { h.pipelines(&c).get(&i.object_type, &i.pipeline).await as Result<Pipeline> }),

        // ── owners ──
        operation("owners.list", "List the people and queues records can be assigned to, or the one with an email address.", Read, &["crm.objects.owners.read"],
            |h: HubSpot, c: Connection, i: People| async move { h.owners(&c).list(i.options).await as Result<Page<Owner>> }),
        operation("owners.get", "Get one owner, by the id a record's hubspot_owner_id holds or by the id of the user behind it.", Read, &["crm.objects.owners.read"],
            |h: HubSpot, c: Connection, i: OneOwner| async move { h.owners(&c).get(&i.owner, i.options).await as Result<Owner> }),
    ]
}
