//! Property-based tests for store traits using proptest.
//!
//! `PropStoreTester<S>` exercises `ObjectStore + RefStore` against any backend
//! with randomly generated inputs. Each backend's test module invokes
//! `prop_store_tests!` with a constructor, mirroring `store_tests!`.

#![allow(clippy::similar_names, clippy::needless_pass_by_value, clippy::doc_markdown)]

use std::collections::HashMap;

use clayers_xml::ContentHash;
use tokio_stream::StreamExt;

use super::{ObjectStore, RefStore};
use crate::object::{DocumentObject, ElementObject, Object, TextObject};
use crate::store::prop_strategies::{self, StoreOp};

/// Create a single-threaded Tokio runtime for bridging async store ops
/// into proptest's synchronous test functions.
fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().unwrap()
}

pub(crate) struct PropStoreTester<S: ObjectStore + RefStore> {
    pub store: S,
}

impl<S: ObjectStore + RefStore> PropStoreTester<S> {
    // ── Group A: ObjectStore properties ──────────────────────────────

    /// A1: Put + commit + get round-trips any object.
    pub fn prop_object_roundtrip(&self, (hash, obj): (ContentHash, Object)) {
        rt().block_on(async {
            let mut tx = self.store.transaction().await.unwrap();
            tx.put(hash, obj.clone()).await.unwrap();
            tx.commit().await.unwrap();

            let got = self.store.get(&hash).await.unwrap();
            assert_eq!(got, Some(obj));
        });
    }

    /// A2: Putting the same (hash, obj) in two transactions is idempotent.
    pub fn prop_idempotent_put(&self, (hash, obj): (ContentHash, Object)) {
        rt().block_on(async {
            let mut tx1 = self.store.transaction().await.unwrap();
            tx1.put(hash, obj.clone()).await.unwrap();
            tx1.commit().await.unwrap();

            let mut tx2 = self.store.transaction().await.unwrap();
            tx2.put(hash, obj.clone()).await.unwrap();
            tx2.commit().await.unwrap();

            let got = self.store.get(&hash).await.unwrap();
            assert_eq!(got, Some(obj));
        });
    }

    /// A3: Contains returns true after put + commit.
    pub fn prop_contains_after_commit(&self, (hash, obj): (ContentHash, Object)) {
        rt().block_on(async {
            let mut tx = self.store.transaction().await.unwrap();
            tx.put(hash, obj).await.unwrap();
            tx.commit().await.unwrap();

            assert!(self.store.contains(&hash).await.unwrap());
        });
    }

    /// A4: Rollback discards pending writes; contains returns false.
    pub fn prop_rollback_isolation(&self, (hash, obj): (ContentHash, Object)) {
        rt().block_on(async {
            let mut tx = self.store.transaction().await.unwrap();
            tx.put(hash, obj).await.unwrap();
            tx.rollback().await.unwrap();

            assert!(!self.store.contains(&hash).await.unwrap());
        });
    }

    /// A5: ElementObject inclusive_hash is indexed and retrievable.
    pub fn prop_inclusive_hash_index(
        &self,
        identity_hash: ContentHash,
        elem: ElementObject,
    ) {
        let inclusive = elem.inclusive_hash;
        let obj = Object::Element(elem);
        rt().block_on(async {
            let mut tx = self.store.transaction().await.unwrap();
            tx.put(identity_hash, obj).await.unwrap();
            tx.commit().await.unwrap();

            let result = self
                .store
                .get_by_inclusive_hash(&inclusive)
                .await
                .unwrap();
            assert!(result.is_some(), "inclusive hash lookup should succeed");
            let (found_id, _) = result.unwrap();
            assert_eq!(found_id, identity_hash);
        });
    }

    /// A6: Get on a fresh store for a random hash returns None.
    pub fn prop_get_nonexistent(&self, hash: ContentHash) {
        rt().block_on(async {
            let got = self.store.get(&hash).await.unwrap();
            assert_eq!(got, None);
        });
    }

    /// A7: Transaction atomicity -- objects are invisible before commit,
    /// all visible after commit, and all invisible after rollback.
    pub fn prop_transaction_atomicity(
        &self,
        objects: Vec<(ContentHash, Object)>,
    ) {
        rt().block_on(async {
            // Deduplicate hashes (proptest shrinking can produce duplicates)
            let unique: std::collections::HashMap<_, _> =
                objects.into_iter().collect();
            let objects: Vec<_> = unique.into_iter().collect();
            if objects.is_empty() {
                return;
            }

            // Phase 1: put objects in a transaction but DON'T commit yet.
            // Verify none are visible via the store.
            let mut tx = self.store.transaction().await.unwrap();
            for (h, o) in &objects {
                tx.put(*h, o.clone()).await.unwrap();
            }
            for (h, _) in &objects {
                assert!(
                    !self.store.contains(h).await.unwrap(),
                    "object should NOT be visible before commit"
                );
            }

            // Phase 2: commit. Now ALL must be visible.
            tx.commit().await.unwrap();
            for (h, expected) in &objects {
                let got = self.store.get(h).await.unwrap();
                assert_eq!(
                    got.as_ref(),
                    Some(expected),
                    "object should be visible after commit"
                );
            }

            // Phase 3: put NEW objects in a second transaction, then rollback.
            // The new objects must NOT be visible, and the old ones must still be.
            let extra_hash = ContentHash::from_canonical(b"atomicity_rollback_probe");
            let extra_obj = Object::Text(crate::object::TextObject {
                content: "should not survive rollback".into(),
            });
            let mut tx2 = self.store.transaction().await.unwrap();
            tx2.put(extra_hash, extra_obj).await.unwrap();
            tx2.rollback().await.unwrap();

            assert!(
                !self.store.contains(&extra_hash).await.unwrap(),
                "rolled-back object should NOT be visible"
            );
            // Original objects must still be intact
            for (h, expected) in &objects {
                let got = self.store.get(h).await.unwrap();
                assert_eq!(
                    got.as_ref(),
                    Some(expected),
                    "previously committed object should survive a later rollback"
                );
            }
        });
    }

    /// A8: Subtree over a generated DAG yields exactly the expected set of hashes.
    pub fn prop_subtree_completeness(
        &self,
        dag: Vec<(ContentHash, Object)>,
        root: ContentHash,
    ) {
        rt().block_on(async {
            let mut tx = self.store.transaction().await.unwrap();
            for (h, o) in &dag {
                tx.put(*h, o.clone()).await.unwrap();
            }
            tx.commit().await.unwrap();

            let pairs: Vec<(ContentHash, Object)> = self
                .store
                .subtree(&root)
                .map(|r| r.unwrap())
                .collect()
                .await;
            let result_hashes: std::collections::HashSet<_> =
                pairs.iter().map(|(h, _)| *h).collect();
            let expected_hashes: std::collections::HashSet<_> =
                dag.iter().map(|(h, _)| *h).collect();
            assert_eq!(result_hashes, expected_hashes);
        });
    }

    /// A9: Diamond DAG (shared leaf) yields exactly 5 objects, not 6.
    pub fn prop_subtree_deduplication(&self, leaf_content: String) {
        rt().block_on(async {
            let shared_hash = ContentHash::from_canonical(leaf_content.as_bytes());
            let shared = Object::Text(TextObject {
                content: leaf_content,
            });

            let ea_hash = ContentHash::from_canonical(b"prop_diamond_a");
            let ea = Object::Element(ElementObject {
                local_name: "a".into(),
                namespace_uri: None,
                namespace_prefix: None,
                extra_namespaces: vec![],
                attributes: vec![],
                children: vec![shared_hash],
                inclusive_hash: ea_hash,
            });

            let eb_hash = ContentHash::from_canonical(b"prop_diamond_b");
            let eb = Object::Element(ElementObject {
                local_name: "b".into(),
                namespace_uri: None,
                namespace_prefix: None,
                extra_namespaces: vec![],
                attributes: vec![],
                children: vec![shared_hash],
                inclusive_hash: eb_hash,
            });

            let root_hash = ContentHash::from_canonical(b"prop_diamond_root");
            let root = Object::Element(ElementObject {
                local_name: "root".into(),
                namespace_uri: None,
                namespace_prefix: None,
                extra_namespaces: vec![],
                attributes: vec![],
                children: vec![ea_hash, eb_hash],
                inclusive_hash: root_hash,
            });

            let doc_hash = ContentHash::from_canonical(b"prop_diamond_doc");
            let doc = Object::Document(DocumentObject {
                root: root_hash,
                prologue: vec![],
            });

            let mut tx = self.store.transaction().await.unwrap();
            tx.put(shared_hash, shared).await.unwrap();
            tx.put(ea_hash, ea).await.unwrap();
            tx.put(eb_hash, eb).await.unwrap();
            tx.put(root_hash, root).await.unwrap();
            tx.put(doc_hash, doc).await.unwrap();
            tx.commit().await.unwrap();

            let pairs: Vec<(ContentHash, Object)> = self
                .store
                .subtree(&doc_hash)
                .map(|r| r.unwrap())
                .collect()
                .await;
            let objects: HashMap<ContentHash, Object> = pairs.into_iter().collect();
            assert_eq!(objects.len(), 5, "shared leaf must appear once, not twice");
        });
    }

    /// A10: Subtree produces an error when a referenced child is missing.
    pub fn prop_subtree_missing_object(&self, missing_child: ContentHash) {
        rt().block_on(async {
            let elem_hash = ContentHash::from_canonical(b"prop_subtree_parent_missing");
            let doc_hash = ContentHash::from_canonical(b"prop_subtree_doc_missing");

            let mut tx = self.store.transaction().await.unwrap();
            tx.put(
                elem_hash,
                Object::Element(ElementObject {
                    local_name: "root".into(),
                    namespace_uri: None,
                    namespace_prefix: None,
                    extra_namespaces: vec![],
                    attributes: vec![],
                    children: vec![missing_child],
                    inclusive_hash: elem_hash,
                }),
            )
            .await
            .unwrap();
            tx.put(
                doc_hash,
                Object::Document(DocumentObject {
                    root: elem_hash,
                    prologue: vec![],
                }),
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();

            let results: Vec<crate::error::Result<(ContentHash, Object)>> =
                self.store.subtree(&doc_hash).collect().await;
            let has_error = results.iter().any(std::result::Result::is_err);
            assert!(has_error, "subtree should error when a referenced object is missing");
        });
    }

    // ── Group B: RefStore properties ─────────────────────────────────

    /// B1: set_ref + get_ref round-trips.
    pub fn prop_ref_roundtrip(&self, name: String, hash: ContentHash) {
        rt().block_on(async {
            self.store.set_ref(&name, hash).await.unwrap();
            let got = self.store.get_ref(&name).await.unwrap();
            assert_eq!(got, Some(hash));
        });
    }

    /// B2: set_ref + delete_ref + get_ref returns None.
    pub fn prop_ref_delete(&self, name: String, hash: ContentHash) {
        rt().block_on(async {
            self.store.set_ref(&name, hash).await.unwrap();
            self.store.delete_ref(&name).await.unwrap();
            let got = self.store.get_ref(&name).await.unwrap();
            assert_eq!(got, None);
        });
    }

    /// B3: cas_ref(name, None, h) on fresh store succeeds.
    pub fn prop_cas_create(&self, name: String, hash: ContentHash) {
        rt().block_on(async {
            let ok = self.store.cas_ref(&name, None, hash).await.unwrap();
            assert!(ok, "cas_ref create-if-absent should succeed");
        });
    }

    /// B4: set_ref(name, h1); cas_ref(name, Some(h1), h2) succeeds.
    pub fn prop_cas_swap(&self, name: String, h1: ContentHash, h2: ContentHash) {
        rt().block_on(async {
            self.store.set_ref(&name, h1).await.unwrap();
            let ok = self.store.cas_ref(&name, Some(h1), h2).await.unwrap();
            assert!(ok, "cas_ref swap should succeed");
            let got = self.store.get_ref(&name).await.unwrap();
            assert_eq!(got, Some(h2));
        });
    }

    /// B5: cas_ref with wrong expected value is rejected.
    pub fn prop_cas_reject(
        &self,
        name: String,
        h1: ContentHash,
        h_wrong: ContentHash,
        h2: ContentHash,
    ) {
        if h_wrong == h1 {
            return; // Only test when values differ
        }
        rt().block_on(async {
            self.store.set_ref(&name, h1).await.unwrap();
            let ok = self.store.cas_ref(&name, Some(h_wrong), h2).await.unwrap();
            assert!(!ok, "cas_ref should reject on mismatch");
            let got = self.store.get_ref(&name).await.unwrap();
            assert_eq!(got, Some(h1));
        });
    }

    /// B6: list_refs with prefix filters correctly.
    pub fn prop_list_refs_prefix(
        &self,
        head_suffixes: Vec<String>,
        tag_suffixes: Vec<String>,
        hash: ContentHash,
    ) {
        // Deduplicate: set_ref overwrites, so duplicate suffixes collapse to one ref.
        let unique_heads: std::collections::HashSet<_> = head_suffixes.iter().collect();
        let unique_tags: std::collections::HashSet<_> = tag_suffixes.iter().collect();
        rt().block_on(async {
            for suffix in &head_suffixes {
                self.store
                    .set_ref(&format!("refs/heads/{suffix}"), hash)
                    .await
                    .unwrap();
            }
            for suffix in &tag_suffixes {
                self.store
                    .set_ref(&format!("refs/tags/{suffix}"), hash)
                    .await
                    .unwrap();
            }

            let heads = self.store.list_refs("refs/heads/").await.unwrap();
            assert_eq!(heads.len(), unique_heads.len());

            let tags = self.store.list_refs("refs/tags/").await.unwrap();
            assert_eq!(tags.len(), unique_tags.len());
        });
    }

    /// B7: Deleting one ref does not affect another.
    pub fn prop_ref_independence(
        &self,
        name_a: String,
        name_b: String,
        h_a: ContentHash,
        h_b: ContentHash,
    ) {
        if name_a == name_b {
            return;
        }
        rt().block_on(async {
            self.store.set_ref(&name_a, h_a).await.unwrap();
            self.store.set_ref(&name_b, h_b).await.unwrap();
            self.store.delete_ref(&name_a).await.unwrap();

            assert_eq!(self.store.get_ref(&name_a).await.unwrap(), None);
            assert_eq!(self.store.get_ref(&name_b).await.unwrap(), Some(h_b));
        });
    }

    /// B8: set_ref overwrites the previous value.
    pub fn prop_ref_overwrite(&self, name: String, h1: ContentHash, h2: ContentHash) {
        rt().block_on(async {
            self.store.set_ref(&name, h1).await.unwrap();
            self.store.set_ref(&name, h2).await.unwrap();
            let got = self.store.get_ref(&name).await.unwrap();
            assert_eq!(got, Some(h2));
        });
    }

    /// B9: Adversarial ref names with SQL LIKE wildcards (%, _).
    /// Tests that `list_refs` prefix matching is exact, not LIKE-based.
    ///
    /// Creates a ref with % in its name AND a decoy ref that `SQLite` LIKE
    /// would match (because % acts as wildcard) but `starts_with` would not.
    /// If the store uses LIKE for prefix matching, it will return the decoy
    /// as a false positive.
    pub fn prop_list_refs_adversarial(
        &self,
        scenario: prop_strategies::AdversarialRefScenario,
        hash: ContentHash,
    ) {
        rt().block_on(async {
            let adv = &scenario.adversarial_name;
            let decoy = &scenario.decoy_name;

            // Store both the adversarial ref and the decoy
            self.store.set_ref(adv, hash).await.unwrap();
            self.store.set_ref(decoy, hash).await.unwrap();

            // get_ref should work for the adversarial name
            let got = self.store.get_ref(adv).await.unwrap();
            assert_eq!(got, Some(hash), "get_ref failed for adversarial name {adv:?}");

            // list_refs with adversarial name as prefix:
            // The decoy does NOT start with the adversarial name,
            // but LIKE would match it because % is a wildcard.
            let found = self.store.list_refs(adv).await.unwrap();

            // Every result must genuinely start with the prefix
            for (name, _) in &found {
                assert!(
                    name.starts_with(adv),
                    "list_refs({adv:?}) returned {name:?} which doesn't start with prefix \
                     (LIKE wildcard false positive!)"
                );
            }

            // The adversarial ref itself must be found
            assert!(
                found.iter().any(|(n, _)| n == adv),
                "list_refs should find the adversarial ref itself"
            );
        });
    }

    /// B10: Two sequential CAS operations compose correctly.
    pub fn prop_cas_linearizability(&self, name: String, h1: ContentHash, h2: ContentHash) {
        rt().block_on(async {
            let ok1 = self.store.cas_ref(&name, None, h1).await.unwrap();
            assert!(ok1, "first cas_ref (create) should succeed");

            let ok2 = self.store.cas_ref(&name, Some(h1), h2).await.unwrap();
            assert!(ok2, "second cas_ref (swap) should succeed");

            let got = self.store.get_ref(&name).await.unwrap();
            assert_eq!(got, Some(h2));
        });
    }

    // ── Group F: Model-based testing ─────────────────────────────────

    /// F1: Apply a sequence of operations to both a real store and a model,
    /// then verify observable state matches after every commit/rollback.
    pub fn prop_model_consistency(&self, ops: Vec<StoreOp>) {
        rt().block_on(async {
            let mut model = StoreModel::new();
            let mut tx: Option<Box<dyn super::Transaction>> = None;

            for op in &ops {
                match op {
                    StoreOp::Put(hash, object) => {
                        if tx.is_none() {
                            tx = Some(self.store.transaction().await.unwrap());
                        }
                        if let Some(ref mut t) = tx {
                            t.put(*hash, object.clone()).await.unwrap();
                            model.put(*hash, object.clone());
                        }
                    }
                    StoreOp::CommitTx => {
                        if let Some(mut t) = tx.take() {
                            t.commit().await.unwrap();
                            model.commit();
                            self.verify_model(&model).await;
                        }
                    }
                    StoreOp::RollbackTx => {
                        if let Some(mut t) = tx.take() {
                            t.rollback().await.unwrap();
                            model.rollback();
                            self.verify_model(&model).await;
                        }
                    }
                    StoreOp::SetRef(name, hash) => {
                        self.store.set_ref(name, *hash).await.unwrap();
                        model.set_ref(name.clone(), *hash);
                    }
                    StoreOp::DeleteRef(name) => {
                        self.store.delete_ref(name).await.unwrap();
                        model.delete_ref(name);
                    }
                    StoreOp::CasRef(name, expected, new) => {
                        let real_ok = self
                            .store
                            .cas_ref(name, *expected, *new)
                            .await
                            .unwrap();
                        let model_ok = model.cas_ref(name.clone(), *expected, *new);
                        assert_eq!(
                            real_ok, model_ok,
                            "cas_ref result mismatch for {name}"
                        );
                    }
                    StoreOp::ListRefs(prefix) => {
                        let mut real = self.store.list_refs(prefix).await.unwrap();
                        real.sort_by(|a, b| a.0.cmp(&b.0));
                        let mut expected = model.list_refs(prefix);
                        expected.sort_by(|a, b| a.0.cmp(&b.0));
                        assert_eq!(
                            real, expected,
                            "list_refs({prefix:?}) mismatch"
                        );
                    }
                }
            }

            // Clean up any leftover open transaction.
            if let Some(mut t) = tx.take() {
                t.rollback().await.unwrap();
            }
        });
    }

    /// Compare committed objects, refs, and inclusive hash index against model.
    async fn verify_model(&self, model: &StoreModel) {
        // Check up to 5 committed objects.
        for (i, (hash, expected)) in model.committed_objects.iter().enumerate() {
            if i >= 5 {
                break;
            }
            let got = self.store.get(hash).await.unwrap();
            assert_eq!(got.as_ref(), Some(expected), "object mismatch for {hash}");
        }

        // Check all refs.
        for (name, expected_hash) in &model.committed_refs {
            let got = self.store.get_ref(name).await.unwrap();
            assert_eq!(got, Some(*expected_hash), "ref mismatch for {name}");
        }

        // Check inclusive hash index for committed elements.
        for (inclusive, identity) in &model.inclusive_index {
            let result = self.store.get_by_inclusive_hash(inclusive).await.unwrap();
            assert!(result.is_some(), "inclusive hash {inclusive} should be indexed");
            let (found_id, _) = result.unwrap();
            assert_eq!(found_id, *identity);
        }
    }
}

/// In-memory model of the store for model-based testing.
struct StoreModel {
    committed_objects: HashMap<ContentHash, Object>,
    committed_refs: HashMap<String, ContentHash>,
    pending: Vec<(ContentHash, Object)>,
    inclusive_index: HashMap<ContentHash, ContentHash>,
}

impl StoreModel {
    fn new() -> Self {
        Self {
            committed_objects: HashMap::new(),
            committed_refs: HashMap::new(),
            pending: Vec::new(),
            inclusive_index: HashMap::new(),
        }
    }

    fn put(&mut self, hash: ContentHash, object: Object) {
        self.pending.push((hash, object));
    }

    fn commit(&mut self) {
        for (hash, obj) in self.pending.drain(..) {
            if let Object::Element(ref elem) = obj {
                self.inclusive_index.insert(elem.inclusive_hash, hash);
            }
            self.committed_objects.insert(hash, obj);
        }
    }

    fn rollback(&mut self) {
        self.pending.clear();
    }

    fn set_ref(&mut self, name: String, hash: ContentHash) {
        self.committed_refs.insert(name, hash);
    }

    fn delete_ref(&mut self, name: &str) {
        self.committed_refs.remove(name);
    }

    fn cas_ref(
        &mut self,
        name: String,
        expected: Option<ContentHash>,
        new: ContentHash,
    ) -> bool {
        let current = self.committed_refs.get(&name).copied();
        if current == expected {
            self.committed_refs.insert(name, new);
            true
        } else {
            false
        }
    }

    fn list_refs(&self, prefix: &str) -> Vec<(String, ContentHash)> {
        self.committed_refs
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    }
}

/// Generate `proptest!` functions that delegate to `PropStoreTester` methods.
macro_rules! prop_store_tests {
    ($create:expr) => {
        use proptest::prelude::*;
        use crate::store::prop_tests::PropStoreTester;
        use crate::store::prop_strategies;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            #[test]
            fn prop_object_roundtrip(obj in prop_strategies::arb_object()) {
                PropStoreTester { store: $create }.prop_object_roundtrip(obj);
            }

            #[test]
            fn prop_idempotent_put(obj in prop_strategies::arb_object()) {
                PropStoreTester { store: $create }.prop_idempotent_put(obj);
            }

            #[test]
            fn prop_contains_after_commit(obj in prop_strategies::arb_object()) {
                PropStoreTester { store: $create }.prop_contains_after_commit(obj);
            }

            #[test]
            fn prop_rollback_isolation(obj in prop_strategies::arb_object()) {
                PropStoreTester { store: $create }.prop_rollback_isolation(obj);
            }

            #[test]
            fn prop_inclusive_hash_index(
                hash in prop_strategies::arb_content_hash(),
                elem in prop_strategies::arb_element_object(),
            ) {
                PropStoreTester { store: $create }.prop_inclusive_hash_index(hash, elem);
            }

            #[test]
            fn prop_get_nonexistent(hash in prop_strategies::arb_content_hash()) {
                PropStoreTester { store: $create }.prop_get_nonexistent(hash);
            }

            #[test]
            fn prop_transaction_atomicity(
                objects in prop::collection::vec(prop_strategies::arb_object(), 2..=8),
            ) {
                PropStoreTester { store: $create }.prop_transaction_atomicity(objects);
            }

            #[test]
            fn prop_subtree_completeness(
                (dag, root) in prop_strategies::arb_object_dag()
            ) {
                PropStoreTester { store: $create }.prop_subtree_completeness(dag, root);
            }

            #[test]
            fn prop_subtree_deduplication(content in "[a-zA-Z0-9]{1,20}") {
                PropStoreTester { store: $create }.prop_subtree_deduplication(content);
            }

            #[test]
            fn prop_subtree_missing_object(missing in prop_strategies::arb_content_hash()) {
                PropStoreTester { store: $create }.prop_subtree_missing_object(missing);
            }

            #[test]
            fn prop_ref_roundtrip(
                name in prop_strategies::arb_ref_name(),
                hash in prop_strategies::arb_content_hash(),
            ) {
                PropStoreTester { store: $create }.prop_ref_roundtrip(name, hash);
            }

            #[test]
            fn prop_ref_delete(
                name in prop_strategies::arb_ref_name(),
                hash in prop_strategies::arb_content_hash(),
            ) {
                PropStoreTester { store: $create }.prop_ref_delete(name, hash);
            }

            #[test]
            fn prop_cas_create(
                name in prop_strategies::arb_ref_name(),
                hash in prop_strategies::arb_content_hash(),
            ) {
                PropStoreTester { store: $create }.prop_cas_create(name, hash);
            }

            #[test]
            fn prop_cas_swap(
                name in prop_strategies::arb_ref_name(),
                h1 in prop_strategies::arb_content_hash(),
                h2 in prop_strategies::arb_content_hash(),
            ) {
                PropStoreTester { store: $create }.prop_cas_swap(name, h1, h2);
            }

            #[test]
            fn prop_cas_reject(
                name in prop_strategies::arb_ref_name(),
                h1 in prop_strategies::arb_content_hash(),
                h_wrong in prop_strategies::arb_content_hash(),
                h2 in prop_strategies::arb_content_hash(),
            ) {
                PropStoreTester { store: $create }.prop_cas_reject(name, h1, h_wrong, h2);
            }

            #[test]
            fn prop_list_refs_prefix(
                head_suffixes in prop::collection::vec("[a-z]{1,8}", 2..=4),
                tag_suffixes in prop::collection::vec("[a-z]{1,8}", 1..=3),
                hash in prop_strategies::arb_content_hash(),
            ) {
                PropStoreTester { store: $create }.prop_list_refs_prefix(head_suffixes, tag_suffixes, hash);
            }

            #[test]
            fn prop_ref_independence(
                name_a in prop_strategies::arb_ref_name(),
                name_b in prop_strategies::arb_ref_name(),
                h_a in prop_strategies::arb_content_hash(),
                h_b in prop_strategies::arb_content_hash(),
            ) {
                PropStoreTester { store: $create }.prop_ref_independence(name_a, name_b, h_a, h_b);
            }

            #[test]
            fn prop_ref_overwrite(
                name in prop_strategies::arb_ref_name(),
                h1 in prop_strategies::arb_content_hash(),
                h2 in prop_strategies::arb_content_hash(),
            ) {
                PropStoreTester { store: $create }.prop_ref_overwrite(name, h1, h2);
            }

            #[test]
            fn prop_list_refs_adversarial(
                scenario in prop_strategies::arb_adversarial_ref_scenario(),
                hash in prop_strategies::arb_content_hash(),
            ) {
                PropStoreTester { store: $create }.prop_list_refs_adversarial(scenario, hash);
            }

            #[test]
            fn prop_cas_linearizability(
                name in prop_strategies::arb_ref_name(),
                h1 in prop_strategies::arb_content_hash(),
                h2 in prop_strategies::arb_content_hash(),
            ) {
                PropStoreTester { store: $create }.prop_cas_linearizability(name, h1, h2);
            }

            #[test]
            fn prop_model_consistency(ops in prop_strategies::arb_op_sequence()) {
                PropStoreTester { store: $create }.prop_model_consistency(ops);
            }
        }
    };
}
pub(crate) use prop_store_tests;

// ---------------------------------------------------------------------------
// F2: Store equivalence -- standalone test comparing MemoryStore and SqliteStore
// ---------------------------------------------------------------------------

#[cfg(feature = "sqlite")]
mod equivalence {
    use proptest::prelude::*;

    use crate::store::memory::MemoryStore;
    use crate::store::sqlite::SqliteStore;
    use crate::store::prop_strategies::{self, StoreOp};
    use crate::store::{ObjectStore, RefStore};
    use crate::object::Object;

    /// Apply an operation sequence to a store, returning the set of hashes put
    /// and ref names touched.
    async fn apply_ops(
        store: &(impl ObjectStore + RefStore),
        ops: &[StoreOp],
    ) -> (Vec<clayers_xml::ContentHash>, Vec<String>) {
        let mut all_hashes = Vec::new();
        let mut all_ref_names = Vec::new();
        let mut tx: Option<Box<dyn crate::store::Transaction>> = None;

        for op in ops {
            match op {
                StoreOp::Put(hash, object) => {
                    if tx.is_none() {
                        tx = Some(store.transaction().await.unwrap());
                    }
                    if let Some(ref mut t) = tx {
                        t.put(*hash, object.clone()).await.unwrap();
                        all_hashes.push(*hash);
                    }
                }
                StoreOp::CommitTx => {
                    if let Some(mut t) = tx.take() {
                        t.commit().await.unwrap();
                    }
                }
                StoreOp::RollbackTx => {
                    if let Some(mut t) = tx.take() {
                        t.rollback().await.unwrap();
                    }
                }
                StoreOp::SetRef(name, hash) => {
                    store.set_ref(name, *hash).await.unwrap();
                    all_ref_names.push(name.clone());
                }
                StoreOp::DeleteRef(name) => {
                    store.delete_ref(name).await.unwrap();
                    all_ref_names.push(name.clone());
                }
                StoreOp::CasRef(name, expected, new) => {
                    let _ = store.cas_ref(name, *expected, *new).await.unwrap();
                    all_ref_names.push(name.clone());
                }
                StoreOp::ListRefs(_) => {
                    // ListRefs is a read-only operation; we verify equivalence
                    // separately after applying all ops.
                }
            }
        }
        // Clean up open tx
        if let Some(mut t) = tx.take() {
            t.rollback().await.unwrap();
        }
        (all_hashes, all_ref_names)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// F2: Run the same operation sequence against MemoryStore and SqliteStore,
        /// then verify ALL observable state matches: get, contains, get_ref, list_refs,
        /// get_by_inclusive_hash.
        #[test]
        fn prop_store_equivalence(ops in prop_strategies::arb_op_sequence()) {
            let rt = prop_strategies::runtime();
            rt.block_on(async {
                let mem = MemoryStore::new();
                let sql = SqliteStore::open_in_memory().unwrap();

                let (hashes, ref_names) = apply_ops(&mem, &ops).await;
                apply_ops(&sql, &ops).await;

                // Compare get() for every hash that was ever put
                for hash in &hashes {
                    let mem_obj = mem.get(hash).await.unwrap();
                    let sql_obj = sql.get(hash).await.unwrap();
                    assert_eq!(mem_obj, sql_obj, "get mismatch for {hash}");
                }

                // Compare contains() for every hash
                for hash in &hashes {
                    let mem_has = mem.contains(hash).await.unwrap();
                    let sql_has = sql.contains(hash).await.unwrap();
                    assert_eq!(mem_has, sql_has, "contains mismatch for {hash}");
                }

                // Compare get_ref() for every ref name touched
                for name in &ref_names {
                    let mem_ref = mem.get_ref(name).await.unwrap();
                    let sql_ref = sql.get_ref(name).await.unwrap();
                    assert_eq!(mem_ref, sql_ref, "get_ref mismatch for {name}");
                }

                // Compare list_refs("") -- all refs
                let mut mem_refs = mem.list_refs("").await.unwrap();
                let mut sql_refs = sql.list_refs("").await.unwrap();
                mem_refs.sort_by(|a, b| a.0.cmp(&b.0));
                sql_refs.sort_by(|a, b| a.0.cmp(&b.0));
                assert_eq!(mem_refs, sql_refs, "list_refs mismatch");

                // Compare get_by_inclusive_hash for element objects
                for hash in &hashes {
                    if let Some(Object::Element(elem)) = mem.get(hash).await.unwrap() {
                        let mem_incl = mem.get_by_inclusive_hash(&elem.inclusive_hash).await.unwrap();
                        let sql_incl = sql.get_by_inclusive_hash(&elem.inclusive_hash).await.unwrap();
                        assert_eq!(
                            mem_incl.is_some(), sql_incl.is_some(),
                            "inclusive hash lookup mismatch for {hash}"
                        );
                        if let (Some((mh, _)), Some((sh, _))) = (mem_incl, sql_incl) {
                            assert_eq!(mh, sh, "inclusive hash identity mismatch");
                        }
                    }
                }
            });
        }
    }
}
