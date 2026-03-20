//! `PyStore`: bridge from a Python store object to the Rust `ObjectStore`,
//! `RefStore`, and `QueryStore` traits.
//!
//! Each trait method acquires the GIL via `Python::attach`, calls the
//! corresponding Python method, converts the result, and releases the GIL.
//! Python stores are synchronous; the async wrappers simply run inside
//! `attach`.

use std::collections::HashSet;

use async_stream::try_stream;
use async_trait::async_trait;
use clayers_repo::error::{Error, Result};
use clayers_repo::object::Object;
use clayers_repo::query::{
    default_query_document, NamespaceMap, QueryMode, QueryResult, QueryStore,
};
use clayers_repo::store::{ObjectStore, RefStore, Transaction};
use clayers_xml::ContentHash;
use futures_core::stream::BoxStream;
use pyo3::prelude::*;

use crate::repo::py_objects::StoreObject;
use crate::xml::ContentHash as PyContentHash;

// ---------------------------------------------------------------------------
// Error conversion
// ---------------------------------------------------------------------------

fn py_to_store_err(e: PyErr) -> Error {
    Error::Storage(e.to_string())
}

fn conversion_err(msg: String) -> Error {
    Error::Storage(msg)
}

// ---------------------------------------------------------------------------
// PyStore
// ---------------------------------------------------------------------------

/// A store backed by a Python object implementing the store protocol.
///
/// The Python object must provide methods matching `ObjectStore`, `RefStore`,
/// and optionally `QueryStore`. See the compliance test runner for the full
/// protocol surface.
pub struct PyStore {
    py_object: Py<PyAny>,
}

impl PyStore {
    pub fn new(py_object: Py<PyAny>) -> Self {
        Self { py_object }
    }
}

// ---------------------------------------------------------------------------
// ObjectStore
// ---------------------------------------------------------------------------

#[async_trait]
impl ObjectStore for PyStore {
    async fn get(&self, hash: &ContentHash) -> Result<Option<Object>> {
        let hash_inner = *hash;

        Python::attach(|py| {
            let py_hash = PyContentHash::from_inner(hash_inner);
            let result = self
                .py_object
                .call_method1(py, "get", (py_hash,))
                .map_err(py_to_store_err)?;

            if result.is_none(py) {
                return Ok(None);
            }

            let bound = result.bind(py);
            let store_obj = bound
                .cast::<StoreObject>()
                .map_err(|e| Error::Storage(e.to_string()))?;
            let rust_obj = store_obj.get().to_rust().map_err(conversion_err)?;
            Ok(Some(rust_obj))
        })
    }

    async fn contains(&self, hash: &ContentHash) -> Result<bool> {
        let hash_inner = *hash;

        Python::attach(|py| {
            let py_hash = PyContentHash::from_inner(hash_inner);
            let result = self
                .py_object
                .call_method1(py, "contains", (py_hash,))
                .map_err(py_to_store_err)?;
            let val: bool = result.bind(py).extract().map_err(py_to_store_err)?;
            Ok(val)
        })
    }

    async fn transaction(&self) -> Result<Box<dyn Transaction>> {
        Python::attach(|py| {
            let tx_obj = self
                .py_object
                .call_method0(py, "transaction")
                .map_err(py_to_store_err)?;
            Ok(Box::new(PyTransaction::new(tx_obj)) as Box<dyn Transaction>)
        })
    }

    async fn get_by_inclusive_hash(
        &self,
        inclusive_hash: &ContentHash,
    ) -> Result<Option<(ContentHash, Object)>> {
        let hash_inner = *inclusive_hash;

        Python::attach(|py| {
            let py_hash = PyContentHash::from_inner(hash_inner);
            let result = self
                .py_object
                .call_method1(py, "get_by_inclusive_hash", (py_hash,))
                .map_err(py_to_store_err)?;

            if result.is_none(py) {
                return Ok(None);
            }

            // Expect a tuple (ContentHash, StoreObject)
            let bound = result.bind(py);
            let tuple = bound
                .cast::<pyo3::types::PyTuple>()
                .map_err(|e| Error::Storage(e.to_string()))?;

            let item0 = tuple.get_item(0).map_err(py_to_store_err)?;
            let py_hash = item0
                .cast::<PyContentHash>()
                .map_err(|e| Error::Storage(e.to_string()))?;
            let identity = py_hash.get().inner();

            let item1 = tuple.get_item(1).map_err(py_to_store_err)?;
            let store_obj = item1
                .cast::<StoreObject>()
                .map_err(|e| Error::Storage(e.to_string()))?;
            let rust_obj = store_obj.get().to_rust().map_err(conversion_err)?;

            Ok(Some((identity, rust_obj)))
        })
    }

    /// Walk the Merkle DAG via `get()`, yielding each object exactly once.
    ///
    /// This replicates the `subtree_walk` algorithm from `clayers-repo`
    /// (which is `pub(crate)` and not accessible from this crate).
    fn subtree<'a>(
        &'a self,
        root: &ContentHash,
    ) -> BoxStream<'a, Result<(ContentHash, Object)>> {
        let root = *root;
        Box::pin(try_stream! {
            let mut visited = HashSet::new();
            let mut stack = vec![root];
            while let Some(hash) = stack.pop() {
                if !visited.insert(hash) { continue; }
                let obj = self.get(&hash).await?.ok_or(Error::NotFound(hash))?;
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
                    Object::Document(d) => {
                        stack.push(d.root);
                        stack.extend(&d.prologue);
                    }
                    Object::Element(e) => { stack.extend(&e.children); }
                    Object::Text(_) | Object::Comment(_) | Object::PI(_) => {}
                }
                yield (hash, obj);
            }
        })
    }
}

// ---------------------------------------------------------------------------
// RefStore
// ---------------------------------------------------------------------------

#[async_trait]
impl RefStore for PyStore {
    async fn get_ref(&self, name: &str) -> Result<Option<ContentHash>> {
        let name = name.to_string();

        Python::attach(|py| {
            let result = self
                .py_object
                .call_method1(py, "get_ref", (name,))
                .map_err(py_to_store_err)?;

            if result.is_none(py) {
                return Ok(None);
            }

            let bound = result.bind(py);
            let py_hash = bound
                .cast::<PyContentHash>()
                .map_err(|e| Error::Storage(e.to_string()))?;
            Ok(Some(py_hash.get().inner()))
        })
    }

    async fn set_ref(&self, name: &str, hash: ContentHash) -> Result<()> {
        let name = name.to_string();

        Python::attach(|py| {
            let py_hash = PyContentHash::from_inner(hash);
            self.py_object
                .call_method1(py, "set_ref", (name, py_hash))
                .map_err(py_to_store_err)?;
            Ok(())
        })
    }

    async fn delete_ref(&self, name: &str) -> Result<()> {
        let name = name.to_string();

        Python::attach(|py| {
            self.py_object
                .call_method1(py, "delete_ref", (name,))
                .map_err(py_to_store_err)?;
            Ok(())
        })
    }

    async fn list_refs(&self, prefix: &str) -> Result<Vec<(String, ContentHash)>> {
        let prefix = prefix.to_string();

        Python::attach(|py| {
            let result = self
                .py_object
                .call_method1(py, "list_refs", (prefix,))
                .map_err(py_to_store_err)?;

            let bound = result.bind(py);
            let list = bound
                .cast::<pyo3::types::PyList>()
                .map_err(|e| Error::Storage(e.to_string()))?;
            let mut pairs = Vec::new();
            for item in list.iter() {
                let tuple = item
                    .cast::<pyo3::types::PyTuple>()
                    .map_err(|e| Error::Storage(e.to_string()))?;
                let name: String = tuple
                    .get_item(0)
                    .map_err(py_to_store_err)?
                    .extract()
                    .map_err(py_to_store_err)?;
                let hash_item = tuple
                    .get_item(1)
                    .map_err(py_to_store_err)?;
                let hash = hash_item
                    .cast::<PyContentHash>()
                    .map_err(|e| Error::Storage(e.to_string()))?;
                pairs.push((name, hash.get().inner()));
            }
            Ok(pairs)
        })
    }

    async fn cas_ref(
        &self,
        name: &str,
        expected: Option<ContentHash>,
        new: ContentHash,
    ) -> Result<bool> {
        let name = name.to_string();

        Python::attach(|py| {
            let py_expected: Option<PyContentHash> = expected.map(PyContentHash::from_inner);
            let py_new = PyContentHash::from_inner(new);
            let result = self
                .py_object
                .call_method1(py, "cas_ref", (name, py_expected, py_new))
                .map_err(py_to_store_err)?;
            let val: bool = result.bind(py).extract().map_err(py_to_store_err)?;
            Ok(val)
        })
    }
}

// ---------------------------------------------------------------------------
// QueryStore
// ---------------------------------------------------------------------------

#[async_trait]
impl QueryStore for PyStore {
    async fn query_document(
        &self,
        doc_hash: ContentHash,
        xpath: &str,
        mode: QueryMode,
        namespaces: &NamespaceMap,
    ) -> Result<QueryResult> {
        // Use the default implementation which builds XML from objects and
        // evaluates XPath via xee-xpath. Python stores don't need to
        // implement query_document directly.
        default_query_document(self, doc_hash, xpath, mode, namespaces).await
    }
}

// ---------------------------------------------------------------------------
// PyTransaction
// ---------------------------------------------------------------------------

/// A transaction backed by a Python transaction object.
pub struct PyTransaction {
    py_object: Py<PyAny>,
}

impl PyTransaction {
    pub fn new(py_object: Py<PyAny>) -> Self {
        Self { py_object }
    }
}

#[async_trait]
impl Transaction for PyTransaction {
    async fn put(&mut self, hash: ContentHash, object: Object) -> Result<()> {
        Python::attach(|py| {
            let py_hash = PyContentHash::from_inner(hash);
            let py_obj = StoreObject::from(object);
            self.py_object
                .call_method1(py, "put", (py_hash, py_obj))
                .map_err(py_to_store_err)?;
            Ok(())
        })
    }

    async fn commit(&mut self) -> Result<()> {
        Python::attach(|py| {
            self.py_object
                .call_method0(py, "commit")
                .map_err(py_to_store_err)?;
            Ok(())
        })
    }

    async fn rollback(&mut self) -> Result<()> {
        Python::attach(|py| {
            self.py_object
                .call_method0(py, "rollback")
                .map_err(py_to_store_err)?;
            Ok(())
        })
    }
}
