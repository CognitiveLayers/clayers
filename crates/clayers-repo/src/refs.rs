//! Ref path helpers and HEAD resolution.
//!
//! Refs are named mutable pointers to commit hashes:
//! - `refs/heads/{name}` — branches
//! - `refs/tags/{name}` — lightweight tags
//! - `HEAD` — current branch (symbolic ref) or detached commit

use clayers_xml::ContentHash;

use crate::error::Result;
use crate::store::RefStore;

/// Prefix for branch refs.
pub const HEADS_PREFIX: &str = "refs/heads/";

/// Prefix for tag refs.
pub const TAGS_PREFIX: &str = "refs/tags/";

/// The HEAD ref name.
pub const HEAD: &str = "HEAD";

/// Construct the full ref path for a branch.
#[must_use]
pub fn branch_ref(name: &str) -> String {
    format!("{HEADS_PREFIX}{name}")
}

/// Construct the full ref path for a tag.
#[must_use]
pub fn tag_ref(name: &str) -> String {
    format!("{TAGS_PREFIX}{name}")
}

/// Resolve HEAD to its target commit hash.
///
/// HEAD is stored as a branch ref name (symbolic ref). This function
/// dereferences it to the actual commit hash.
///
/// # Errors
///
/// Returns an error if refs cannot be read.
pub async fn resolve_head(store: &dyn RefStore) -> Result<Option<ContentHash>> {
    // HEAD stores the branch name as a string in the ref value.
    // For simplicity in the in-memory implementation, HEAD directly
    // stores the commit hash. Symbolic ref support (HEAD -> refs/heads/main)
    // is a future enhancement.
    store.get_ref(HEAD).await
}

/// Set HEAD to point to a commit hash.
///
/// # Errors
///
/// Returns an error if the ref cannot be written.
pub async fn set_head(store: &dyn RefStore, hash: ContentHash) -> Result<()> {
    store.set_ref(HEAD, hash).await
}

/// Get the commit hash that a branch points to.
///
/// # Errors
///
/// Returns an error if the ref cannot be read.
pub async fn get_branch(store: &dyn RefStore, name: &str) -> Result<Option<ContentHash>> {
    store.get_ref(&branch_ref(name)).await
}

/// List all branches with their target commit hashes.
///
/// # Errors
///
/// Returns an error if refs cannot be listed.
pub async fn list_branches(store: &dyn RefStore) -> Result<Vec<(String, ContentHash)>> {
    let refs = store.list_refs(HEADS_PREFIX).await?;
    Ok(refs
        .into_iter()
        .map(|(full, hash)| {
            let name = full
                .strip_prefix(HEADS_PREFIX)
                .unwrap_or(&full)
                .to_string();
            (name, hash)
        })
        .collect())
}

/// List all tags with their target hashes.
///
/// # Errors
///
/// Returns an error if refs cannot be listed.
pub async fn list_tags(store: &dyn RefStore) -> Result<Vec<(String, ContentHash)>> {
    let refs = store.list_refs(TAGS_PREFIX).await?;
    Ok(refs
        .into_iter()
        .map(|(full, hash)| {
            let name = full
                .strip_prefix(TAGS_PREFIX)
                .unwrap_or(&full)
                .to_string();
            (name, hash)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_ref_path() {
        assert_eq!(branch_ref("main"), "refs/heads/main");
        assert_eq!(branch_ref("feature/x"), "refs/heads/feature/x");
    }

    #[test]
    fn tag_ref_path() {
        assert_eq!(tag_ref("v1.0"), "refs/tags/v1.0");
    }
}
