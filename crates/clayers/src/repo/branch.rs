//! `branch` and `checkout` command implementations.

use std::path::Path;

use anyhow::{Context, Result, bail};
use clayers_repo::{ObjectStore, RefStore, SqliteStore};
use clayers_xml::ContentHash;
use clayers_repo::refs::get_branch;

use super::{block_on, discover_repo, open_cli_db};
use super::schema::{get_meta, set_meta};

/// List, create, or delete branches.
///
/// - `name = None`: list all branches with `*` on current
/// - `name = Some(n)`: create branch `n` at current HEAD
/// - `delete = Some(n)`: delete branch `n`
///
/// # Errors
///
/// Returns an error if branch operations fail.
pub fn cmd_branch(name: Option<&str>, delete: Option<&str>) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get CWD")?;
    let (_, db_path) = discover_repo(&cwd)?;
    let conn = open_cli_db(&db_path)?;
    let current = get_meta(&conn, "current_branch")?.unwrap_or_else(|| "main".into());

    if let Some(del) = delete {
        // Delete branch.
        if del == current {
            bail!("cannot delete the currently checked-out branch '{del}'");
        }
        let del_owned = del.to_string();
        block_on(async move {
            let store = SqliteStore::open(&db_path)?;
            let repo = clayers_repo::Repo::init(store);
            repo.delete_branch(&del_owned).await.with_context(|| format!("failed to delete branch '{del_owned}'"))?;
            Ok(())
        })?;
        println!("Deleted branch '{del}'");
        return Ok(());
    }

    if let Some(new_branch) = name {
        // Create branch at current HEAD.
        let new_branch_owned = new_branch.to_string();
        block_on(async move {
            let store = SqliteStore::open(&db_path)?;
            let tip = get_branch(&store, &current)
                .await?
                .with_context(|| format!("no commits on branch '{current}'"))?;
            let repo = clayers_repo::Repo::init(store);
            repo.create_branch(&new_branch_owned, tip)
                .await
                .with_context(|| format!("failed to create branch '{new_branch_owned}'"))?;
            Ok(())
        })?;
        println!("Created branch '{new_branch}'");
        return Ok(());
    }

    // List branches.
    block_on(async move {
        let store = SqliteStore::open(&db_path)?;
        let repo = clayers_repo::Repo::init(store);
        let mut branches = repo.list_branches().await?;
        branches.sort_by(|a, b| a.0.cmp(&b.0));
        for (branch, _) in &branches {
            let marker = if branch == &current { "* " } else { "  " };
            println!("{marker}{branch}");
        }
        if branches.is_empty() {
            println!("(no branches)");
        }
        Ok(())
    })
}

/// Switch to a different branch (optionally creating it with `-b`).
///
/// Checks for a dirty staging area before switching.
///
/// # Errors
///
/// Returns an error if the branch doesn't exist (without `-b`) or is dirty.
pub fn cmd_checkout(branch: &str, create: bool, orphan: bool) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get CWD")?;
    let (repo_root, db_path) = discover_repo(&cwd)?;
    let conn = open_cli_db(&db_path)?;

    // Check for uncommitted staged changes.
    let staged_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM staging", [], |row| row.get(0))
        .context("failed to check staging")?;
    if staged_count > 0 {
        bail!(
            "cannot checkout: you have {staged_count} staged change(s). \
             Commit or revert them first."
        );
    }

    let current = get_meta(&conn, "current_branch")?.unwrap_or_else(|| "main".into());
    if branch == current && !create && !orphan {
        println!("Already on '{branch}'");
        return Ok(());
    }

    if orphan {
        // Orphan branch: switch to a new branch with no history.
        // Remove all tracked files from disk, clear working_copy table.
        // The next commit on this branch will be a root commit (no parents).
        super::init::export_working_copy_with_old(
            &db_path, &repo_root, branch, Some(&current),
        )?;

        // Clear working_copy (orphan has no tree yet).
        conn.execute("DELETE FROM working_copy", [])
            .context("failed to clear working_copy")?;

        set_meta(&conn, "current_branch", branch)?;

        // Remove all tracked files from disk (orphan starts empty).
        remove_tracked_files(&conn, &db_path, &repo_root, &current)?;

        println!("Switched to a new orphan branch '{branch}'");
        return Ok(());
    }

    let branch_owned = branch.to_string();
    let db_path_clone = db_path.clone();
    let current_for_export = current.clone();

    block_on(async move {
        let store = SqliteStore::open(&db_path_clone)?;

        if create {
            // Get current HEAD hash.
            let tip = get_branch(&store, &current).await
                .map_err(|e| anyhow::anyhow!(e))?;
            match tip {
                Some(h) => {
                    let repo = clayers_repo::Repo::init(store);
                    repo.create_branch(&branch_owned, h).await
                        .with_context(|| format!("failed to create branch '{branch_owned}'"))?;
                }
                None => {
                    bail!("cannot create branch from empty repository");
                }
            }
        } else {
            // Branch must exist.
            let repo = clayers_repo::Repo::init(store);
            let branches = repo.list_branches().await?;
            if !branches.iter().any(|(n, _)| n == &branch_owned) {
                bail!(
                    "branch '{branch_owned}' not found (use 'clayers checkout -b {branch_owned}' to create)"
                );
            }
        }
        Ok(())
    })?;

    // Export tree to disk, removing files from old branch not in new branch.
    super::init::export_working_copy_with_old(
        &db_path, &repo_root, branch, Some(&current_for_export),
    )?;

    // Update current_branch.
    set_meta(&conn, "current_branch", branch)?;

    // Refresh working_copy table.
    refresh_working_copy_table(&conn, &db_path, branch)?;

    println!("Switched to branch '{branch}'");
    Ok(())
}

/// Remove all tracked files for a branch from disk.
fn remove_tracked_files(
    _conn: &rusqlite::Connection,
    db_path: &Path,
    repo_root: &Path,
    branch: &str,
) -> Result<()> {
    let db_path = db_path.to_path_buf();
    let branch = branch.to_string();
    let repo_root = repo_root.to_path_buf();

    let paths: Vec<String> = block_on(async move {
        let store = SqliteStore::open(&db_path)?;
        let branch_ref = clayers_repo::refs::branch_ref(&branch);
        let Some(tip) = store.get_ref(&branch_ref).await? else {
            return Ok(vec![]);
        };
        let Some(clayers_repo::object::Object::Commit(c)) = store.get(&tip).await? else {
            return Ok(vec![]);
        };
        let Some(clayers_repo::object::Object::Tree(t)) = store.get(&c.tree).await? else {
            return Ok(vec![]);
        };
        Ok(t.entries.iter().map(|e| e.path.clone()).collect())
    })?;

    for path in &paths {
        let file_path = repo_root.join(path);
        if file_path.exists() {
            std::fs::remove_file(&file_path).ok();
        }
    }
    Ok(())
}

/// Update the `working_copy` table to match the current tree on a branch.
///
/// Called after checkout and pull.
pub fn refresh_working_copy_table(
    conn: &rusqlite::Connection,
    db_path: &Path,
    branch: &str,
) -> Result<()> {
    let db_path = db_path.to_path_buf();
    let branch = branch.to_string();

    let entries: Vec<(String, ContentHash)> = block_on(async move {
        let store = SqliteStore::open(&db_path)?;
        let Some(tip) = get_branch(&store, &branch).await? else { return Ok(vec![]) };
        let Some(obj) = store.get(&tip).await? else { return Ok(vec![]) };
        let tree_hash = match obj {
            clayers_repo::object::Object::Commit(c) => c.tree,
            _ => return Ok(vec![]),
        };
        let Some(clayers_repo::object::Object::Tree(tree_obj)) = store.get(&tree_hash).await? else { return Ok(vec![]) };
        Ok(tree_obj.entries.iter()
            .map(|e| (e.path.clone(), e.document))
            .collect::<Vec<_>>())
    })?;

    conn.execute("DELETE FROM working_copy", [])
        .context("failed to clear working_copy")?;
    for (path, hash) in &entries {
        conn.execute(
            "INSERT INTO working_copy (file_path, doc_hash) VALUES (?1, ?2)",
            rusqlite::params![path, hash.0.as_slice()],
        )
        .context("failed to update working_copy")?;
    }
    Ok(())
}

