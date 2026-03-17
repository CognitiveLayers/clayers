//! Storage traits for the object store, ref store, and remote operations.
//!
//! Three independent async traits. Each backend implements what it supports.

pub mod memory;
#[cfg(feature = "sqlite")]
pub mod sqlite;
#[cfg(test)]
pub(crate) mod tests;

use std::collections::HashSet;

use async_trait::async_trait;
use async_stream::try_stream;
use clayers_xml::ContentHash;
use futures_core::stream::BoxStream;

use crate::error::{Error, Result};
use crate::object::Object;

/// Content-addressed object storage.
///
/// Objects are stored by their identity hash (Exclusive C14N SHA-256).
#[async_trait]
pub trait ObjectStore: Send + Sync {
    /// Retrieve an object by its identity hash.
    async fn get(&self, hash: &ContentHash) -> Result<Option<Object>>;

    /// Check whether an object exists in the store.
    async fn contains(&self, hash: &ContentHash) -> Result<bool>;

    /// Begin a new write transaction.
    async fn transaction(&self) -> Result<Box<dyn Transaction>>;

    /// Look up an object by its Inclusive C14N hash (secondary index).
    ///
    /// Returns the identity (Exclusive C14N) hash and the object.
    /// Used for drift detection / coverage integration with `clayers-spec`.
    async fn get_by_inclusive_hash(
        &self,
        inclusive_hash: &ContentHash,
    ) -> Result<Option<(ContentHash, Object)>>;

    /// Stream all objects in the subtree rooted at `root`.
    ///
    /// Walks the Merkle DAG (Commit->tree+parents, Tag->target,
    /// Tree->documents, Document->root, Element->children, leaf nodes). Each object
    /// is yielded exactly once.
    ///
    /// Use [`subtree_walk`] for the default walk-via-`get()` implementation.
    /// Remote stores can override to stream from a bulk endpoint.
    fn subtree<'a>(
        &'a self,
        root: &ContentHash,
    ) -> BoxStream<'a, Result<(ContentHash, Object)>>;
}

/// Default `subtree()` implementation: walk DAG via `get()`, yield each
/// object exactly once.
pub(crate) fn subtree_walk<'a>(
    store: &'a (dyn ObjectStore + 'a),
    root: &ContentHash,
) -> BoxStream<'a, Result<(ContentHash, Object)>> {
    let root = *root;
    Box::pin(try_stream! {
        let mut visited = HashSet::new();
        let mut stack = vec![root];
        while let Some(hash) = stack.pop() {
            if !visited.insert(hash) { continue; }
            let obj = store.get(&hash).await?.ok_or(Error::NotFound(hash))?;
            match &obj {
                Object::Commit(c) => {
                    stack.push(c.tree);
                    stack.extend(&c.parents);
                }
                Object::Tag(t) => { stack.push(t.target); }
                Object::Tree(t) => {
                    for entry in &t.entries {
                        stack.push(entry.document);
                    }
                }
                Object::Document(d) => { stack.push(d.root); }
                Object::Element(e) => { stack.extend(&e.children); }
                Object::Text(_) | Object::Comment(_) | Object::PI(_) => {}
            }
            yield (hash, obj);
        }
    })
}

/// Named mutable pointers (branches, tags, HEAD).
#[async_trait]
pub trait RefStore: Send + Sync {
    /// Get the hash a ref points to.
    async fn get_ref(&self, name: &str) -> Result<Option<ContentHash>>;

    /// Set a ref to point to a hash.
    async fn set_ref(&self, name: &str, hash: ContentHash) -> Result<()>;

    /// Delete a ref.
    async fn delete_ref(&self, name: &str) -> Result<()>;

    /// List refs matching a prefix (e.g., `"refs/heads/"` for branches).
    async fn list_refs(&self, prefix: &str) -> Result<Vec<(String, ContentHash)>>;

    /// Compare-and-swap: update ref only if current value matches `expected`.
    ///
    /// `expected: None` means "create only if ref does not exist".
    /// Returns `true` if the swap succeeded.
    async fn cas_ref(
        &self,
        name: &str,
        expected: Option<ContentHash>,
        new: ContentHash,
    ) -> Result<bool>;
}

/// A write transaction for batching object insertions.
///
/// `commit` and `rollback` take `&mut self` (not `self`) so the
/// transaction can be recovered on commit failure.
#[async_trait]
pub trait Transaction: Send {
    /// Store an object with its pre-computed identity hash.
    ///
    /// For element objects, the inclusive hash is extracted from the
    /// `ElementObject::inclusive_hash` field and indexed automatically.
    async fn put(&mut self, hash: ContentHash, object: Object) -> Result<()>;

    /// Atomically commit all staged objects and update secondary indices.
    ///
    /// On error, the transaction is NOT consumed: caller can retry or
    /// call `rollback()`.
    async fn commit(&mut self) -> Result<()>;

    /// Discard all staged objects.
    async fn rollback(&mut self) -> Result<()>;
}
