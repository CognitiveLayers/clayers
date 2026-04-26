//! `init` and `clone` command implementations.

use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clayers_repo::SqliteStore;
use clayers_repo::refs::HEADS_PREFIX;
use clayers_repo::sync::{FastForwardOnly, sync_refs};
use clayers_xml::ContentHash;

use super::remote::{RemoteTarget, open_remote, parse_remote_url};
use super::schema::init_cli_schema;
use clayers_repo::{ObjectStore, RefStore};

use super::{block_on, open_cli_db};

/// Initialize a new clayers repository at `path`.
///
/// Creates the directory if it does not exist, opens/creates `.clayers.db`,
/// initializes both the clayers-repo object/ref schema and the CLI tables.
///
/// # Errors
///
/// Returns an error if the directory cannot be created or the database fails.
pub fn cmd_init(path: &Path) -> Result<()> {
    // Create the directory if it doesn't exist.
    if !path.exists() {
        std::fs::create_dir_all(path)
            .with_context(|| format!("failed to create directory {}", path.display()))?;
    }

    let db_path = path.join(".clayers.db");
    if db_path.exists() {
        bail!("repository already exists at {}", db_path.display());
    }

    // Open the SqliteStore (creates object/ref tables).
    block_on(async {
        SqliteStore::open(&db_path)
            .with_context(|| format!("failed to create store at {}", db_path.display()))?;
        Ok(())
    })?;

    // Open the connection and add CLI tables.
    let conn = open_cli_db(&db_path)?;
    init_cli_schema(&conn, false)?;

    println!("Initialized clayers repository in {}", path.display());
    Ok(())
}

/// Initialize a bare repository (a single `.db` file with no working copy).
///
/// # Errors
///
/// Returns an error if the file already exists or the database fails.
pub fn cmd_init_bare(db_path: &Path) -> Result<()> {
    if db_path.exists() {
        bail!("file already exists: {}", db_path.display());
    }

    // Create parent directory if needed.
    if let Some(parent) = db_path.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    block_on(async {
        SqliteStore::open(db_path)
            .with_context(|| format!("failed to create bare store at {}", db_path.display()))?;
        Ok(())
    })?;

    let conn = open_cli_db(db_path)?;
    init_cli_schema(&conn, true)?;

    println!(
        "Initialized bare clayers repository at {}",
        db_path.display()
    );
    Ok(())
}

/// Clone a repository from `source` into `target` directory.
///
/// The source can be a local path (to a `.db` file or directory) or a `ws://`
/// URL pointing at a remote server.
///
/// 1. Create and initialize the target directory.
/// 2. Sync all refs from source to target (fast-forward only).
/// 3. Add `origin` remote pointing to source.
/// 4. Check out the `main` branch (or first available branch).
///
/// # Errors
///
/// Returns an error if source doesn't exist (for local paths), the connection
/// fails (for ws:// URLs), or cloning fails.
pub fn cmd_clone(source: &str, target: &PathBuf, token: Option<&str>) -> Result<()> {
    let remote_target = parse_remote_url(source, token.map(String::from))?;

    // For local sources, verify the path exists.
    if let RemoteTarget::Local(ref path) = remote_target
        && !path.exists()
    {
        bail!("source not found: {}", path.display());
    }

    if target.exists() {
        bail!("target already exists: {}", target.display());
    }

    // Initialize target repo.
    std::fs::create_dir_all(target)
        .with_context(|| format!("failed to create {}", target.display()))?;

    let dst_db = target.join(".clayers.db");

    block_on(async {
        let src_store = open_remote(&remote_target)
            .await
            .context("failed to open source")?;
        let dst_store = SqliteStore::open(&dst_db)
            .with_context(|| format!("failed to create clone at {}", dst_db.display()))?;

        // Sync all branch refs.
        sync_refs(
            src_store.as_ref(),
            src_store.as_ref(),
            &dst_store,
            &dst_store,
            HEADS_PREFIX,
            &FastForwardOnly,
        )
        .await
        .context("failed to sync refs from source")?;

        Ok(())
    })?;

    // Initialize CLI tables.
    let conn = open_cli_db(&dst_db)?;
    init_cli_schema(&conn, false)?;

    // Add origin remote (include token for ws:// URLs).
    let origin_token: Option<&str> = match remote_target {
        RemoteTarget::WebSocket { ref token, .. } => token.as_deref(),
        RemoteTarget::Local(_) => None,
    };
    conn.execute(
        "INSERT OR REPLACE INTO remotes (name, url, token) VALUES ('origin', ?1, ?2)",
        rusqlite::params![source, origin_token],
    )
    .context("failed to add origin remote")?;

    // Export working copy from the default branch.
    let default_branch = find_default_branch(&dst_db)?;
    if let Some(branch) = default_branch {
        super::schema::set_meta(&conn, "current_branch", &branch)?;
        export_working_copy(&dst_db, target, &branch)?;
        super::branch::refresh_working_copy_table(&conn, &dst_db, &branch)?;
    }

    println!("Cloned into {}", target.display());
    Ok(())
}

/// Collect all file paths from a branch's tree.
async fn collect_tree_paths(
    store: &SqliteStore,
    branch: &str,
) -> Result<std::collections::HashSet<String>> {
    let branch_ref = clayers_repo::refs::branch_ref(branch);
    let Some(tip) = store.get_ref(&branch_ref).await? else {
        return Ok(std::collections::HashSet::new());
    };
    let Some(clayers_repo::object::Object::Commit(c)) = store.get(&tip).await? else {
        return Ok(std::collections::HashSet::new());
    };
    let Some(clayers_repo::object::Object::Tree(t)) = store.get(&c.tree).await? else {
        return Ok(std::collections::HashSet::new());
    };
    Ok(t.entries.iter().map(|e| e.path.clone()).collect())
}

/// Find the default branch (prefers "main", falls back to first available).
fn find_default_branch(db_path: &Path) -> Result<Option<String>> {
    block_on(async {
        let store = SqliteStore::open(db_path)?;
        let repo = clayers_repo::Repo::init(store);
        let branches = repo.list_branches().await?;
        if branches.is_empty() {
            return Ok(None);
        }
        // Prefer "main", else first alphabetically.
        if let Some((name, _)) = branches.iter().find(|(n, _)| n == "main") {
            return Ok(Some(name.clone()));
        }
        Ok(Some(branches[0].0.clone()))
    })
}

/// Export all files from a branch's tree to `working_dir`.
///
/// Removes files from the old tree that aren't in the new tree,
/// and writes/overwrites files from the new tree (like `git checkout`).
pub(crate) fn export_working_copy(db_path: &Path, working_dir: &Path, branch: &str) -> Result<()> {
    export_working_copy_with_old(db_path, working_dir, branch, None)
}

/// Export working copy, optionally removing files from `old_branch` not in `branch`.
pub(crate) fn export_working_copy_with_old(
    db_path: &Path,
    working_dir: &Path,
    branch: &str,
    old_branch: Option<&str>,
) -> Result<()> {
    let conn = open_cli_db(db_path)?;
    let tracked_hashes = super::staging::get_tracked_working_copy_hashes(&conn)?;
    let branch = branch.to_string();
    let old_branch = old_branch.map(String::from);
    let db_path = db_path.to_path_buf();
    let working_dir = working_dir.to_path_buf();

    block_on(async move {
        let store = SqliteStore::open(&db_path)?;

        // Collect old tree paths (if switching from another branch).
        let old_paths: std::collections::HashSet<String> = match &old_branch {
            Some(old_b) => collect_tree_paths(&store, old_b).await?,
            None => std::collections::HashSet::new(),
        };

        // Get new tree.
        let branch_ref = clayers_repo::refs::branch_ref(&branch);
        let Some(tip) = store.get_ref(&branch_ref).await? else {
            return Ok(());
        };
        let Some(obj) = store.get(&tip).await? else {
            return Ok(());
        };
        let tree_hash = match obj {
            clayers_repo::object::Object::Commit(c) => c.tree,
            _ => return Ok(()),
        };

        let files = clayers_repo::export::export_tree(&store, tree_hash).await?;
        let new_paths: std::collections::HashSet<String> =
            files.iter().map(|(p, _)| p.clone()).collect();

        // Remove files from old tree that aren't in new tree.
        for old_path in &old_paths {
            if !new_paths.contains(old_path) {
                ensure_export_path_safe(&store, &working_dir, &tracked_hashes, old_path, "removed")
                    .await?;
                let file_path = working_dir.join(old_path);
                if file_path.exists() {
                    std::fs::remove_file(&file_path).ok();
                }
            }
        }

        // Write new tree files.
        for (path, xml) in &files {
            ensure_export_path_safe(&store, &working_dir, &tracked_hashes, path, "overwritten")
                .await?;
            let file_path = working_dir.join(path);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&file_path, xml.as_bytes())?;
        }

        Ok(())
    })
}

async fn ensure_export_path_safe(
    store: &SqliteStore,
    working_dir: &Path,
    tracked_hashes: &HashMap<String, ContentHash>,
    rel_path: &str,
    effect: &str,
) -> Result<()> {
    let Some((expected_hash, xml)) =
        read_tracked_export_path(working_dir, tracked_hashes, rel_path, effect).await?
    else {
        return Ok(());
    };

    match clayers_repo::import::import_xml(store, &xml).await {
        Ok(current_hash) if current_hash == expected_hash => Ok(()),
        _ => bail!("cannot update working copy: unstaged change would be {effect}: {rel_path}"),
    }
}

async fn read_tracked_export_path(
    working_dir: &Path,
    tracked_hashes: &HashMap<String, ContentHash>,
    rel_path: &str,
    effect: &str,
) -> Result<Option<(ContentHash, String)>> {
    let file_path = working_dir.join(rel_path);
    let Some(expected_hash) = tracked_hashes.get(rel_path) else {
        let exists = tokio::fs::try_exists(&file_path).await.with_context(|| {
            format!("cannot update working copy: cannot inspect target path that would be {effect}: {rel_path}")
        })?;
        if exists {
            bail!("cannot update working copy: untracked file would be {effect}: {rel_path}");
        }
        return Ok(None);
    };

    match tokio::fs::metadata(&file_path).await {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => {
            bail!("cannot update working copy: unstaged deletion would be {effect}: {rel_path}")
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {
            bail!("cannot update working copy: unstaged deletion would be {effect}: {rel_path}");
        }
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "cannot update working copy: unreadable tracked file would be {effect}: {rel_path}"
                )
            });
        }
    }

    let xml = tokio::fs::read_to_string(&file_path)
        .await
        .with_context(|| {
            format!(
                "cannot update working copy: unreadable tracked file would be {effect}: {rel_path}"
            )
        })?;
    Ok(Some((*expected_hash, xml)))
}

pub(crate) async fn ensure_export_paths_safe(
    store: &SqliteStore,
    working_dir: &Path,
    tracked_hashes: &HashMap<String, ContentHash>,
    paths: &[String],
    effect: &str,
) -> Result<()> {
    for path in paths {
        ensure_export_path_safe(store, working_dir, tracked_hashes, path, effect).await?;
    }
    Ok(())
}
