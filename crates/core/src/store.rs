use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::provider::ProviderId;
use crate::secret::TokenSet;

/// Which stored authorization: one provider, for one of the application's tenants.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionKey {
    pub provider: ProviderId,
    /// Opaque to Socket: a user id, a workspace id.
    pub tenant: String,
}

impl ConnectionKey {
    pub fn new(provider: ProviderId, tenant: impl Into<String>) -> Self {
        Self {
            provider,
            tenant: tenant.into(),
        }
    }
}

/// Where tokens live. The application implements this; Socket never persists.
///
/// Methods take owned values and return `Result` so the trait can be
/// implemented by an object in another language. Socket holds no lock while
/// calling a store, so an implementation may be slow or call back into Socket.
#[async_trait]
pub trait TokenStore: Send + Sync {
    async fn load(&self, key: ConnectionKey) -> Result<Option<TokenSet>>;
    async fn save(&self, key: ConnectionKey, tokens: TokenSet) -> Result<()>;
    async fn delete(&self, key: ConnectionKey) -> Result<()>;
}

/// A store that keeps tokens in memory. For command-line tools, examples and tests.
#[derive(Debug, Default)]
pub struct MemoryTokenStore {
    tokens: Mutex<HashMap<ConnectionKey, TokenSet>>,
}

impl MemoryTokenStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<ConnectionKey, TokenSet>> {
        // A poisoned lock only means another thread panicked mid-insert; the map is still usable.
        self.tokens.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[async_trait]
impl TokenStore for MemoryTokenStore {
    async fn load(&self, key: ConnectionKey) -> Result<Option<TokenSet>> {
        Ok(self.lock().get(&key).cloned())
    }

    async fn save(&self, key: ConnectionKey, tokens: TokenSet) -> Result<()> {
        self.lock().insert(key, tokens);
        Ok(())
    }

    async fn delete(&self, key: ConnectionKey) -> Result<()> {
        self.lock().remove(&key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn key(provider: &str, tenant: &str) -> ConnectionKey {
        ConnectionKey::new(ProviderId::new(provider).unwrap(), tenant)
    }

    #[tokio::test]
    async fn save_then_load_returns_the_tokens_and_delete_removes_them() {
        let store = MemoryTokenStore::new();
        assert_eq!(store.load(key("slack", "acme")).await.unwrap(), None);

        store.save(key("slack", "acme"), TokenSet::bearer("one")).await.unwrap();
        store.save(key("slack", "acme"), TokenSet::bearer("two")).await.unwrap();
        let loaded = store.load(key("slack", "acme")).await.unwrap().unwrap();
        assert_eq!(loaded.access_token.expose(), "two", "save replaces");

        store.delete(key("slack", "acme")).await.unwrap();
        assert_eq!(store.load(key("slack", "acme")).await.unwrap(), None);
        store.delete(key("slack", "acme")).await.unwrap();
    }

    #[tokio::test]
    async fn tenants_and_providers_are_isolated() {
        let store = MemoryTokenStore::new();
        store
            .save(key("slack", "acme"), TokenSet::bearer("acme-slack"))
            .await
            .unwrap();
        assert_eq!(store.load(key("slack", "globex")).await.unwrap(), None);
        assert_eq!(store.load(key("github", "acme")).await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_store_is_usable_as_a_shared_trait_object() {
        let store: Arc<dyn TokenStore> = Arc::new(MemoryTokenStore::new());
        let other = Arc::clone(&store);
        other.save(key("github", "u1"), TokenSet::bearer("t")).await.unwrap();
        assert!(store.load(key("github", "u1")).await.unwrap().is_some());
    }
}
