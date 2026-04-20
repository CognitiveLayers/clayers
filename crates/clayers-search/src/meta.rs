//! `SQLite` metadata store for the sidecar index.
//!
//! Table `nodes(id, file, line_start, line_end, layer, namespace,
//! node_hash, preview, key)` — doubles as the incremental-rebuild
//! cache via `node_hash`.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

/// Filename of the `SQLite` metadata store inside `.clayers/search/`.
pub const META_FILENAME: &str = "meta.sqlite";

/// One `nodes` row.
#[derive(Debug, Clone)]
pub struct NodeMeta {
    pub id: String,
    pub file: String,
    pub line_start: i64,
    pub line_end: i64,
    pub layer: String,
    pub namespace: String,
    pub node_hash: String,
    pub preview: String,
    pub key: i64,
}

pub struct MetaStore {
    conn: Connection,
}

impl MetaStore {
    /// Open or create the meta store at `path`.
    ///
    /// # Errors
    /// Returns an error if `SQLite` cannot open the file or create
    /// the schema.
    pub fn open_or_create(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("open {}", path.display()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS nodes (
                id         TEXT PRIMARY KEY,
                file       TEXT NOT NULL,
                line_start INTEGER NOT NULL,
                line_end   INTEGER NOT NULL,
                layer      TEXT NOT NULL,
                namespace  TEXT NOT NULL,
                node_hash  TEXT NOT NULL,
                preview    TEXT NOT NULL,
                key        INTEGER NOT NULL UNIQUE
            );
            CREATE INDEX IF NOT EXISTS idx_nodes_hash ON nodes(node_hash);",
        )?;
        Ok(Self { conn })
    }

    /// Look up a node row by its `@id`.
    ///
    /// # Errors
    /// Returns an error on any `SQLite` failure.
    pub fn lookup_by_id(&self, id: &str) -> Result<Option<NodeMeta>> {
        self.conn
            .query_row(
                "SELECT id, file, line_start, line_end, layer, namespace, \
                        node_hash, preview, key FROM nodes WHERE id = ?1",
                params![id],
                |row| {
                    Ok(NodeMeta {
                        id: row.get(0)?,
                        file: row.get(1)?,
                        line_start: row.get(2)?,
                        line_end: row.get(3)?,
                        layer: row.get(4)?,
                        namespace: row.get(5)?,
                        node_hash: row.get(6)?,
                        preview: row.get(7)?,
                        key: row.get(8)?,
                    })
                },
            )
            .optional()
            .context("lookup_by_id")
    }

    /// Look up a node row by its usearch `key`.
    ///
    /// # Errors
    /// Returns an error on any `SQLite` failure.
    pub fn get_by_key(&self, key: i64) -> Result<Option<NodeMeta>> {
        self.conn
            .query_row(
                "SELECT id, file, line_start, line_end, layer, namespace, \
                        node_hash, preview, key FROM nodes WHERE key = ?1",
                params![key],
                |row| {
                    Ok(NodeMeta {
                        id: row.get(0)?,
                        file: row.get(1)?,
                        line_start: row.get(2)?,
                        line_end: row.get(3)?,
                        layer: row.get(4)?,
                        namespace: row.get(5)?,
                        node_hash: row.get(6)?,
                        preview: row.get(7)?,
                        key: row.get(8)?,
                    })
                },
            )
            .optional()
            .context("get_by_key")
    }

    /// Insert or update a node row.
    ///
    /// # Errors
    /// Returns an error on any `SQLite` failure.
    pub fn upsert(&self, node: &NodeMeta) -> Result<()> {
        self.conn.execute(
            "INSERT INTO nodes (id, file, line_start, line_end, layer, \
                                namespace, node_hash, preview, key) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
             ON CONFLICT(id) DO UPDATE SET \
               file = excluded.file, \
               line_start = excluded.line_start, \
               line_end = excluded.line_end, \
               layer = excluded.layer, \
               namespace = excluded.namespace, \
               node_hash = excluded.node_hash, \
               preview = excluded.preview, \
               key = excluded.key",
            params![
                node.id, node.file, node.line_start, node.line_end, node.layer,
                node.namespace, node.node_hash, node.preview, node.key,
            ],
        )?;
        Ok(())
    }

    /// Remove a node row by `id`.
    ///
    /// # Errors
    /// Returns an error on any `SQLite` failure.
    pub fn delete(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM nodes WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Return every stored node `id`.
    ///
    /// # Errors
    /// Returns an error on any `SQLite` failure.
    pub fn all_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT id FROM nodes")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Return the largest stored usearch `key` (0 if empty).
    ///
    /// # Errors
    /// Returns an error on any `SQLite` failure.
    pub fn max_key(&self) -> Result<i64> {
        let k: Option<i64> = self
            .conn
            .query_row("SELECT MAX(key) FROM nodes", [], |row| row.get(0))
            .optional()?
            .flatten();
        Ok(k.unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample(id: &str, key: i64, hash: &str) -> NodeMeta {
        NodeMeta {
            id: id.into(),
            file: "/tmp/x.xml".into(),
            line_start: 1,
            line_end: 2,
            layer: "prose".into(),
            namespace: "urn:clayers:prose".into(),
            node_hash: hash.into(),
            preview: "p".into(),
            key,
        }
    }

    fn store() -> (TempDir, MetaStore) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("meta.sqlite");
        let s = MetaStore::open_or_create(&path).unwrap();
        (dir, s)
    }

    #[test]
    fn open_or_create_creates_schema() {
        let (_dir, s) = store();
        assert_eq!(s.all_ids().unwrap().len(), 0);
        assert_eq!(s.max_key().unwrap(), 0);
    }

    #[test]
    fn upsert_inserts_new_rows() {
        let (_dir, s) = store();
        s.upsert(&sample("a", 1, "sha256:aaa")).unwrap();
        s.upsert(&sample("b", 2, "sha256:bbb")).unwrap();
        assert_eq!(s.all_ids().unwrap().len(), 2);
        let row = s.lookup_by_id("a").unwrap().unwrap();
        assert_eq!(row.key, 1);
        assert_eq!(row.node_hash, "sha256:aaa");
    }

    #[test]
    fn upsert_updates_existing_row() {
        let (_dir, s) = store();
        s.upsert(&sample("a", 1, "sha256:aaa")).unwrap();
        // Same id, different hash + key.
        s.upsert(&sample("a", 42, "sha256:zzz")).unwrap();
        assert_eq!(s.all_ids().unwrap().len(), 1);
        let row = s.lookup_by_id("a").unwrap().unwrap();
        assert_eq!(row.key, 42);
        assert_eq!(row.node_hash, "sha256:zzz");
    }

    #[test]
    fn lookup_by_id_missing_returns_none() {
        let (_dir, s) = store();
        assert!(s.lookup_by_id("nope").unwrap().is_none());
    }

    #[test]
    fn get_by_key_roundtrip() {
        let (_dir, s) = store();
        s.upsert(&sample("x", 7, "sha256:h")).unwrap();
        let row = s.get_by_key(7).unwrap().unwrap();
        assert_eq!(row.id, "x");
        assert!(s.get_by_key(99).unwrap().is_none());
    }

    #[test]
    fn delete_removes_row() {
        let (_dir, s) = store();
        s.upsert(&sample("a", 1, "sha256:h")).unwrap();
        s.delete("a").unwrap();
        assert!(s.lookup_by_id("a").unwrap().is_none());
        // Deleting missing id is a no-op.
        s.delete("nope").unwrap();
    }

    #[test]
    fn max_key_tracks_largest() {
        let (_dir, s) = store();
        s.upsert(&sample("a", 5, "sha256:h")).unwrap();
        s.upsert(&sample("b", 3, "sha256:h")).unwrap();
        s.upsert(&sample("c", 99, "sha256:h")).unwrap();
        assert_eq!(s.max_key().unwrap(), 99);
    }

    #[test]
    fn unique_key_constraint_rejects_duplicate() {
        let (_dir, s) = store();
        s.upsert(&sample("a", 1, "sha256:h")).unwrap();
        // Inserting a *different* id with the same key should fail
        // the UNIQUE constraint.
        let err = s.upsert(&sample("b", 1, "sha256:h")).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("unique"),
            "expected unique-violation, got {err}"
        );
    }

    #[test]
    fn all_ids_returns_every_id() {
        let (_dir, s) = store();
        for (i, id) in ["x", "y", "z"].iter().enumerate() {
            #[allow(clippy::cast_possible_wrap)]
            s.upsert(&sample(id, i as i64 + 1, "sha256:h")).unwrap();
        }
        let mut ids = s.all_ids().unwrap();
        ids.sort();
        assert_eq!(ids, vec!["x".to_string(), "y".into(), "z".into()]);
    }
}
