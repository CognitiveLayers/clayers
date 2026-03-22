//! `merge` command implementation.

use anyhow::{Context, Result, bail};
use clayers_repo::{ObjectStore, SqliteStore};
use clayers_repo::merge::{AutoMerge, Manual, MergeOutcome, MergePolicy, Ours, Theirs};
use clayers_repo::refs::get_branch;

use super::{block_on, discover_repo, open_cli_db, resolve_author};
use super::branch::refresh_working_copy_table;
use super::schema::get_meta;

/// Execute the `merge` command.
///
/// # Errors
///
/// Returns an error if merge operations fail.
pub fn cmd_merge(
    branch: &str,
    strategy: &str,
    message: Option<&str>,
    flag_author: Option<&str>,
    flag_email: Option<&str>,
) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get CWD")?;
    let (repo_root, db_path) = discover_repo(&cwd)?;
    let conn = open_cli_db(&db_path)?;
    let current = get_meta(&conn, "current_branch")?.unwrap_or_else(|| "main".into());

    if branch == current {
        bail!("cannot merge a branch into itself");
    }

    let author = resolve_author(flag_author, flag_email)?;
    let msg = message.map_or_else(
        || format!("Merge branch '{branch}' into {current}"),
        ToString::to_string,
    );

    let policy = match strategy {
        "ours" => MergePolicy {
            default: Box::new(Ours),
            file_overrides: vec![],
        },
        "theirs" => MergePolicy {
            default: Box::new(Theirs),
            file_overrides: vec![],
        },
        "manual" => MergePolicy {
            default: Box::new(Manual),
            file_overrides: vec![],
        },
        _ => MergePolicy {
            default: Box::new(AutoMerge),
            file_overrides: vec![],
        },
    };

    let branch_owned = branch.to_string();
    let current_owned = current.clone();
    let db_path_for_merge = db_path.clone();

    let outcome = block_on(async move {
        let store = SqliteStore::open(&db_path_for_merge)?;
        let repo = clayers_repo::Repo::init(store);
        repo.merge(&current_owned, &branch_owned, &author, &msg, &policy)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    })?;

    match outcome {
        MergeOutcome::FastForward { commit } => {
            // Export working copy to disk.
            export_merged_tree(&db_path, &repo_root, &current)?;
            refresh_working_copy_table(&conn, &db_path, &current)?;
            println!("Fast-forward merge to {commit}");
        }
        MergeOutcome::Merged { commit, result } => {
            // Export working copy to disk.
            export_merged_tree(&db_path, &repo_root, &current)?;
            refresh_working_copy_table(&conn, &db_path, &current)?;

            println!("Merge commit: {commit}");
            if !result.auto_merged.is_empty() {
                println!("Auto-merged:");
                for f in &result.auto_merged {
                    println!("  {f}");
                }
            }
            if !result.ours_only.is_empty() {
                println!("Changed on ours:");
                for f in &result.ours_only {
                    println!("  {f}");
                }
            }
            if !result.theirs_only.is_empty() {
                println!("Changed on theirs:");
                for f in &result.theirs_only {
                    println!("  {f}");
                }
            }
            if !result.conflicts.is_empty() {
                println!("CONFLICTS:");
                for c in &result.conflicts {
                    println!("  {}: {} ({})", c.path, c.description, c.divergence_path);
                }
                std::process::exit(1);
            }
        }
        MergeOutcome::UpToDate => {
            println!("Already up to date.");
        }
        MergeOutcome::NoCommonAncestor => {
            bail!("cannot merge: no common ancestor found");
        }
    }

    Ok(())
}

/// Export all files from the current branch tree to disk, removing
/// files that were in the old working copy but are absent from the
/// merged tree.
fn export_merged_tree(
    db_path: &std::path::Path,
    repo_root: &std::path::Path,
    branch: &str,
) -> Result<()> {
    let conn = super::open_cli_db(db_path)?;

    // Collect old working copy paths (pre-merge state).
    let mut stmt = conn
        .prepare("SELECT file_path FROM working_copy")
        .context("failed to query working_copy")?;
    let old_paths: std::collections::HashSet<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(Result::ok)
        .collect();

    let db_path = db_path.to_path_buf();
    let branch = branch.to_string();
    let repo_root = repo_root.to_path_buf();

    block_on(async move {
        let store = SqliteStore::open(&db_path)?;
        let tip = get_branch(&store, &branch)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .context("branch tip not found after merge")?;
        let obj = store
            .get(&tip)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .context("commit not found")?;
        let tree_hash = match obj {
            clayers_repo::Object::Commit(c) => c.tree,
            _ => bail!("expected commit"),
        };
        let files = clayers_repo::export::export_tree(&store, tree_hash)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        // Write new/updated files.
        let new_paths: std::collections::HashSet<String> =
            files.iter().map(|(p, _)| p.clone()).collect();
        for (path, xml) in &files {
            let file_path = repo_root.join(path);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(&file_path, xml)?;
        }

        // Remove files that were in the old tree but not in the merged tree.
        for old_path in &old_paths {
            if !new_paths.contains(old_path) {
                let file_path = repo_root.join(old_path);
                std::fs::remove_file(&file_path).ok();
            }
        }

        Ok(())
    })
}
