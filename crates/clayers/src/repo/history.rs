//! `log` command implementation.

use anyhow::{Context, Result};
use clayers_repo::SqliteStore;
use clayers_repo::refs::get_branch;

use super::{block_on, discover_repo};

/// Display the commit history for the current branch.
///
/// # Errors
///
/// Returns an error if the database cannot be read or history walking fails.
pub fn cmd_log(limit: Option<usize>) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get CWD")?;
    let (_, db_path) = discover_repo(&cwd)?;

    let conn = super::open_cli_db(&db_path)?;
    let branch = super::schema::get_meta(&conn, "current_branch")?.unwrap_or_else(|| "main".into());

    block_on(async move {
        let store = SqliteStore::open(&db_path)?;

        let Some(tip) = get_branch(&store, &branch).await? else {
            println!("(no commits yet on branch '{branch}')");
            return Ok(());
        };

        let repo = clayers_repo::Repo::init(store);
        let commits = repo.log(tip, limit).await.context("failed to walk history")?;

        for (hash, commit) in &commits {
            println!("commit {hash}");
            println!("Author: {} <{}>", commit.author.name, commit.author.email);
            println!("Date:   {}", commit.timestamp.format("%a %b %e %H:%M:%S %Y %z"));
            println!();
            // Indent message.
            for line in commit.message.lines() {
                println!("    {line}");
            }
            println!();
        }

        Ok(())
    })
}
