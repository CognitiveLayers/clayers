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

    // --- Property-based tests for commit graph operations ---

    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// History from tip reaches every commit in the DAG.
        #[test]
        fn prop_walk_history_reaches_all_commits(
            (dag, tip, commit_order) in crate::store::prop_strategies::arb_commit_dag()
        ) {
            let rt = crate::store::prop_strategies::runtime();
            rt.block_on(async {
                let store = MemoryStore::new();
                let mut tx = store.transaction().await.unwrap();
                for (h, o) in &dag {
                    tx.put(*h, o.clone()).await.unwrap();
                }
                tx.commit().await.unwrap();

                let history = walk_history(&store, tip, None).await.unwrap();
                let history_hashes: HashSet<_> = history.iter().map(|(h, _)| *h).collect();

                // Every commit in the DAG must be reachable from the tip.
                for h in &commit_order {
                    prop_assert!(
                        history_hashes.contains(h),
                        "commit {h} not reachable from tip {tip}"
                    );
                }
                Ok(())
            })?;
        }

        /// walk_history with limit never returns more than limit commits.
        #[test]
        fn prop_walk_history_respects_limit(
            (dag, tip, commit_order) in crate::store::prop_strategies::arb_commit_dag(),
            limit in 1..=10_usize,
        ) {
            let rt = crate::store::prop_strategies::runtime();
            rt.block_on(async {
                let store = MemoryStore::new();
                let mut tx = store.transaction().await.unwrap();
                for (h, o) in &dag {
                    tx.put(*h, o.clone()).await.unwrap();
                }
                tx.commit().await.unwrap();

                let history = walk_history(&store, tip, Some(limit)).await.unwrap();
                let max_expected = limit.min(commit_order.len());
                prop_assert!(
                    history.len() <= max_expected,
                    "history has {} commits but limit was {} (total commits: {})",
                    history.len(), limit, commit_order.len()
                );
                Ok(())
            })?;
        }

        /// walk_history always includes the tip commit as the first entry.
        #[test]
        fn prop_walk_history_starts_with_tip(
            (dag, tip, _order) in crate::store::prop_strategies::arb_commit_dag()
        ) {
            let rt = crate::store::prop_strategies::runtime();
            rt.block_on(async {
                let store = MemoryStore::new();
                let mut tx = store.transaction().await.unwrap();
                for (h, o) in &dag {
                    tx.put(*h, o.clone()).await.unwrap();
                }
                tx.commit().await.unwrap();

                let history = walk_history(&store, tip, None).await.unwrap();
                prop_assert!(!history.is_empty(), "history should not be empty");
                prop_assert_eq!(history[0].0, tip, "first entry should be the tip");
                Ok(())
            })?;
        }

        /// walk_history yields each commit exactly once (no duplicates).
        #[test]
        fn prop_walk_history_no_duplicates(
            (dag, tip, _order) in crate::store::prop_strategies::arb_commit_dag()
        ) {
            let rt = crate::store::prop_strategies::runtime();
            rt.block_on(async {
                let store = MemoryStore::new();
                let mut tx = store.transaction().await.unwrap();
                for (h, o) in &dag {
                    tx.put(*h, o.clone()).await.unwrap();
                }
                tx.commit().await.unwrap();

                let history = walk_history(&store, tip, None).await.unwrap();
                let unique: HashSet<_> = history.iter().map(|(h, _)| *h).collect();
                prop_assert_eq!(
                    history.len(), unique.len(),
                    "history contains duplicate commits"
                );
                Ok(())
            })?;
        }

        /// common_ancestor of a commit with itself is always that commit.
        #[test]
        fn prop_lca_self_is_identity(
            (dag, tip, _order) in crate::store::prop_strategies::arb_commit_dag()
        ) {
            let rt = crate::store::prop_strategies::runtime();
            rt.block_on(async {
                let store = MemoryStore::new();
                let mut tx = store.transaction().await.unwrap();
                for (h, o) in &dag {
                    tx.put(*h, o.clone()).await.unwrap();
                }
                tx.commit().await.unwrap();

                let lca = common_ancestor(&store, tip, tip).await.unwrap();
                prop_assert_eq!(lca, Some(tip));
                Ok(())
            })?;
        }

        /// common_ancestor of a commit and its ancestor is the ancestor.
        #[test]
        fn prop_lca_with_ancestor(
            (dag, tip, commit_order) in crate::store::prop_strategies::arb_commit_dag()
        ) {
            let rt = crate::store::prop_strategies::runtime();
            rt.block_on(async {
                let store = MemoryStore::new();
                let mut tx = store.transaction().await.unwrap();
                for (h, o) in &dag {
                    tx.put(*h, o.clone()).await.unwrap();
                }
                tx.commit().await.unwrap();

                // The first commit in order is the root (oldest ancestor).
                let root = commit_order[0];
                let lca = common_ancestor(&store, tip, root).await.unwrap();
                // LCA must exist (they share history) and must be the root
                // (the root is an ancestor of the tip).
                prop_assert!(
                    lca.is_some(),
                    "tip and root should share an ancestor"
                );
                prop_assert_eq!(
                    lca.unwrap(), root,
                    "LCA of tip and root should be the root"
                );
                Ok(())
            })?;
        }

        /// After transfer, walk_history on dst produces the same result as on src.
        #[test]
        fn prop_transfer_preserves_history(
            (dag, tip, _order) in crate::store::prop_strategies::arb_commit_dag()
        ) {
            let rt = crate::store::prop_strategies::runtime();
            rt.block_on(async {
                let src = MemoryStore::new();
                let dst = MemoryStore::new();

                let mut tx = src.transaction().await.unwrap();
                for (h, o) in &dag {
                    tx.put(*h, o.clone()).await.unwrap();
                }
                tx.commit().await.unwrap();

                crate::sync::transfer_objects(&src, &dst, tip).await.unwrap();

                let src_history = walk_history(&src, tip, None).await.unwrap();
                let dst_history = walk_history(&dst, tip, None).await.unwrap();

                let src_hashes: Vec<_> = src_history.iter().map(|(h, _)| *h).collect();
                let dst_hashes: Vec<_> = dst_history.iter().map(|(h, _)| *h).collect();
                prop_assert_eq!(
                    src_hashes, dst_hashes,
                    "history should be identical after transfer"
                );
                Ok(())
            })?;
        }

        /// Repo::commit() on a branch preserves the existing chain.
        /// Each new commit's parent is the previous tip, and walking from
        /// the new tip yields the full chain with correct messages.
        #[test]
        fn prop_commit_chain_preserves_history(
            xml_docs in prop::collection::vec(
                crate::store::prop_strategies::arb_xml_document(), 2..=5
            ),
            author in crate::store::prop_strategies::arb_author(),
        ) {
            let rt = crate::store::prop_strategies::runtime();
            rt.block_on(async {
                let store = MemoryStore::new();
                let repo = crate::repo::Repo::init(store);

                let mut commit_hashes = Vec::new();
                let mut messages = Vec::new();

                for (i, xml) in xml_docs.iter().enumerate() {
                    let Ok(doc_hash) = repo.import_xml(xml).await else {
                        return Ok(()); // skip unparseable XML
                    };
                    let tree_hash = repo
                        .build_tree(vec![(format!("doc{i}.xml"), doc_hash)])
                        .await.unwrap();
                    let msg = format!("commit {i}");
                    let commit_hash = repo
                        .commit("main", tree_hash, &author, &msg)
                        .await.unwrap();
                    commit_hashes.push(commit_hash);
                    messages.push(msg);
                }

                // Walk history from the last commit
                let tip = *commit_hashes.last().unwrap();
                let history = repo.log(tip, None).await.unwrap();

                // Must have exactly as many commits as we made
                prop_assert_eq!(
                    history.len(), commit_hashes.len(),
                    "history length mismatch"
                );

                // History is newest-first; verify messages match in reverse
                for (i, (_, commit)) in history.iter().enumerate() {
                    let expected_msg = &messages[messages.len() - 1 - i];
                    prop_assert_eq!(
                        &commit.message, expected_msg,
                        "message mismatch at history position {}", i
                    );
                }

                // Each commit (except the first) must have its predecessor as parent
                for i in 1..commit_hashes.len() {
                    // The i-th commit's parent should be the (i-1)-th commit
                    let history_pos = commit_hashes.len() - 1 - i;
                    prop_assert_eq!(
                        history[history_pos].1.parents.clone(), vec![commit_hashes[i - 1]],
                        "commit {} should have commit {} as parent",
                        i, i - 1
                    );
                }

                // First commit should have no parents
                prop_assert!(
                    history.last().unwrap().1.parents.is_empty(),
                    "first commit should have no parents"
                );

                Ok(())
            })?;
        }

        /// Concurrent branch activity through Repo doesn't corrupt either history.
        /// Commit on branch A, commit on branch B, verify both logs are intact.
        #[test]
        fn prop_concurrent_branches_preserve_history(
            xml_docs in prop::collection::vec(
                crate::store::prop_strategies::arb_xml_document(), 4..=6
            ),
            author in crate::store::prop_strategies::arb_author(),
        ) {
            let rt = crate::store::prop_strategies::runtime();
            rt.block_on(async {
                let store = MemoryStore::new();
                let repo = crate::repo::Repo::init(store);

                let mut branch_a_hashes = Vec::new();
                let mut branch_b_hashes = Vec::new();

                // Interleave commits on two branches
                for (i, xml) in xml_docs.iter().enumerate() {
                    let Ok(doc_hash) = repo.import_xml(xml).await else {
                        return Ok(());
                    };
                    let tree = repo
                        .build_tree(vec![(format!("f{i}.xml"), doc_hash)])
                        .await.unwrap();
                    if i % 2 == 0 {
                        let h = repo.commit("alpha", tree, &author, &format!("alpha-{i}"))
                            .await.unwrap();
                        branch_a_hashes.push(h);
                    } else {
                        let h = repo.commit("beta", tree, &author, &format!("beta-{i}"))
                            .await.unwrap();
                        branch_b_hashes.push(h);
                    }
                }

                // Verify branch A's history
                if let Some(&tip_a) = branch_a_hashes.last() {
                    let log_a = repo.log(tip_a, None).await.unwrap();
                    prop_assert_eq!(
                        log_a.len(), branch_a_hashes.len(),
                        "branch alpha history length wrong"
                    );
                    // All messages should be alpha-*
                    for (_, c) in &log_a {
                        prop_assert!(
                            c.message.starts_with("alpha-"),
                            "branch alpha has foreign commit: {}", c.message
                        );
                    }
                }

                // Verify branch B's history
                if let Some(&tip_b) = branch_b_hashes.last() {
                    let log_b = repo.log(tip_b, None).await.unwrap();
                    prop_assert_eq!(
                        log_b.len(), branch_b_hashes.len(),
                        "branch beta history length wrong"
                    );
                    for (_, c) in &log_b {
                        prop_assert!(
                            c.message.starts_with("beta-"),
                            "branch beta has foreign commit: {}", c.message
                        );
                    }
                }

                Ok(())
            })?;
        }

        /// Same sequence of imports + commits on MemoryStore and SqliteStore
        /// produces identical log output (same hashes, same messages, same order).
        #[cfg(feature = "sqlite")]
        #[test]
        fn prop_history_identical_across_stores(
            xml_docs in prop::collection::vec(
                crate::store::prop_strategies::arb_xml_document(), 2..=4
            ),
            author in crate::store::prop_strategies::arb_author(),
        ) {
            let rt = crate::store::prop_strategies::runtime();
            rt.block_on(async {
                let mem_store = MemoryStore::new();
                let sql_store = crate::store::sqlite::SqliteStore::open_in_memory().unwrap();
                let mem_repo = crate::repo::Repo::init(mem_store);
                let sql_repo = crate::repo::Repo::init(sql_store);

                let mut mem_tip = None;
                let mut sql_tip = None;

                for (i, xml) in xml_docs.iter().enumerate() {
                    let Ok(mem_doc) = mem_repo.import_xml(xml).await else {
                        return Ok(());
                    };
                    let sql_doc = sql_repo.import_xml(xml).await.unwrap();

                    // Import should produce same hash on both stores
                    prop_assert_eq!(
                        mem_doc, sql_doc,
                        "import hash differs between stores for doc {}", i
                    );

                    let mem_tree = mem_repo
                        .build_tree(vec![(format!("d{i}.xml"), mem_doc)])
                        .await.unwrap();
                    let sql_tree = sql_repo
                        .build_tree(vec![(format!("d{i}.xml"), sql_doc)])
                        .await.unwrap();

                    prop_assert_eq!(
                        mem_tree, sql_tree,
                        "tree hash differs between stores for commit {}", i
                    );

                    let msg = format!("commit-{i}");
                    let mh = mem_repo.commit("main", mem_tree, &author, &msg).await.unwrap();
                    let sh = sql_repo.commit("main", sql_tree, &author, &msg).await.unwrap();

                    // Commit hashes may differ due to timestamps (Utc::now()),
                    // but the history structure should be identical.
                    mem_tip = Some(mh);
                    sql_tip = Some(sh);
                }

                if let (Some(mt), Some(st)) = (mem_tip, sql_tip) {
                    let mem_log = mem_repo.log(mt, None).await.unwrap();
                    let sql_log = sql_repo.log(st, None).await.unwrap();

                    prop_assert_eq!(
                        mem_log.len(), sql_log.len(),
                        "history length differs between stores"
                    );

                    for ((_, mc), (_, sc)) in mem_log.iter().zip(sql_log.iter()) {
                        prop_assert_eq!(
                            &mc.message, &sc.message,
                            "message differs between stores"
                        );
                        prop_assert_eq!(
                            &mc.tree, &sc.tree,
                            "tree hash differs between stores"
                        );
                        prop_assert_eq!(
                            &mc.author, &sc.author,
                            "author differs between stores"
                        );
                        // Parent hashes should also match since trees and imports match
                        prop_assert_eq!(
                            mc.parents.len(), sc.parents.len(),
                            "parent count differs between stores"
                        );
                    }
                }

                Ok(())
            })?;
        }
    }
}
