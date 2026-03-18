//! CLI `diff` command: compare working copy, branches, or commits.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clayers_repo::FileChange;
use serde::Serialize;

use super::{block_on, discover_repo, open_cli_db, schema::get_meta};

/// A file-level diff entry with optional element-level detail.
#[derive(Serialize)]
struct FileDiff {
    /// `"added"`, `"deleted"`, or `"modified"`.
    status: String,
    /// File path in the repository.
    path: String,
    /// Element-level changes (empty for added/deleted files).
    #[serde(skip_serializing_if = "Option::is_none")]
    changes: Option<clayers_xml::XmlDiff>,
}

/// Top-level JSON output for `clayers diff --json`.
#[derive(Serialize)]
struct DiffOutput {
    files: Vec<FileDiff>,
}

/// Execute the `diff` command.
///
/// Modes:
/// - No args: working copy vs HEAD
/// - One revspec: HEAD vs revspec
/// - Two revspecs: `rev_a` vs `rev_b`
///
/// # Errors
///
/// Returns an error if the repository cannot be opened or the revspecs cannot
/// be resolved.
pub fn cmd_diff(rev_a: Option<&str>, rev_b: Option<&str>, json: bool) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get CWD")?;
    let (repo_root, db_path) = discover_repo(&cwd)?;

    let conn = open_cli_db(&db_path)?;
    let branch = get_meta(&conn, "current_branch")?.unwrap_or_else(|| "main".into());

    let file_diffs = match (rev_a, rev_b) {
        (None, None) => diff_working_copy(&repo_root, &db_path, &branch)?,
        (Some(rev), None) => {
            diff_revspecs(&db_path, &format!("refs/heads/{branch}"), rev)?
        }
        (Some(a), Some(b)) => diff_revspecs(&db_path, a, b)?,
        (None, Some(_)) => bail!("unexpected: rev_b without rev_a"),
    };

    if json {
        let output = DiffOutput { files: file_diffs };
        println!(
            "{}",
            serde_json::to_string_pretty(&output).context("JSON serialization failed")?
        );
    } else {
        for fd in &file_diffs {
            match fd.status.as_str() {
                "added" => println!("added: {}", fd.path),
                "deleted" => println!("deleted: {}", fd.path),
                "modified" => {
                    println!("modified: {}", fd.path);
                    if let Some(ref diff) = fd.changes {
                        print!("{diff}");
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}

/// Diff the on-disk working copy against HEAD.
fn diff_working_copy(
    repo_root: &Path,
    db_path: &Path,
    _branch: &str,
) -> Result<Vec<FileDiff>> {
    let conn = open_cli_db(db_path)?;

    let working_copy = get_all_working_copy(&conn)?;

    if working_copy.is_empty() {
        // No commits yet – nothing to diff.
        return Ok(Vec::new());
    }

    // Collect on-disk XML files.
    let xml_files = super::staging::collect_xml_files(repo_root)?;
    let mut on_disk: HashMap<String, String> = HashMap::new();
    for abs_path in &xml_files {
        let rel_path = abs_path
            .strip_prefix(repo_root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let content = std::fs::read_to_string(abs_path)
            .with_context(|| format!("failed to read {}", abs_path.display()))?;
        on_disk.insert(rel_path, content);
    }

    // Find modified files: returns (path, committed_xml, disk_xml) triples.
    let db_path_clone = db_path.to_path_buf();
    let modified_files: Vec<(String, String, String)> = block_on(async move {
        let store = clayers_repo::SqliteStore::open(&db_path_clone)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut modified = Vec::new();

        for (rel_path, stored_hash_bytes) in &working_copy {
            if let Some(disk_xml) = on_disk.get(rel_path) {
                let current_hash =
                    clayers_repo::import::import_xml(&store, disk_xml).await.ok();
                let stored_arr: [u8; 32] = stored_hash_bytes[..32.min(stored_hash_bytes.len())]
                    .try_into()
                    .unwrap_or([0u8; 32]);
                let stored_hash = clayers_xml::ContentHash(stored_arr);

                if let Some(ch) = current_hash
                    && ch != stored_hash
                    && let Ok(committed_xml) =
                        clayers_repo::export::export_xml(&store, stored_hash).await
                {
                    modified.push((
                        rel_path.clone(),
                        committed_xml,
                        disk_xml.clone(),
                    ));
                }
            }
        }

        Ok(modified)
    })?;

    let mut result = Vec::new();
    for (path, committed_xml, disk_xml) in &modified_files {
        let xml_diff = clayers_xml::diff_xml(committed_xml, disk_xml).ok();
        let has_changes = xml_diff.as_ref().is_some_and(|d| !d.is_empty());
        if has_changes {
            result.push(FileDiff {
                status: "modified".into(),
                path: path.clone(),
                changes: xml_diff,
            });
        }
    }

    Ok(result)
}

/// Diff two revspecs (branches, tags, or commit hashes).
fn diff_revspecs(db_path: &Path, rev_a: &str, rev_b: &str) -> Result<Vec<FileDiff>> {
    let db_path = db_path.to_path_buf();
    let rev_a = rev_a.to_string();
    let rev_b = rev_b.to_string();

    block_on(async move {
        let store = clayers_repo::SqliteStore::open(&db_path)
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let (_, tree_a) = clayers_repo::query::resolve_to_tree(&store, &store, &rev_a)
            .await
            .with_context(|| format!("cannot resolve '{rev_a}'"))?;
        let (_, tree_b) = clayers_repo::query::resolve_to_tree(&store, &store, &rev_b)
            .await
            .with_context(|| format!("cannot resolve '{rev_b}'"))?;

        let file_changes = clayers_repo::diff::diff_trees(&tree_a, &tree_b);
        let mut result = Vec::new();

        for fc in &file_changes {
            match fc {
                FileChange::Added { path, .. } => {
                    result.push(FileDiff {
                        status: "added".into(),
                        path: path.clone(),
                        changes: None,
                    });
                }
                FileChange::Removed { path, .. } => {
                    result.push(FileDiff {
                        status: "deleted".into(),
                        path: path.clone(),
                        changes: None,
                    });
                }
                FileChange::Modified {
                    path,
                    old_doc,
                    new_doc,
                } => {
                    let xml_diff =
                        if let (Ok(xa), Ok(xb)) = (
                            clayers_repo::export::export_xml(&store, *old_doc).await,
                            clayers_repo::export::export_xml(&store, *new_doc).await,
                        ) {
                            clayers_xml::diff_xml(&xa, &xb).ok()
                        } else {
                            None
                        };
                    result.push(FileDiff {
                        status: "modified".into(),
                        path: path.clone(),
                        changes: xml_diff,
                    });
                }
            }
        }

        Ok(result)
    })
}

/// Get all working copy entries as `(path, hash_bytes)`.
fn get_all_working_copy(conn: &rusqlite::Connection) -> Result<Vec<(String, Vec<u8>)>> {
    let mut stmt = conn
        .prepare("SELECT file_path, doc_hash FROM working_copy WHERE doc_hash IS NOT NULL")
        .context("failed to query working_copy")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .context("failed to iterate working_copy")?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.context("failed to read working_copy row")?);
    }
    Ok(result)
}
