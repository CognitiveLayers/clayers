//! `commit` command implementation.

use anyhow::{Context, Result, bail};
use clayers_repo::SqliteStore;
use clayers_xml::ContentHash;

use super::{block_on, discover_repo, open_cli_db, resolve_author};
use super::schema::get_meta;
use super::staging::{get_all_working_copy_hashes, get_staged_full, update_working_copy_from_tree};

/// Create a new commit on the current branch.
///
/// Builds a tree from the working copy entries + staged modifications,
/// creates a commit object, and updates the branch ref.
///
/// # Errors
///
/// Returns an error if staging is empty, author cannot be resolved, or storage fails.
pub fn cmd_commit(
    message: &str,
    flag_author: Option<&str>,
    flag_email: Option<&str>,
) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get CWD")?;
    let (_, db_path) = discover_repo(&cwd)?;

    let conn = open_cli_db(&db_path)?;
    let branch = get_meta(&conn, "current_branch")?.unwrap_or_else(|| "main".into());

    // Check staging area is not empty.
    let staged = get_staged_full(&conn)?;
    if staged.is_empty() {
        bail!("nothing to commit (staging area is empty)");
    }

    // Resolve author.
    let author = resolve_author(flag_author, flag_email)?;

    // Build new tree: start from current working copy, apply staged changes.
    let mut tree_entries: std::collections::HashMap<String, ContentHash> = {
        get_all_working_copy_hashes(&conn)?
            .into_iter()
            .collect()
    };

    // Apply staged changes.
    for (staged_path, action, hash) in &staged {
        match action.as_str() {
            "add" | "modify" => {
                if let Some(h) = hash {
                    tree_entries.insert(staged_path.clone(), *h);
                }
            }
            "delete" => {
                tree_entries.remove(staged_path);
            }
            _ => {}
        }
    }

    let entries_vec: Vec<(String, ContentHash)> = tree_entries.into_iter().collect();
    let entries_clone = entries_vec.clone();

    let db_path_for_commit = db_path.clone();
    block_on(async move {
        let store = SqliteStore::open(&db_path_for_commit)?;
        let repo = clayers_repo::Repo::init(store);

        // Build tree object.
        let tree_entries_for_build: Vec<(String, ContentHash)> = entries_clone;
        let tree_hash = repo
            .build_tree(tree_entries_for_build)
            .await
            .context("failed to build tree")?;

        // Create commit.
        let commit_hash = repo
            .commit(&branch, tree_hash, &author, message)
            .await
            .context("failed to create commit")?;

        let short_hash = commit_hash.0[..4]
            .iter()
            .fold(String::new(), |mut s, b| {
                use std::fmt::Write;
                let _ = write!(s, "{b:02x}");
                s
            });
        println!("[{branch} {short_hash}] {message}");
        Ok(())
    })?;

    // Update working copy table and clear staging (re-open conn for CLI tables).
    let conn = open_cli_db(&db_path)?;
    let entries_for_wc: Vec<(String, ContentHash)> = entries_vec;
    update_working_copy_from_tree(&conn, &entries_for_wc)?;

    Ok(())
}
