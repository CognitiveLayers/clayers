//! `USearch`-backed concatenated index with custom metric + incremental rebuild.
//!
//! Stores `[text_384 | struct_256_as_f32]` vectors at dim [`CONCAT_DIM`].
//! The custom metric `alpha * cosine(text) + beta * tanimoto(struct)` is
//! installed via `Index::change_metric` and is invoked during `HNSW`
//! traversal (verified in derisk H2).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use clayers_spec::chunker::extract_chunks;
use serde::{Deserialize, Serialize};
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

use crate::embedder::{DEFAULT_TEXT_DIM, Embedder, resolve_cache_dir};
use crate::fingerprint::{FINGERPRINT_BITS, FINGERPRINT_BYTES, fingerprint};
use crate::meta::{META_FILENAME, MetaStore, NodeMeta};

/// Dimension of the concatenated vector stored in the `usearch` index:
/// 384 text dims + 256 structural-bit dims (each bit stored as an
/// `f32` of value 0.0 or 1.0).
pub const CONCAT_DIM: usize = DEFAULT_TEXT_DIM + FINGERPRINT_BITS;

/// Filename of the `usearch` index inside `.clayers/search/`.
pub const INDEX_FILENAME: &str = "index.usearch";

/// Filename of the build configuration.
pub const CONFIG_FILENAME: &str = "config.json";

/// Lockfile protecting the sidecar from concurrent builds.
pub const LOCK_FILENAME: &str = ".lock";

/// Current fingerprint-layout version. Bump when the
/// [`crate::fingerprint`] bit layout changes; triggers full rebuild.
pub const FINGERPRINT_VERSION: &str = "1";

/// On-disk build configuration. Changes to `model` or `fingerprint_version`
/// trigger a full rebuild.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub model: String,
    pub dim_text: usize,
    pub dim_struct: usize,
    pub built_at: String,
    pub spec_revision: String,
    pub fingerprint_version: String,
}

/// Summary of a build/update run.
#[derive(Debug, Clone, Copy)]
pub struct BuildReport {
    pub total_nodes: usize,
    pub re_embedded: usize,
    pub reused: usize,
    pub removed: usize,
}

/// Path of the sidecar directory for a given spec.
#[must_use]
pub fn sidecar_path(spec_dir: &Path) -> PathBuf {
    spec_dir.join(".clayers").join("search")
}

/// Build or incrementally update the search index for `spec_dir`.
///
/// # Errors
///
/// Returns an error on any I/O, embedding, or index-manipulation failure.
#[allow(clippy::too_many_lines)]
pub fn build_or_update(
    spec_dir: &Path,
    model_name: &str,
    force_rebuild: bool,
    verbose: bool,
) -> Result<BuildReport> {
    let sidecar = sidecar_path(spec_dir);
    std::fs::create_dir_all(&sidecar)
        .with_context(|| format!("create {}", sidecar.display()))?;

    // Acquire an exclusive build lock; released on drop.
    let _lock = BuildLock::acquire(&sidecar)?;

    let cfg_path = sidecar.join(CONFIG_FILENAME);
    let meta_path = sidecar.join(META_FILENAME);
    let index_path = sidecar.join(INDEX_FILENAME);

    // Detect model-name change → forces full rebuild.
    let existing_cfg = std::fs::read_to_string(&cfg_path)
        .ok()
        .and_then(|s| serde_json::from_str::<Config>(&s).ok());
    let model_changed = existing_cfg.as_ref().is_some_and(|c| c.model != model_name);
    let fp_changed = existing_cfg
        .as_ref()
        .is_some_and(|c| c.fingerprint_version != FINGERPRINT_VERSION);

    if force_rebuild || model_changed || fp_changed {
        let _ = std::fs::remove_file(&meta_path);
        let _ = std::fs::remove_file(&index_path);
    }

    let meta = MetaStore::open_or_create(&meta_path)?;
    let chunks = extract_chunks(spec_dir).context("chunker failed")?;

    // ---- usearch index: load existing or create fresh ----
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
    if index_path.exists() {
        index
            .load(&index_path.to_string_lossy())
            .with_context(|| format!("load {}", index_path.display()))?;
    }
    install_metric(&mut index, DEFAULT_ALPHA, DEFAULT_BETA);
    index.reserve(chunks.len().max(16))?;

    // ---- Cleanup: remove ids that vanished from the spec ----
    let current_ids: HashSet<String> = chunks.iter().map(|c| c.id.clone()).collect();
    let mut removed = 0usize;
    for existing_id in meta.all_ids()? {
        if !current_ids.contains(&existing_id)
            && let Some(existing) = meta.lookup_by_id(&existing_id)?
        {
            #[allow(clippy::cast_sign_loss)]
            let key_u64 = existing.key as u64;
            let _ = index.remove(key_u64);
            meta.delete(&existing_id)?;
            removed += 1;
        }
    }

    // ---- Decide which chunks need re-embedding ----
    let mut to_embed: Vec<(usize, i64)> = Vec::new(); // (chunk idx, key)
    let mut reused = 0usize;
    let mut max_key = meta.max_key()?;

    for (idx, chunk) in chunks.iter().enumerate() {
        match meta.lookup_by_id(&chunk.id)? {
            Some(existing) if existing.node_hash == chunk.node_hash => {
                reused += 1;
            }
            Some(existing) => {
                to_embed.push((idx, existing.key));
            }
            None => {
                max_key += 1;
                to_embed.push((idx, max_key));
            }
        }
    }

    // ---- Embed + insert ----
    let re_embedded = to_embed.len();
    if !to_embed.is_empty() {
        let cache = resolve_cache_dir();
        let mut embedder = Embedder::new(model_name, &cache, verbose)?;
        let texts: Vec<String> =
            to_embed.iter().map(|(i, _)| chunks[*i].text.clone()).collect();
        let embeddings = embedder.embed(texts)?;

        for ((chunk_idx, key), emb) in to_embed.iter().zip(embeddings) {
            let chunk = &chunks[*chunk_idx];
            let fp_bytes = fingerprint(chunk);
            let full = build_concat_vector(&emb, &fp_bytes);

            #[allow(clippy::cast_sign_loss)]
            let key_u64 = *key as u64;

            // If key already present (changed hash case), remove first.
            let _ = index.remove(key_u64);
            index
                .add(key_u64, &full)
                .with_context(|| format!("usearch add for {}", chunk.id))?;

            meta.upsert(&NodeMeta {
                id: chunk.id.clone(),
                file: chunk.file.display().to_string(),
                line_start: i64::try_from(chunk.line_start).unwrap_or(0),
                line_end: i64::try_from(chunk.line_end).unwrap_or(0),
                layer: chunk.layer.clone(),
                namespace: chunk.namespace.clone(),
                node_hash: chunk.node_hash.clone(),
                preview: preview(&chunk.text),
                key: *key,
            })?;
        }
    }

    // ---- Persist ----
    index
        .save(&index_path.to_string_lossy())
        .with_context(|| format!("save {}", index_path.display()))?;

    let cfg = Config {
        model: model_name.to_string(),
        dim_text: DEFAULT_TEXT_DIM,
        dim_struct: FINGERPRINT_BITS,
        built_at: iso8601_now(),
        spec_revision: existing_cfg
            .as_ref()
            .map_or_else(|| "draft-1".into(), |c| c.spec_revision.clone()),
        fingerprint_version: FINGERPRINT_VERSION.into(),
    };
    std::fs::write(&cfg_path, serde_json::to_vec_pretty(&cfg)?)
        .with_context(|| format!("write {}", cfg_path.display()))?;

    // Pinned log format consumed by test_step_07_incremental_rebuild_skips_unchanged.
    eprintln!("re-embedded: {re_embedded} reused: {reused} removed: {removed}");

    Ok(BuildReport {
        total_nodes: chunks.len(),
        re_embedded,
        reused,
        removed,
    })
}

/// Default cosine-side weight (α).
pub const DEFAULT_ALPHA: f32 = 0.7;
/// Default tanimoto-side weight (β).
pub const DEFAULT_BETA: f32 = 0.3;

/// Install the weighted custom metric
/// `alpha * cosine_distance(text) + beta * tanimoto_distance(struct)`.
///
/// Invoked during HNSW graph construction (via index builds) and
/// during `Index::search` (via query-time invocations). The closure
/// captures `alpha`/`beta` by value; call again to update weights.
pub fn install_metric(index: &mut Index, alpha: f32, beta: f32) {
    let dim = CONCAT_DIM;
    let text_dim = DEFAULT_TEXT_DIM;
    let closure = move |a: *const f32, b: *const f32| -> f32 {
        // SAFETY: usearch passes pointers to same-length vectors.
        let (sa, sb) = unsafe {
            (
                std::slice::from_raw_parts(a, dim),
                std::slice::from_raw_parts(b, dim),
            )
        };
        let cos_sim = cosine_sim(&sa[..text_dim], &sb[..text_dim]);
        let tan_sim = tanimoto_sim_bits(&sa[text_dim..], &sb[text_dim..]);
        alpha * (1.0 - cos_sim) + beta * (1.0 - tan_sim)
    };
    index.change_metric(Box::new(closure));
}

/// Cosine similarity over two equal-length slices.
#[must_use]
pub fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    cosine_sim_impl(a, b)
}

/// Tanimoto similarity over two bit-packed f32 vectors (values ≥ 0.5
/// treated as set bits).
#[must_use]
pub fn tanimoto_sim_bits(a: &[f32], b: &[f32]) -> f32 {
    tanimoto_sim_bits_impl(a, b)
}

/// File-based exclusive lock for an index build. Created atomically
/// via `create_new`; if another process already holds the lock, we
/// bail out with a clear error instead of clobbering state. Deleted
/// when this guard drops (happy or panic path).
struct BuildLock {
    path: PathBuf,
}

/// Consider a lockfile stale (crashed/abandoned) after this long.
const STALE_LOCK_SECS: u64 = 600; // 10 minutes

impl BuildLock {
    fn acquire(sidecar: &Path) -> Result<Self> {
        let path = sidecar.join(LOCK_FILENAME);
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
        {
            Ok(_) => Ok(Self { path }),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // A crashed build may have left the lockfile behind.
                // If it's older than STALE_LOCK_SECS, reclaim it.
                if let Ok(meta) = std::fs::metadata(&path)
                    && let Ok(modified) = meta.modified()
                    && let Ok(age) = SystemTime::now().duration_since(modified)
                    && age.as_secs() > STALE_LOCK_SECS
                {
                    eprintln!(
                        "clayers-search: reclaiming stale lock at {} (age {}s)",
                        path.display(),
                        age.as_secs(),
                    );
                    let _ = std::fs::remove_file(&path);
                    return Self::acquire(sidecar);
                }
                anyhow::bail!(
                    "another clayers search build is in progress ({}); \
                     if you're sure no build is running, remove the file.",
                    path.display()
                );
            }
            Err(e) => Err(e).with_context(|| format!("create {}", path.display())),
        }
    }
}

impl Drop for BuildLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn cosine_sim_impl(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na < 1e-12 || nb < 1e-12 {
        return 0.0;
    }
    dot / (na * nb)
}

fn tanimoto_sim_bits_impl(a: &[f32], b: &[f32]) -> f32 {
    let mut and = 0u32;
    let mut or = 0u32;
    for (x, y) in a.iter().zip(b) {
        let xa = *x >= 0.5;
        let yb = *y >= 0.5;
        if xa && yb {
            and += 1;
        }
        if xa || yb {
            or += 1;
        }
    }
    if or == 0 {
        1.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        (and as f32 / or as f32)
    }
}

fn build_concat_vector(text: &[f32], fp: &[u8; FINGERPRINT_BYTES]) -> Vec<f32> {
    let mut out = Vec::with_capacity(CONCAT_DIM);
    out.extend_from_slice(text);
    for byte in fp {
        for bit in 0..8 {
            out.push(if (byte >> bit) & 1 == 1 { 1.0 } else { 0.0 });
        }
    }
    out
}

fn preview(text: &str) -> String {
    // Strip the chunker's context header `[layer=X path=...]\n` — it's
    // a feature for the embedder, not for human readers. Everything
    // after the first newline is the body.
    let body_raw = text
        .split_once('\n')
        .map_or(text, |(_, rest)| rest);
    // Collapse internal whitespace so the renderer controls wrapping.
    let body: String = body_raw
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    // Store plenty of context (≤600 chars) — the CLI renderer
    // wraps/truncates at display time based on terminal width.
    let max = 600;
    if body.chars().count() <= max {
        body
    } else {
        let truncated: String = body.chars().take(max).collect();
        format!("{truncated}…")
    }
}

#[cfg(test)]
mod preview_tests {
    use super::*;

    #[test]
    fn preview_strips_context_header() {
        let raw = "[layer=terminology path=spec>term]\nDrift\n\nDivergence between spec and code.";
        let p = preview(raw);
        assert!(!p.contains("layer="), "preview must not leak header: {p}");
        assert!(!p.contains("path="), "preview must not leak path: {p}");
        assert!(p.starts_with("Drift"), "preview lost body start: {p}");
    }

    #[test]
    fn preview_collapses_internal_whitespace() {
        let raw = "[layer=pr path=s>p]\nHello\n\n  world\n    again";
        let p = preview(raw);
        assert_eq!(p, "Hello world again");
    }

    #[test]
    fn preview_truncates_very_long_bodies() {
        let raw = format!("[layer=x path=y]\n{}", "a".repeat(2000));
        let p = preview(&raw);
        assert!(p.ends_with('…'));
        assert!(p.chars().count() <= 601);
    }

    #[test]
    fn preview_keeps_short_bodies_intact() {
        let raw = "[layer=trm path=spec>term]\nShort term.";
        let p = preview(raw);
        assert!(!p.ends_with('…'));
        assert_eq!(p, "Short term.");
    }
}

#[allow(clippy::many_single_char_names)]
fn iso8601_now() -> String {
    use std::time::UNIX_EPOCH;
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = secs / 86_400;
    let in_day = secs % 86_400;
    let hour = in_day / 3600;
    let min = (in_day / 60) % 60;
    let sec = in_day % 60;
    let (year, month, day) = civil_date(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
}

/// Convert days since 1970-01-01 into (year, month, day) using Howard
/// Hinnant's `days_from_civil`. Valid for any reasonable future/past
/// within `u64::MAX` / `86_400`.
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
fn civil_date(days_since_epoch: u64) -> (u16, u8, u8) {
    let z = days_since_epoch as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { (mp + 3) as u8 } else { (mp - 9) as u8 };
    if month <= 2 {
        year += 1;
    }
    (year as u16, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_sim_identical_is_one() {
        let v = vec![1.0, 2.0, 3.0];
        assert!((cosine_sim(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_sim_orthogonal_is_zero() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_sim(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn tanimoto_identical_is_one() {
        let a = vec![1.0, 0.0, 1.0, 1.0];
        assert!((tanimoto_sim_bits(&a, &a) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn tanimoto_disjoint_is_zero() {
        let a = vec![1.0, 1.0, 0.0, 0.0];
        let b = vec![0.0, 0.0, 1.0, 1.0];
        assert!(tanimoto_sim_bits(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn build_concat_vector_dim_is_640() {
        let text = vec![0.0f32; DEFAULT_TEXT_DIM];
        let fp = [0u8; FINGERPRINT_BYTES];
        let full = build_concat_vector(&text, &fp);
        assert_eq!(full.len(), CONCAT_DIM);
    }

    #[test]
    fn build_concat_vector_packs_bits_correctly() {
        let text = vec![0.0f32; DEFAULT_TEXT_DIM];
        let mut fp = [0u8; FINGERPRINT_BYTES];
        fp[0] = 0b0000_0011; // bits 0 and 1 set
        let full = build_concat_vector(&text, &fp);
        assert!((full[DEFAULT_TEXT_DIM] - 1.0).abs() < 1e-6);
        assert!((full[DEFAULT_TEXT_DIM + 1] - 1.0).abs() < 1e-6);
        assert!(full[DEFAULT_TEXT_DIM + 2].abs() < 1e-6);
    }
}
