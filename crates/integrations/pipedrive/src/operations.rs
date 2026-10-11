//! Every typed method as a named operation.
//!
//! An operation's input is a JSON object with the method's plain arguments
//! and its options side by side: `{ "deal": 42, "stage_id": 3 }`.
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

use crate::Pipedrive;
use crate::models::{
    Activity, CreateActivity, CreateDeal, CreateLead, CreateNote, CreateOrganization, CreatePerson, Deal, Field, Lead,
    ListActivities, ListDeals, ListLeads, ListNotes, ListOrganizations, ListPersons, Note, Organization, Paging,
    Person, Pipeline, SearchDeals, SearchItems, SearchLeads, SearchOrganizations, SearchPersons, SearchResult, Stage,
    UpdateActivity, UpdateDeal, UpdateLead, UpdateNote, UpdateOrganization, UpdatePerson, User,
};

type Running = std::pin::Pin<Box<dyn Future<Output = Result<Value>> + Send>>;

/// One named operation: what it says about itself, and how to run it.
pub(crate) struct Operation {
    pub(crate) info: OperationInfo,
    run: Box<dyn Fn(Pipedrive, Connection, Value) -> Running + Send + Sync>,
}

impl Operation {
    pub(crate) fn run(&self, pipedrive: Pipedrive, connection: Connection, input: Value) -> Running {
        (self.run)(pipedrive, connection, input)
    }
}

fn invalid(message: String) -> Error {
    Error::new(ErrorKind::InvalidInput, message)
}

/// `schema` with every object in it closed: a field it does not list is not
/// allowed. The schema then says what [`unknown_field`] enforces.
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
    // An input with no fields lists none, and so allows none.
    if let Value::Object(root) = &mut schema {
        root.entry("properties")
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
    }
    close(&mut schema);
    schema
}

/// Where in `input` the first field is that the schema at `node` does not
/// list, and that field's name.
///
/// A field that is not known would be dropped in silence, with what it said:
/// a stage, a value, the person a note is about. The input types cannot refuse one
/// themselves, because their options are flattened into one object.
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
    F: Fn(Pipedrive, Connection, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<O>> + Send + 'static,
{
    let info = OperationInfo {
        name: format!("pipedrive.{name}"),
        description: description.to_owned(),
        input_schema: closed(schema_of::<I>()),
        output_schema: schema_of::<O>(),
        effect,
        required_scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
    };
    let schema = info.input_schema.clone();
    let run = move |pipedrive: Pipedrive, connection: Connection, input: Value| -> Running {
        let provider = connection.provider().id.clone();
        if let Some(found) = unknown_field(&schema, &schema, &input) {
            return Box::pin(std::future::ready(Err(not_a_field(found).with_provider(provider))));
        }
        match serde_path_to_error::deserialize::<_, I>(input) {
            Err(e) => {
                // serde's own message quotes the offending value, which may be
                // the text of a note or a credential. Only the field's name,
                // which comes from our own types, goes into the error.
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
                let output = handler(pipedrive, connection, input);
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

input!(Nothing {});
input!(Listing {} + Paging);

input!(OneDeal {
    /// A deal's id.
    deal: u64
});
input!(Deals {} + ListDeals);
input!(FindDeals {} + SearchDeals);
input!(NewDeal {} + CreateDeal);
input!(
    ChangeDeal {
        /// The id of the deal to change.
        deal: u64
    } + UpdateDeal
);

input!(OnePerson {
    /// A person's id.
    person: u64
});
input!(Persons {} + ListPersons);
input!(FindPersons {} + SearchPersons);
input!(NewPerson {} + CreatePerson);
input!(
    ChangePerson {
        /// The id of the person to change.
        person: u64
    } + UpdatePerson
);

input!(OneOrganization {
    /// An organization's id.
    organization: u64
});
input!(Organizations {} + ListOrganizations);
input!(FindOrganizations {} + SearchOrganizations);
input!(NewOrganization {} + CreateOrganization);
input!(
    ChangeOrganization {
        /// The id of the organization to change.
        organization: u64
    } + UpdateOrganization
);

input!(OneLead {
    /// A lead's id, which is a UUID.
    lead: String
});
input!(Leads {} + ListLeads);
input!(FindLeads {} + SearchLeads);
input!(NewLead {} + CreateLead);
input!(
    ChangeLead {
        /// The id of the lead to change, which is a UUID.
        lead: String
    } + UpdateLead
);

input!(OneActivity {
    /// An activity's id.
    activity: u64
});
input!(Activities {} + ListActivities);
input!(NewActivity {} + CreateActivity);
input!(
    ChangeActivity {
        /// The id of the activity to change.
        activity: u64
    } + UpdateActivity
);

input!(OneNote {
    /// A note's id.
    note: u64
});
input!(Notes {} + ListNotes);
input!(NewNote {} + CreateNote);
input!(
    ChangeNote {
        /// The id of the note to change.
        note: u64
    } + UpdateNote
);

input!(
    Stages {
        /// A pipeline's id. The stages of every pipeline when not given.
        pipeline: Option<u64>
    } + Paging
);
input!(Items {} + SearchItems);

// `Read` changes nothing. `Write` creates a record or changes one, and what it
// changed can be changed back. `Destructive` deletes. A host lets a read run
// freely and asks a person before the other two.
use Effect::{Destructive, Read, Write};

/// Every Pipedrive operation except identity and resource lookup, which the
/// integration adds itself.
pub(crate) fn all() -> &'static [Operation] {
    static ALL: OnceLock<Vec<Operation>> = OnceLock::new();
    ALL.get_or_init(build)
}

// The scopes named are the least that serve each operation. With a personal
// API token there are no scopes: the token can do whatever its user can.
#[rustfmt::skip]
fn build() -> Vec<Operation> {
    vec![
        // ── deals ──
        operation("deals.list", "List deals: all that are not deleted, or those of one owner, person, organization, pipeline, stage or status. Rows carry no custom fields unless some are asked for.", Read, &["deals:read"],
            |p: Pipedrive, c: Connection, i: Deals| async move { p.deals(&c).list(i.options).await as Result<Page<Deal>> }),
        operation("deals.get", "Get one deal, with its custom fields under their keys. fields.deal_fields names them.", Read, &["deals:read"],
            |p: Pipedrive, c: Connection, i: OneDeal| async move { p.deals(&c).get(i.deal).await as Result<Deal> }),
        operation("deals.search", "Search deals by title, notes and custom fields. Returns matches, best first, not whole deals.", Read, &["deals:read"],
            |p: Pipedrive, c: Connection, i: FindDeals| async move { p.deals(&c).search(i.options).await as Result<Page<SearchResult>> }),
        operation("deals.create", "Create a deal.", Write, &["deals:full"],
            |p: Pipedrive, c: Connection, i: NewDeal| async move { p.deals(&c).create(i.options).await as Result<Deal> }),
        operation("deals.update", "Change a deal: move it to another stage, mark it won or lost, or change any field given. Fields not given are left as they are.", Write, &["deals:full"],
            |p: Pipedrive, c: Connection, i: ChangeDeal| async move { p.deals(&c).update(i.deal, i.options).await as Result<Deal> }),
        operation("deals.delete", "Delete a deal. Pipedrive keeps it for 30 days, then removes it for good.", Destructive, &["deals:full"],
            |p: Pipedrive, c: Connection, i: OneDeal| async move { p.deals(&c).delete(i.deal).await as Result<()> }),

        // ── persons ──
        operation("persons.list", "List persons: all that are not deleted, or those of one owner, organization or deal. Rows carry no custom fields unless some are asked for.", Read, &["contacts:read"],
            |p: Pipedrive, c: Connection, i: Persons| async move { p.persons(&c).list(i.options).await as Result<Page<Person>> }),
        operation("persons.get", "Get one person, with their custom fields under their keys. fields.person_fields names them.", Read, &["contacts:read"],
            |p: Pipedrive, c: Connection, i: OnePerson| async move { p.persons(&c).get(i.person).await as Result<Person> }),
        operation("persons.search", "Search persons by name, email, phone, notes and custom fields. Returns matches, best first, not whole persons.", Read, &["contacts:read"],
            |p: Pipedrive, c: Connection, i: FindPersons| async move { p.persons(&c).search(i.options).await as Result<Page<SearchResult>> }),
        operation("persons.create", "Create a person.", Write, &["contacts:full"],
            |p: Pipedrive, c: Connection, i: NewPerson| async move { p.persons(&c).create(i.options).await as Result<Person> }),
        operation("persons.update", "Change a person. Fields not given are left as they are; a list of emails, phones or labels replaces the one that was there.", Write, &["contacts:full"],
            |p: Pipedrive, c: Connection, i: ChangePerson| async move { p.persons(&c).update(i.person, i.options).await as Result<Person> }),
        operation("persons.delete", "Delete a person. Pipedrive keeps them for 30 days, then removes them for good.", Destructive, &["contacts:full"],
            |p: Pipedrive, c: Connection, i: OnePerson| async move { p.persons(&c).delete(i.person).await as Result<()> }),

        // ── organizations ──
        operation("organizations.list", "List organizations: all that are not deleted, or those of one owner. Rows carry no custom fields unless some are asked for.", Read, &["contacts:read"],
            |p: Pipedrive, c: Connection, i: Organizations| async move { p.organizations(&c).list(i.options).await as Result<Page<Organization>> }),
        operation("organizations.get", "Get one organization, with its custom fields under their keys. fields.organization_fields names them.", Read, &["contacts:read"],
            |p: Pipedrive, c: Connection, i: OneOrganization| async move { p.organizations(&c).get(i.organization).await as Result<Organization> }),
        operation("organizations.search", "Search organizations by name, address, notes and custom fields. Returns matches, best first, not whole organizations.", Read, &["contacts:read"],
            |p: Pipedrive, c: Connection, i: FindOrganizations| async move { p.organizations(&c).search(i.options).await as Result<Page<SearchResult>> }),
        operation("organizations.create", "Create an organization.", Write, &["contacts:full"],
            |p: Pipedrive, c: Connection, i: NewOrganization| async move { p.organizations(&c).create(i.options).await as Result<Organization> }),
        operation("organizations.update", "Change an organization. Fields not given are left as they are.", Write, &["contacts:full"],
            |p: Pipedrive, c: Connection, i: ChangeOrganization| async move { p.organizations(&c).update(i.organization, i.options).await as Result<Organization> }),
        operation("organizations.delete", "Delete an organization. Pipedrive keeps it for 30 days, then removes it for good.", Destructive, &["contacts:full"],
            |p: Pipedrive, c: Connection, i: OneOrganization| async move { p.organizations(&c).delete(i.organization).await as Result<()> }),

        // ── leads ──
        operation("leads.list", "List the leads that are not archived: all of them, or those of one owner, person or organization. Rows carry no custom fields.", Read, &["leads:read"],
            |p: Pipedrive, c: Connection, i: Leads| async move { p.leads(&c).list(i.options).await as Result<Page<Lead>> }),
        operation("leads.get", "Get one lead, with its custom fields under their keys. A lead has a deal's fields; fields.deal_fields names them.", Read, &["leads:read"],
            |p: Pipedrive, c: Connection, i: OneLead| async move { p.leads(&c).get(&i.lead).await as Result<Lead> }),
        operation("leads.search", "Search leads by title, notes and custom fields. Returns matches, best first, not whole leads.", Read, &["leads:read"],
            |p: Pipedrive, c: Connection, i: FindLeads| async move { p.leads(&c).search(i.options).await as Result<Page<SearchResult>> }),
        operation("leads.create", "Create a lead, linked to a person, an organization or both.", Write, &["leads:full"],
            |p: Pipedrive, c: Connection, i: NewLead| async move { p.leads(&c).create(i.options).await as Result<Lead> }),
        operation("leads.update", "Change a lead, or archive it. Fields not given are left as they are.", Write, &["leads:full"],
            |p: Pipedrive, c: Connection, i: ChangeLead| async move { p.leads(&c).update(&i.lead, i.options).await as Result<Lead> }),
        operation("leads.delete", "Delete a lead.", Destructive, &["leads:full"],
            |p: Pipedrive, c: Connection, i: OneLead| async move { p.leads(&c).delete(&i.lead).await as Result<()> }),

        // ── activities ──
        operation("activities.list", "List activities (calls, meetings, tasks, emails): all that are not deleted, or those of one owner, deal, lead, person or organization, done or not. Rows leave out note, public_description and attendees: those fields are absent from a row, not empty, and activities.get returns them.", Read, &["activities:read"],
            |p: Pipedrive, c: Connection, i: Activities| async move { p.activities(&c).list(i.options).await as Result<Page<Activity>> }),
        operation("activities.get", "Get one activity, with its note, its public_description and its attendees.", Read, &["activities:read"],
            |p: Pipedrive, c: Connection, i: OneActivity| async move { p.activities(&c).get(i.activity).await as Result<Activity> }),
        operation("activities.create", "Create an activity: a task to do, or a call, a meeting or an email that took place.", Write, &["activities:full"],
            |p: Pipedrive, c: Connection, i: NewActivity| async move { p.activities(&c).create(i.options).await as Result<Activity> }),
        operation("activities.update", "Change an activity, or mark it done. Fields not given are left as they are.", Write, &["activities:full"],
            |p: Pipedrive, c: Connection, i: ChangeActivity| async move { p.activities(&c).update(i.activity, i.options).await as Result<Activity> }),
        operation("activities.delete", "Delete an activity. Pipedrive keeps it for 30 days, then removes it for good.", Destructive, &["activities:full"],
            |p: Pipedrive, c: Connection, i: OneActivity| async move { p.activities(&c).delete(i.activity).await as Result<()> }),

        // ── notes ──
        // A note belongs to a deal or to a contact, and Pipedrive takes either
        // scope at the endpoint. Both are listed, and each description says
        // that one is enough, so that a host does not ask for both.
        operation("notes.list", "List notes: all of them, or those on one deal, person, organization or lead. A long note is cut short and marked truncated. Either one of the scopes listed is enough.", Read, &["deals:read", "contacts:read"],
            |p: Pipedrive, c: Connection, i: Notes| async move { p.notes(&c).list(i.options).await as Result<Page<Note>> }),
        operation("notes.get", "Get one note, with its whole text. Either one of the scopes listed is enough.", Read, &["deals:read", "contacts:read"],
            |p: Pipedrive, c: Connection, i: OneNote| async move { p.notes(&c).get(i.note).await as Result<Note> }),
        operation("notes.create", "Write a note on a deal, a person, an organization or a lead. Either one of the scopes listed is enough.", Write, &["deals:full", "contacts:full"],
            |p: Pipedrive, c: Connection, i: NewNote| async move { p.notes(&c).create(i.options).await as Result<Note> }),
        operation("notes.update", "Change a note. New content replaces the text that was there. Either one of the scopes listed is enough.", Write, &["deals:full", "contacts:full"],
            |p: Pipedrive, c: Connection, i: ChangeNote| async move { p.notes(&c).update(i.note, i.options).await as Result<Note> }),
        operation("notes.delete", "Delete a note. Either one of the scopes listed is enough.", Destructive, &["deals:full", "contacts:full"],
            |p: Pipedrive, c: Connection, i: OneNote| async move { p.notes(&c).delete(i.note).await as Result<()> }),

        // ── pipelines ──
        operation("pipelines.list", "List the company's pipelines.", Read, &["deals:read"],
            |p: Pipedrive, c: Connection, i: Listing| async move { p.pipelines(&c).list(i.options).await as Result<Page<Pipeline>> }),
        operation("pipelines.stages", "List the stages of one pipeline, or of every pipeline. A deal's stage_id is one of these.", Read, &["deals:read"],
            |p: Pipedrive, c: Connection, i: Stages| async move { p.pipelines(&c).stages(i.pipeline, i.options).await as Result<Page<Stage>> }),

        // ── fields ──
        operation("fields.deal_fields", "List the fields of a deal, with the name and choices of each custom field. A record holds a custom field under its field_code. Leads have the same custom fields.", Read, &["deals:read"],
            |p: Pipedrive, c: Connection, i: Listing| async move { p.fields(&c).deal_fields(i.options).await as Result<Page<Field>> }),
        operation("fields.person_fields", "List the fields of a person, with the name and choices of each custom field. A record holds a custom field under its field_code.", Read, &["contacts:read"],
            |p: Pipedrive, c: Connection, i: Listing| async move { p.fields(&c).person_fields(i.options).await as Result<Page<Field>> }),
        operation("fields.organization_fields", "List the fields of an organization, with the name and choices of each custom field. A record holds a custom field under its field_code.", Read, &["contacts:read"],
            |p: Pipedrive, c: Connection, i: Listing| async move { p.fields(&c).organization_fields(i.options).await as Result<Page<Field>> }),

        // ── users ──
        operation("users.list", "List every user of the company's Pipedrive.", Read, &["users:read"],
            |p: Pipedrive, c: Connection, _: Nothing| async move { p.users(&c).list().await as Result<Vec<User>> }),
        operation("users.me", "Get the signed-in user, with the company the connection is to.", Read, &["base"],
            |p: Pipedrive, c: Connection, _: Nothing| async move { p.users(&c).me().await as Result<User> }),

        // ── search ──
        operation("search.items", "Search every kind of record at once (deals, persons, organizations, leads and more), or only the kinds named. Returns matches, best first, not whole records.", Read, &["search:read"],
            |p: Pipedrive, c: Connection, i: Items| async move { p.search(&c).items(i.options).await as Result<Page<SearchResult>> }),
    ]
}
