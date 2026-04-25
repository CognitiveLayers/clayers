//! Concurrency tests for store backends.
//!
//! `ConcurrencyTester<S>` exercises `ObjectStore + RefStore` under
//! contention by spawning multiple tokio tasks that share a single
//! `Arc<S>` and race on overlapping keys/refs. Each backend's test
//! module invokes `concurrency_tests!` and `prop_concurrency_tests!`
//! with a constructor.
//!
//! ## Property test sanity
//!
//! Concurrent property cases are heavier than sequential ones: each
//! case spawns N tasks against a fresh store. To keep total runtime
//! bounded we use:
//!
//! - **Reduced case counts** (32-64 instead of 256)
//! - **Bounded task counts** (4-8)
//! - **Deterministic assertions** that account for non-determinism
//!   (e.g., "exactly one CAS won", not "task k won")

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use clayers_xml::ContentHash;
use tokio::task::JoinSet;

use super::{ObjectStore, RefStore};
use crate::object::{Object, TextObject};

/// Generate `#[tokio::test(flavor = "multi_thread")]` functions delegating
/// to `ConcurrencyTester` methods.
#[cfg(test)]
macro_rules! concurrency_tests {
    ($create:expr) => {
        use crate::store::concurrency_tests::ConcurrencyTester;

        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_cas_create_one_winner() {
            ConcurrencyTester::new($create).test_cas_create_one_winner().await;
        }
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_cas_swap_one_winner() {
            ConcurrencyTester::new($create).test_cas_swap_one_winner().await;
        }
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_set_ref_final_in_inputs() {
            ConcurrencyTester::new($create).test_set_ref_final_in_inputs().await;
        }
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_put_idempotent_same_object() {
            ConcurrencyTester::new($create).test_put_idempotent_same_object().await;
        }
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_put_distinct_all_visible() {
            ConcurrencyTester::new($create).test_put_distinct_all_visible().await;
        }
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_transactions_both_visible() {
            ConcurrencyTester::new($create).test_transactions_both_visible().await;
        }
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_subtree_readers_consistent() {
            ConcurrencyTester::new($create).test_subtree_readers_consistent().await;
        }
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_independent_refs() {
            ConcurrencyTester::new($create).test_independent_refs().await;
        }
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_reader_during_writer() {
            ConcurrencyTester::new($create).test_reader_during_writer().await;
        }
    };
}

#[cfg(test)]
pub(crate) use concurrency_tests;

/// Tester for concurrency invariants on a shared `Arc<S>`.
///
/// `S` must be `Send + Sync + 'static` so spawned tokio tasks can hold
/// `Arc<S>` clones across `.await` points.
pub struct ConcurrencyTester<S: ObjectStore + RefStore + Send + Sync + 'static> {
    pub store: Arc<S>,
}

impl<S: ObjectStore + RefStore + Send + Sync + 'static> ConcurrencyTester<S> {
    pub fn new(store: S) -> Self {
        Self { store: Arc::new(store) }
    }

    fn text_obj(s: &str) -> Object {
        Object::Text(TextObject { content: s.to_string() })
    }

    /// N tasks race `cas_ref(name, None, hash_i)`. Exactly one returns
    /// true; the final value is the winner's hash.
    pub async fn test_cas_create_one_winner(self) {
        const N: usize = 8;
        let name = "refs/heads/cas_create_race";

        let hashes: Vec<ContentHash> = (0..N)
            .map(|i| ContentHash::from_canonical(format!("cas_create_{i}").as_bytes()))
            .collect();

        let mut set = JoinSet::new();
        for h in &hashes {
            let store = Arc::clone(&self.store);
            let h = *h;
            set.spawn(async move {
                let won = store.cas_ref(name, None, h).await.unwrap();
                (won, h)
            });
        }

        let mut winners = 0_usize;
        let mut winning_hash: Option<ContentHash> = None;
        while let Some(res) = set.join_next().await {
            let (won, h) = res.unwrap();
            if won {
                winners += 1;
                winning_hash = Some(h);
            }
        }

        assert_eq!(winners, 1, "exactly one CAS create must win");
        assert_eq!(self.store.get_ref(name).await.unwrap(), winning_hash);
    }

    /// Ref starts at H0. N tasks race `cas_ref(name, Some(H0), hash_i)`.
    /// Exactly one wins; the final value is the winner's hash.
    pub async fn test_cas_swap_one_winner(self) {
        const N: usize = 8;
        let name = "refs/heads/cas_swap_race";
        let h0 = ContentHash::from_canonical(b"cas_swap_h0");
        self.store.set_ref(name, h0).await.unwrap();

        let new_hashes: Vec<ContentHash> = (0..N)
            .map(|i| ContentHash::from_canonical(format!("cas_swap_new_{i}").as_bytes()))
            .collect();

        let mut set = JoinSet::new();
        for h in &new_hashes {
            let store = Arc::clone(&self.store);
            let h = *h;
            set.spawn(async move {
                let won = store.cas_ref(name, Some(h0), h).await.unwrap();
                (won, h)
            });
        }

        let mut winners = 0_usize;
        let mut winning_hash: Option<ContentHash> = None;
        while let Some(res) = set.join_next().await {
            let (won, h) = res.unwrap();
            if won {
                winners += 1;
                winning_hash = Some(h);
            }
        }

        assert_eq!(winners, 1, "exactly one CAS swap must win");
        assert_eq!(self.store.get_ref(name).await.unwrap(), winning_hash);
    }

    /// N tasks each `set_ref(name, hash_i)` with no synchronization. Final
    /// value must be one of the inputs (last-write-wins among some order).
    pub async fn test_set_ref_final_in_inputs(self) {
        const N: usize = 8;
        let name = "refs/heads/last_write_race";

        let hashes: Vec<ContentHash> = (0..N)
            .map(|i| ContentHash::from_canonical(format!("lww_{i}").as_bytes()))
            .collect();
        let allowed: HashSet<ContentHash> = hashes.iter().copied().collect();

        let mut set = JoinSet::new();
        for h in &hashes {
            let store = Arc::clone(&self.store);
            let h = *h;
            set.spawn(async move {
                store.set_ref(name, h).await.unwrap();
            });
        }
        while let Some(res) = set.join_next().await {
            res.unwrap();
        }

        let final_hash = self.store.get_ref(name).await.unwrap()
            .expect("ref must be set");
        assert!(
            allowed.contains(&final_hash),
            "final ref value must be one of the inputs"
        );
    }

    /// N tasks all put the same (hash, object) concurrently. None must
    /// error and the object must be readable. Note: content-addressing
    /// makes "exactly one copy" inherent to the data model — this test
    /// specifically verifies that concurrent insertion of an identical
    /// key does not crash, deadlock, or corrupt the result.
    pub async fn test_put_idempotent_same_object(self) {
        const N: usize = 8;
        let h = ContentHash::from_canonical(b"concurrent_idem");
        let obj = Self::text_obj("same");

        let mut set = JoinSet::new();
        for _ in 0..N {
            let store = Arc::clone(&self.store);
            let obj = obj.clone();
            set.spawn(async move {
                let mut tx = store.transaction().await.unwrap();
                tx.put(h, obj).await.unwrap();
                tx.commit().await.unwrap();
            });
        }
        while let Some(res) = set.join_next().await {
            res.unwrap();
        }

        let got = self.store.get(&h).await.unwrap();
        assert_eq!(got, Some(obj));
    }

    /// N tasks each put a distinct (hash, object). All N must be visible
    /// after all complete.
    pub async fn test_put_distinct_all_visible(self) {
        const N: usize = 16;

        let hashes: Vec<ContentHash> = (0..N)
            .map(|i| ContentHash::from_canonical(format!("concurrent_put_{i}").as_bytes()))
            .collect();

        let mut set = JoinSet::new();
        for (i, h) in hashes.iter().enumerate() {
            let store = Arc::clone(&self.store);
            let h = *h;
            set.spawn(async move {
                let mut tx = store.transaction().await.unwrap();
                tx.put(h, Self::text_obj(&format!("v{i}"))).await.unwrap();
                tx.commit().await.unwrap();
            });
        }
        while let Some(res) = set.join_next().await {
            res.unwrap();
        }

        for h in &hashes {
            assert!(self.store.contains(h).await.unwrap(), "object {h:?} missing");
        }
    }

    /// Two concurrent transactions on the same store. Both commit. Both
    /// of their writes are visible afterwards.
    pub async fn test_transactions_both_visible(self) {
        let h_a = ContentHash::from_canonical(b"two_tx_a");
        let h_b = ContentHash::from_canonical(b"two_tx_b");

        let store_a = Arc::clone(&self.store);
        let task_a = tokio::spawn(async move {
            let mut tx = store_a.transaction().await.unwrap();
            tx.put(h_a, Self::text_obj("a")).await.unwrap();
            tx.commit().await.unwrap();
        });

        let store_b = Arc::clone(&self.store);
        let task_b = tokio::spawn(async move {
            let mut tx = store_b.transaction().await.unwrap();
            tx.put(h_b, Self::text_obj("b")).await.unwrap();
            tx.commit().await.unwrap();
        });

        task_a.await.unwrap();
        task_b.await.unwrap();

        assert!(self.store.contains(&h_a).await.unwrap());
        assert!(self.store.contains(&h_b).await.unwrap());
    }

    /// N tasks each call subtree on the same root. All must yield the
    /// same set of (hash, object) pairs (snapshot consistency).
    pub async fn test_subtree_readers_consistent(self) {
        use tokio_stream::StreamExt;

        const READERS: usize = 8;

        // Build a small subtree.
        let text_hash = ContentHash::from_canonical(b"concurrent_sub_text");
        let elem_hash = ContentHash::from_canonical(b"concurrent_sub_elem");
        let doc_hash = ContentHash::from_canonical(b"concurrent_sub_doc");

        let mut tx = self.store.transaction().await.unwrap();
        tx.put(text_hash, Self::text_obj("hi")).await.unwrap();
        tx.put(elem_hash, Object::Element(crate::object::ElementObject {
            local_name: "r".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![text_hash],
            inclusive_hash: elem_hash,
        })).await.unwrap();
        tx.put(doc_hash, Object::Document(crate::object::DocumentObject {
            root: elem_hash,
            prologue: vec![],
        })).await.unwrap();
        tx.commit().await.unwrap();

        let mut set = JoinSet::new();
        for _ in 0..READERS {
            let store = Arc::clone(&self.store);
            set.spawn(async move {
                let pairs: Vec<ContentHash> = store
                    .subtree(&doc_hash)
                    .map(|r| r.unwrap().0)
                    .collect()
                    .await;
                let s: HashSet<ContentHash> = pairs.into_iter().collect();
                s
            });
        }

        let mut snapshots: Vec<HashSet<ContentHash>> = Vec::with_capacity(READERS);
        while let Some(res) = set.join_next().await {
            snapshots.push(res.unwrap());
        }

        let first = snapshots.remove(0);
        for snap in &snapshots {
            assert_eq!(snap, &first, "concurrent subtree readers must agree");
        }
        assert_eq!(first.len(), 3, "subtree must yield doc + elem + text");
    }

    /// N tasks each operate on a disjoint ref name. Final state has all
    /// N refs set to their respective values; no cross-contamination.
    pub async fn test_independent_refs(self) {
        const N: usize = 16;

        let mut set = JoinSet::new();
        for i in 0..N {
            let store = Arc::clone(&self.store);
            set.spawn(async move {
                let name = format!("refs/heads/independent_{i}");
                let h = ContentHash::from_canonical(format!("indep_{i}").as_bytes());
                store.set_ref(&name, h).await.unwrap();
                (name, h)
            });
        }

        let mut expected: Vec<(String, ContentHash)> = Vec::with_capacity(N);
        while let Some(res) = set.join_next().await {
            expected.push(res.unwrap());
        }

        for (name, expected_hash) in &expected {
            let got = self.store.get_ref(name).await.unwrap();
            assert_eq!(got, Some(*expected_hash), "ref {name} corrupted");
        }
    }

    /// While one task does `set_ref` in a hot loop, readers calling
    /// `get_ref` must (a) only ever observe a value the writer has set,
    /// and (b) eventually observe at least one value *other than* the
    /// seed — proving cross-task visibility actually works. Without
    /// the second invariant, a backend whose writes are invisible to
    /// other tasks would pass by always returning the seed.
    pub async fn test_reader_during_writer(self) {
        const ITERATIONS: usize = 50;
        const READERS: usize = 4;
        let name = "refs/heads/reader_during_writer";

        // Seed is a *distinct* hash not in the writer's iteration set.
        // This way "saw something other than the seed" proves the
        // reader observed a writer-published value.
        let seed = ContentHash::from_canonical(b"rdw_seed");
        let hashes: Vec<ContentHash> = (0..ITERATIONS)
            .map(|i| ContentHash::from_canonical(format!("rdw_{i}").as_bytes()))
            .collect();
        let mut allowed: HashSet<ContentHash> = hashes.iter().copied().collect();
        allowed.insert(seed);

        self.store.set_ref(name, seed).await.unwrap();

        let stop = Arc::new(AtomicUsize::new(0));
        let saw_writer_value = Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Spawn readers
        let mut readers = JoinSet::new();
        for _ in 0..READERS {
            let store = Arc::clone(&self.store);
            let stop = Arc::clone(&stop);
            let allowed = allowed.clone();
            let saw_writer = Arc::clone(&saw_writer_value);
            readers.spawn(async move {
                while stop.load(Ordering::Relaxed) == 0 {
                    if let Some(h) = store.get_ref(name).await.unwrap() {
                        assert!(allowed.contains(&h), "torn read: {h:?}");
                        if h != seed {
                            saw_writer.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    tokio::task::yield_now().await;
                }
            });
        }

        // Writer
        let writer_store = Arc::clone(&self.store);
        let writer_hashes = hashes.clone();
        let writer = tokio::spawn(async move {
            for h in writer_hashes {
                writer_store.set_ref(name, h).await.unwrap();
                tokio::task::yield_now().await;
            }
        });

        writer.await.unwrap();

        // Give readers one more tick to observe the final write.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        stop.store(1, Ordering::Relaxed);
        while let Some(r) = readers.join_next().await {
            r.unwrap();
        }

        assert!(
            saw_writer_value.load(std::sync::atomic::Ordering::Relaxed),
            "readers never observed any writer-published value — \
             cross-task visibility may be broken",
        );
    }
}

// ---------------------------------------------------------------------------
// Property-based concurrency tests
// ---------------------------------------------------------------------------

/// Generate proptest functions for concurrent invariants. Cases counts
/// are reduced (default 32) because each case spawns N tasks against a
/// fresh store.
#[cfg(test)]
macro_rules! prop_concurrency_tests {
    ($create:expr) => {
        use proptest::prelude::*;
        #[allow(unused_imports)]
        use crate::store::{ObjectStore, RefStore};

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(32))]

            /// Property: when N tasks each attempt CAS-create on the same
            /// ref with distinct hashes, exactly one wins.
            #[test]
            fn prop_concurrent_cas_create_unique_winner(
                hashes in prop::collection::vec(
                    crate::store::prop_strategies::arb_content_hash(),
                    2..=8,
                ).prop_filter(
                    "hashes must be unique",
                    |hs| hs.iter().collect::<std::collections::HashSet<_>>().len() == hs.len(),
                ),
            ) {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(4)
                    .enable_all()
                    .build()
                    .unwrap();
                let store = std::sync::Arc::new($create);
                rt.block_on(async {
                    let name = "refs/heads/prop_cas_create";
                    let mut set = tokio::task::JoinSet::new();
                    for h in &hashes {
                        let store = std::sync::Arc::clone(&store);
                        let h = *h;
                        set.spawn(async move {
                            store.cas_ref(name, None, h).await.unwrap()
                        });
                    }
                    let mut wins = 0;
                    while let Some(r) = set.join_next().await {
                        if r.unwrap() {
                            wins += 1;
                        }
                    }
                    assert_eq!(wins, 1, "exactly one CAS create must win");
                });
            }

            /// Property: N tasks racing `set_ref` leave the ref in some
            /// state that is one of the inputs.
            #[test]
            fn prop_concurrent_set_ref_final_in_inputs(
                hashes in prop::collection::vec(
                    crate::store::prop_strategies::arb_content_hash(),
                    2..=8,
                ),
            ) {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(4)
                    .enable_all()
                    .build()
                    .unwrap();
                let store = std::sync::Arc::new($create);
                rt.block_on(async {
                    let name = "refs/heads/prop_set_race";
                    let allowed: std::collections::HashSet<_> = hashes.iter().copied().collect();
                    let mut set = tokio::task::JoinSet::new();
                    for h in &hashes {
                        let store = std::sync::Arc::clone(&store);
                        let h = *h;
                        set.spawn(async move {
                            store.set_ref(name, h).await.unwrap();
                        });
                    }
                    while let Some(r) = set.join_next().await {
                        r.unwrap();
                    }
                    let final_h = store.get_ref(name).await.unwrap().unwrap();
                    assert!(allowed.contains(&final_h));
                });
            }

            /// Property: N tasks each putting their own distinct object
            /// produce a store that contains all N.
            #[test]
            fn prop_concurrent_put_distinct_all_visible(
                count in 2usize..=8,
            ) {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(4)
                    .enable_all()
                    .build()
                    .unwrap();
                let store = std::sync::Arc::new($create);
                rt.block_on(async {
                    let hashes: Vec<_> = (0..count)
                        .map(|i| clayers_xml::ContentHash::from_canonical(
                            format!("prop_put_distinct_{i}").as_bytes()
                        ))
                        .collect();
                    let mut set = tokio::task::JoinSet::new();
                    for (i, h) in hashes.iter().enumerate() {
                        let store = std::sync::Arc::clone(&store);
                        let h = *h;
                        set.spawn(async move {
                            let mut tx = store.transaction().await.unwrap();
                            tx.put(h, crate::object::Object::Text(
                                crate::object::TextObject { content: format!("v{i}") }
                            )).await.unwrap();
                            tx.commit().await.unwrap();
                        });
                    }
                    while let Some(r) = set.join_next().await {
                        r.unwrap();
                    }
                    for h in &hashes {
                        assert!(store.contains(h).await.unwrap());
                    }
                });
            }
        }
    };
}

#[cfg(test)]
pub(crate) use prop_concurrency_tests;
