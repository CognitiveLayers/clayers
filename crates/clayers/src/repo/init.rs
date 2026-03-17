//! `init` and `clone` command implementations.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clayers_repo::SqliteStore;
use clayers_repo::refs::HEADS_PREFIX;
use clayers_repo::sync::{FastForwardOnly, sync_refs};

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
        && !parent.exists() {
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

    println!("Initialized bare clayers repository at {}", db_path.display());
    Ok(())
}

/// Clone a repository from `source` (a `.db` file) into `target` directory.
///
/// 1. Create and initialize the target directory.
/// 2. Sync all refs from source → target (fast-forward only).
/// 3. Add `origin` remote pointing to source.
/// 4. Check out the `main` branch (or first available branch).
///
/// # Errors
///
/// Returns an error if source doesn't exist or cloning fails.
pub fn cmd_clone(source: &Path, target: &PathBuf) -> Result<()> {
    if !source.exists() {
        bail!("source not found: {}", source.display());
    }

    if target.exists() {
        bail!("target already exists: {}", target.display());
    }

    // Initialize target repo.
    std::fs::create_dir_all(target)
        .with_context(|| format!("failed to create {}", target.display()))?;

    let dst_db = target.join(".clayers.db");

    block_on(async {
        let src_store = SqliteStore::open(source)
            .with_context(|| format!("failed to open source {}", source.display()))?;
        let dst_store = SqliteStore::open(&dst_db)
            .with_context(|| format!("failed to create clone at {}", dst_db.display()))?;

        // Sync all branch refs.
        sync_refs(
            &src_store,
            &src_store,
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

    // Add origin remote.
    conn.execute(
        "INSERT OR REPLACE INTO remotes (name, url) VALUES ('origin', ?1)",
        rusqlite::params![source.to_string_lossy().as_ref()],
    )
    .context("failed to add origin remote")?;

    // Export working copy from the default branch.
    let default_branch = find_default_branch(&dst_db)?;
    if let Some(branch) = default_branch {
        super::schema::set_meta(&conn, "current_branch", &branch)?;
        export_working_copy(&dst_db, target, &branch)?;
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
pub(crate) fn export_working_copy(
    db_path: &Path,
    working_dir: &Path,
    branch: &str,
) -> Result<()> {
    export_working_copy_with_old(db_path, working_dir, branch, None)
}

/// Export working copy, optionally removing files from `old_branch` not in `branch`.
pub(crate) fn export_working_copy_with_old(
    db_path: &Path,
    working_dir: &Path,
    branch: &str,
    old_branch: Option<&str>,
) -> Result<()> {
    let branch = branch.to_string();
    let old_branch = old_branch.map(String::from);
    let db_path = db_path.to_path_buf();
    let working_dir = working_dir.to_path_buf();

    block_on(async move {
        let store = SqliteStore::open(&db_path)?;

        // Collect old tree paths (if switching from another branch).
        let old_paths: std::collections::HashSet<String> = match &old_branch {
            Some(old_b) => collect_tree_paths(&store, old_b).await.unwrap_or_default(),
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
                let file_path = working_dir.join(old_path);
                if file_path.exists() {
                    std::fs::remove_file(&file_path).ok();
                }
            }
        }

        // Write new tree files.
        for (path, xml) in &files {
            let file_path = working_dir.join(path);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&file_path, xml.as_bytes())?;
        }

        Ok(())
    })
}
