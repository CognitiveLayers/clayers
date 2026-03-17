//! `remote`, `push`, and `pull` command implementations.

use std::path::Path;

use anyhow::{Context, Result, bail};
use clayers_repo::SqliteStore;
use clayers_repo::refs::HEADS_PREFIX;
use clayers_repo::sync::{FastForwardOnly, sync_refs};

use super::{block_on, discover_repo, open_cli_db};
use super::schema::get_meta;

/// Subcommand for `remote`.
pub enum RemoteAction {
    Add { name: String, url: String },
    Remove { name: String },
    List,
}

/// Execute a `remote` subcommand.
///
/// # Errors
///
/// Returns an error if the operation fails.
pub fn cmd_remote(action: RemoteAction) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get CWD")?;
    let (_, db_path) = discover_repo(&cwd)?;
    let conn = open_cli_db(&db_path)?;

    match action {
        RemoteAction::Add { name, url } => {
            conn.execute(
                "INSERT OR REPLACE INTO remotes (name, url) VALUES (?1, ?2)",
                rusqlite::params![name, url],
            )
            .context("failed to add remote")?;
            println!("remote '{name}' added -> {url}");
        }
        RemoteAction::Remove { name } => {
            let n = conn
                .execute("DELETE FROM remotes WHERE name = ?1", rusqlite::params![name])
                .context("failed to remove remote")?;
            if n == 0 {
                bail!("remote '{name}' not found");
            }
            println!("remote '{name}' removed");
        }
        RemoteAction::List => {
            let mut stmt = conn
                .prepare("SELECT name, url FROM remotes ORDER BY name")
                .context("failed to query remotes")?;
            let rows = stmt
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .context("failed to iterate remotes")?;
            for row in rows {
                let (name, url) = row.context("failed to read remote row")?;
                println!("{name}\t{url}");
            }
        }
    }

    Ok(())
}

/// Push local refs to a remote.
///
/// # Errors
///
/// Returns an error if the remote is not found or push fails.
pub fn cmd_push(remote_name: Option<&str>) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get CWD")?;
    let (_, db_path) = discover_repo(&cwd)?;
    let conn = open_cli_db(&db_path)?;

    let remote = resolve_remote(&conn, remote_name)?;
    let remote_url = remote.url;

    println!("Pushing to {remote_url}...");

    block_on(async move {
        let local = SqliteStore::open(&db_path)?;
        let remote_store = SqliteStore::open(Path::new(&remote_url))
            .with_context(|| format!("failed to open remote at {remote_url}"))?;

        let count = sync_refs(
            &local,
            &local,
            &remote_store,
            &remote_store,
            HEADS_PREFIX,
            &FastForwardOnly,
        )
        .await
        .context("push failed")?;

        if count == 0 {
            println!("Everything up-to-date");
        } else {
            println!("Pushed {count} ref(s)");
        }
        Ok(())
    })
}

/// Pull refs from a remote and update working copy.
///
/// # Errors
///
/// Returns an error if the remote is not found or pull fails.
pub fn cmd_pull(remote_name: Option<&str>) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get CWD")?;
    let (repo_root, db_path) = discover_repo(&cwd)?;
    let conn = open_cli_db(&db_path)?;

    let remote = resolve_remote(&conn, remote_name)?;
    let remote_url = remote.url.clone();
    let current_branch = get_meta(&conn, "current_branch")?.unwrap_or_else(|| "main".into());

    println!("Pulling from {remote_url}...");

    let db_path_clone = db_path.clone();

    block_on(async move {
        let remote_store = SqliteStore::open(Path::new(&remote_url))
            .with_context(|| format!("failed to open remote at {remote_url}"))?;
        let local_store = SqliteStore::open(&db_path_clone)?;

        let count = sync_refs(
            &remote_store,
            &remote_store,
            &local_store,
            &local_store,
            HEADS_PREFIX,
            &FastForwardOnly,
        )
        .await
        .context("pull failed")?;

        if count == 0 {
            println!("Already up-to-date");
        } else {
            println!("Pulled {count} ref(s)");
        }
        Ok(())
    })?;

    // Update working copy on disk if current branch was updated.
    super::init::export_working_copy(&db_path, &repo_root, &current_branch)?;

    // Update working_copy table.
    let conn = open_cli_db(&db_path)?;
    super::branch::refresh_working_copy_table(&conn, &db_path, &current_branch)?;

    Ok(())
}

struct RemoteInfo {
    url: String,
}

fn resolve_remote(conn: &rusqlite::Connection, name: Option<&str>) -> Result<RemoteInfo> {
    let name = name.unwrap_or("origin");
    let result = conn.query_row(
        "SELECT url FROM remotes WHERE name = ?1",
        rusqlite::params![name],
        |row| row.get::<_, String>(0),
    );
    match result {
        Ok(url) => Ok(RemoteInfo { url }),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            bail!("remote '{name}' not found (use 'clayers remote add {name} <url>')")
        }
        Err(e) => Err(anyhow::anyhow!(e).context("failed to query remotes")),
    }
}
