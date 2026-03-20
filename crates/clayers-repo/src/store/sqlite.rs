//! `SQLite` storage backend.
//!
//! Persists objects and refs to a `SQLite` database. Thread-safe via
//! `std::sync::Mutex` around the `rusqlite::Connection`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use clayers_xml::ContentHash;
use rusqlite::{Connection, params};

use futures_core::stream::BoxStream;

use super::{ObjectStore, RefStore, Transaction, subtree_walk};
use crate::error::{Error, Result};
use crate::object::Object;
use crate::query::{QueryStore, QueryMode, QueryResult, NamespaceMap, default_query_document};

/// A SQLite-backed object store and ref store.
pub struct SqliteStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStore {
    /// Open (or create) a `SQLite` store at the given path.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened or schema
    /// migration fails.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| Error::Storage(e.to_string()))?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Create an in-memory `SQLite` store (useful for testing).
    ///
    /// # Errors
    ///
    /// Returns an error if schema creation fails.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Current schema version. Bump when making incompatible changes.
    const SCHEMA_VERSION: i64 = 1;

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock()
            .map_err(|e| Error::Storage(e.to_string()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS objects (
                hash BLOB PRIMARY KEY,
                data BLOB NOT NULL,
                inclusive_hash BLOB
            );
            CREATE INDEX IF NOT EXISTS idx_inclusive_hash
                ON objects(inclusive_hash)
                WHERE inclusive_hash IS NOT NULL;
            CREATE TABLE IF NOT EXISTS refs (
                name TEXT PRIMARY KEY,
                hash BLOB NOT NULL
            );",
        )
        .map_err(|e| Error::Storage(e.to_string()))?;

        // Insert or verify schema version.
        let current: Option<i64> = conn
            .query_row(
                "SELECT version FROM schema_version LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();
        match current {
            None => {
                conn.execute(
                    "INSERT INTO schema_version (version) VALUES (?1)",
                    params![Self::SCHEMA_VERSION],
                )
                .map_err(|e| Error::Storage(e.to_string()))?;
            }
            Some(v) if v != Self::SCHEMA_VERSION => {
                return Err(Error::Storage(format!(
                    "schema version mismatch: database has v{v}, expected v{}",
                    Self::SCHEMA_VERSION
                )));
            }
            Some(_) => {}
        }
        Ok(())
    }
}

fn hash_to_blob(h: &ContentHash) -> &[u8] {
    &h.0
}

fn blob_to_hash(bytes: &[u8]) -> Result<ContentHash> {
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| Error::Storage("invalid hash length in database".into()))?;
    Ok(ContentHash(arr))
}

#[async_trait]
impl ObjectStore for SqliteStore {
    async fn get(&self, hash: &ContentHash) -> Result<Option<Object>> {
        let conn = self.conn.lock()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let mut stmt = conn
            .prepare_cached("SELECT data FROM objects WHERE hash = ?1")
            .map_err(|e| Error::Storage(e.to_string()))?;
        let result = stmt
            .query_row(params![hash_to_blob(hash)], |row| {
                let data: Vec<u8> = row.get(0)?;
                Ok(data)
            });
        match result {
            Ok(data) => Ok(Some(serde_json::from_slice(&data).map_err(|e| Error::Storage(e.to_string()))?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Storage(e.to_string())),
        }
    }

    async fn contains(&self, hash: &ContentHash) -> Result<bool> {
        let conn = self.conn.lock()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let mut stmt = conn
            .prepare_cached("SELECT 1 FROM objects WHERE hash = ?1")
            .map_err(|e| Error::Storage(e.to_string()))?;
        let exists = stmt
            .exists(params![hash_to_blob(hash)])
            .map_err(|e| Error::Storage(e.to_string()))?;
        Ok(exists)
    }

    async fn transaction(&self) -> Result<Box<dyn Transaction>> {
        Ok(Box::new(SqliteTransaction {
            pending: Vec::new(),
            conn: Arc::clone(&self.conn),
        }))
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
        let conn = self.conn.lock()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let mut stmt = conn
            .prepare_cached(
                "SELECT hash, data FROM objects WHERE inclusive_hash = ?1",
            )
            .map_err(|e| Error::Storage(e.to_string()))?;
        let result = stmt.query_row(
            params![hash_to_blob(inclusive_hash)],
            |row| {
                let hash_bytes: Vec<u8> = row.get(0)?;
                let data: Vec<u8> = row.get(1)?;
                Ok((hash_bytes, data))
            },
        );
        match result {
            Ok((hash_bytes, data)) => {
                let hash = blob_to_hash(&hash_bytes)?;
                let obj = serde_json::from_slice(&data).map_err(|e| Error::Storage(e.to_string()))?;
                Ok(Some((hash, obj)))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Storage(e.to_string())),
        }
    }
}

#[async_trait]
impl QueryStore for SqliteStore {
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
impl RefStore for SqliteStore {
    async fn get_ref(&self, name: &str) -> Result<Option<ContentHash>> {
        let conn = self.conn.lock()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let mut stmt = conn
            .prepare_cached("SELECT hash FROM refs WHERE name = ?1")
            .map_err(|e| Error::Storage(e.to_string()))?;
        let result = stmt.query_row(params![name], |row| {
            let bytes: Vec<u8> = row.get(0)?;
            Ok(bytes)
        });
        match result {
            Ok(bytes) => Ok(Some(blob_to_hash(&bytes)?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Storage(e.to_string())),
        }
    }

    async fn set_ref(&self, name: &str, hash: ContentHash) -> Result<()> {
        let conn = self.conn.lock()
            .map_err(|e| Error::Storage(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO refs (name, hash) VALUES (?1, ?2)",
            params![name, hash_to_blob(&hash)],
        )
        .map_err(|e| Error::Storage(e.to_string()))?;
        Ok(())
    }

    async fn delete_ref(&self, name: &str) -> Result<()> {
        let conn = self.conn.lock()
            .map_err(|e| Error::Storage(e.to_string()))?;
        conn.execute("DELETE FROM refs WHERE name = ?1", params![name])
            .map_err(|e| Error::Storage(e.to_string()))?;
        Ok(())
    }

    async fn list_refs(&self, prefix: &str) -> Result<Vec<(String, ContentHash)>> {
        let conn = self.conn.lock()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let mut stmt = conn
            .prepare_cached(
                "SELECT name, hash FROM refs WHERE name LIKE ?1 ESCAPE '\\'",
            )
            .map_err(|e| Error::Storage(e.to_string()))?;
        // Escape LIKE wildcards (% and _) in the prefix so they match literally.
        let escaped_prefix = prefix.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        let pattern = format!("{escaped_prefix}%");
        let rows = stmt
            .query_map(params![pattern], |row| {
                let name: String = row.get(0)?;
                let bytes: Vec<u8> = row.get(1)?;
                Ok((name, bytes))
            })
            .map_err(|e| Error::Storage(e.to_string()))?;
        let mut result = Vec::new();
        for row in rows {
            let (name, bytes) = row.map_err(|e| Error::Storage(e.to_string()))?;
            result.push((name, blob_to_hash(&bytes)?));
        }
        Ok(result)
    }

    async fn cas_ref(
        &self,
        name: &str,
        expected: Option<ContentHash>,
        new: ContentHash,
    ) -> Result<bool> {
        let conn = self.conn.lock()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let current = {
            let mut stmt = conn
                .prepare_cached("SELECT hash FROM refs WHERE name = ?1")
                .map_err(|e| Error::Storage(e.to_string()))?;
            match stmt.query_row(params![name], |row| {
                let bytes: Vec<u8> = row.get(0)?;
                Ok(bytes)
            }) {
                Ok(bytes) => Some(blob_to_hash(&bytes)?),
                Err(rusqlite::Error::QueryReturnedNoRows) => None,
                Err(e) => return Err(Error::Storage(e.to_string())),
            }
        };
        if current == expected {
            conn.execute(
                "INSERT OR REPLACE INTO refs (name, hash) VALUES (?1, ?2)",
                params![name, hash_to_blob(&new)],
            )
            .map_err(|e| Error::Storage(e.to_string()))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// Pending write entry for a `SQLite` transaction.
struct PendingEntry {
    hash: ContentHash,
    data: Vec<u8>,
    inclusive_hash: Option<ContentHash>,
}

/// A write transaction that collects objects and flushes atomically.
pub struct SqliteTransaction {
    pending: Vec<PendingEntry>,
    conn: Arc<Mutex<Connection>>,
}

#[async_trait]
impl Transaction for SqliteTransaction {
    async fn put(&mut self, hash: ContentHash, object: Object) -> Result<()> {
        let inclusive_hash =
            if let Object::Element(ref el) = object {
                Some(el.inclusive_hash)
            } else {
                None
            };
        let data = serde_json::to_vec(&object).map_err(|e| Error::Storage(e.to_string()))?;
        self.pending.push(PendingEntry {
            hash,
            data,
            inclusive_hash,
        });
        Ok(())
    }

    async fn commit(&mut self) -> Result<()> {
        let conn = self.conn.lock()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| Error::Storage(e.to_string()))?;
        {
            let mut stmt = tx
                .prepare_cached(
                    "INSERT OR REPLACE INTO objects (hash, data, inclusive_hash) \
                     VALUES (?1, ?2, ?3)",
                )
                .map_err(|e| Error::Storage(e.to_string()))?;
            for entry in self.pending.drain(..) {
                let incl: Option<Vec<u8>> =
                    entry.inclusive_hash.map(|h| h.0.to_vec());
                stmt.execute(params![
                    hash_to_blob(&entry.hash),
                    entry.data,
                    incl,
                ])
                .map_err(|e| Error::Storage(e.to_string()))?;
            }
        }
        tx.commit().map_err(|e| Error::Storage(e.to_string()))?;
        Ok(())
    }

    async fn rollback(&mut self) -> Result<()> {
        self.pending.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::SqliteStore;
    crate::store::tests::store_tests!(SqliteStore::open_in_memory().unwrap());
}

#[cfg(test)]
mod query_tests {
    use super::SqliteStore;
    crate::query::tests::query_tests!(SqliteStore::open_in_memory().unwrap());
}

#[cfg(test)]
mod prop_tests {
    use super::SqliteStore;
    crate::store::prop_tests::prop_store_tests!(SqliteStore::open_in_memory().unwrap());
}
