//! `remote`, `push`, `pull`, and `list-repos` command implementations.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clayers_repo::SqliteStore;
use clayers_repo::object::Object;
use clayers_repo::refs::HEADS_PREFIX;
use clayers_repo::store::remote::Store;
use clayers_repo::store::remote::{
    BearerToken, JsonCodec, RemoteStore, WsTransport, list_repositories,
};
use clayers_repo::sync::{FastForwardOnly, sync_refs};
use clayers_xml::ContentHash;

use super::schema::get_meta;
use super::{block_on, discover_repo, open_cli_db};

/// Parsed remote URL target.
pub enum RemoteTarget {
    /// A local path to a `.db` file.
    Local(std::path::PathBuf),
    /// A WebSocket URL (`ws://` or `wss://`) with an optional bearer token and
    /// the repository name (last path segment).
    WebSocket {
        /// Full URL including the repo path segment.
        url: String,
        /// Optional bearer token for authentication.
        token: Option<String>,
        /// Repository name extracted from the URL path.
        repo: String,
    },
}

/// Parse a remote URL string into a [`RemoteTarget`].
///
/// # Errors
///
/// Returns an error if the URL is empty or a `ws://` URL has no path segment.
pub fn parse_remote_url(url: &str, token: Option<String>) -> Result<RemoteTarget> {
    if url.starts_with("ws://") || url.starts_with("wss://") {
        let repo = url
            .rsplit('/')
            .find(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("ws:// URL must contain a repository path segment"))?
            .to_string();
        Ok(RemoteTarget::WebSocket {
            url: url.to_string(),
            token,
            repo,
        })
    } else {
        Ok(RemoteTarget::Local(std::path::PathBuf::from(url)))
    }
}

/// Open a store for the given [`RemoteTarget`].
///
/// # Errors
///
/// Returns an error if the store cannot be opened or the WebSocket connection fails.
pub async fn open_remote(target: &RemoteTarget) -> Result<Box<dyn Store>> {
    match target {
        RemoteTarget::Local(path) => {
            let store = SqliteStore::open(path)
                .with_context(|| format!("failed to open local remote at {}", path.display()))?;
            Ok(Box::new(store))
        }
        RemoteTarget::WebSocket { url, token, repo } => {
            let auth: Option<BearerToken> = token.as_ref().map(|t| BearerToken(t.clone()));
            let auth_ref: Option<&dyn clayers_repo::store::remote::WsRequestTransformer> = auth
                .as_ref()
                .map(|a| a as &dyn clayers_repo::store::remote::WsRequestTransformer);
            let transport = WsTransport::connect(url, JsonCodec, auth_ref)
                .await
                .with_context(|| format!("failed to connect to {url}"))?;
            let store = RemoteStore::new(transport, repo);
            Ok(Box::new(store))
        }
    }
}

/// Subcommand for `remote`.
pub enum RemoteAction {
    /// Add a named remote.
    Add {
        /// Remote name.
        name: String,
        /// Remote URL.
        url: String,
        /// Optional bearer token.
        token: Option<String>,
    },
    /// Remove a named remote.
    Remove {
        /// Remote name.
        name: String,
    },
    /// List all remotes.
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
        RemoteAction::Add { name, url, token } => {
            conn.execute(
                "INSERT OR REPLACE INTO remotes (name, url, token) VALUES (?1, ?2, ?3)",
                rusqlite::params![name, url, token],
            )
            .context("failed to add remote")?;
            println!("remote '{name}' added -> {url}");
        }
        RemoteAction::Remove { name } => {
            let n = conn
                .execute(
                    "DELETE FROM remotes WHERE name = ?1",
                    rusqlite::params![name],
                )
                .context("failed to remove remote")?;
            if n == 0 {
                bail!("remote '{name}' not found");
            }
            println!("remote '{name}' removed");
        }
        RemoteAction::List => {
            let mut stmt = conn
                .prepare("SELECT name, url, token FROM remotes ORDER BY name")
                .context("failed to query remotes")?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })
                .context("failed to iterate remotes")?;
            for row in rows {
                let (name, url, token) = row.context("failed to read remote row")?;
                if token.is_some() {
                    println!("{name}\t{url}\t(token set)");
                } else {
                    println!("{name}\t{url}");
                }
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
    let remote_url = remote.url.clone();

    println!("Pushing to {remote_url}...");

    block_on(async move {
        let local = SqliteStore::open(&db_path)?;
        let target = parse_remote_url(&remote.url, remote.token)?;
        let remote_store = open_remote(&target).await?;

        let count = sync_refs(
            &local,
            &local,
            remote_store.as_ref(),
            remote_store.as_ref(),
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

    super::staging::ensure_clean_working_copy(&conn, &db_path, &repo_root, "pull")?;
    let tracked_hashes = super::staging::get_tracked_working_copy_hashes(&conn)?;

    println!("Pulling from {remote_url}...");

    let db_path_clone = db_path.clone();
    let repo_root_clone = repo_root.clone();
    let current_branch_clone = current_branch.clone();

    block_on(async move {
        let target = parse_remote_url(&remote_url, remote.token)?;
        let remote_store = open_remote(&target).await?;
        let local_store = SqliteStore::open(&db_path_clone)?;

        preflight_pull_working_copy(
            remote_store.as_ref(),
            &local_store,
            &repo_root_clone,
            &tracked_hashes,
            &current_branch_clone,
        )
        .await?;

        let count = sync_refs(
            remote_store.as_ref(),
            remote_store.as_ref(),
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

async fn preflight_pull_working_copy(
    remote_store: &dyn Store,
    local_store: &SqliteStore,
    repo_root: &Path,
    tracked_hashes: &HashMap<String, ContentHash>,
    branch: &str,
) -> Result<()> {
    let branch_ref = clayers_repo::refs::branch_ref(branch);
    let Some(remote_tip) = remote_store.get_ref(&branch_ref).await? else {
        return Ok(());
    };
    let Some(remote_obj) = remote_store.get(&remote_tip).await? else {
        bail!("remote branch '{branch}' points to missing commit {remote_tip}");
    };
    let Object::Commit(commit) = remote_obj else {
        bail!("remote branch '{branch}' does not point to a commit");
    };
    let Some(tree_obj) = remote_store.get(&commit.tree).await? else {
        bail!(
            "remote branch '{branch}' points to missing tree {}",
            commit.tree
        );
    };
    let Object::Tree(tree) = tree_obj else {
        bail!("remote branch '{branch}' commit does not point to a tree");
    };
    let target_paths: Vec<String> = tree
        .entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect();

    super::init::ensure_export_paths_safe(
        local_store,
        repo_root,
        tracked_hashes,
        &target_paths,
        "overwritten",
    )
    .await
}

/// List repositories available on a remote server.
///
/// # Errors
///
/// Returns an error if the connection or listing fails.
pub fn cmd_list_repos(url: &str, token: Option<&str>) -> Result<()> {
    block_on(async move {
        let auth: Option<BearerToken> = token.map(|t| BearerToken(t.to_string()));
        let auth_ref: Option<&dyn clayers_repo::store::remote::WsRequestTransformer> = auth
            .as_ref()
            .map(|a| a as &dyn clayers_repo::store::remote::WsRequestTransformer);
        let transport = WsTransport::connect(url, JsonCodec, auth_ref)
            .await
            .with_context(|| format!("failed to connect to {url}"))?;
        let repos = list_repositories(&transport)
            .await
            .context("failed to list repositories")?;

        if repos.is_empty() {
            println!("No repositories found");
        } else {
            for repo in &repos {
                println!("{repo}");
            }
        }
        Ok(())
    })
}

struct RemoteInfo {
    url: String,
    token: Option<String>,
}

fn resolve_remote(conn: &rusqlite::Connection, name: Option<&str>) -> Result<RemoteInfo> {
    let name = name.unwrap_or("origin");
    let result = conn.query_row(
        "SELECT url, token FROM remotes WHERE name = ?1",
        rusqlite::params![name],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
    );
    match result {
        Ok((url, token)) => Ok(RemoteInfo { url, token }),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            bail!("remote '{name}' not found (use 'clayers remote add {name} <url>')")
        }
        Err(e) => Err(anyhow::anyhow!(e).context("failed to query remotes")),
    }
}
