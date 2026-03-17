//! In-memory storage backend for testing and prototyping.
//!
//! Thread-safe via `tokio::sync::RwLock`. Implements all three store traits.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use clayers_xml::ContentHash;
use tokio::sync::RwLock;

use futures_core::stream::BoxStream;

use super::{ObjectStore, RefStore, Transaction, subtree_walk};
use crate::error::Result;
use crate::object::{Object, ElementObject};
use crate::query::{QueryStore, QueryMode, QueryResult, NamespaceMap, default_query_document};

/// Shared inner state for the memory store.
pub(crate) struct MemoryStoreInner {
    pub(crate) objects: RwLock<HashMap<ContentHash, Object>>,
    pub(crate) refs: RwLock<HashMap<String, ContentHash>>,
    /// Secondary index: Inclusive C14N hash -> identity (Exclusive C14N) hash.
    pub(crate) inclusive_index: RwLock<HashMap<ContentHash, ContentHash>>,
}

/// An in-memory object store, ref store, and remote store.
///
/// Suitable for testing and prototyping. All data lives in memory and is
/// lost when the store is dropped.
pub struct MemoryStore {
    inner: Arc<MemoryStoreInner>,
}

impl MemoryStore {
    /// Create a new empty memory store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(MemoryStoreInner {
                objects: RwLock::new(HashMap::new()),
                refs: RwLock::new(HashMap::new()),
                inclusive_index: RwLock::new(HashMap::new()),
            }),
        }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ObjectStore for MemoryStore {
    async fn get(&self, hash: &ContentHash) -> Result<Option<Object>> {
        let objects = self.inner.objects.read().await;
        Ok(objects.get(hash).cloned())
    }

    async fn contains(&self, hash: &ContentHash) -> Result<bool> {
        let objects = self.inner.objects.read().await;
        Ok(objects.contains_key(hash))
    }

    async fn transaction(&self) -> Result<Box<dyn Transaction>> {
        Ok(Box::new(MemoryTransaction::new(Arc::clone(&self.inner))))
    }

    fn subtree<'a>(
        &'a self,
        root: &ContentHash,
    ) -> BoxStream<'a, Result<(ContentHash, Object)>> {
        subtree_walk(self, root)
    }

    async fn get_by_inclusive_hash(
        &self,
        inclusive_hash: &ContentHash,
    ) -> Result<Option<(ContentHash, Object)>> {
        let index = self.inner.inclusive_index.read().await;
        let Some(identity_hash) = index.get(inclusive_hash).copied() else {
            return Ok(None);
        };
        drop(index);
        let objects = self.inner.objects.read().await;
        Ok(objects
            .get(&identity_hash)
            .map(|obj| (identity_hash, obj.clone())))
    }
}

#[async_trait]
impl QueryStore for MemoryStore {
    async fn query_document(
        &self,
        doc_hash: ContentHash,
        xpath: &str,
        mode: QueryMode,
        namespaces: &NamespaceMap,
    ) -> Result<QueryResult> {
        default_query_document(self, doc_hash, xpath, mode, namespaces).await
    }
}

#[async_trait]
impl RefStore for MemoryStore {
    async fn get_ref(&self, name: &str) -> Result<Option<ContentHash>> {
        let refs = self.inner.refs.read().await;
        Ok(refs.get(name).copied())
    }

    async fn set_ref(&self, name: &str, hash: ContentHash) -> Result<()> {
        let mut refs = self.inner.refs.write().await;
        refs.insert(name.to_string(), hash);
        Ok(())
    }

    async fn delete_ref(&self, name: &str) -> Result<()> {
        let mut refs = self.inner.refs.write().await;
        refs.remove(name);
        Ok(())
    }

    async fn list_refs(&self, prefix: &str) -> Result<Vec<(String, ContentHash)>> {
        let refs = self.inner.refs.read().await;
        Ok(refs
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), *v))
            .collect())
    }

    async fn cas_ref(
        &self,
        name: &str,
        expected: Option<ContentHash>,
        new: ContentHash,
    ) -> Result<bool> {
        let mut refs = self.inner.refs.write().await;
        let current = refs.get(name).copied();
        if current == expected {
            refs.insert(name.to_string(), new);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// Pending write entry: identity hash, object, and optional inclusive hash.
struct PendingEntry {
    hash: ContentHash,
    object: Object,
    inclusive_hash: Option<ContentHash>,
}

/// An in-memory transaction that collects writes and flushes atomically on commit.
pub(crate) struct MemoryTransaction {
    pending: Vec<PendingEntry>,
    inner: Arc<MemoryStoreInner>,
}

impl MemoryTransaction {
    pub(crate) fn new(inner: Arc<MemoryStoreInner>) -> Self {
        Self {
            pending: Vec::new(),
            inner,
        }
    }
}

#[async_trait]
impl Transaction for MemoryTransaction {
    async fn put(&mut self, hash: ContentHash, object: Object) -> Result<()> {
        let inclusive_hash = if let Object::Element(ElementObject { inclusive_hash, .. }) = &object {
            Some(*inclusive_hash)
        } else {
            None
        };
        self.pending.push(PendingEntry {
            hash,
            object,
            inclusive_hash,
        });
        Ok(())
    }

    async fn commit(&mut self) -> Result<()> {
        let mut objects = self.inner.objects.write().await;
        let mut inclusive_index = self.inner.inclusive_index.write().await;

        for entry in self.pending.drain(..) {
            objects.insert(entry.hash, entry.object);
            if let Some(inclusive) = entry.inclusive_hash {
                inclusive_index.insert(inclusive, entry.hash);
            }
        }

        Ok(())
    }

    async fn rollback(&mut self) -> Result<()> {
        self.pending.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::MemoryStore;
    crate::store::tests::store_tests!(MemoryStore::new());
}

#[cfg(test)]
mod query_tests {
    use super::MemoryStore;
    crate::query::tests::query_tests!(MemoryStore::new());
}
