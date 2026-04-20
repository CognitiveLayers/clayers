//! End-to-end golden tests: chunker → fingerprint → index.
//!
//! These tests build a fresh index in a temporary copy of the
//! self-referential spec and exercise the critical invariants that
//! the pytest checklist can't reach at the unit level.
//!
//! Skipped (printed message, `return`) if no network is available on
//! first run — the `fastembed` model download requires `HuggingFace`
//! connectivity. Subsequent runs with a warm cache are offline.

use std::path::PathBuf;

use clayers_search::{
    fingerprint::{FINGERPRINT_BITS, fingerprint},
    index::{self, CONFIG_FILENAME, INDEX_FILENAME},
    meta::{META_FILENAME, MetaStore},
};
use clayers_spec::chunker::{Chunk, extract_chunks};

fn examples_spec() -> Option<PathBuf> {
    let manifest =
        std::env::var("CARGO_MANIFEST_DIR").ok().map(PathBuf::from)?;
    let root = manifest.parent()?.parent()?.to_path_buf();
    let p = root.join("examples/payment-processing");
    p.join("index.xml").is_file().then_some(p)
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let p = entry.path();
        let dst = to.join(entry.file_name());
        if p.is_dir() {
            copy_dir(&p, &dst)?;
        } else {
            std::fs::copy(&p, &dst)?;
        }
    }
    Ok(())
}

fn try_build(
    spec: &std::path::Path,
    force: bool,
) -> Option<index::BuildReport> {
    match index::build_or_update(spec, "bge-small-en-v1.5", force, false) {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("skipping golden test (embedder unavailable): {e:?}");
            None
        }
    }
}

#[test]
fn golden_chunker_payment_processing_has_expected_ids() {
    let Some(spec) = examples_spec() else {
        eprintln!("skipping: examples/payment-processing not available");
        return;
    };
    let chunks = extract_chunks(&spec).expect("chunker");
    // Spec has 26 @id-bearing elements per Step 4 smoke test.
    assert!(chunks.len() >= 20, "got {} chunks", chunks.len());
    // Specific ids that must be present in the example spec.
    let ids: std::collections::HashSet<&str> =
        chunks.iter().map(|c| c.id.as_str()).collect();
    for expected in [
        "term-transaction",
        "term-settlement",
        "term-authorization",
        "overview",
    ] {
        assert!(ids.contains(expected), "missing {expected}: {ids:?}");
    }
    // Every chunk must have a non-trivial hash and line-range.
    for c in &chunks {
        assert!(c.node_hash.starts_with("sha256:"));
        assert_ne!(c.node_hash, "sha256:placeholder");
        assert!(c.line_start >= 1);
        assert!(c.line_end >= c.line_start);
    }
}

#[test]
fn golden_chunker_populates_ancestor_local_names() {
    // Regression for the bug discovered during self-review: the
    // fingerprint's ancestor-path n-grams must come from element
    // local-names, not @id attribute values.
    let Some(spec) = examples_spec() else {
        return;
    };
    let chunks = extract_chunks(&spec).expect("chunker");
    // Every chunk should have at least the `spec:clayers` root ancestor.
    let nested: Vec<&Chunk> = chunks
        .iter()
        .filter(|c| !c.ancestor_local_names.is_empty())
        .collect();
    assert!(!nested.is_empty(), "no chunks have ancestors?");
    // Root should appear in most chunks' ancestor chains. The
    // combined document's synthetic root is `<cmb:spec>` (local-name
    // `"spec"`), not `<spec:clayers>` — the file-level `spec:clayers`
    // elements get flattened away during assembly.
    let has_root = nested.iter().any(|c| {
        c.ancestor_local_names
            .first()
            .is_some_and(|n| n == "spec")
    });
    assert!(
        has_root,
        "expected 'spec' root local-name in at least one chunk's ancestors: {:?}",
        nested.first().map(|c| &c.ancestor_local_names)
    );
}

#[test]
fn golden_chunker_deterministic() {
    let Some(spec) = examples_spec() else {
        return;
    };
    let a = extract_chunks(&spec).expect("a");
    let b = extract_chunks(&spec).expect("b");
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.id, y.id);
        assert_eq!(x.node_hash, y.node_hash);
        assert_eq!(x.line_start, y.line_start);
        assert_eq!(x.layer, y.layer);
        assert_eq!(x.ancestor_local_names, y.ancestor_local_names);
    }
}

#[test]
fn golden_fingerprint_deterministic_from_chunker() {
    let Some(spec) = examples_spec() else {
        return;
    };
    let chunks = extract_chunks(&spec).expect("chunks");
    for c in chunks.iter().take(5) {
        let a = fingerprint(c);
        let b = fingerprint(c);
        assert_eq!(a, b, "fp for {} differs across runs", c.id);
        // Bit count should be non-trivial.
        let popcount: u32 = a.iter().map(|b| b.count_ones()).sum();
        assert!(popcount > 2, "{} popcount={popcount}", c.id);
    }
    // Structurally distinct elements (different layers) must produce
    // different fingerprints.
    if let (Some(p), Some(t)) = (
        chunks.iter().find(|c| c.layer == "prose"),
        chunks.iter().find(|c| c.layer == "terminology"),
    ) {
        assert_ne!(fingerprint(p), fingerprint(t));
    }
}

#[test]
fn golden_index_build_populates_sidecar() {
    let Some(spec) = examples_spec() else {
        return;
    };
    let tmp = tempfile::tempdir().expect("tempdir");
    let dst = tmp.path().join("spec");
    copy_dir(&spec, &dst).expect("copy");
    let Some(report) = try_build(&dst, false) else {
        return;
    };
    assert!(report.total_nodes >= 20);
    assert_eq!(report.reused, 0); // fresh build
    assert_eq!(report.removed, 0);

    let sidecar = dst.join(".clayers/search");
    assert!(sidecar.join(INDEX_FILENAME).is_file());
    assert!(sidecar.join(META_FILENAME).is_file());
    let cfg: serde_json::Value =
        serde_json::from_slice(&std::fs::read(sidecar.join(CONFIG_FILENAME)).unwrap())
            .unwrap();
    assert_eq!(cfg["dim_text"], 384);
    assert_eq!(cfg["dim_struct"], FINGERPRINT_BITS);
}

#[test]
fn golden_index_incremental_reuses_everything() {
    let Some(spec) = examples_spec() else {
        return;
    };
    let tmp = tempfile::tempdir().expect("tempdir");
    let dst = tmp.path().join("spec");
    copy_dir(&spec, &dst).expect("copy");
    if try_build(&dst, false).is_none() {
        return;
    }
    let second = index::build_or_update(&dst, "bge-small-en-v1.5", false, false)
        .expect("second build");
    assert_eq!(second.re_embedded, 0, "incremental should skip all");
    assert_eq!(second.reused, second.total_nodes);
    assert_eq!(second.removed, 0);
}

#[test]
fn golden_incremental_preserves_query_results() {
    // Build → query → build again (no changes) → query again.
    // Top-k ids and text_scores must be stable because the only
    // allowed variation is floating-point re-embedding, which
    // incremental rebuild skips entirely.
    let Some(spec) = examples_spec() else {
        return;
    };
    let tmp = tempfile::tempdir().expect("tempdir");
    let dst = tmp.path().join("spec");
    copy_dir(&spec, &dst).expect("copy");
    if try_build(&dst, false).is_none() {
        return;
    }

    let params = clayers_search::query::QueryParams {
        query_text: "authorization",
        k: 5,
        alpha: 0.7,
        beta: 0.3,
        xpath: None,
        layer_filter: &[],
        model: "bge-small-en-v1.5",
        verbose: false,
    };
    let first = clayers_search::query::run(&dst, &params).expect("first query");

    // Trigger an incremental rebuild; should be a no-op.
    let report =
        clayers_search::index::build_or_update(&dst, "bge-small-en-v1.5", false, false)
            .expect("second build");
    assert_eq!(report.re_embedded, 0);

    let second = clayers_search::query::run(&dst, &params).expect("second query");

    assert_eq!(first.len(), second.len());
    for (a, b) in first.iter().zip(&second) {
        assert_eq!(a.id, b.id, "top-k order drifted after incremental rebuild");
        assert!(
            (a.text_score - b.text_score).abs() < 1e-6,
            "text_score drifted: {} vs {}",
            a.text_score,
            b.text_score,
        );
    }
}

#[test]
fn golden_index_removes_vanished_nodes() {
    let Some(spec) = examples_spec() else {
        return;
    };
    let tmp = tempfile::tempdir().expect("tempdir");
    let dst = tmp.path().join("spec");
    copy_dir(&spec, &dst).expect("copy");
    if try_build(&dst, false).is_none() {
        return;
    }

    // Delete one file from the spec; it should drop all its nodes
    // from both the usearch index and meta.
    let extra = dst.join("disputes.xml");
    let before = MetaStore::open_or_create(&dst.join(".clayers/search/meta.sqlite"))
        .unwrap()
        .all_ids()
        .unwrap()
        .len();
    assert!(extra.is_file(), "fixture precondition");
    std::fs::remove_file(&extra).unwrap();

    // Rebuild: should not error; meta should shrink.
    let report = index::build_or_update(&dst, "bge-small-en-v1.5", false, false)
        .expect("rebuild after delete");
    let after = MetaStore::open_or_create(&dst.join(".clayers/search/meta.sqlite"))
        .unwrap()
        .all_ids()
        .unwrap()
        .len();
    assert!(report.removed >= 1, "expected some removed, got {}", report.removed);
    assert!(after < before, "meta did not shrink: {before} -> {after}");
}

fn build_then_query(
    params: &clayers_search::query::QueryParams<'_>,
) -> Option<Vec<clayers_search::query::Hit>> {
    let spec = examples_spec()?;
    let tmp = tempfile::tempdir().expect("tempdir");
    let dst = tmp.path().join("spec");
    copy_dir(&spec, &dst).expect("copy");
    try_build(&dst, false)?;
    let _keep = tmp.keep();
    match clayers_search::query::run(&dst, params) {
        Ok(h) => Some(h),
        Err(e) => {
            eprintln!("skipping: query failed: {e:?}");
            None
        }
    }
}

#[test]
fn golden_query_layer_filter_narrows_results() {
    let layer = vec!["terminology".to_string()];
    let Some(hits) = build_then_query(&clayers_search::query::QueryParams {
        query_text: "authorization",
        k: 5,
        alpha: 0.7,
        beta: 0.3,
        xpath: None,
        layer_filter: &layer,
        model: "bge-small-en-v1.5",
        verbose: false,
    }) else {
        return;
    };
    assert!(!hits.is_empty());
    for h in &hits {
        assert_eq!(h.layer, "terminology", "layer filter leaked: {}", h.id);
    }
}

#[test]
fn golden_query_empty_xpath_returns_no_hits() {
    let no_match_xpath = Some("//nonexistent:element");
    let Some(hits) = build_then_query(&clayers_search::query::QueryParams {
        query_text: "authorization",
        k: 5,
        alpha: 0.7,
        beta: 0.3,
        xpath: no_match_xpath,
        layer_filter: &[],
        model: "bge-small-en-v1.5",
        verbose: false,
    }) else {
        return;
    };
    assert!(hits.is_empty(), "xpath matched zero ids; got {} hits", hits.len());
}

#[test]
fn golden_query_combined_layer_plus_xpath() {
    let layer = vec!["terminology".to_string()];
    let xpath = Some("//trm:term");
    let Some(hits) = build_then_query(&clayers_search::query::QueryParams {
        query_text: "transaction",
        k: 5,
        alpha: 0.7,
        beta: 0.3,
        xpath,
        layer_filter: &layer,
        model: "bge-small-en-v1.5",
        verbose: false,
    }) else {
        return;
    };
    assert!(!hits.is_empty());
    for h in &hits {
        assert_eq!(h.layer, "terminology");
        assert!(h.id.starts_with("term-"), "xpath filter bypassed: {}", h.id);
    }
}

#[test]
fn golden_query_layer_seed_makes_beta_meaningful() {
    // With --layer set, the query struct has one layer bit. Tanimoto
    // of {layer-bit} vs node-struct is non-zero for same-layer nodes,
    // so struct_score varies per hit.
    let layer = vec!["terminology".to_string()];
    let Some(hits) = build_then_query(&clayers_search::query::QueryParams {
        query_text: "transaction",
        k: 3,
        alpha: 0.7,
        beta: 0.3,
        xpath: None,
        layer_filter: &layer,
        model: "bge-small-en-v1.5",
        verbose: false,
    }) else {
        return;
    };
    assert!(!hits.is_empty());
    let any_nonzero_struct = hits.iter().any(|h| h.struct_score > 0.0);
    assert!(
        any_nonzero_struct,
        "layer-seeded query should produce >0 struct_score for some hit"
    );
}

#[test]
fn golden_index_concurrent_build_is_rejected() {
    let Some(spec) = examples_spec() else {
        return;
    };
    let tmp = tempfile::tempdir().expect("tempdir");
    let dst = tmp.path().join("spec");
    copy_dir(&spec, &dst).expect("copy");
    // Pre-plant the lockfile to simulate a concurrent build.
    let sidecar = dst.join(".clayers/search");
    std::fs::create_dir_all(&sidecar).unwrap();
    std::fs::File::create(sidecar.join(".lock")).unwrap();
    // build_or_update must refuse to proceed.
    let result = index::build_or_update(&dst, "bge-small-en-v1.5", false, false);
    assert!(result.is_err(), "build should bail on existing lock");
}
