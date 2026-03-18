//! Repository discovery, async bridge, and author resolution.
//!
//! Provides:
//! - `discover_repo()`: walk upward from CWD to find `.clayers.db`
//! - `block_on()`: create a tokio runtime to bridge sync CLI → async repo ops
//! - `resolve_author()`: check flags → env → git config → error

pub mod branch;
pub mod commit;
pub mod diff;
pub mod history;
pub mod init;
pub mod remote;
pub mod revert;
pub mod schema;
pub mod staging;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clayers_repo::object::Author;
use rusqlite::Connection;

/// Discover the repository root by walking upward from `start_dir` looking for
/// `.clayers.db`.
///
/// Returns `(repo_root, db_path)` on success.
///
/// # Errors
///
/// Returns an error if `.clayers.db` is not found in any ancestor directory.
pub fn discover_repo(start_dir: &Path) -> Result<(PathBuf, PathBuf)> {
    let mut dir = start_dir.to_path_buf();
    loop {
        let db = dir.join(".clayers.db");
        if db.exists() {
            return Ok((dir, db));
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => bail!("not a clayers repository (no .clayers.db found)"),
        }
    }
}

/// Open the `SQLite` connection at `db_path` (for CLI tables only; use
/// `SqliteStore::open` for the object/ref store).
///
/// # Errors
///
/// Returns an error if the database cannot be opened.
pub fn open_cli_db(db_path: &Path) -> Result<Connection> {
    Connection::open(db_path)
        .with_context(|| format!("failed to open {}", db_path.display()))
}

/// Bridge sync code to async `clayers-repo` operations using a tokio runtime.
///
/// # Errors
///
/// Returns an error if the runtime cannot be created or the future fails.
pub fn block_on<F, T>(future: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(future)
}

/// Resolve the commit author from flags, environment variables, or git config.
///
/// Resolution order:
/// 1. `--author` / `--email` flags (both required if either is given)
/// 2. `CLAYERS_AUTHOR_NAME` / `CLAYERS_AUTHOR_EMAIL` env vars
/// 3. `git config user.name` / `git config user.email`
///
/// # Errors
///
/// Returns an error if no author information can be found.
pub fn resolve_author(
    flag_name: Option<&str>,
    flag_email: Option<&str>,
) -> Result<Author> {
    // 1. Explicit flags.
    if let (Some(name), Some(email)) = (flag_name, flag_email) {
        return Ok(Author {
            name: name.to_string(),
            email: email.to_string(),
        });
    }

    // 2. Environment variables.
    let env_name = std::env::var("CLAYERS_AUTHOR_NAME").ok();
    let env_email = std::env::var("CLAYERS_AUTHOR_EMAIL").ok();
    if let (Some(name), Some(email)) = (env_name, env_email) {
        return Ok(Author { name, email });
    }

    // 3. Git config.
    let git_name = run_git_config("user.name");
    let git_email = run_git_config("user.email");
    if let (Some(name), Some(email)) = (git_name, git_email) {
        return Ok(Author { name, email });
    }

    bail!(
        "no author found: set --author/--email, CLAYERS_AUTHOR_NAME/EMAIL env vars, \
         or git config user.name/user.email"
    )
}

fn run_git_config(key: &str) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["config", key])
        .output()
        .ok()?;
    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    } else {
        None
    }
}
