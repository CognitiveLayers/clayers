//! Staging area: `add`, `rm`, and `status` commands.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clayers_repo::SqliteStore;
use clayers_xml::ContentHash;

use super::schema::get_meta;
use super::{block_on, discover_repo, open_cli_db};

/// Stage files for the next commit.
///
/// - `files`: explicit list of files; if empty or contains ".", stage all
///   untracked/modified XML files in CWD.
///
/// # Errors
///
/// Returns an error if any file cannot be read or imported.
pub fn cmd_add(files: &[PathBuf]) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get CWD")?;
    let (repo_root, db_path) = discover_repo(&cwd)?;

    // When scanning a directory (add .), skip files that fail to parse.
    // When files are explicitly named, errors are fatal.
    let scanning = files.is_empty() || (files.len() == 1 && files[0] == Path::new("."));
    let resolved = resolve_files(files, &cwd, &repo_root)?;

    block_on(async move {
        let store = SqliteStore::open(&db_path)?;
        let conn = open_cli_db(&db_path)?;

        for abs_path in &resolved {
            let rel_path = abs_path
                .strip_prefix(&repo_root)
                .with_context(|| format!("{} is outside the repository", abs_path.display()))?
                .to_string_lossy()
                .replace('\\', "/");

            let xml = std::fs::read_to_string(abs_path)
                .with_context(|| format!("failed to read {}", abs_path.display()))?;

            // Import into object store.
            let doc_hash = match clayers_repo::import::import_xml(&store, &xml).await {
                Ok(h) => h,
                Err(e) if scanning => {
                    eprintln!("warning: skipping {}: {e}", abs_path.display());
                    continue;
                }
                Err(e) => {
                    return Err(e)
                        .with_context(|| format!("failed to import {}", abs_path.display()));
                }
            };

            // Determine action: add vs modify.
            let action = get_working_copy_hash(&conn, &rel_path)?.map_or("add", |_| "modify");

            let hash_bytes = doc_hash.0.as_slice();
            conn.execute(
                "INSERT OR REPLACE INTO staging (file_path, action, doc_hash) VALUES (?1, ?2, ?3)",
                rusqlite::params![rel_path, action, hash_bytes],
            )
            .context("failed to update staging")?;

            println!("staged: {rel_path}");
        }

        Ok(())
    })
}

/// Remove files from the staging area or stage a deletion.
///
/// With `--cached`, only removes from staging (unstage). Without, stages
/// a deletion and removes the file from disk.
///
/// # Errors
///
/// Returns an error if staging fails.
pub fn cmd_rm(files: &[PathBuf], cached: bool) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get CWD")?;
    let (repo_root, db_path) = discover_repo(&cwd)?;

    let conn = open_cli_db(&db_path)?;

    for file in files {
        let abs_path = if file.is_absolute() {
            file.clone()
        } else {
            cwd.join(file)
        };
        let rel_path = abs_path
            .strip_prefix(&repo_root)
            .with_context(|| format!("{} is outside the repository", abs_path.display()))?
            .to_string_lossy()
            .replace('\\', "/");

        if cached {
            // Just remove from staging.
            conn.execute(
                "DELETE FROM staging WHERE file_path = ?1",
                rusqlite::params![rel_path],
            )
            .context("failed to remove from staging")?;
            println!("unstaged: {rel_path}");
        } else {
            // Stage deletion.
            conn.execute(
                "INSERT OR REPLACE INTO staging (file_path, action, doc_hash) VALUES (?1, 'delete', NULL)",
                rusqlite::params![rel_path],
            )
            .context("failed to stage deletion")?;

            // Remove from disk.
            if abs_path.exists() {
                std::fs::remove_file(&abs_path)
                    .with_context(|| format!("failed to remove {}", abs_path.display()))?;
            }
            println!("deleted: {rel_path}");
        }
    }

    Ok(())
}

/// Show the working tree status.
///
/// Displays staged changes, unstaged modifications, and untracked XML files.
///
/// # Errors
///
/// Returns an error if the database cannot be read.
pub fn cmd_status() -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get CWD")?;
    let (repo_root, db_path) = discover_repo(&cwd)?;

    let conn = open_cli_db(&db_path)?;
    let branch = get_meta(&conn, "current_branch")?.unwrap_or_else(|| "main".into());

    println!("On branch {branch}");

    // Staged changes.
    let staged = get_staged_entries(&conn)?;
    if !staged.is_empty() {
        println!("\nChanges to be committed:");
        for (path, action) in &staged {
            println!("  {action}: {path}");
        }
    }

    // Working copy vs filesystem: compare hashes to detect real modifications.
    let working_copy = get_all_working_copy(&conn)?;
    let mut unstaged_changes = Vec::new();
    let mut untracked = Vec::new();

    // Scan directory for XML files.
    let xml_files = collect_xml_files(&repo_root)?;
    let staged_paths: std::collections::HashSet<_> =
        staged.iter().map(|(p, _)| p.clone()).collect();
    let mut disk_paths = std::collections::HashSet::new();

    // Collect file paths that need hash comparison, then do them all in one block_on.
    let mut to_check: Vec<(String, Vec<u8>, std::path::PathBuf)> = Vec::new();
    let mut no_hash_modified: Vec<String> = Vec::new();

    for abs_path in &xml_files {
        let rel_path = abs_path
            .strip_prefix(&repo_root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        disk_paths.insert(rel_path.clone());

        if staged_paths.contains(&rel_path) {
            continue; // already staged
        }

        if let Some(stored_hash) = working_copy.get(&rel_path) {
            if let Some(stored) = stored_hash {
                to_check.push((rel_path, stored.clone(), abs_path.clone()));
            } else {
                no_hash_modified.push(rel_path);
            }
        } else {
            untracked.push(rel_path);
        }
    }

    for (rel_path, stored_hash) in &working_copy {
        if staged_paths.contains(rel_path) || disk_paths.contains(rel_path) {
            continue;
        }
        if stored_hash.is_some() {
            unstaged_changes.push(("deleted", rel_path.clone()));
        }
    }

    unstaged_changes.extend(no_hash_modified.into_iter().map(|path| ("modified", path)));

    if !to_check.is_empty() {
        let db_path_clone = db_path.clone();
        let check_results: Vec<(String, bool)> = block_on(async move {
            let store = SqliteStore::open(&db_path_clone).map_err(|e| anyhow::anyhow!("{e}"))?;
            let mut results = Vec::new();
            for (rel_path, stored_bytes, abs_path) in &to_check {
                let Ok(xml) = std::fs::read_to_string(abs_path) else {
                    results.push((rel_path.clone(), true));
                    continue;
                };
                if let Ok(h) = clayers_repo::import::import_xml(&store, &xml).await {
                    let stored_arr: [u8; 32] = stored_bytes[..32.min(stored_bytes.len())]
                        .try_into()
                        .unwrap_or([0u8; 32]);
                    results.push((rel_path.clone(), h.0 != stored_arr));
                } else {
                    results.push((rel_path.clone(), true));
                }
            }
            Ok(results)
        })?;

        for (rel_path, is_modified) in check_results {
            if is_modified {
                unstaged_changes.push(("modified", rel_path));
            }
        }
    }

    unstaged_changes.sort();
    untracked.sort();

    if !unstaged_changes.is_empty() {
        println!("\nChanges not staged for commit:");
        for (action, path) in &unstaged_changes {
            println!("  {action}: {path}");
        }
    }

    if !untracked.is_empty() {
        println!("\nUntracked files:");
        for p in &untracked {
            println!("  {p}");
        }
        println!("\n(use \"clayers add <file>...\" to stage)");
    }

    if staged.is_empty() && unstaged_changes.is_empty() && untracked.is_empty() {
        println!("nothing to commit, working tree clean");
    }

    Ok(())
}

/// Fail if tracked local work would be overwritten by a repository operation.
///
/// This checks staged changes plus unstaged modifications/deletions of tracked
/// files recorded in the CLI `working_copy` table. Untracked obstruction checks
/// live in the export path because they depend on the target tree paths.
pub fn ensure_clean_working_copy(
    conn: &rusqlite::Connection,
    db_path: &Path,
    repo_root: &Path,
    operation: &str,
) -> Result<()> {
    let staged_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM staging", [], |row| row.get(0))
        .context("failed to check staging")?;
    if staged_count > 0 {
        bail!(
            "cannot {operation}: you have {staged_count} staged change(s). \
             Commit or revert them first."
        );
    }

    let dirty = unstaged_tracked_changes(conn, db_path, repo_root)?;
    if !dirty.is_empty() {
        bail!(
            "cannot {operation}: you have unstaged change(s): {}. \
             Commit, stage, or revert them first.",
            dirty.join(", ")
        );
    }

    Ok(())
}

/// Get tracked working-copy file hashes.
pub fn get_tracked_working_copy_hashes(
    conn: &rusqlite::Connection,
) -> Result<std::collections::HashMap<String, ContentHash>> {
    let working_copy = get_all_working_copy(conn)?;
    let mut result = std::collections::HashMap::new();
    for (path, hash) in working_copy {
        if let Some(hash_bytes) = hash {
            result.insert(path, content_hash_from_bytes(&hash_bytes));
        }
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn get_working_copy_hash(conn: &rusqlite::Connection, file_path: &str) -> Result<Option<Vec<u8>>> {
    let result = conn.query_row(
        "SELECT doc_hash FROM working_copy WHERE file_path = ?1",
        rusqlite::params![file_path],
        |row| row.get::<_, Option<Vec<u8>>>(0),
    );
    match result {
        Ok(v) => Ok(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(anyhow::anyhow!(e)),
    }
}

fn get_staged_entries(conn: &rusqlite::Connection) -> Result<Vec<(String, String)>> {
    let mut stmt = conn
        .prepare("SELECT file_path, action FROM staging ORDER BY file_path")
        .context("failed to query staging")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .context("failed to iterate staging")?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.context("failed to read staging row")?);
    }
    Ok(result)
}

fn get_all_working_copy(
    conn: &rusqlite::Connection,
) -> Result<std::collections::HashMap<String, Option<Vec<u8>>>> {
    let mut stmt = conn
        .prepare("SELECT file_path, doc_hash FROM working_copy")
        .context("failed to query working_copy")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<Vec<u8>>>(1)?))
        })
        .context("failed to iterate working_copy")?;
    let mut map = std::collections::HashMap::new();
    for row in rows {
        let (path, hash) = row.context("failed to read working_copy row")?;
        map.insert(path, hash);
    }
    Ok(map)
}

fn content_hash_from_bytes(hash_bytes: &[u8]) -> ContentHash {
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&hash_bytes[..32.min(hash_bytes.len())]);
    ContentHash(arr)
}

fn unstaged_tracked_changes(
    conn: &rusqlite::Connection,
    db_path: &Path,
    repo_root: &Path,
) -> Result<Vec<String>> {
    let working_copy = get_all_working_copy(conn)?;
    let mut dirty = Vec::new();
    let mut to_check = Vec::new();

    for (rel_path, stored_hash) in working_copy {
        let Some(stored_hash) = stored_hash else {
            dirty.push(rel_path);
            continue;
        };
        let abs_path = repo_root.join(&rel_path);
        if !abs_path.exists() || !abs_path.is_file() {
            dirty.push(rel_path);
            continue;
        }
        to_check.push((rel_path, content_hash_from_bytes(&stored_hash), abs_path));
    }

    if to_check.is_empty() {
        return Ok(dirty);
    }

    let db_path = db_path.to_path_buf();
    let changed: Vec<String> = block_on(async move {
        let store = SqliteStore::open(&db_path)?;
        let mut changed = Vec::new();
        for (rel_path, stored_hash, abs_path) in to_check {
            let Ok(xml) = std::fs::read_to_string(&abs_path) else {
                changed.push(rel_path);
                continue;
            };
            match clayers_repo::import::import_xml(&store, &xml).await {
                Ok(current_hash) if current_hash == stored_hash => {}
                _ => changed.push(rel_path),
            }
        }
        Ok(changed)
    })?;

    dirty.extend(changed);
    dirty.sort();
    Ok(dirty)
}

pub fn collect_xml_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_xml_recursive(root, root, &mut files)?;
    Ok(files)
}

#[allow(clippy::only_used_in_recursion)]
fn collect_xml_recursive(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(dir).context("failed to read directory")?;
    for entry in entries {
        let entry = entry.context("failed to read dir entry")?;
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden directories and the db file.
        if name_str.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            collect_xml_recursive(root, &path, files)?;
        } else if path.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn resolve_files(files: &[PathBuf], cwd: &Path, _repo_root: &Path) -> Result<Vec<PathBuf>> {
    if files.is_empty() || (files.len() == 1 && files[0] == Path::new(".")) {
        // Stage all XML files in the working directory.
        return collect_xml_files(cwd);
    }

    let mut resolved = Vec::new();
    for f in files {
        let abs = if f.is_absolute() {
            f.clone()
        } else {
            cwd.join(f)
        };
        if !abs.exists() {
            bail!("path not found: {}", abs.display());
        }
        if abs.is_dir() {
            resolved.extend(collect_xml_files(&abs)?);
        } else {
            resolved.push(abs);
        }
    }
    Ok(resolved)
}

/// Update the `working_copy` table after a successful commit.
///
/// Called from commit.rs after building the new tree.
pub fn update_working_copy_from_tree(
    conn: &rusqlite::Connection,
    entries: &[(String, ContentHash)],
) -> Result<()> {
    // Clear old working copy entries.
    conn.execute("DELETE FROM working_copy", [])
        .context("failed to clear working_copy")?;

    // Insert new entries.
    for (path, hash) in entries {
        conn.execute(
            "INSERT INTO working_copy (file_path, doc_hash) VALUES (?1, ?2)",
            rusqlite::params![path, hash.0.as_slice()],
        )
        .context("failed to update working_copy")?;
    }

    // Clear staging.
    conn.execute("DELETE FROM staging", [])
        .context("failed to clear staging")?;

    Ok(())
}

/// Get all staged entries as `(path, action, Option<doc_hash>)` triples.
pub fn get_staged_full(
    conn: &rusqlite::Connection,
) -> Result<Vec<(String, String, Option<ContentHash>)>> {
    let mut stmt = conn
        .prepare("SELECT file_path, action, doc_hash FROM staging ORDER BY file_path")
        .context("failed to query staging")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<Vec<u8>>>(2)?,
            ))
        })
        .context("failed to iterate staging")?;
    let mut result = Vec::new();
    for row in rows {
        let (path, action, hash_bytes) = row.context("failed to read staging row")?;
        let hash = hash_bytes.map(|b| {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&b[..32.min(b.len())]);
            ContentHash(arr)
        });
        result.push((path, action, hash));
    }
    Ok(result)
}

/// Get all working copy entries as `(path, doc_hash)` pairs.
pub fn get_all_working_copy_hashes(
    conn: &rusqlite::Connection,
) -> Result<Vec<(String, ContentHash)>> {
    let mut stmt = conn
        .prepare("SELECT file_path, doc_hash FROM working_copy ORDER BY file_path")
        .context("failed to query working_copy")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .context("failed to iterate working_copy")?;
    let mut result = Vec::new();
    for row in rows {
        let (path, hash_bytes) = row.context("failed to read working_copy row")?;
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&hash_bytes[..32.min(hash_bytes.len())]);
        result.push((path, ContentHash(arr)));
    }
    Ok(result)
}
