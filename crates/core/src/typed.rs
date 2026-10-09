//! Turning an integration's typed methods into named operations.
//!
//! An integration lists its operations as a table of [`TypedOperation`]s.
//! Each is built from a typed handler: the handler's input type gives the
//! input schema and the parsing, and its output type gives the output schema,
//! so calling by name and calling the typed method cannot drift apart.

use std::fmt;
use std::future::Future;
use std::pin::Pin;

use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{Error, ErrorKind, Result};
use crate::operation::{Connection, Effect, OperationInfo, schema_of};

type Running = Pin<Box<dyn Future<Output = Result<Value>> + Send>>;

/// One named operation of the integration `S`: what it says about itself,
/// and how to run it.
pub struct TypedOperation<S> {
    pub info: OperationInfo,
    run: Box<dyn Fn(S, Connection, Value) -> Running + Send + Sync>,
}

impl<S> fmt::Debug for TypedOperation<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TypedOperation")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

impl<S> TypedOperation<S> {
    /// Parses `input`, runs the handler, and encodes what it returns.
    pub fn run(&self, integration: S, connection: Connection, input: Value) -> Running {
        (self.run)(integration, connection, input)
    }
}

/// Builds an operation called `name` from a typed handler.
///
/// `name` is the full name, provider id included: `"slack.chat.post_message"`.
pub fn typed_operation<S, I, O, F, Fut>(
    name: String,
    description: &str,
    effect: Effect,
    scopes: &[&str],
    handler: F,
) -> TypedOperation<S>
where
    S: 'static,
    I: DeserializeOwned + JsonSchema + Send + 'static,
    O: Serialize + JsonSchema + 'static,
    F: Fn(S, Connection, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<O>> + Send + 'static,
{
    let info = OperationInfo {
        name,
        description: description.to_owned(),
        input_schema: schema_of::<I>(),
        output_schema: schema_of::<O>(),
        effect,
        required_scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
    };
    let run = move |integration: S, connection: Connection, input: Value| -> Running {
        let provider = connection.provider().id.clone();
        match serde_path_to_error::deserialize::<_, I>(input) {
            Err(e) => {
                // serde's own message quotes the offending value, which may be
                // message text or a credential. Only the field's name, which
                // comes from our own types, goes into the error.
                let path = e.path().to_string();
                let inner = e.inner().to_string();
                let message = if inner.starts_with("missing field") {
                    inner
                } else if path == "." {
                    "the input has a field of the wrong type".to_owned()
                } else {
                    format!("`{path}` has the wrong type")
                };
                let error = Error::new(ErrorKind::InvalidInput, message).with_provider(provider);
                Box::pin(std::future::ready(Err(error)))
            }
            Ok(input) => {
                let output = handler(integration, connection, input);
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
    TypedOperation {
        info,
        run: Box::new(run),
    }
}

/// Defines the input of a named operation: its plain arguments, and
/// optionally one options struct whose fields sit beside them in the JSON.
///
/// ```ignore
/// operation_input!(ChannelPost {
///     /// A channel id.
///     channel: String
/// } + PostMessage);
/// ```
///
/// The crate that uses it needs `serde` and `schemars` as dependencies.
#[macro_export]
macro_rules! operation_input {
    ($name:ident { $($(#[$doc:meta])* $field:ident : $kind:ty),* $(,)? }) => {
        #[derive(Debug, ::serde::Deserialize, ::schemars::JsonSchema)]
        struct $name { $($(#[$doc])* $field: $kind,)* }
    };
    ($name:ident { $($(#[$doc:meta])* $field:ident : $kind:ty),* $(,)? } + $options:ty) => {
        #[derive(Debug, ::serde::Deserialize, ::schemars::JsonSchema)]
        struct $name {
            $($(#[$doc])* $field: $kind,)*
            #[serde(flatten)]
            options: $options,
        }
    };
}
