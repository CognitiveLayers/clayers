//! Commit graph traversal and LCA finding.
//!
//! Finding the lowest common ancestor of two commits is essential for
//! three-way diff and reconciliation.

use std::collections::{HashSet, VecDeque};

use clayers_xml::ContentHash;

use crate::error::{Error, Result};
use crate::object::Object;
use crate::store::ObjectStore;

/// Find the lowest common ancestor of two commits.
///
/// Walks the commit graph from both tips simultaneously using BFS.
/// Returns `None` if the commits have no common ancestor (disjoint histories).
///
/// # Errors
///
/// Returns an error if commit objects cannot be loaded.
pub async fn common_ancestor(
    store: &dyn ObjectStore,
    a: ContentHash,
    b: ContentHash,
) -> Result<Option<ContentHash>> {
    if a == b {
        return Ok(Some(a));
    }

    // BFS from both sides simultaneously.
    let mut ancestors_a: HashSet<ContentHash> = HashSet::new();
    let mut ancestors_b: HashSet<ContentHash> = HashSet::new();
    let mut queue_a: VecDeque<ContentHash> = VecDeque::new();
    let mut queue_b: VecDeque<ContentHash> = VecDeque::new();

    ancestors_a.insert(a);
    ancestors_b.insert(b);
    queue_a.push_back(a);
    queue_b.push_back(b);

    loop {
        let a_done = queue_a.is_empty();
        let b_done = queue_b.is_empty();

        if a_done && b_done {
            return Ok(None);
        }

        // Expand one level from side A.
        if let Some(current) = queue_a.pop_front() {
            if ancestors_b.contains(&current) {
                return Ok(Some(current));
            }
            let parents = get_commit_parents(store, current).await?;
            for p in parents {
                if ancestors_a.insert(p) {
                    if ancestors_b.contains(&p) {
                        return Ok(Some(p));
                    }
                    queue_a.push_back(p);
                }
            }
        }

        // Expand one level from side B.
        if let Some(current) = queue_b.pop_front() {
            if ancestors_a.contains(&current) {
                return Ok(Some(current));
            }
            let parents = get_commit_parents(store, current).await?;
            for p in parents {
                if ancestors_b.insert(p) {
                    if ancestors_a.contains(&p) {
                        return Ok(Some(p));
                    }
                    queue_b.push_back(p);
                }
            }
        }
    }
}

/// Walk commit history from a starting point, collecting up to `limit` commits.
///
/// # Errors
///
/// Returns an error if commit objects cannot be loaded.
pub async fn walk_history(
    store: &dyn ObjectStore,
    from: ContentHash,
    limit: Option<usize>,
) -> Result<Vec<(ContentHash, crate::object::CommitObject)>> {
    let mut result = Vec::new();
    let mut queue: VecDeque<ContentHash> = VecDeque::new();
    let mut visited: HashSet<ContentHash> = HashSet::new();

    queue.push_back(from);
    visited.insert(from);

    while let Some(hash) = queue.pop_front() {
        if let Some(max) = limit
            && result.len() >= max
        {
            break;
        }

        let obj = store.get(&hash).await?.ok_or(Error::NotFound(hash))?;
        if let Object::Commit(commit) = obj {
            for parent in &commit.parents {
                if visited.insert(*parent) {
                    queue.push_back(*parent);
                }
            }
            result.push((hash, commit));
        }
    }

    Ok(result)
}

/// Get the parent hashes of a commit.
async fn get_commit_parents(
    store: &dyn ObjectStore,
    hash: ContentHash,
) -> Result<Vec<ContentHash>> {
    let obj = store.get(&hash).await?.ok_or(Error::NotFound(hash))?;
    match obj {
        Object::Commit(commit) => Ok(commit.parents),
        _ => Ok(Vec::new()), // Non-commit objects have no parents.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{Author, CommitObject};
    use crate::store::memory::MemoryStore;
    use chrono::Utc;

    async fn store_commit(
        store: &MemoryStore,
        id: &[u8],
        parents: Vec<ContentHash>,
    ) -> ContentHash {
        let hash = ContentHash::from_canonical(id);
        let commit = CommitObject {
            tree: ContentHash::from_canonical(b"doc"),
            parents,
            author: Author {
                name: "Test".into(),
                email: "test@test.com".into(),
            },
            timestamp: Utc::now(),
            message: "test".into(),
        };
        let mut tx = store.transaction().await.unwrap();
        tx.put(hash, Object::Commit(commit)).await.unwrap();
        tx.commit().await.unwrap();
        hash
    }

    #[tokio::test]
    async fn lca_same_commit() {
        let store = MemoryStore::new();
        let c = store_commit(&store, b"root", vec![]).await;
        let lca = common_ancestor(&store, c, c).await.unwrap();
        assert_eq!(lca, Some(c));
    }

    #[tokio::test]
    async fn lca_linear_history() {
        let store = MemoryStore::new();
        let c1 = store_commit(&store, b"c1", vec![]).await;
        let c2 = store_commit(&store, b"c2", vec![c1]).await;
        let c3 = store_commit(&store, b"c3", vec![c2]).await;

        let lca = common_ancestor(&store, c2, c3).await.unwrap();
        assert_eq!(lca, Some(c2));
    }

    #[tokio::test]
    async fn lca_diamond() {
        let store = MemoryStore::new();
        //    c1
        //   / \
        //  c2  c3
        let c1 = store_commit(&store, b"c1", vec![]).await;
        let c2 = store_commit(&store, b"c2", vec![c1]).await;
        let c3 = store_commit(&store, b"c3", vec![c1]).await;

        let lca = common_ancestor(&store, c2, c3).await.unwrap();
        assert_eq!(lca, Some(c1));
    }

    #[tokio::test]
    async fn lca_disjoint_histories() {
        let store = MemoryStore::new();
        let c1 = store_commit(&store, b"c1", vec![]).await;
        let c2 = store_commit(&store, b"c2", vec![]).await;

        let lca = common_ancestor(&store, c1, c2).await.unwrap();
        assert_eq!(lca, None);
    }

    #[tokio::test]
    async fn walk_history_with_limit() {
        let store = MemoryStore::new();
        let c1 = store_commit(&store, b"c1", vec![]).await;
        let c2 = store_commit(&store, b"c2", vec![c1]).await;
        let c3 = store_commit(&store, b"c3", vec![c2]).await;

        let history = walk_history(&store, c3, Some(2)).await.unwrap();
        assert_eq!(history.len(), 2);
    }
}
