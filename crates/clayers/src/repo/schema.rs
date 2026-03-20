//! CLI `SQLite` table creation and migration.
//!
//! Manages the four tables used by the clayers CLI on top of the
//! clayers-repo object/ref tables: `cli_meta`, `working_copy`, `staging`,
//! and `remotes`.

use anyhow::{Context, Result, bail};
use rusqlite::Connection;

/// Initialize the CLI schema tables in an existing `SQLite` connection.
///
/// Safe to call multiple times: all CREATE statements use `IF NOT EXISTS`.
///
/// # Errors
///
/// Returns an error if any SQL execution fails.
pub fn init_cli_schema(conn: &Connection, bare: bool) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS cli_meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS working_copy (
            file_path TEXT PRIMARY KEY,
            doc_hash  BLOB
        );
        CREATE TABLE IF NOT EXISTS staging (
            file_path TEXT PRIMARY KEY,
            action    TEXT NOT NULL CHECK(action IN ('add', 'modify', 'delete')),
            doc_hash  BLOB
        );
        CREATE TABLE IF NOT EXISTS remotes (
            name  TEXT PRIMARY KEY,
            url   TEXT NOT NULL,
            token TEXT
        );",
    )
    .context("failed to create CLI tables")?;

    // Set initial meta values (only if not already present).
    conn.execute(
        "INSERT OR IGNORE INTO cli_meta (key, value) VALUES ('schema_version', '2')",
        [],
    )
    .context("failed to set schema_version")?;

    let bare_val = if bare { "1" } else { "0" };
    conn.execute(
        "INSERT OR IGNORE INTO cli_meta (key, value) VALUES ('bare', ?1)",
        rusqlite::params![bare_val],
    )
    .context("failed to set bare flag")?;

    conn.execute(
        "INSERT OR IGNORE INTO cli_meta (key, value) VALUES ('current_branch', 'main')",
        [],
    )
    .context("failed to set current_branch")?;

    Ok(())
}

/// Read a value from `cli_meta`.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn get_meta(conn: &Connection, key: &str) -> Result<Option<String>> {
    let result = conn.query_row(
        "SELECT value FROM cli_meta WHERE key = ?1",
        rusqlite::params![key],
        |row| row.get::<_, String>(0),
    );
    match result {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(anyhow::anyhow!(e).context(format!("failed to read cli_meta key '{key}'"))),
    }
}

/// Set a value in `cli_meta`.
///
/// # Errors
///
/// Returns an error if the update fails.
pub fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO cli_meta (key, value) VALUES (?1, ?2)",
        rusqlite::params![key, value],
    )
    .context("failed to update cli_meta")?;
    Ok(())
}

/// Run forward migrations on an existing database.
///
/// Reads `schema_version` from `cli_meta` and applies any pending migrations.
/// Currently migrates from version 1 to 2 (adds `token` column to `remotes`).
///
/// # Errors
///
/// Returns an error if migration SQL fails or the schema version is unrecognised.
pub fn migrate_schema(conn: &Connection) -> Result<()> {
    // If cli_meta doesn't exist yet, skip migration (init_cli_schema will
    // create tables at the current version).
    let has_meta: bool = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='cli_meta'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);

    if !has_meta {
        return Ok(());
    }

    let version = get_meta(conn, "schema_version")?;
    let version = version.as_deref().unwrap_or("1");

    match version {
        "1" => {
            conn.execute_batch("ALTER TABLE remotes ADD COLUMN token TEXT;")
                .context("migration v1->v2: failed to add token column")?;
            set_meta(conn, "schema_version", "2")?;
        }
        "2" => { /* already current */ }
        other => bail!("unknown schema_version '{other}'"),
    }

    Ok(())
}
