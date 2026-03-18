//! Pull/push synchronization between any two `ObjectStore + RefStore` implementations.
//!
//! Operates via free functions rather than methods on store traits, keeping sync
//! as an external concern. Efficiently transfers only missing objects by walking
//! the Merkle DAG from ref tips.

use std::collections::HashMap;
use std::pin::pin;

use async_trait::async_trait;
use clayers_xml::ContentHash;
use futures_core::Stream;

use crate::error::{Error, Result};
use crate::graph;
use crate::object::Object;
use crate::store::{ObjectStore, RefStore};

/// Collect a stream of `Result<(K, V)>` into a `HashMap`, short-circuiting on error.
async fn try_collect_stream<S>(stream: S) -> Result<HashMap<ContentHash, Object>>
where
    S: Stream<Item = Result<(ContentHash, Object)>>,
{
    let mut stream = pin!(stream);
    let mut map = HashMap::new();
    while let Some(item) = std::future::poll_fn(|cx| stream.as_mut().poll_next(cx)).await {
        let (hash, obj) = item?;
        map.insert(hash, obj);
    }
    Ok(map)
}

// ---------------------------------------------------------------------------
// Ref conflict resolution
// ---------------------------------------------------------------------------

/// How to resolve a ref that already exists on the destination with a different value.
#[async_trait]
pub trait RefConflict: Send + Sync {
    /// Decide whether to update `ref_name` from `dst_hash` to `src_hash`.
    ///
    /// `store` is the **destination** object store, after source objects have
    /// already been transferred. It contains both the source and destination
    /// commit histories, so graph operations like `common_ancestor` will work.
    ///
    /// Returns `Ok(true)` to proceed, `Ok(false)` to skip, `Err` to abort.
    async fn resolve(
        &self,
        store: &dyn ObjectStore,
        ref_name: &str,
        src_hash: ContentHash,
        dst_hash: ContentHash,
    ) -> Result<bool>;
}

/// Update only if dst is an ancestor of src (no history loss).
pub struct FastForwardOnly;

#[async_trait]
impl RefConflict for FastForwardOnly {
    async fn resolve(
        &self,
        store: &dyn ObjectStore,
        _ref_name: &str,
        src_hash: ContentHash,
        dst_hash: ContentHash,
    ) -> Result<bool> {
        let lca = graph::common_ancestor(store, src_hash, dst_hash).await?;
        if lca == Some(dst_hash) {
            Ok(true)
        } else {
            Err(Error::Ref(
                "cannot fast-forward: destination is not an ancestor of source".into(),
            ))
        }
    }
}

/// Always overwrite the destination ref.
pub struct Overwrite;

#[async_trait]
impl RefConflict for Overwrite {
    async fn resolve(
        &self,
        _store: &dyn ObjectStore,
        _ref_name: &str,
        _src_hash: ContentHash,
        _dst_hash: ContentHash,
    ) -> Result<bool> {
        Ok(true)
    }
}

/// Fail if the destination ref differs.
pub struct Reject;

#[async_trait]
impl RefConflict for Reject {
    async fn resolve(
        &self,
        _store: &dyn ObjectStore,
        _ref_name: &str,
        _src_hash: ContentHash,
        _dst_hash: ContentHash,
    ) -> Result<bool> {
        Err(Error::Ref(
            "destination ref already exists with a different value".into(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Transfer
// ---------------------------------------------------------------------------

/// Copy objects reachable from `root` that `dst` doesn't already have.
///
/// Uses `subtree()` to stream all reachable objects, filters to those
/// missing on `dst`, and batch-inserts them in a single transaction.
///
/// Returns the number of objects transferred.
///
/// # Errors
///
/// Returns an error if objects cannot be read from `src` or written to `dst`.
pub async fn transfer_objects(
    src: &dyn ObjectStore,
    dst: &dyn ObjectStore,
    root: ContentHash,
) -> Result<usize> {
    let src_objects = try_collect_stream(src.subtree(&root)).await?;

    // Filter to objects missing from dst.
    let mut missing = Vec::new();
    for (hash, obj) in &src_objects {
        if !dst.contains(hash).await? {
            missing.push((*hash, obj.clone()));
        }
    }

    if missing.is_empty() {
        return Ok(0);
    }

    // Batch into a single transaction.
    let count = missing.len();
    let mut tx = dst.transaction().await?;
    for (hash, obj) in missing {
        tx.put(hash, obj).await?;
    }
    tx.commit().await?;

    Ok(count)
}

// ---------------------------------------------------------------------------
// Ref sync
// ---------------------------------------------------------------------------

/// Sync a single ref: transfer objects reachable from the source ref, then
/// update the ref on the destination.
///
/// Uses `on_conflict` to decide what to do when the destination already has
/// a different value for the ref.
///
/// Returns `true` if the ref was updated, `false` if it was already
/// up-to-date or the conflict policy chose to skip.
///
/// # Errors
///
/// Returns an error if the source ref is missing, objects cannot be transferred,
/// or the conflict policy rejects the update.
pub async fn sync_ref(
    src_objects: &dyn ObjectStore,
    src_refs: &dyn RefStore,
    dst_objects: &dyn ObjectStore,
    dst_refs: &dyn RefStore,
    ref_name: &str,
    on_conflict: &dyn RefConflict,
) -> Result<bool> {
    let src_hash = src_refs
        .get_ref(ref_name)
        .await?
        .ok_or_else(|| Error::Ref(format!("source ref not found: {ref_name}")))?;

    let dst_hash = dst_refs.get_ref(ref_name).await?;

    if let Some(dst_hash) = dst_hash {
        if dst_hash == src_hash {
            // Already up-to-date.
            return Ok(false);
        }
        // Transfer first so conflict resolution can walk the full graph on dst.
        transfer_objects(src_objects, dst_objects, src_hash).await?;
        let proceed = on_conflict
            .resolve(dst_objects, ref_name, src_hash, dst_hash)
            .await?;
        if !proceed {
            return Ok(false);
        }
    } else {
        transfer_objects(src_objects, dst_objects, src_hash).await?;
    }

    dst_refs.set_ref(ref_name, src_hash).await?;
    Ok(true)
}

/// Sync all refs matching a prefix.
///
/// Returns the number of refs synced.
///
/// # Errors
///
/// Returns an error if refs cannot be listed or any individual ref sync fails.
pub async fn sync_refs(
    src_objects: &dyn ObjectStore,
    src_refs: &dyn RefStore,
    dst_objects: &dyn ObjectStore,
    dst_refs: &dyn RefStore,
    prefix: &str,
    on_conflict: &dyn RefConflict,
) -> Result<usize> {
    let refs = src_refs.list_refs(prefix).await?;
    let mut count = 0;

    for (ref_name, _) in &refs {
        let updated = sync_ref(
            src_objects,
            src_refs,
            dst_objects,
            dst_refs,
            ref_name,
            on_conflict,
        )
        .await?;
        if updated {
            count += 1;
        }
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{
        Author, CommentObject, CommitObject, DocumentObject, ElementObject, PIObject, TagObject,
        TextObject, TreeEntry, TreeObject,
    };
    use crate::store::memory::MemoryStore;
    use chrono::Utc;

    fn author() -> Author {
        Author {
            name: "Test".into(),
            email: "test@test.com".into(),
        }
    }

    /// Build a minimal commit chain in `store`:
    /// text -> element -> document -> tree -> commit
    /// Returns `(commit_hash, document_hash)`.
    async fn build_commit(
        store: &MemoryStore,
        id: &[u8],
        parents: Vec<ContentHash>,
    ) -> (ContentHash, ContentHash) {
        let text_hash = ContentHash::from_canonical(id);
        let text = TextObject {
            content: String::from_utf8_lossy(id).into(),
        };

        let elem_id: Vec<u8> = id.iter().chain(b"elem").copied().collect();
        let elem_hash = ContentHash::from_canonical(&elem_id);
        let elem = ElementObject {
            local_name: "root".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![text_hash],
            inclusive_hash: elem_hash,
        };

        let doc_id: Vec<u8> = id.iter().chain(b"doc").copied().collect();
        let doc_hash = ContentHash::from_canonical(&doc_id);
        let doc = DocumentObject { root: elem_hash, prologue: vec![] };

        let tree_id: Vec<u8> = id.iter().chain(b"tree").copied().collect();
        let tree_hash = ContentHash::from_canonical(&tree_id);
        let tree = TreeObject::new(vec![
            TreeEntry { path: "doc.xml".into(), document: doc_hash },
        ]);

        let commit_id: Vec<u8> = id.iter().chain(b"commit").copied().collect();
        let commit_hash = ContentHash::from_canonical(&commit_id);
        let commit = CommitObject {
            tree: tree_hash,
            parents,
            author: author(),
            timestamp: Utc::now(),
            message: format!("commit {}", String::from_utf8_lossy(id)),
        };

        let mut tx = store.transaction().await.unwrap();
        tx.put(text_hash, Object::Text(text)).await.unwrap();
        tx.put(elem_hash, Object::Element(elem)).await.unwrap();
        tx.put(doc_hash, Object::Document(doc)).await.unwrap();
        tx.put(tree_hash, Object::Tree(tree)).await.unwrap();
        tx.put(commit_hash, Object::Commit(commit)).await.unwrap();
        tx.commit().await.unwrap();

        (commit_hash, doc_hash)
    }

    #[tokio::test]
    async fn sync_transfer_objects_copies_missing() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        let (commit_hash, _) = build_commit(&src, b"c1", vec![]).await;

        // 5 objects: text, element, document, tree, commit
        let count = transfer_objects(&src, &dst, commit_hash).await.unwrap();
        assert_eq!(count, 5);

        // All objects present on dst.
        assert!(dst.contains(&commit_hash).await.unwrap());
    }

    #[tokio::test]
    async fn sync_transfer_idempotent() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        let (commit_hash, _) = build_commit(&src, b"c1", vec![]).await;

        transfer_objects(&src, &dst, commit_hash).await.unwrap();
        let second = transfer_objects(&src, &dst, commit_hash).await.unwrap();
        assert_eq!(second, 0, "second transfer should copy 0 objects");
    }

    #[tokio::test]
    async fn sync_ref_fast_forward() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        // Linear: c1 <- c2
        let (c1, _) = build_commit(&src, b"c1", vec![]).await;
        let (c2, _) = build_commit(&src, b"c2", vec![c1]).await;

        // dst has c1 on the ref.
        transfer_objects(&src, &dst, c1).await.unwrap();
        dst.set_ref("refs/heads/main", c1).await.unwrap();

        // src has c2 on the ref.
        src.set_ref("refs/heads/main", c2).await.unwrap();

        // Fast-forward should succeed and report updated.
        let updated = sync_ref(&src, &src, &dst, &dst, "refs/heads/main", &FastForwardOnly)
            .await
            .unwrap();
        assert!(updated, "fast-forward should report ref was updated");

        let dst_ref = dst.get_ref("refs/heads/main").await.unwrap();
        assert_eq!(dst_ref, Some(c2));
    }

    #[tokio::test]
    async fn sync_ref_fast_forward_rejects_diverged() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        // Diverged: c1 <- c2 (src) and c1 <- c3 (dst)
        let (c1, _) = build_commit(&src, b"c1", vec![]).await;
        let (c2, _) = build_commit(&src, b"c2", vec![c1]).await;
        let (c3, _) = build_commit(&dst, b"c3", vec![c1]).await;

        // Need c1 in dst too.
        transfer_objects(&src, &dst, c1).await.unwrap();

        src.set_ref("refs/heads/main", c2).await.unwrap();
        dst.set_ref("refs/heads/main", c3).await.unwrap();

        let result = sync_ref(&src, &src, &dst, &dst, "refs/heads/main", &FastForwardOnly).await;
        assert!(result.is_err(), "should reject diverged histories");
    }

    #[tokio::test]
    async fn sync_ref_overwrite_always_succeeds() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        let (c1, _) = build_commit(&src, b"c1", vec![]).await;
        let (c2, _) = build_commit(&src, b"c2", vec![]).await; // no parent, diverged

        transfer_objects(&src, &dst, c1).await.unwrap();
        dst.set_ref("refs/heads/main", c1).await.unwrap();
        src.set_ref("refs/heads/main", c2).await.unwrap();

        let updated = sync_ref(&src, &src, &dst, &dst, "refs/heads/main", &Overwrite)
            .await
            .unwrap();
        assert!(updated, "overwrite should report ref was updated");

        let dst_ref = dst.get_ref("refs/heads/main").await.unwrap();
        assert_eq!(dst_ref, Some(c2));
    }

    #[tokio::test]
    async fn sync_ref_reject_fails_when_different() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        let (c1, _) = build_commit(&src, b"c1", vec![]).await;
        let (c2, _) = build_commit(&src, b"c2", vec![]).await;

        transfer_objects(&src, &dst, c1).await.unwrap();
        dst.set_ref("refs/heads/main", c1).await.unwrap();
        src.set_ref("refs/heads/main", c2).await.unwrap();

        let result = sync_ref(&src, &src, &dst, &dst, "refs/heads/main", &Reject).await;
        assert!(result.is_err(), "should reject when refs differ");
    }

    #[tokio::test]
    async fn sync_refs_with_prefix() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        let (c1, _) = build_commit(&src, b"c1", vec![]).await;
        let (c2, _) = build_commit(&src, b"c2", vec![]).await;

        src.set_ref("refs/heads/main", c1).await.unwrap();
        src.set_ref("refs/heads/feature", c2).await.unwrap();

        let count = sync_refs(&src, &src, &dst, &dst, "refs/heads/", &Overwrite)
            .await
            .unwrap();

        assert_eq!(count, 2);
        assert_eq!(dst.get_ref("refs/heads/main").await.unwrap(), Some(c1));
        assert_eq!(dst.get_ref("refs/heads/feature").await.unwrap(), Some(c2));
    }

    #[tokio::test]
    async fn sync_ref_missing_src_ref_errors() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        let result =
            sync_ref(&src, &src, &dst, &dst, "refs/heads/missing", &Overwrite).await;
        assert!(result.is_err(), "missing source ref should error");
    }

    #[tokio::test]
    async fn sync_ref_already_up_to_date() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        let (c1, _) = build_commit(&src, b"c1", vec![]).await;

        transfer_objects(&src, &dst, c1).await.unwrap();
        src.set_ref("refs/heads/main", c1).await.unwrap();
        dst.set_ref("refs/heads/main", c1).await.unwrap();

        // Should succeed without calling conflict resolution, report not updated.
        let updated = sync_ref(&src, &src, &dst, &dst, "refs/heads/main", &Reject)
            .await
            .unwrap();
        assert!(!updated, "already up-to-date should report false");
    }

    // --- Gap 1: Tag, Comment, PI reachability ---

    #[tokio::test]
    async fn sync_reachable_follows_tag_to_commit() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        let (commit_hash, _) = build_commit(&src, b"c1", vec![]).await;

        // Create a tag pointing at the commit.
        let tag_id = b"tag-v1";
        let tag_hash = ContentHash::from_canonical(tag_id);
        let tag = TagObject {
            target: commit_hash,
            name: "v1.0".into(),
            tagger: author(),
            timestamp: Utc::now(),
            message: "release".into(),
        };
        let mut tx = src.transaction().await.unwrap();
        tx.put(tag_hash, Object::Tag(tag)).await.unwrap();
        tx.commit().await.unwrap();

        // Transfer from the tag root: should pull tag + commit + tree + doc + elem + text = 6.
        let count = transfer_objects(&src, &dst, tag_hash).await.unwrap();
        assert_eq!(count, 6);
        assert!(dst.contains(&tag_hash).await.unwrap());
        assert!(dst.contains(&commit_hash).await.unwrap());
    }

    #[tokio::test]
    async fn sync_reachable_follows_comment_and_pi_leaves() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        // Build: comment + PI as children of an element -> document -> commit.
        let comment_hash = ContentHash::from_canonical(b"comment1");
        let comment = CommentObject {
            content: "a comment".into(),
        };

        let pi_hash = ContentHash::from_canonical(b"pi1");
        let pi = PIObject {
            target: "xml-stylesheet".into(),
            data: Some("type=\"text/xsl\"".into()),
        };

        let elem_hash = ContentHash::from_canonical(b"elem-mixed");
        let elem = ElementObject {
            local_name: "root".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![comment_hash, pi_hash],
            inclusive_hash: elem_hash,
        };

        let doc_hash = ContentHash::from_canonical(b"doc-mixed");
        let doc = DocumentObject { root: elem_hash, prologue: vec![] };

        let tree_hash = ContentHash::from_canonical(b"tree-mixed");
        let tree = TreeObject::new(vec![
            TreeEntry { path: "doc.xml".into(), document: doc_hash },
        ]);

        let commit_hash = ContentHash::from_canonical(b"commit-mixed");
        let commit = CommitObject {
            tree: tree_hash,
            parents: vec![],
            author: author(),
            timestamp: Utc::now(),
            message: "mixed content".into(),
        };

        let mut tx = src.transaction().await.unwrap();
        tx.put(comment_hash, Object::Comment(comment)).await.unwrap();
        tx.put(pi_hash, Object::PI(pi)).await.unwrap();
        tx.put(elem_hash, Object::Element(elem)).await.unwrap();
        tx.put(doc_hash, Object::Document(doc)).await.unwrap();
        tx.put(tree_hash, Object::Tree(tree)).await.unwrap();
        tx.put(commit_hash, Object::Commit(commit)).await.unwrap();
        tx.commit().await.unwrap();

        let count = transfer_objects(&src, &dst, commit_hash).await.unwrap();
        assert_eq!(count, 6); // commit + tree + doc + elem + comment + pi
        assert!(dst.contains(&comment_hash).await.unwrap());
        assert!(dst.contains(&pi_hash).await.unwrap());
    }

    // --- Gap 2: Verify all inner objects land on dst ---

    #[tokio::test]
    async fn sync_transfer_copies_all_inner_objects() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        let (commit_hash, doc_hash) = build_commit(&src, b"c1", vec![]).await;

        // Recover the text, element, and tree hashes used by build_commit.
        let text_hash = ContentHash::from_canonical(b"c1");
        let elem_hash = ContentHash::from_canonical(b"c1elem");
        let tree_hash = ContentHash::from_canonical(b"c1tree");

        transfer_objects(&src, &dst, commit_hash).await.unwrap();

        // Every single object must be on dst, not just the commit.
        assert!(dst.contains(&commit_hash).await.unwrap(), "commit missing");
        assert!(dst.contains(&tree_hash).await.unwrap(), "tree missing");
        assert!(dst.contains(&doc_hash).await.unwrap(), "document missing");
        assert!(dst.contains(&elem_hash).await.unwrap(), "element missing");
        assert!(dst.contains(&text_hash).await.unwrap(), "text missing");

        // Also verify the objects are identical.
        let src_text = src.get(&text_hash).await.unwrap().unwrap();
        let dst_text = dst.get(&text_hash).await.unwrap().unwrap();
        assert_eq!(src_text, dst_text);
    }

    // --- Gap 3: Multi-commit chain transfer follows parent links ---

    #[tokio::test]
    async fn sync_transfer_follows_parent_chain() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        // c1 <- c2 <- c3, each with its own doc subtree (5 objects each).
        let (c1, _) = build_commit(&src, b"c1", vec![]).await;
        let (c2, _) = build_commit(&src, b"c2", vec![c1]).await;
        let (c3, _) = build_commit(&src, b"c3", vec![c2]).await;

        // Transfer from c3 tip only. Must pull all 3 commits + all subtrees.
        let count = transfer_objects(&src, &dst, c3).await.unwrap();
        assert_eq!(count, 15); // 3 commits * 5 objects each

        assert!(dst.contains(&c1).await.unwrap(), "ancestor c1 missing");
        assert!(dst.contains(&c2).await.unwrap(), "ancestor c2 missing");
        assert!(dst.contains(&c3).await.unwrap(), "tip c3 missing");
    }

    // --- Gap 4: sync_refs prefix actually filters ---

    #[tokio::test]
    async fn sync_refs_prefix_excludes_non_matching() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        let (c1, _) = build_commit(&src, b"c1", vec![]).await;
        let (c2, _) = build_commit(&src, b"c2", vec![]).await;

        src.set_ref("refs/heads/main", c1).await.unwrap();
        src.set_ref("refs/tags/v1", c2).await.unwrap();

        // Sync only heads, not tags.
        let count = sync_refs(&src, &src, &dst, &dst, "refs/heads/", &Overwrite)
            .await
            .unwrap();

        assert_eq!(count, 1);
        assert_eq!(dst.get_ref("refs/heads/main").await.unwrap(), Some(c1));
        assert_eq!(
            dst.get_ref("refs/tags/v1").await.unwrap(),
            None,
            "tag ref should NOT have been synced"
        );
    }

    // --- Gap 5: sync_refs partial failure aborts on error ---

    #[tokio::test]
    async fn sync_refs_aborts_on_conflict_error() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        let (c1, _) = build_commit(&src, b"c1", vec![]).await;
        let (c2, _) = build_commit(&src, b"c2", vec![]).await;
        let (c3, _) = build_commit(&src, b"c3", vec![]).await;

        // src has two branches.
        src.set_ref("refs/heads/alpha", c1).await.unwrap();
        src.set_ref("refs/heads/beta", c2).await.unwrap();

        // dst has a conflicting value for one of them.
        transfer_objects(&src, &dst, c3).await.unwrap();
        dst.set_ref("refs/heads/alpha", c3).await.unwrap(); // different from src's c1

        // Reject policy: the conflicting ref causes an error.
        let result = sync_refs(&src, &src, &dst, &dst, "refs/heads/", &Reject).await;
        assert!(result.is_err(), "should abort when a ref conflicts under Reject");
    }

    // --- Gap 6: FastForwardOnly via sync_ref (full flow, no manual pre-transfer) ---

    #[tokio::test]
    async fn sync_ref_fast_forward_full_flow() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        // Build c1 <- c2 entirely in src.
        let (c1, _) = build_commit(&src, b"c1", vec![]).await;
        let (c2, _) = build_commit(&src, b"c2", vec![c1]).await;

        // Sync c1 to dst first (sets up the ref on both sides).
        src.set_ref("refs/heads/main", c1).await.unwrap();
        let created = sync_ref(&src, &src, &dst, &dst, "refs/heads/main", &Overwrite)
            .await
            .unwrap();
        assert!(created, "initial sync should report updated");

        // Now advance src to c2. dst still at c1.
        src.set_ref("refs/heads/main", c2).await.unwrap();

        // FastForwardOnly through sync_ref: it should transfer objects first,
        // then resolve on dst where both c1 and c2 now exist.
        let updated = sync_ref(&src, &src, &dst, &dst, "refs/heads/main", &FastForwardOnly)
            .await
            .unwrap();
        assert!(updated, "fast-forward should report updated");

        assert_eq!(dst.get_ref("refs/heads/main").await.unwrap(), Some(c2));
        // Verify c1 is still reachable on dst (ancestor was preserved).
        assert!(dst.contains(&c1).await.unwrap());
    }

    // --- Gap 7: Shared subtree deduplication ---

    #[tokio::test]
    async fn sync_transfer_deduplicates_shared_subtree() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        // c1 has its own subtree (5 objects: text, elem, doc, tree, commit).
        let (c1, _) = build_commit(&src, b"c1", vec![]).await;

        // c2 has its own subtree from build_commit = 5 new objects.
        let (c2, _) = build_commit(&src, b"c2", vec![c1]).await;

        // Transfer c1 first.
        let first = transfer_objects(&src, &dst, c1).await.unwrap();
        assert_eq!(first, 5);

        // Transfer c2: should NOT re-transfer c1's objects.
        // c2 adds: its own text + element + document + tree + commit = 5 new objects.
        // c1's subtree is already on dst.
        let second = transfer_objects(&src, &dst, c2).await.unwrap();
        assert_eq!(second, 5, "should only transfer c2's new objects, not c1's");
    }

    // --- Gap 8: sync_ref creates ref on fresh dst ---

    #[tokio::test]
    async fn sync_ref_creates_new_ref_on_empty_dst() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        let (c1, _) = build_commit(&src, b"c1", vec![]).await;
        src.set_ref("refs/heads/main", c1).await.unwrap();

        // dst has no refs at all. sync_ref should create it.
        let updated = sync_ref(&src, &src, &dst, &dst, "refs/heads/main", &FastForwardOnly)
            .await
            .unwrap();
        assert!(updated, "creating a new ref should report updated");

        assert_eq!(dst.get_ref("refs/heads/main").await.unwrap(), Some(c1));
        assert!(dst.contains(&c1).await.unwrap(), "objects should be transferred");
    }

    // --- Skip policy: exercises the Ok(false) return path ---

    /// A custom policy that skips conflicting refs instead of erroring.
    struct Skip;

    #[async_trait]
    impl RefConflict for Skip {
        async fn resolve(
            &self,
            _store: &dyn ObjectStore,
            _ref_name: &str,
            _src_hash: ContentHash,
            _dst_hash: ContentHash,
        ) -> Result<bool> {
            Ok(false)
        }
    }

    #[tokio::test]
    async fn sync_ref_skip_leaves_dst_ref_unchanged() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        let (c1, _) = build_commit(&src, b"c1", vec![]).await;
        let (c2, _) = build_commit(&src, b"c2", vec![]).await;

        transfer_objects(&src, &dst, c1).await.unwrap();
        dst.set_ref("refs/heads/main", c1).await.unwrap();
        src.set_ref("refs/heads/main", c2).await.unwrap();

        // Skip policy: resolve returns Ok(false). Ref must NOT be updated.
        let updated = sync_ref(&src, &src, &dst, &dst, "refs/heads/main", &Skip)
            .await
            .unwrap();
        assert!(!updated, "skip should report false");

        assert_eq!(
            dst.get_ref("refs/heads/main").await.unwrap(),
            Some(c1),
            "ref should remain at c1 after skip"
        );
    }

    #[tokio::test]
    async fn sync_ref_skip_still_transfers_objects() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        let (c1, _) = build_commit(&src, b"c1", vec![]).await;
        let (c2, _) = build_commit(&src, b"c2", vec![]).await;

        transfer_objects(&src, &dst, c1).await.unwrap();
        dst.set_ref("refs/heads/main", c1).await.unwrap();
        src.set_ref("refs/heads/main", c2).await.unwrap();

        let updated = sync_ref(&src, &src, &dst, &dst, "refs/heads/main", &Skip)
            .await
            .unwrap();
        assert!(!updated, "skip should not report updated");

        // Objects are transferred before resolve is called, so c2's
        // objects should be on dst even though the ref wasn't updated.
        assert!(
            dst.contains(&c2).await.unwrap(),
            "c2 objects should be on dst even after skip"
        );
    }

    // --- Merge commit: multi-parent DAG walking ---

    #[tokio::test]
    async fn sync_transfer_follows_merge_commit_parents() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        //    c1
        //   /  \
        //  c2   c3
        //   \  /
        //    c4 (merge)
        let (c1, _) = build_commit(&src, b"c1", vec![]).await;
        let (c2, _) = build_commit(&src, b"c2", vec![c1]).await;
        let (c3, _) = build_commit(&src, b"c3", vec![c1]).await;
        let (c4, _) = build_commit(&src, b"c4", vec![c2, c3]).await;

        // Transfer from merge tip. Must follow both parents.
        let count = transfer_objects(&src, &dst, c4).await.unwrap();
        // c1=5, c2=5, c3=5, c4=5 = 20 total objects.
        assert_eq!(count, 20);

        assert!(dst.contains(&c1).await.unwrap(), "root c1 missing");
        assert!(dst.contains(&c2).await.unwrap(), "left parent c2 missing");
        assert!(dst.contains(&c3).await.unwrap(), "right parent c3 missing");
        assert!(dst.contains(&c4).await.unwrap(), "merge c4 missing");
    }

    // --- Objects leak on conflict rejection ---

    #[tokio::test]
    async fn sync_ref_reject_still_leaves_objects_on_dst() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        let (c1, _) = build_commit(&src, b"c1", vec![]).await;
        let (c2, _) = build_commit(&src, b"c2", vec![]).await;

        transfer_objects(&src, &dst, c1).await.unwrap();
        dst.set_ref("refs/heads/main", c1).await.unwrap();
        src.set_ref("refs/heads/main", c2).await.unwrap();

        // Reject will error, but objects are transferred before resolve.
        let result = sync_ref(&src, &src, &dst, &dst, "refs/heads/main", &Reject).await;
        assert!(result.is_err());

        // c2's objects are on dst even though the ref update was rejected.
        assert!(
            dst.contains(&c2).await.unwrap(),
            "objects should be on dst despite rejection"
        );
        // Ref stays at c1.
        assert_eq!(dst.get_ref("refs/heads/main").await.unwrap(), Some(c1));
    }

    // --- sync_refs count includes skipped refs ---

    #[tokio::test]
    async fn sync_refs_excludes_skipped_from_count() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        let (c1, _) = build_commit(&src, b"c1", vec![]).await;
        let (c2, _) = build_commit(&src, b"c2", vec![]).await;

        src.set_ref("refs/heads/main", c1).await.unwrap();
        src.set_ref("refs/heads/feature", c2).await.unwrap();

        // dst already has main at a different hash -> Skip will skip it.
        let (c3, _) = build_commit(&dst, b"c3", vec![]).await;
        dst.set_ref("refs/heads/main", c3).await.unwrap();

        let count = sync_refs(&src, &src, &dst, &dst, "refs/heads/", &Skip)
            .await
            .unwrap();

        // Only feature was actually synced; main was skipped.
        assert_eq!(count, 1, "count should exclude skipped refs");

        // main was skipped, still at c3.
        assert_eq!(dst.get_ref("refs/heads/main").await.unwrap(), Some(c3));
        // feature was created.
        assert_eq!(dst.get_ref("refs/heads/feature").await.unwrap(), Some(c2));
    }

    // --- sync_refs with empty prefix matches all ---

    #[tokio::test]
    async fn sync_refs_empty_prefix_matches_all() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        let (c1, _) = build_commit(&src, b"c1", vec![]).await;
        let (c2, _) = build_commit(&src, b"c2", vec![]).await;

        src.set_ref("refs/heads/main", c1).await.unwrap();
        src.set_ref("refs/tags/v1", c2).await.unwrap();

        let count = sync_refs(&src, &src, &dst, &dst, "", &Overwrite)
            .await
            .unwrap();

        assert_eq!(count, 2);
        assert_eq!(dst.get_ref("refs/heads/main").await.unwrap(), Some(c1));
        assert_eq!(dst.get_ref("refs/tags/v1").await.unwrap(), Some(c2));
    }

    // --- sync_refs excludes already-up-to-date from count ---

    #[tokio::test]
    async fn sync_refs_excludes_up_to_date_from_count() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        let (c1, _) = build_commit(&src, b"c1", vec![]).await;
        let (c2, _) = build_commit(&src, b"c2", vec![]).await;

        src.set_ref("refs/heads/main", c1).await.unwrap();
        src.set_ref("refs/heads/feature", c2).await.unwrap();

        // First sync: both refs are new.
        let count = sync_refs(&src, &src, &dst, &dst, "refs/heads/", &Overwrite)
            .await
            .unwrap();
        assert_eq!(count, 2);

        // Second sync: both already up-to-date.
        let count = sync_refs(&src, &src, &dst, &dst, "refs/heads/", &Overwrite)
            .await
            .unwrap();
        assert_eq!(count, 0, "nothing changed, count should be 0");
    }

    // --- sync_refs returns 0 for non-matching prefix ---

    #[tokio::test]
    async fn sync_refs_no_matching_refs_returns_zero() {
        let src = MemoryStore::new();
        let dst = MemoryStore::new();

        let (c1, _) = build_commit(&src, b"c1", vec![]).await;
        src.set_ref("refs/heads/main", c1).await.unwrap();

        let count = sync_refs(&src, &src, &dst, &dst, "refs/remotes/", &Overwrite)
            .await
            .unwrap();

        assert_eq!(count, 0);
    }
}
