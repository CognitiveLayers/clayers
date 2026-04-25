//! Store compliance runner: exercises the deterministic, property-based, and
//! query test suites against a Python store via `PyStore`.
//!
//! Gated behind `#[cfg(feature = "compliance")]`.

use std::panic::AssertUnwindSafe;

use pyo3::prelude::*;

use crate::repo::py_store::PyStore;

// ---------------------------------------------------------------------------
// ComplianceResult
// ---------------------------------------------------------------------------

#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct ComplianceResult {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub category: String,
    #[pyo3(get)]
    pub passed: bool,
    #[pyo3(get)]
    pub error: Option<String>,
}

#[pymethods]
impl ComplianceResult {
    fn __repr__(&self) -> String {
        if self.passed {
            format!("ComplianceResult('{}', '{}', PASSED)", self.category, self.name)
        } else {
            format!(
                "ComplianceResult('{}', '{}', FAILED: {})",
                self.category,
                self.name,
                self.error.as_deref().unwrap_or("unknown")
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_panic(e: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = e.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = e.downcast_ref::<&str>() {
        (*s).to_string()
    } else {
        "unknown panic".to_string()
    }
}

fn run_test(
    name: &str,
    category: &str,
    f: impl FnOnce() + std::panic::UnwindSafe,
) -> ComplianceResult {
    match std::panic::catch_unwind(f) {
        Ok(()) => ComplianceResult {
            name: name.into(),
            category: category.into(),
            passed: true,
            error: None,
        },
        Err(e) => ComplianceResult {
            name: name.into(),
            category: category.into(),
            passed: false,
            error: Some(format_panic(e)),
        },
    }
}

/// Create a fresh `PyStore` from the Python factory, releasing the GIL
/// after construction so that test methods can re-acquire it.
fn create_store(factory: &Py<PyAny>) -> PyResult<PyStore> {
    Python::attach(|py| {
        let store_obj = factory.call0(py)?;
        Ok(PyStore::new(store_obj))
    })
}

// ---------------------------------------------------------------------------
// Deterministic store tests
// ---------------------------------------------------------------------------

fn run_deterministic_tests(
    factory: &Py<PyAny>,
    results: &mut Vec<ComplianceResult>,
) {
    use clayers_repo::store::tests::StoreTester;

    macro_rules! det_test {
        ($name:ident) => {{
            let store = match create_store(factory) {
                Ok(s) => s,
                Err(e) => {
                    results.push(ComplianceResult {
                        name: stringify!($name).into(),
                        category: "deterministic".into(),
                        passed: false,
                        error: Some(format!("factory error: {e}")),
                    });
                    return;
                }
            };
            // GIL is released here.
            let result = run_test(
                stringify!($name),
                "deterministic",
                AssertUnwindSafe(|| {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(StoreTester { store }.  $name());
                }),
            );
            results.push(result);
        }};
    }

    det_test!(test_put_and_get);
    det_test!(test_contains_after_commit);
    det_test!(test_rollback_discards);
    det_test!(test_inclusive_hash_index);
    det_test!(test_cas_ref_create_if_absent);
    det_test!(test_cas_ref_swap);
    det_test!(test_cas_ref_reject_mismatch);
    det_test!(test_list_refs_with_prefix);
    det_test!(test_delete_ref);
    det_test!(test_roundtrip_all_object_types);
    det_test!(test_subtree_document);
    det_test!(test_subtree_commit);
    det_test!(test_subtree_diamond_dag);
    det_test!(test_subtree_tag);
    det_test!(test_subtree_mixed_content);
    det_test!(test_subtree_missing_object);
    det_test!(test_subtree_empty_element);
    det_test!(test_subtree_nonexistent_root);
    det_test!(test_subtree_tree);
    det_test!(test_subtree_tree_shared_elements);

    // ── Transaction lifecycle edges (Cat B) ──────────────────────────
    det_test!(test_tx_empty_commit);
    det_test!(test_tx_drop_without_commit);
    det_test!(test_tx_two_independent);
    det_test!(test_tx_many_puts);
    det_test!(test_tx_put_idempotent_within);
    det_test!(test_tx_rollback_then_new_tx);
    det_test!(test_tx_visibility_only_after_commit);
    det_test!(test_tx_consumed_after_commit);
    det_test!(test_tx_consumed_after_rollback);
    det_test!(test_tx_double_commit_errors);
    det_test!(test_tx_double_rollback_errors);
    det_test!(test_tx_commit_after_rollback_errors);

    // ── Ref name pathology (Cat C) ──────────────────────────────────
    det_test!(test_ref_unicode_name);
    det_test!(test_ref_long_name);
    det_test!(test_ref_special_chars_name);
    det_test!(test_ref_prefix_overlap);
    det_test!(test_ref_list_empty_prefix_returns_all);
    det_test!(test_ref_list_no_match_returns_empty);
    det_test!(test_ref_set_to_unstored_hash);
    det_test!(test_ref_delete_nonexistent_is_noop);
    det_test!(test_cas_with_same_expected_and_new);

    // ── Object content variants (Cat D) ─────────────────────────────
    det_test!(test_commit_octopus_merge);
    det_test!(test_element_extra_namespaces);
    det_test!(test_document_multi_pi_prologue);
    det_test!(test_tag_chain);
    det_test!(test_text_empty);
    det_test!(test_text_large);
    det_test!(test_comment_with_newlines);
    det_test!(test_pi_no_data);
    det_test!(test_element_zero_children);

    // ── Subtree consumer behavior (Cat E) ───────────────────────────
    det_test!(test_subtree_deep_chain);
    det_test!(test_subtree_wide_element);
    det_test!(test_subtree_consumer_drop_safe);
    det_test!(test_subtree_take_one_then_continue);
}

// ---------------------------------------------------------------------------
// Property-based store tests
// ---------------------------------------------------------------------------

fn run_property_tests(
    factory: &Py<PyAny>,
    results: &mut Vec<ComplianceResult>,
) {
    use clayers_repo::store::prop_strategies;
    use clayers_repo::store::prop_tests::PropStoreTester;
    use proptest::test_runner::{Config, TestRunner};

    let config = Config::with_cases(256);

    /// Helper: run a proptest with a single strategy. Creates a fresh store
    /// per test case.
    macro_rules! prop_test {
        ($name:ident, $strategy:expr, $call:ident) => {{
            let factory_ref = factory;
            let mut runner = TestRunner::new(config.clone());
            let run_result = runner.run(&$strategy, |val| {
                let store = create_store(factory_ref)
                    .map_err(|e| {
                        proptest::test_runner::TestCaseError::fail(format!("factory error: {e}"))
                    })?;
                PropStoreTester { store }.$call(val);
                Ok(())
            });
            results.push(match run_result {
                Ok(()) => ComplianceResult {
                    name: stringify!($name).into(),
                    category: "property".into(),
                    passed: true,
                    error: None,
                },
                Err(e) => ComplianceResult {
                    name: stringify!($name).into(),
                    category: "property".into(),
                    passed: false,
                    error: Some(e.to_string()),
                },
            });
        }};
    }

    // A1: object round-trip
    prop_test!(prop_object_roundtrip, prop_strategies::arb_object(), prop_object_roundtrip);

    // A2: idempotent put
    prop_test!(prop_idempotent_put, prop_strategies::arb_object(), prop_idempotent_put);

    // A3: contains after commit
    prop_test!(prop_contains_after_commit, prop_strategies::arb_object(), prop_contains_after_commit);

    // A4: rollback isolation
    prop_test!(prop_rollback_isolation, prop_strategies::arb_object(), prop_rollback_isolation);

    // A5: inclusive hash index
    {
        let factory_ref = factory;
        let mut runner = TestRunner::new(config.clone());
        let strategy = (
            prop_strategies::arb_content_hash(),
            prop_strategies::arb_element_object(),
        );
        let run_result = runner.run(&strategy, |(hash, elem)| {
            let store = create_store(factory_ref).map_err(|e| {
                proptest::test_runner::TestCaseError::fail(format!("factory error: {e}"))
            })?;
            PropStoreTester { store }.prop_inclusive_hash_index(hash, elem);
            Ok(())
        });
        results.push(match run_result {
            Ok(()) => ComplianceResult {
                name: "prop_inclusive_hash_index".into(),
                category: "property".into(),
                passed: true,
                error: None,
            },
            Err(e) => ComplianceResult {
                name: "prop_inclusive_hash_index".into(),
                category: "property".into(),
                passed: false,
                error: Some(e.to_string()),
            },
        });
    }

    // A6: get nonexistent
    prop_test!(prop_get_nonexistent, prop_strategies::arb_content_hash(), prop_get_nonexistent);

    // A7: transaction atomicity
    {
        let factory_ref = factory;
        let mut runner = TestRunner::new(config.clone());
        let strategy = proptest::collection::vec(prop_strategies::arb_object(), 2..=8);
        let run_result = runner.run(&strategy, |objects| {
            let store = create_store(factory_ref).map_err(|e| {
                proptest::test_runner::TestCaseError::fail(format!("factory error: {e}"))
            })?;
            PropStoreTester { store }.prop_transaction_atomicity(objects);
            Ok(())
        });
        results.push(match run_result {
            Ok(()) => ComplianceResult {
                name: "prop_transaction_atomicity".into(),
                category: "property".into(),
                passed: true,
                error: None,
            },
            Err(e) => ComplianceResult {
                name: "prop_transaction_atomicity".into(),
                category: "property".into(),
                passed: false,
                error: Some(e.to_string()),
            },
        });
    }

    // A8: subtree completeness
    {
        let factory_ref = factory;
        let mut runner = TestRunner::new(config.clone());
        let strategy = prop_strategies::arb_object_dag();
        let run_result = runner.run(&strategy, |(dag, root)| {
            let store = create_store(factory_ref).map_err(|e| {
                proptest::test_runner::TestCaseError::fail(format!("factory error: {e}"))
            })?;
            PropStoreTester { store }.prop_subtree_completeness(dag, root);
            Ok(())
        });
        results.push(match run_result {
            Ok(()) => ComplianceResult {
                name: "prop_subtree_completeness".into(),
                category: "property".into(),
                passed: true,
                error: None,
            },
            Err(e) => ComplianceResult {
                name: "prop_subtree_completeness".into(),
                category: "property".into(),
                passed: false,
                error: Some(e.to_string()),
            },
        });
    }

    // A9: subtree deduplication
    prop_test!(prop_subtree_deduplication, "[a-zA-Z0-9]{1,20}", prop_subtree_deduplication);

    // A10: subtree missing object
    prop_test!(prop_subtree_missing_object, prop_strategies::arb_content_hash(), prop_subtree_missing_object);

    // B1: ref roundtrip
    {
        let factory_ref = factory;
        let mut runner = TestRunner::new(config.clone());
        let strategy = (
            prop_strategies::arb_ref_name(),
            prop_strategies::arb_content_hash(),
        );
        let run_result = runner.run(&strategy, |(name, hash)| {
            let store = create_store(factory_ref).map_err(|e| {
                proptest::test_runner::TestCaseError::fail(format!("factory error: {e}"))
            })?;
            PropStoreTester { store }.prop_ref_roundtrip(name, hash);
            Ok(())
        });
        results.push(match run_result {
            Ok(()) => ComplianceResult {
                name: "prop_ref_roundtrip".into(),
                category: "property".into(),
                passed: true,
                error: None,
            },
            Err(e) => ComplianceResult {
                name: "prop_ref_roundtrip".into(),
                category: "property".into(),
                passed: false,
                error: Some(e.to_string()),
            },
        });
    }

    // B2: ref delete
    {
        let factory_ref = factory;
        let mut runner = TestRunner::new(config.clone());
        let strategy = (
            prop_strategies::arb_ref_name(),
            prop_strategies::arb_content_hash(),
        );
        let run_result = runner.run(&strategy, |(name, hash)| {
            let store = create_store(factory_ref).map_err(|e| {
                proptest::test_runner::TestCaseError::fail(format!("factory error: {e}"))
            })?;
            PropStoreTester { store }.prop_ref_delete(name, hash);
            Ok(())
        });
        results.push(match run_result {
            Ok(()) => ComplianceResult {
                name: "prop_ref_delete".into(),
                category: "property".into(),
                passed: true,
                error: None,
            },
            Err(e) => ComplianceResult {
                name: "prop_ref_delete".into(),
                category: "property".into(),
                passed: false,
                error: Some(e.to_string()),
            },
        });
    }

    // B3: cas create
    {
        let factory_ref = factory;
        let mut runner = TestRunner::new(config.clone());
        let strategy = (
            prop_strategies::arb_ref_name(),
            prop_strategies::arb_content_hash(),
        );
        let run_result = runner.run(&strategy, |(name, hash)| {
            let store = create_store(factory_ref).map_err(|e| {
                proptest::test_runner::TestCaseError::fail(format!("factory error: {e}"))
            })?;
            PropStoreTester { store }.prop_cas_create(name, hash);
            Ok(())
        });
        results.push(match run_result {
            Ok(()) => ComplianceResult {
                name: "prop_cas_create".into(),
                category: "property".into(),
                passed: true,
                error: None,
            },
            Err(e) => ComplianceResult {
                name: "prop_cas_create".into(),
                category: "property".into(),
                passed: false,
                error: Some(e.to_string()),
            },
        });
    }

    // B4: cas swap
    {
        let factory_ref = factory;
        let mut runner = TestRunner::new(config.clone());
        let strategy = (
            prop_strategies::arb_ref_name(),
            prop_strategies::arb_content_hash(),
            prop_strategies::arb_content_hash(),
        );
        let run_result = runner.run(&strategy, |(name, h1, h2)| {
            let store = create_store(factory_ref).map_err(|e| {
                proptest::test_runner::TestCaseError::fail(format!("factory error: {e}"))
            })?;
            PropStoreTester { store }.prop_cas_swap(name, h1, h2);
            Ok(())
        });
        results.push(match run_result {
            Ok(()) => ComplianceResult {
                name: "prop_cas_swap".into(),
                category: "property".into(),
                passed: true,
                error: None,
            },
            Err(e) => ComplianceResult {
                name: "prop_cas_swap".into(),
                category: "property".into(),
                passed: false,
                error: Some(e.to_string()),
            },
        });
    }

    // B5: cas reject
    {
        let factory_ref = factory;
        let mut runner = TestRunner::new(config.clone());
        let strategy = (
            prop_strategies::arb_ref_name(),
            prop_strategies::arb_content_hash(),
            prop_strategies::arb_content_hash(),
            prop_strategies::arb_content_hash(),
        );
        let run_result = runner.run(&strategy, |(name, h1, h_wrong, h2)| {
            let store = create_store(factory_ref).map_err(|e| {
                proptest::test_runner::TestCaseError::fail(format!("factory error: {e}"))
            })?;
            PropStoreTester { store }.prop_cas_reject(name, h1, h_wrong, h2);
            Ok(())
        });
        results.push(match run_result {
            Ok(()) => ComplianceResult {
                name: "prop_cas_reject".into(),
                category: "property".into(),
                passed: true,
                error: None,
            },
            Err(e) => ComplianceResult {
                name: "prop_cas_reject".into(),
                category: "property".into(),
                passed: false,
                error: Some(e.to_string()),
            },
        });
    }

    // B6: list refs prefix
    {
        let factory_ref = factory;
        let mut runner = TestRunner::new(config.clone());
        let strategy = (
            proptest::collection::vec("[a-z]{1,8}", 2..=4),
            proptest::collection::vec("[a-z]{1,8}", 1..=3),
            prop_strategies::arb_content_hash(),
        );
        let run_result = runner.run(&strategy, |(head_suffixes, tag_suffixes, hash)| {
            let store = create_store(factory_ref).map_err(|e| {
                proptest::test_runner::TestCaseError::fail(format!("factory error: {e}"))
            })?;
            PropStoreTester { store }.prop_list_refs_prefix(head_suffixes, tag_suffixes, hash);
            Ok(())
        });
        results.push(match run_result {
            Ok(()) => ComplianceResult {
                name: "prop_list_refs_prefix".into(),
                category: "property".into(),
                passed: true,
                error: None,
            },
            Err(e) => ComplianceResult {
                name: "prop_list_refs_prefix".into(),
                category: "property".into(),
                passed: false,
                error: Some(e.to_string()),
            },
        });
    }

    // B7: ref independence
    {
        let factory_ref = factory;
        let mut runner = TestRunner::new(config.clone());
        let strategy = (
            prop_strategies::arb_ref_name(),
            prop_strategies::arb_ref_name(),
            prop_strategies::arb_content_hash(),
            prop_strategies::arb_content_hash(),
        );
        let run_result = runner.run(&strategy, |(name_a, name_b, h_a, h_b)| {
            let store = create_store(factory_ref).map_err(|e| {
                proptest::test_runner::TestCaseError::fail(format!("factory error: {e}"))
            })?;
            PropStoreTester { store }.prop_ref_independence(name_a, name_b, h_a, h_b);
            Ok(())
        });
        results.push(match run_result {
            Ok(()) => ComplianceResult {
                name: "prop_ref_independence".into(),
                category: "property".into(),
                passed: true,
                error: None,
            },
            Err(e) => ComplianceResult {
                name: "prop_ref_independence".into(),
                category: "property".into(),
                passed: false,
                error: Some(e.to_string()),
            },
        });
    }

    // B8: ref overwrite
    {
        let factory_ref = factory;
        let mut runner = TestRunner::new(config.clone());
        let strategy = (
            prop_strategies::arb_ref_name(),
            prop_strategies::arb_content_hash(),
            prop_strategies::arb_content_hash(),
        );
        let run_result = runner.run(&strategy, |(name, h1, h2)| {
            let store = create_store(factory_ref).map_err(|e| {
                proptest::test_runner::TestCaseError::fail(format!("factory error: {e}"))
            })?;
            PropStoreTester { store }.prop_ref_overwrite(name, h1, h2);
            Ok(())
        });
        results.push(match run_result {
            Ok(()) => ComplianceResult {
                name: "prop_ref_overwrite".into(),
                category: "property".into(),
                passed: true,
                error: None,
            },
            Err(e) => ComplianceResult {
                name: "prop_ref_overwrite".into(),
                category: "property".into(),
                passed: false,
                error: Some(e.to_string()),
            },
        });
    }

    // B9: adversarial refs
    {
        let factory_ref = factory;
        let mut runner = TestRunner::new(config.clone());
        let strategy = (
            prop_strategies::arb_adversarial_ref_scenario(),
            prop_strategies::arb_content_hash(),
        );
        let run_result = runner.run(&strategy, |(scenario, hash)| {
            let store = create_store(factory_ref).map_err(|e| {
                proptest::test_runner::TestCaseError::fail(format!("factory error: {e}"))
            })?;
            PropStoreTester { store }.prop_list_refs_adversarial(scenario, hash);
            Ok(())
        });
        results.push(match run_result {
            Ok(()) => ComplianceResult {
                name: "prop_list_refs_adversarial".into(),
                category: "property".into(),
                passed: true,
                error: None,
            },
            Err(e) => ComplianceResult {
                name: "prop_list_refs_adversarial".into(),
                category: "property".into(),
                passed: false,
                error: Some(e.to_string()),
            },
        });
    }

    // B10: cas linearizability
    {
        let factory_ref = factory;
        let mut runner = TestRunner::new(config.clone());
        let strategy = (
            prop_strategies::arb_ref_name(),
            prop_strategies::arb_content_hash(),
            prop_strategies::arb_content_hash(),
        );
        let run_result = runner.run(&strategy, |(name, h1, h2)| {
            let store = create_store(factory_ref).map_err(|e| {
                proptest::test_runner::TestCaseError::fail(format!("factory error: {e}"))
            })?;
            PropStoreTester { store }.prop_cas_linearizability(name, h1, h2);
            Ok(())
        });
        results.push(match run_result {
            Ok(()) => ComplianceResult {
                name: "prop_cas_linearizability".into(),
                category: "property".into(),
                passed: true,
                error: None,
            },
            Err(e) => ComplianceResult {
                name: "prop_cas_linearizability".into(),
                category: "property".into(),
                passed: false,
                error: Some(e.to_string()),
            },
        });
    }

    // F1: model consistency
    {
        let factory_ref = factory;
        let mut runner = TestRunner::new(config);
        let strategy = prop_strategies::arb_op_sequence();
        let run_result = runner.run(&strategy, |ops| {
            let store = create_store(factory_ref).map_err(|e| {
                proptest::test_runner::TestCaseError::fail(format!("factory error: {e}"))
            })?;
            PropStoreTester { store }.prop_model_consistency(ops);
            Ok(())
        });
        results.push(match run_result {
            Ok(()) => ComplianceResult {
                name: "prop_model_consistency".into(),
                category: "property".into(),
                passed: true,
                error: None,
            },
            Err(e) => ComplianceResult {
                name: "prop_model_consistency".into(),
                category: "property".into(),
                passed: false,
                error: Some(e.to_string()),
            },
        });
    }
}

// ---------------------------------------------------------------------------
// Concurrency tests
// ---------------------------------------------------------------------------

fn run_concurrency_tests(
    factory: &Py<PyAny>,
    results: &mut Vec<ComplianceResult>,
) {
    use clayers_repo::store::concurrency_tests::ConcurrencyTester;

    fn multi_thread_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("multi-thread runtime")
    }

    macro_rules! conc_test {
        ($name:ident) => {{
            let store = match create_store(factory) {
                Ok(s) => s,
                Err(e) => {
                    results.push(ComplianceResult {
                        name: stringify!($name).into(),
                        category: "concurrency".into(),
                        passed: false,
                        error: Some(format!("factory error: {e}")),
                    });
                    return;
                }
            };
            let result = run_test(
                stringify!($name),
                "concurrency",
                AssertUnwindSafe(|| {
                    let rt = multi_thread_runtime();
                    rt.block_on(ConcurrencyTester::new(store).$name());
                }),
            );
            results.push(result);
        }};
    }

    conc_test!(test_cas_create_one_winner);
    conc_test!(test_cas_swap_one_winner);
    conc_test!(test_set_ref_final_in_inputs);
    conc_test!(test_put_idempotent_same_object);
    conc_test!(test_put_distinct_all_visible);
    conc_test!(test_transactions_both_visible);
    conc_test!(test_subtree_readers_consistent);
    conc_test!(test_independent_refs);
    conc_test!(test_reader_during_writer);
}

// ---------------------------------------------------------------------------
// Property-based concurrency tests
// ---------------------------------------------------------------------------

fn run_concurrency_property_tests(
    factory: &Py<PyAny>,
    results: &mut Vec<ComplianceResult>,
) {
    use clayers_repo::store::concurrency_tests::ConcurrencyTester;
    use clayers_repo::store::prop_strategies;
    use proptest::strategy::Strategy;
    use proptest::test_runner::{Config, TestRunner};

    // Concurrent property cases each spawn N tasks against a fresh store.
    // 32 cases keeps total runtime bounded.
    let config = Config::with_cases(32);

    fn multi_thread_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("multi-thread runtime")
    }

    // P-A1: concurrent CAS create -> exactly one winner.
    {
        let factory_ref = factory;
        let mut runner = TestRunner::new(config.clone());
        let strategy = proptest::collection::vec(
            prop_strategies::arb_content_hash(),
            2..=8_usize,
        )
        .prop_filter(
            "hashes must be unique",
            |hs| hs.iter().collect::<std::collections::HashSet<_>>().len() == hs.len(),
        );
        let run_result = runner.run(&strategy, |hashes| {
            let store = create_store(factory_ref).map_err(|e| {
                proptest::test_runner::TestCaseError::fail(format!("factory error: {e}"))
            })?;
            let rt = multi_thread_runtime();
            rt.block_on(async move {
                let store_arc = std::sync::Arc::new(store);
                let name = "refs/heads/prop_compliance_cas_create";
                let mut set = tokio::task::JoinSet::new();
                for h in &hashes {
                    let s = std::sync::Arc::clone(&store_arc);
                    let h = *h;
                    set.spawn(async move {
                        clayers_repo::store::RefStore::cas_ref(&*s, name, None, h)
                            .await
                            .unwrap()
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
            Ok(())
        });
        results.push(match run_result {
            Ok(()) => ComplianceResult {
                name: "prop_concurrent_cas_create_unique_winner".into(),
                category: "property:concurrency".into(),
                passed: true,
                error: None,
            },
            Err(e) => ComplianceResult {
                name: "prop_concurrent_cas_create_unique_winner".into(),
                category: "property:concurrency".into(),
                passed: false,
                error: Some(e.to_string()),
            },
        });
    }

    // P-A2: concurrent set_ref -> final value in inputs.
    {
        let factory_ref = factory;
        let mut runner = TestRunner::new(config.clone());
        let strategy = proptest::collection::vec(
            prop_strategies::arb_content_hash(),
            2..=8_usize,
        );
        let run_result = runner.run(&strategy, |hashes| {
            let store = create_store(factory_ref).map_err(|e| {
                proptest::test_runner::TestCaseError::fail(format!("factory error: {e}"))
            })?;
            let rt = multi_thread_runtime();
            rt.block_on(async move {
                let store_arc = std::sync::Arc::new(store);
                let name = "refs/heads/prop_compliance_set_race";
                let allowed: std::collections::HashSet<_> = hashes.iter().copied().collect();
                let mut set = tokio::task::JoinSet::new();
                for h in &hashes {
                    let s = std::sync::Arc::clone(&store_arc);
                    let h = *h;
                    set.spawn(async move {
                        clayers_repo::store::RefStore::set_ref(&*s, name, h)
                            .await
                            .unwrap();
                    });
                }
                while let Some(r) = set.join_next().await {
                    r.unwrap();
                }
                let final_h = clayers_repo::store::RefStore::get_ref(&*store_arc, name)
                    .await
                    .unwrap()
                    .unwrap();
                assert!(allowed.contains(&final_h), "final value not in inputs");
            });
            Ok(())
        });
        results.push(match run_result {
            Ok(()) => ComplianceResult {
                name: "prop_concurrent_set_ref_final_in_inputs".into(),
                category: "property:concurrency".into(),
                passed: true,
                error: None,
            },
            Err(e) => ComplianceResult {
                name: "prop_concurrent_set_ref_final_in_inputs".into(),
                category: "property:concurrency".into(),
                passed: false,
                error: Some(e.to_string()),
            },
        });
    }

    // P-A3: concurrent distinct put -> all visible.
    {
        let factory_ref = factory;
        let mut runner = TestRunner::new(config.clone());
        let strategy = 2usize..=8_usize;
        let run_result = runner.run(&strategy, |count| {
            let store = create_store(factory_ref).map_err(|e| {
                proptest::test_runner::TestCaseError::fail(format!("factory error: {e}"))
            })?;
            let rt = multi_thread_runtime();
            rt.block_on(async move {
                let store_arc = std::sync::Arc::new(store);
                let hashes: Vec<_> = (0..count)
                    .map(|i| {
                        clayers_xml::ContentHash::from_canonical(
                            format!("compliance_prop_put_{i}").as_bytes(),
                        )
                    })
                    .collect();
                let mut set = tokio::task::JoinSet::new();
                for (i, h) in hashes.iter().enumerate() {
                    let s = std::sync::Arc::clone(&store_arc);
                    let h = *h;
                    set.spawn(async move {
                        let mut tx = clayers_repo::store::ObjectStore::transaction(&*s)
                            .await
                            .unwrap();
                        tx.put(
                            h,
                            clayers_repo::object::Object::Text(
                                clayers_repo::object::TextObject {
                                    content: format!("v{i}"),
                                },
                            ),
                        )
                        .await
                        .unwrap();
                        tx.commit().await.unwrap();
                    });
                }
                while let Some(r) = set.join_next().await {
                    r.unwrap();
                }
                for h in &hashes {
                    assert!(
                        clayers_repo::store::ObjectStore::contains(&*store_arc, h)
                            .await
                            .unwrap(),
                        "object {h:?} missing"
                    );
                }
            });
            Ok(())
        });
        results.push(match run_result {
            Ok(()) => ComplianceResult {
                name: "prop_concurrent_put_distinct_all_visible".into(),
                category: "property:concurrency".into(),
                passed: true,
                error: None,
            },
            Err(e) => ComplianceResult {
                name: "prop_concurrent_put_distinct_all_visible".into(),
                category: "property:concurrency".into(),
                passed: false,
                error: Some(e.to_string()),
            },
        });
    }

    // Suppress dead_code warning when compliance feature is enabled but
    // the type isn't otherwise referenced from this scope.
    let _ = std::marker::PhantomData::<ConcurrencyTester<crate::repo::py_store::PyStore>>;
}

// ---------------------------------------------------------------------------
// Query tests
// ---------------------------------------------------------------------------

fn run_query_tests(
    factory: &Py<PyAny>,
    results: &mut Vec<ComplianceResult>,
) {
    use clayers_repo::query::tests::QueryTester;

    macro_rules! query_test {
        ($name:ident) => {{
            let store = match create_store(factory) {
                Ok(s) => s,
                Err(e) => {
                    results.push(ComplianceResult {
                        name: stringify!($name).into(),
                        category: "query".into(),
                        passed: false,
                        error: Some(format!("factory error: {e}")),
                    });
                    return;
                }
            };
            let result = run_test(
                stringify!($name),
                "query",
                AssertUnwindSafe(|| {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(QueryTester { store }.$name());
                }),
            );
            results.push(result);
        }};
    }

    query_test!(test_query_count);
    query_test!(test_query_text);
    query_test!(test_query_xml);
    query_test!(test_query_with_predicate);
    query_test!(test_query_nested_path);
    query_test!(test_query_no_matches);
    query_test!(test_query_by_branch);
    query_test!(test_query_by_tag);
    query_test!(test_query_different_revisions);
    query_test!(test_query_all_refs);
    query_test!(test_query_all_refs_deduplicates);
    query_test!(test_query_nonexistent_ref);
    query_test!(test_query_malformed_xpath_no_slashes);
    query_test!(test_query_malformed_xpath_unbalanced_bracket);
    query_test!(test_query_malformed_predicate_no_at);
    query_test!(test_query_unknown_prefix_returns_zero);
    query_test!(test_query_prefix_vs_no_namespace);
    query_test!(test_query_on_commit_hash_errors);
    query_test!(test_query_on_missing_hash_errors);
    query_test!(test_query_mixed_content_skips_non_elements);
    query_test!(test_query_deep_nesting);
    query_test!(test_query_text_concatenation);
    query_test!(test_query_roundtrip_fidelity);
    query_test!(test_resolve_head_not_set);
    query_test!(test_resolve_full_ref_path);
    query_test!(test_resolve_element_hash_errors);
    query_test!(test_query_xml_preserves_namespace);
    query_test!(test_query_xml_preserves_attributes);
    query_test!(test_query_child_order_preserved);
    query_test!(test_export_vs_query_xml_equivalence);
    query_test!(test_query_refs_empty_prefix);
    query_test!(test_query_tree_wide_count);
    query_test!(test_query_file_scoped);
    query_test!(test_resolve_to_tree_from_commit);
    query_test!(test_query_absolute_path);
    query_test!(test_query_count_function);
    query_test!(test_query_contains_predicate);
    query_test!(test_query_positional_predicate);
    query_test!(test_query_last_function);
    query_test!(test_query_parent_axis);
    query_test!(test_query_descendant_axis);
    query_test!(test_query_string_function);
    query_test!(test_query_by_document_returns_paths);
    query_test!(test_query_by_document_file_filter);
    query_test!(test_query_by_document_file_filter_substring);
    query_test!(test_query_by_document_skips_empty);
    query_test!(test_query_by_document_no_filter_queries_all);
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the full store compliance suite against a Python store factory.
///
/// The `store_factory` is a zero-argument Python callable that returns a
/// fresh store instance each time it is called. Each test gets its own
/// store to ensure isolation.
///
/// Returns a list of `ComplianceResult` objects, one per test.
#[pyfunction]
pub fn run_store_compliance(
    _py: Python<'_>,
    store_factory: Py<PyAny>,
) -> PyResult<Vec<ComplianceResult>> {
    let mut results = Vec::new();

    // Release the GIL for the bulk of test execution. Tests re-acquire it
    // as needed when calling into Python via PyStore.
    _py.detach(|| {
        run_deterministic_tests(&store_factory, &mut results);
        run_property_tests(&store_factory, &mut results);
        run_concurrency_tests(&store_factory, &mut results);
        run_concurrency_property_tests(&store_factory, &mut results);
        run_query_tests(&store_factory, &mut results);
    });

    Ok(results)
}

// ---------------------------------------------------------------------------
// ComplianceMemoryStore: Python store backed by Rust MemoryStore
// ---------------------------------------------------------------------------

/// A Python store implementing the full store protocol, backed by the Rust
/// `MemoryStore`. Use this as the factory for `run_store_compliance` to
/// validate the compliance test runner itself.
#[pyclass]
pub struct ComplianceMemoryStore {
    store: std::sync::Arc<clayers_repo::MemoryStore>,
}

/// A transaction for `ComplianceMemoryStore`.
#[pyclass]
pub struct ComplianceTransaction {
    tx: std::sync::Mutex<Option<Box<dyn clayers_repo::store::Transaction>>>,
}

#[pymethods]
impl ComplianceMemoryStore {
    #[new]
    fn new() -> Self {
        Self {
            store: std::sync::Arc::new(clayers_repo::MemoryStore::new()),
        }
    }

    fn get(&self, hash: &crate::xml::ContentHash) -> PyResult<Option<crate::repo::py_objects::StoreObject>> {
        let h = hash.inner();
        let store = self.store.clone();
        let handle = tokio::runtime::Handle::current();
        let result = tokio::task::block_in_place(|| handle.block_on(clayers_repo::store::ObjectStore::get(&*store, &h)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(result.map(crate::repo::py_objects::StoreObject::from))
    }

    fn contains(&self, hash: &crate::xml::ContentHash) -> PyResult<bool> {
        let h = hash.inner();
        let store = self.store.clone();
        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| handle.block_on(clayers_repo::store::ObjectStore::contains(&*store, &h)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    fn transaction(&self) -> PyResult<ComplianceTransaction> {
        let store = self.store.clone();
        let handle = tokio::runtime::Handle::current();
        let tx = tokio::task::block_in_place(|| handle.block_on(clayers_repo::store::ObjectStore::transaction(&*store)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(ComplianceTransaction { tx: std::sync::Mutex::new(Some(tx)) })
    }

    fn get_by_inclusive_hash(&self, hash: &crate::xml::ContentHash) -> PyResult<Option<(crate::xml::ContentHash, crate::repo::py_objects::StoreObject)>> {
        let h = hash.inner();
        let store = self.store.clone();
        let handle = tokio::runtime::Handle::current();
        let result = tokio::task::block_in_place(|| handle.block_on(clayers_repo::store::ObjectStore::get_by_inclusive_hash(&*store, &h)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(result.map(|(ch, obj)| (crate::xml::ContentHash::from_inner(ch), crate::repo::py_objects::StoreObject::from(obj))))
    }

    fn get_ref(&self, name: &str) -> PyResult<Option<crate::xml::ContentHash>> {
        let store = self.store.clone();
        let name = name.to_string();
        let handle = tokio::runtime::Handle::current();
        let result = tokio::task::block_in_place(|| handle.block_on(clayers_repo::store::RefStore::get_ref(&*store, &name)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(result.map(crate::xml::ContentHash::from_inner))
    }

    fn set_ref(&self, name: &str, hash: &crate::xml::ContentHash) -> PyResult<()> {
        let store = self.store.clone();
        let name = name.to_string();
        let h = hash.inner();
        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| handle.block_on(clayers_repo::store::RefStore::set_ref(&*store, &name, h)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    fn delete_ref(&self, name: &str) -> PyResult<()> {
        let store = self.store.clone();
        let name = name.to_string();
        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| handle.block_on(clayers_repo::store::RefStore::delete_ref(&*store, &name)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    fn list_refs(&self, prefix: &str) -> PyResult<Vec<(String, crate::xml::ContentHash)>> {
        let store = self.store.clone();
        let prefix = prefix.to_string();
        let handle = tokio::runtime::Handle::current();
        let refs = tokio::task::block_in_place(|| handle.block_on(clayers_repo::store::RefStore::list_refs(&*store, &prefix)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(refs.into_iter().map(|(n, h)| (n, crate::xml::ContentHash::from_inner(h))).collect())
    }

    fn cas_ref(&self, name: &str, expected: Option<&crate::xml::ContentHash>, new: &crate::xml::ContentHash) -> PyResult<bool> {
        let store = self.store.clone();
        let name = name.to_string();
        let expected = expected.map(|h| h.inner());
        let new_h = new.inner();
        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| handle.block_on(clayers_repo::store::RefStore::cas_ref(&*store, &name, expected, new_h)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    fn query_document(
        &self,
        doc_hash: &crate::xml::ContentHash,
        xpath: &str,
        mode: &str,
        namespaces: Vec<(String, String)>,
    ) -> PyResult<crate::query::QueryResult> {
        let store = self.store.clone();
        let h = doc_hash.inner();
        let xpath = xpath.to_string();
        let qm = crate::query::parse_query_mode_repo(mode)?;
        let handle = tokio::runtime::Handle::current();
        let result = tokio::task::block_in_place(|| handle.block_on(clayers_repo::query::QueryStore::query_document(&*store, h, &xpath, qm, &namespaces)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(result.into())
    }
}

#[pymethods]
impl ComplianceTransaction {
    fn put(&self, hash: &crate::xml::ContentHash, object: &crate::repo::py_objects::StoreObject) -> PyResult<()> {
        let h = hash.inner();
        let rust_obj = object.to_rust().map_err(pyo3::exceptions::PyValueError::new_err)?;
        let mut guard = self.tx.lock().map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let tx = guard.as_mut().ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("transaction already consumed"))?;
        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| handle.block_on(tx.put(h, rust_obj)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    fn commit(&self) -> PyResult<()> {
        let mut guard = self.tx.lock().map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let tx = guard.as_mut().ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("transaction already consumed"))?;
        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| handle.block_on(tx.commit()))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    fn rollback(&self) -> PyResult<()> {
        let mut guard = self.tx.lock().map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let tx = guard.as_mut().ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("transaction already consumed"))?;
        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| handle.block_on(tx.rollback()))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }
}
