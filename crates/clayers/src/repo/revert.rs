//! `revert` command implementation.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clayers_repo::{ObjectStore, SqliteStore};
use clayers_repo::refs::get_branch;
use clayers_repo::object::Object;

use super::{block_on, discover_repo, open_cli_db};
use super::schema::get_meta;

/// Restore files to their committed state and clear them from staging.
///
/// # Errors
///
/// Returns an error if the files cannot be restored.
pub fn cmd_revert(files: &[PathBuf]) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get CWD")?;
    let (repo_root, db_path) = discover_repo(&cwd)?;

    let conn = open_cli_db(&db_path)?;
    let branch = get_meta(&conn, "current_branch")?.unwrap_or_else(|| "main".into());

    // Resolve relative paths to repo-relative.
    let mut rel_paths = Vec::new();
    for file in files {
        let abs = if file.is_absolute() {
            file.clone()
        } else {
            cwd.join(file)
        };
        let rel = abs
            .strip_prefix(&repo_root)
            .with_context(|| format!("{} is outside the repository", abs.display()))?
            .to_string_lossy()
            .replace('\\', "/");
        rel_paths.push((rel, abs));
    }

    let rel_paths_clone: Vec<String> = rel_paths.iter().map(|(r, _)| r.clone()).collect();
    let db_path_clone = db_path.clone();

    let exported: Vec<(String, String)> = block_on(async move {
        let store = SqliteStore::open(&db_path_clone)?;

        let Some(tip) = get_branch(&store, &branch).await? else { return Ok(vec![]) };

        let obj = store.get(&tip).await?.ok_or_else(|| anyhow::anyhow!("commit not found"))?;
        let tree_hash = match obj {
            Object::Commit(c) => c.tree,
            _ => return Err(anyhow::anyhow!("expected commit object")),
        };

        let Some(Object::Tree(tree_obj)) = store.get(&tree_hash).await? else { return Err(anyhow::anyhow!("expected tree object")) };

        let mut result = Vec::new();
        for rel_path in &rel_paths_clone {
            if let Some(entry) = tree_obj.get(rel_path) {
                let xml = clayers_repo::export::export_xml(&store, entry.document).await?;
                result.push((rel_path.clone(), xml));
            }
        }
        Ok(result)
    })?;

    // Write files to disk and clear from staging.
    for (rel_path, xml) in &exported {
        let abs_path = repo_root.join(rel_path);
        if let Some(parent) = abs_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&abs_path, xml.as_bytes())
            .with_context(|| format!("failed to write {}", abs_path.display()))?;

        // Remove from staging.
        conn.execute(
            "DELETE FROM staging WHERE file_path = ?1",
            rusqlite::params![rel_path],
        )
        .context("failed to remove from staging")?;

        println!("reverted: {rel_path}");
    }

    // Report files not in tree.
    for (rel, _) in &rel_paths {
        if !exported.iter().any(|(r, _)| r == rel) {
            println!("warning: '{rel}' not tracked in current commit (skipped)");
        }
    }

    Ok(())
}
