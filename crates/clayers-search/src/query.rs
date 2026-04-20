//! Ranked query execution.
//!
//! Embeds the user's query, pads the struct half with zeros, searches
//! the `usearch` index with a custom metric tuned by `alpha`/`beta`,
//! applies `--xpath` / `--layer` post-filters, and decomposes scores
//! so JSON callers see both text and struct components separately.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

use crate::embedder::{DEFAULT_TEXT_DIM, Embedder, resolve_cache_dir};
use crate::fingerprint::{FINGERPRINT_BITS, FINGERPRINT_BYTES};
use crate::index::{
    CONCAT_DIM, CONFIG_FILENAME, Config, INDEX_FILENAME, build_or_update, cosine_sim,
    install_metric, sidecar_path, tanimoto_sim_bits,
};
use crate::meta::{META_FILENAME, MetaStore};

/// Default pull-back for post-filter mode: usearch returns top-N,
/// then we intersect with xpath/layer allowlists and keep top-K.
const POST_FILTER_POOL: usize = 2_000;

/// One ranked result.
#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub id: String,
    pub file: String,
    pub line_start: i64,
    pub line_end: i64,
    pub layer: String,
    /// Combined score = `alpha * text_score + beta * struct_score`.
    pub score: f32,
    pub text_score: f32,
    pub struct_score: f32,
    pub preview: String,
}

/// Query parameters.
pub struct QueryParams<'a> {
    pub query_text: &'a str,
    pub k: usize,
    pub alpha: f32,
    pub beta: f32,
    pub xpath: Option<&'a str>,
    pub layer_filter: &'a [String],
    pub model: &'a str,
    pub verbose: bool,
}

/// Run a ranked query against the search index.
///
/// Rebuilds the index automatically if the sidecar is missing, stale
/// (node hash mismatch), or the model name differs. Returns up to
/// `k` hits with decomposed scores.
///
/// # Errors
/// Propagates errors from index build, embedder, usearch, or the
/// xpath/layer filters.
#[allow(clippy::too_many_lines)]
pub fn run(spec_dir: &Path, p: &QueryParams<'_>) -> Result<Vec<Hit>> {
    // 1. Ensure the sidecar is up to date.
    let _report = build_or_update(spec_dir, p.model, false, p.verbose)
        .context("auto-build before query")?;

    let sidecar = sidecar_path(spec_dir);
    let cfg_path = sidecar.join(CONFIG_FILENAME);
    let cfg: Config = serde_json::from_slice(&std::fs::read(&cfg_path)?)
        .with_context(|| format!("parse {}", cfg_path.display()))?;
    anyhow::ensure!(
        cfg.dim_text == DEFAULT_TEXT_DIM && cfg.dim_struct == FINGERPRINT_BITS,
        "config.json dim mismatch: got {}+{}, expected {}+{}",
        cfg.dim_text, cfg.dim_struct, DEFAULT_TEXT_DIM, FINGERPRINT_BITS,
    );

    // 2. Load the index with a custom metric tuned to the caller's
    // α/β. Distance is used purely for ranking; we re-compute
    // per-hit text/struct scores manually below.
    let opts = IndexOptions {
        dimensions: CONCAT_DIM,
        metric: MetricKind::Cos,
        quantization: ScalarKind::F32,
        connectivity: 0,
        expansion_add: 0,
        expansion_search: 0,
        multi: false,
    };
    let mut index = Index::new(&opts)?;
    let index_path = sidecar.join(INDEX_FILENAME);
    index
        .load(&index_path.to_string_lossy())
        .with_context(|| format!("load {}", index_path.display()))?;
    install_metric(&mut index, p.alpha, p.beta);

    // 3. Embed the query. For the struct half we seed layer-one-hot
    // bits from `--layer` so Tanimoto ranking against structurally
    // matching nodes becomes non-trivial. For queries without `--layer`
    // the struct half is all zeros → Tanimoto is a constant across
    // candidates and `beta` only shifts absolute scores (it does NOT
    // reorder results); document this in the CLI help.
    let mut embedder = Embedder::new(p.model, &resolve_cache_dir(), p.verbose)?;
    let query_vec_text = embedder
        .embed(vec![p.query_text.to_owned()])?
        .into_iter()
        .next()
        .context("empty embedding")?;
    let query_struct_bits = layer_seeded_struct(p.layer_filter);
    let mut query_vec = Vec::with_capacity(CONCAT_DIM);
    query_vec.extend_from_slice(&query_vec_text);
    extend_with_bits(&mut query_vec, &query_struct_bits);

    // 4. Search: pull back a pool for post-filtering, then trim to k.
    let pool = p.k.max(POST_FILTER_POOL);
    let results = index
        .search(&query_vec, pool)
        .context("usearch search failed")?;

    // 5. Load meta to translate keys → NodeMeta rows.
    let meta_path = sidecar.join(META_FILENAME);
    let meta = MetaStore::open_or_create(&meta_path)?;

    // 6. Build xpath/layer allowlist, if any.
    let allow = build_allowlist(spec_dir, p.xpath, p.layer_filter)?;

    // 7. Iterate results, apply filters, decompose scores.
    let mut hits: Vec<Hit> = Vec::new();
    let mut buf = vec![0f32; CONCAT_DIM];
    for (i, key) in results.keys.iter().enumerate() {
        let Some(node) = meta.get_by_key(i64::try_from(*key).unwrap_or(0))? else {
            continue;
        };
        if !layer_ok(&node.layer, p.layer_filter) {
            continue;
        }
        if let Some(set) = &allow
            && !set.contains(&node.id)
        {
            continue;
        }
        // Retrieve node vector to split into text half + struct half.
        // `get` returns the number of vectors copied (1 on a hit).
        let got = index.get(*key, buf.as_mut_slice()).unwrap_or(0);
        let (text_score, struct_score) = if got >= 1 {
            let text = cosine_sim(&query_vec_text, &buf[..DEFAULT_TEXT_DIM]);
            let struct_s = if query_struct_bits.iter().any(|b| *b != 0) {
                tanimoto_sim_bits(
                    &query_vec[DEFAULT_TEXT_DIM..],
                    &buf[DEFAULT_TEXT_DIM..],
                )
            } else {
                0.0
            };
            (text, struct_s)
        } else {
            (0.0, 0.0)
        };
        let score = p.alpha * text_score + p.beta * struct_score;
        hits.push(Hit {
            id: node.id,
            file: node.file,
            line_start: node.line_start,
            line_end: node.line_end,
            layer: node.layer,
            score,
            text_score,
            struct_score,
            preview: node.preview,
        });
        // Stop when we've retained enough.
        if hits.len() >= p.k && i >= POST_FILTER_POOL / 8 {
            // Still want all high-quality candidates; break at a
            // reasonable cap.
            break;
        }
    }

    // 8. Sort by score descending and truncate to k.
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(p.k);
    Ok(hits)
}

/// Seed a 256-bit structural fingerprint for the query based on the
/// user's `--layer` filter(s). Layer one-hot bits are set; all other
/// segments are zero (we don't know path/element/relations at query
/// time). Returns an all-zeros buffer when no layer filter is given.
fn layer_seeded_struct(layer_filter: &[String]) -> [u8; FINGERPRINT_BYTES] {
    let mut bits = [0u8; FINGERPRINT_BYTES];
    if layer_filter.is_empty() {
        return bits;
    }
    // Must match the canonical order in crate::fingerprint::LAYER_PREFIXES.
    let layers: &[&str] = &[
        "prose", "terminology", "organization", "relation", "decision",
        "source", "plan", "artifact", "llm", "revision", "index",
    ];
    for user in layer_filter {
        if let Some(idx) = layers.iter().position(|l| canonical_match(user, l)) {
            let byte = idx / 8;
            let bit = idx % 8;
            bits[byte] |= 1u8 << bit;
        }
    }
    bits
}

fn canonical_match(user: &str, canonical: &str) -> bool {
    if user == canonical {
        return true;
    }
    let short_pairs: &[(&str, &str)] = &[
        ("pr", "prose"),
        ("trm", "terminology"),
        ("org", "organization"),
        ("rel", "relation"),
        ("dec", "decision"),
        ("src", "source"),
        ("pln", "plan"),
        ("art", "artifact"),
        ("llm", "llm"),
        ("rev", "revision"),
        ("idx", "index"),
    ];
    short_pairs
        .iter()
        .any(|(s, l)| *s == user && *l == canonical)
}

fn extend_with_bits(out: &mut Vec<f32>, fp: &[u8; FINGERPRINT_BYTES]) {
    for byte in fp {
        for bit in 0..8 {
            out.push(if (byte >> bit) & 1 == 1 { 1.0 } else { 0.0 });
        }
    }
}

// Metric math lives in crate::index; reuse via `use` above.

fn layer_ok(layer: &str, filter: &[String]) -> bool {
    if filter.is_empty() {
        return true;
    }
    filter.iter().any(|f| f == layer || match_layer_alias(f, layer))
}

/// Map user-friendly layer names (e.g. "terminology") to the canonical
/// prefixes stored in `Chunk.layer` ("trm").
fn match_layer_alias(user: &str, canonical: &str) -> bool {
    // Accept short prefix forms from --layer flags (e.g. "trm" → "terminology").
    let aliases: &[(&str, &str)] = &[
        ("pr", "prose"),
        ("trm", "terminology"),
        ("org", "organization"),
        ("rel", "relation"),
        ("dec", "decision"),
        ("src", "source"),
        ("pln", "plan"),
        ("art", "artifact"),
        ("llm", "llm"),
        ("rev", "revision"),
        ("idx", "index"),
    ];
    aliases.iter().any(|(short, long)| {
        (*short == user && *long == canonical) || (*long == user && *short == canonical)
    })
}

fn build_allowlist(
    spec_dir: &Path,
    xpath: Option<&str>,
    layer_filter: &[String],
) -> Result<Option<HashSet<String>>> {
    let mut parts: Vec<HashSet<String>> = Vec::new();
    if let Some(expr) = xpath {
        parts.push(run_xpath(spec_dir, expr)?);
    }
    // Layer filter is handled inline via layer_ok (simpler than XPath).
    let _ = layer_filter;
    if parts.is_empty() {
        return Ok(None);
    }
    let mut iter = parts.into_iter();
    let mut acc = iter.next().unwrap_or_default();
    for next in iter {
        acc = &acc & &next;
    }
    Ok(Some(acc))
}

fn run_xpath(spec_dir: &Path, expr: &str) -> Result<HashSet<String>> {
    let index_path = clayers_spec::discovery::find_index_files(spec_dir)
        .with_context(|| format!("find_index_files {}", spec_dir.display()))?
        .into_iter()
        .next()
        .context("no index.xml")?;
    let files = clayers_spec::discovery::discover_spec_files(&index_path)?;
    let combined = combined_xml(&files)?;
    let ids = clayers_xml::query::xpath_to_id_set(&combined, expr, &[])
        .with_context(|| format!("xpath_to_id_set {expr}"))?;
    Ok(ids)
}

fn combined_xml(files: &[PathBuf]) -> Result<String> {
    clayers_spec::assembly::assemble_combined_string(files).context("assemble_combined_string")
}
