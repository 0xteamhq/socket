//! Every typed method as a named operation.
//!
//! An operation's input is a JSON object with the method's plain arguments
//! and its options side by side: `{ "page": "0123…", "max_depth": 3 }`.
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

use crate::Notion;
use crate::models::{
    AppendBlocks, Block, Comment, CreateComment, CreatePage, DataSource, Database, PageContent, PageOrDataSource,
    Paging, PropertyItems, QueryDataSource, ReadPage, SearchQuery, UpdateBlock, UpdatePage, User,
};

type Running = std::pin::Pin<Box<dyn Future<Output = Result<Value>> + Send>>;

/// One named operation: what it says about itself, and how to run it.
pub(crate) struct Operation {
    pub(crate) info: OperationInfo,
    run: Box<dyn Fn(Notion, Connection, Value) -> Running + Send + Sync>,
}

impl Operation {
    pub(crate) fn run(&self, notion: Notion, connection: Connection, input: Value) -> Running {
        (self.run)(notion, connection, input)
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
    close(&mut schema);
    schema
}

/// Where in `input` the first field is that the schema at `node` does not
/// list, and that field's name.
///
/// A field that is not known would be dropped in silence, with what it said:
/// a filter, a parent, the text of a comment. The input types cannot refuse one
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
    F: Fn(Notion, Connection, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<O>> + Send + 'static,
{
    let info = OperationInfo {
        name: format!("notion.{name}"),
        description: description.to_owned(),
        input_schema: closed(schema_of::<I>()),
        output_schema: schema_of::<O>(),
        effect,
        required_scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
    };
    let schema = info.input_schema.clone();
    let run = move |notion: Notion, connection: Connection, input: Value| -> Running {
        let provider = connection.provider().id.clone();
        if let Some(found) = unknown_field(&schema, &schema, &input) {
            return Box::pin(std::future::ready(Err(not_a_field(found).with_provider(provider))));
        }
        match serde_path_to_error::deserialize::<_, I>(input) {
            Err(e) => {
                // serde's own message quotes the offending value, which may be
                // the text of a page or a credential. Only the field's name,
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
                let output = handler(notion, connection, input);
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

input!(Find {} + SearchQuery);
input!(OnePage {
    /// A page's id, with or without dashes, or its address in Notion.
    page: String
});
input!(
    OneProperty {
        /// A page's id, or its address in Notion.
        page: String,
        /// The property's id as the page gives it, or its name.
        property: String
    } + Paging
);
input!(
    WholePage {
        /// A page's id, or its address in Notion.
        page: String
    } + ReadPage
);
input!(NewPage {} + CreatePage);
input!(
    ChangePage {
        /// The id of the page to change, or its address in Notion.
        page: String
    } + UpdatePage
);
input!(OneBlock {
    /// A block's id. A page's id names the page as a block.
    block: String
});
input!(
    Inside {
        /// The id of a block, or of a page for the blocks at its top.
        block: String
    } + Paging
);
input!(
    Append {
        /// The id of the block, or of the page, to add blocks inside.
        block: String
    } + AppendBlocks
);
input!(
    ChangeBlock {
        /// The id of the block to change.
        block: String
    } + UpdateBlock
);
input!(OneDatabase {
    /// A database's id, or its address in Notion.
    database: String
});
input!(OneDataSource {
    /// A data source's id, as `databases.get` or a search gives it.
    data_source: String
});
input!(
    Query {
        /// A data source's id, as `databases.get` or a search gives it.
        data_source: String
    } + QueryDataSource
);
input!(Listing {} + Paging);
input!(OneUser {
    /// A user's id.
    user: String
});
input!(
    CommentsOn {
        /// The id of a page, or of a block, whose comments to list.
        block: String
    } + Paging
);
input!(NewComment {} + CreateComment);

// `Destructive` is anything that deletes or removes what was there. A host
// uses it to ask a person first. Notion keeps what is trashed for a time,
// and it is still marked so: the page or the block is gone from where
// people look for it.
use Effect::{Destructive, Read, Write};

/// Notion has no scopes. What an integration may do is set by the
/// capabilities it was given and by the pages shared with it, and each
/// operation's description names the capability it needs.
const NO_SCOPES: &[&str] = &[];

/// Every Notion operation except identity and resource lookup, which the
/// integration adds itself.
pub(crate) fn all() -> &'static [Operation] {
    static ALL: OnceLock<Vec<Operation>> = OnceLock::new();
    ALL.get_or_init(build)
}

#[rustfmt::skip]
fn build() -> Vec<Operation> {
    vec![
        // ── search ──
        // Notion offers this only as POST. It reads titles and changes nothing.
        operation("search.run", "Search the titles of the pages and data sources shared with the integration, optionally only pages or only data sources, ordered by last edit. Does not search content. Changes nothing.", Read, NO_SCOPES,
            |n: Notion, c: Connection, i: Find| async move { n.search(&c).run(i.options).await as Result<Page<PageOrDataSource>> }),

        // ── pages ──
        operation("pages.get", "Get a page: its properties and where it lives, without its content. Needs the read content capability.", Read, NO_SCOPES,
            |n: Notion, c: Connection, i: OnePage| async move { n.pages(&c).get(&i.page).await as Result<crate::models::Page> }),
        operation("pages.property", "Read one property of a page in full, a page of items at a time. Use it for a relation, a list of people, a title or a text, which pages.get cuts short at 25 references.", Read, NO_SCOPES,
            |n: Notion, c: Connection, i: OneProperty| async move { n.pages(&c).property(&i.page, &i.property, i.options).await as Result<PropertyItems> }),
        operation("pages.read", "Read a page's whole content as Markdown, nested blocks included. Makes one request for the page and one or more for each block with blocks inside it; says when a limit cut the page short. Needs the read content capability.", Read, NO_SCOPES,
            |n: Notion, c: Connection, i: WholePage| async move { n.pages(&c).read(&i.page, i.options).await as Result<PageContent> }),
        operation("pages.create", "Create a page under a page, or as a row of a data source, with its properties and up to 100 blocks of content. Needs the insert content capability.", Write, NO_SCOPES,
            |n: Notion, c: Connection, i: NewPage| async move { n.pages(&c).create(i.options).await as Result<crate::models::Page> }),
        operation("pages.update", "Change a page's properties, icon or cover, leaving the rest as it is. Needs the update content capability.", Write, NO_SCOPES,
            |n: Notion, c: Connection, i: ChangePage| async move { n.pages(&c).update(&i.page, i.options).await as Result<crate::models::Page> }),
        operation("pages.archive", "Move a page to the trash, with everything inside it. Needs the update content capability.", Destructive, NO_SCOPES,
            |n: Notion, c: Connection, i: OnePage| async move { n.pages(&c).archive(&i.page).await as Result<crate::models::Page> }),

        // ── blocks ──
        operation("blocks.get", "Get one block. Needs the read content capability.", Read, NO_SCOPES,
            |n: Notion, c: Connection, i: OneBlock| async move { n.blocks(&c).get(&i.block).await as Result<Block> }),
        operation("blocks.children", "List the blocks directly inside a block or a page, in order, up to 100 at a time. Needs the read content capability.", Read, NO_SCOPES,
            |n: Notion, c: Connection, i: Inside| async move { n.blocks(&c).children(&i.block, i.options).await as Result<Page<Block>> }),
        operation("blocks.append", "Add up to 100 blocks inside a block or a page: at the end, at the start, or after a block already there. Needs the insert content capability.", Write, NO_SCOPES,
            |n: Notion, c: Connection, i: Append| async move { n.blocks(&c).append(&i.block, i.options).await as Result<Page<Block>> }),
        operation("blocks.update", "Change what one block holds, such as its text or whether a to-do is checked. What is given replaces what was there. Needs the update content capability.", Write, NO_SCOPES,
            |n: Notion, c: Connection, i: ChangeBlock| async move { n.blocks(&c).update(&i.block, i.options).await as Result<Block> }),
        operation("blocks.delete", "Move a block to the trash, with everything inside it. Given a page's id, moves the page there. Needs the update content capability.", Destructive, NO_SCOPES,
            |n: Notion, c: Connection, i: OneBlock| async move { n.blocks(&c).delete(&i.block).await as Result<Block> }),

        // ── databases ──
        operation("databases.get", "Get a database: its title, where it lives, and the data sources it holds. Needs the read content capability.", Read, NO_SCOPES,
            |n: Notion, c: Connection, i: OneDatabase| async move { n.databases(&c).get(&i.database).await as Result<Database> }),
        operation("databases.data_source", "Get one data source of a database: the schema its rows follow, which a filter or a sort names. Needs the read content capability.", Read, NO_SCOPES,
            |n: Notion, c: Connection, i: OneDataSource| async move { n.databases(&c).data_source(&i.data_source).await as Result<DataSource> }),
        // Notion offers this only as POST. It reads rows and changes nothing.
        operation("databases.query", "List the rows of a data source, each a page with its properties, that pass a filter, in the order asked for. Changes nothing. Needs the read content capability.", Read, NO_SCOPES,
            |n: Notion, c: Connection, i: Query| async move { n.databases(&c).query(&i.data_source, i.options).await as Result<Page<PageOrDataSource>> }),

        // ── users ──
        operation("users.list", "List the workspace's members and integrations, without guests. Needs a user information capability; email addresses need the one that includes them.", Read, NO_SCOPES,
            |n: Notion, c: Connection, i: Listing| async move { n.users(&c).list(i.options).await as Result<Page<User>> }),
        operation("users.get", "Get one person or integration by id. Needs a user information capability.", Read, NO_SCOPES,
            |n: Notion, c: Connection, i: OneUser| async move { n.users(&c).get(&i.user).await as Result<User> }),

        // ── comments ──
        operation("comments.list", "List the comments on a page or a block that are not resolved. Needs the read comments capability.", Read, NO_SCOPES,
            |n: Notion, c: Connection, i: CommentsOn| async move { n.comments(&c).list(&i.block, i.options).await as Result<Page<Comment>> }),
        operation("comments.create", "Add a comment to a page or a block, or a reply to a thread. Everyone who can see the page sees it. Needs the insert comments capability.", Write, NO_SCOPES,
            |n: Notion, c: Connection, i: NewComment| async move { n.comments(&c).create(i.options).await as Result<Comment> }),
    ]
}
