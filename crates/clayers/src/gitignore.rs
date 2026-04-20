//! `.gitignore` integration for `clayers adopt`.
//!
//! Plants a sentinel-bounded block of clayers-managed ignore entries
//! so adopt can be re-run idempotently without duplicating lines or
//! disturbing user-authored content outside the markers.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

const GI_OPEN: &str = "# clayers:adopt:start";
const GI_CLOSE: &str = "# clayers:adopt:end";

/// Entries managed by adopt. Extend this list to add more ignored paths.
const MANAGED_ENTRIES: &[&str] = &[".clayers/search/"];

/// Freshness of the managed `.gitignore` block.
#[derive(Debug, PartialEq, Eq)]
pub enum GitignoreStatus {
    /// File missing entirely.
    Missing,
    /// File exists but no markers.
    NoMarkers,
    /// Markers exist but contents differ from the managed entries.
    Outdated,
    /// Markers exist and contents match exactly.
    Current,
}

/// Plant or refresh the managed block in `<target>/.gitignore`.
///
/// Idempotent: running twice yields no duplicates. User-authored
/// lines outside the sentinel markers are preserved byte-for-byte.
///
/// # Errors
/// Returns an error on any filesystem I/O failure.
pub fn plant(target: &Path) -> Result<()> {
    let gi = target.join(".gitignore");
    let block = managed_block();
    let new_content = match fs::read_to_string(&gi) {
        Ok(existing) => splice_block(&existing, &block),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => format!("{block}\n"),
        Err(e) => return Err(e).with_context(|| format!("read {}", gi.display())),
    };
    fs::write(&gi, new_content)
        .with_context(|| format!("write {}", gi.display()))?;
    Ok(())
}

/// Inspect the current `.gitignore` state for freshness reporting.
///
/// Consumed by `adopt::check_freshness` so `clayers adopt --update`
/// can detect and repair a tampered or missing block.
#[must_use]
pub fn check(target: &Path) -> GitignoreStatus {
    let gi = target.join(".gitignore");
    let Ok(existing) = fs::read_to_string(&gi) else {
        return GitignoreStatus::Missing;
    };
    if !existing.contains(GI_OPEN) || !existing.contains(GI_CLOSE) {
        return GitignoreStatus::NoMarkers;
    }
    let expected = managed_block();
    if existing.contains(&expected) {
        GitignoreStatus::Current
    } else {
        GitignoreStatus::Outdated
    }
}

fn managed_block() -> String {
    let mut s = String::new();
    s.push_str(GI_OPEN);
    s.push('\n');
    for e in MANAGED_ENTRIES {
        s.push_str(e);
        s.push('\n');
    }
    s.push_str(GI_CLOSE);
    s
}

/// Splice the managed block into `existing` content.
///
/// - If markers present: replace the content between them (inclusive) with
///   `block`.
/// - Else: append a newline (if needed) + `block` + trailing newline.
fn splice_block(existing: &str, block: &str) -> String {
    if let Some(start) = existing.find(GI_OPEN)
        && let Some(end_rel) = existing[start..].find(GI_CLOSE)
    {
        let end = start + end_rel + GI_CLOSE.len();
        let mut out = String::with_capacity(existing.len() + block.len());
        out.push_str(&existing[..start]);
        out.push_str(block);
        out.push_str(&existing[end..]);
        return out;
    }
    // Append block.
    let mut out = existing.to_owned();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(block);
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn plant_creates_file_when_missing() {
        let dir = TempDir::new().unwrap();
        plant(dir.path()).unwrap();
        let content = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(content.contains(GI_OPEN));
        assert!(content.contains(GI_CLOSE));
        assert!(content.contains(".clayers/search/"));
    }

    #[test]
    fn plant_preserves_user_lines() {
        let dir = TempDir::new().unwrap();
        let gi = dir.path().join(".gitignore");
        fs::write(&gi, "node_modules/\n*.log\n# my comment\n").unwrap();
        plant(dir.path()).unwrap();
        let content = fs::read_to_string(&gi).unwrap();
        assert!(content.contains("node_modules/"));
        assert!(content.contains("*.log"));
        assert!(content.contains("# my comment"));
        assert!(content.contains(".clayers/search/"));
    }

    #[test]
    fn plant_is_idempotent() {
        let dir = TempDir::new().unwrap();
        plant(dir.path()).unwrap();
        plant(dir.path()).unwrap();
        let content = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert_eq!(content.matches(GI_OPEN).count(), 1);
        assert_eq!(content.matches(".clayers/search/").count(), 1);
    }

    #[test]
    fn plant_replaces_between_markers() {
        let dir = TempDir::new().unwrap();
        let gi = dir.path().join(".gitignore");
        // Tampered block with a stale entry.
        fs::write(
            &gi,
            format!("user_line\n{GI_OPEN}\nstale/\n{GI_CLOSE}\ntail_line\n"),
        )
        .unwrap();
        plant(dir.path()).unwrap();
        let content = fs::read_to_string(&gi).unwrap();
        assert!(content.contains("user_line"));
        assert!(content.contains("tail_line"));
        assert!(!content.contains("stale/"));
        assert!(content.contains(".clayers/search/"));
    }

    #[test]
    fn check_reports_missing() {
        let dir = TempDir::new().unwrap();
        assert_eq!(check(dir.path()), GitignoreStatus::Missing);
    }

    #[test]
    fn check_reports_current_after_plant() {
        let dir = TempDir::new().unwrap();
        plant(dir.path()).unwrap();
        assert_eq!(check(dir.path()), GitignoreStatus::Current);
    }
}
