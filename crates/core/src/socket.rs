use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use serde_json::Value;

use crate::error::{Error, ErrorKind, Result};
use crate::operation::{Connection, Integration, OperationInfo};
use crate::provider::ProviderId;
use crate::store::{ConnectionKey, TokenStore};

/// The handle an application builds once and shares.
pub struct Socket {
    store: Arc<dyn TokenStore>,
    integrations: HashMap<ProviderId, Arc<dyn Integration>>,
    /// Operation name to the provider that owns it.
    owners: HashMap<String, ProviderId>,
    /// Every operation, sorted by name.
    operations: Vec<OperationInfo>,
}

impl fmt::Debug for Socket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut providers: Vec<_> = self.integrations.keys().collect();
        providers.sort();
        f.debug_struct("Socket")
            .field("providers", &providers)
            .field("operations", &self.operations.len())
            .finish_non_exhaustive()
    }
}

impl Socket {
    pub fn builder(store: Arc<dyn TokenStore>) -> SocketBuilder {
        SocketBuilder {
            store,
            integrations: Vec::new(),
        }
    }

    /// Every operation of every registered integration, sorted by name.
    pub fn operations(&self) -> Vec<OperationInfo> {
        self.operations.clone()
    }

    /// Runs the operation called `operation` on the connection `key`.
    ///
    /// `input` must be a JSON object. The store is asked for the connection's
    /// tokens on every call; nothing is cached here.
    pub async fn invoke(&self, key: ConnectionKey, operation: String, input: Value) -> Result<Value> {
        let Some(owner) = self.owners.get(&operation) else {
            return Err(Error::new(
                ErrorKind::Unsupported,
                format!("no operation named {operation:?}"),
            ));
        };
        if *owner != key.provider {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "operation {operation:?} belongs to {owner}, but the connection is for {}",
                    key.provider
                ),
            ));
        }
        if !input.is_object() {
            return Err(
                Error::new(ErrorKind::InvalidInput, "operation input must be a JSON object")
                    .with_provider(owner.clone()),
            );
        }
        let Some(tokens) = self.store.load(key.clone()).await? else {
            return Err(Error::new(
                ErrorKind::ReconnectRequired,
                format!("no stored connection for {owner}"),
            )
            .with_provider(owner.clone()));
        };
        let integration = &self.integrations[owner];
        integration.invoke(Connection { key, tokens }, operation, input).await
    }
}

/// Collects integrations and checks them before a [`Socket`] exists.
pub struct SocketBuilder {
    store: Arc<dyn TokenStore>,
    integrations: Vec<Arc<dyn Integration>>,
}

impl fmt::Debug for SocketBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SocketBuilder")
            .field("integrations", &self.integrations.len())
            .finish_non_exhaustive()
    }
}

impl SocketBuilder {
    pub fn integration(mut self, integration: Arc<dyn Integration>) -> Self {
        self.integrations.push(integration);
        self
    }

    /// Fails with [`ErrorKind::Config`] when a provider spec is invalid, a
    /// provider is registered twice, or an operation name is duplicated or
    /// does not start with its provider's id and a dot.
    pub fn build(self) -> Result<Socket> {
        let mut integrations = HashMap::new();
        let mut owners = HashMap::new();
        let mut operations = Vec::new();

        for integration in self.integrations {
            let spec = integration.provider();
            spec.validate()?;
            let id = spec.id;
            let config = |message: String| Error::new(ErrorKind::Config, message).with_provider(id.clone());

            let prefix = format!("{id}.");
            for info in integration.operations() {
                if info.name.len() <= prefix.len() || !info.name.starts_with(&prefix) {
                    return Err(config(format!(
                        "operation {:?} must be named {prefix}<name>",
                        info.name
                    )));
                }
                if owners.insert(info.name.clone(), id.clone()).is_some() {
                    return Err(config(format!("operation {:?} is registered twice", info.name)));
                }
                operations.push(info);
            }
            if integrations.insert(id.clone(), integration).is_some() {
                return Err(config(format!("provider {id} is registered twice")));
            }
        }

        operations.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Socket {
            store: self.store,
            integrations,
            owners,
            operations,
        })
    }
}
