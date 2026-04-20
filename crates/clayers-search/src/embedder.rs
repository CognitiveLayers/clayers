//! Text embedder wrapping [`fastembed::TextEmbedding`].
//!
//! Default model: `BGE-small-en-v1.5` (384-dim). Model cache resolves
//! to `$XDG_CACHE_HOME/clayers/models/` or `~/.cache/clayers/models/`
//! unless `HF_HOME` is set, in which case the user's choice wins and
//! the effective path is logged.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};

/// Dimension of the default `BGE-small-en-v1.5` embedding.
pub const DEFAULT_TEXT_DIM: usize = 384;

/// Canonical name of the default embedder model.
pub const DEFAULT_MODEL_NAME: &str = "bge-small-en-v1.5";

/// Models we know produce a 384-dim embedding. Adding a model of a
/// different dimensionality requires parameterizing
/// [`crate::index::CONCAT_DIM`] and friends; until that refactor, we
/// reject non-384-dim models at init.
pub const SUPPORTED_MODELS: &[&str] = &[
    "bge-small-en-v1.5",
    "all-minilm-l6-v2",
    "multilingual-e5-small",
];

/// Resolve the clayers-search model cache directory.
///
/// Precedence: `XDG_CACHE_HOME` → `HOME/.cache` → `/tmp`. The caller
/// should log that `HF_HOME` (if set) silently overrides this path
/// via `fastembed`'s `hf-hub` transitive dep.
#[must_use]
pub fn resolve_cache_dir() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(xdg).join("clayers").join("models");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".cache").join("clayers").join("models");
    }
    PathBuf::from("/tmp/clayers-models")
}

/// Embedder handle around `fastembed::TextEmbedding`.
///
/// `.embed(...)` takes `&mut self` (fastembed 5 API change), so the
/// indexer must hold this exclusively during a build.
pub struct Embedder {
    inner: TextEmbedding,
}

impl Embedder {
    /// Initialize the embedder, downloading the model on first use.
    ///
    /// # Errors
    ///
    /// Returns an error if the model name is unknown or the model
    /// cannot be loaded.
    pub fn new(model_name: &str, cache_dir: &Path, verbose: bool) -> Result<Self> {
        let model = match model_name {
            "bge-small-en-v1.5" | "BGESmallENV15" => EmbeddingModel::BGESmallENV15,
            "all-minilm-l6-v2" | "AllMiniLML6V2" => EmbeddingModel::AllMiniLML6V2,
            "multilingual-e5-small" | "MultilingualE5Small" => {
                EmbeddingModel::MultilingualE5Small
            }
            other => anyhow::bail!(
                "unknown or unsupported model: {other}; supported 384-dim models: {SUPPORTED_MODELS:?}"
            ),
        };
        std::fs::create_dir_all(cache_dir).with_context(|| {
            format!("creating model cache dir {}", cache_dir.display())
        })?;

        let effective = std::env::var_os("HF_HOME").map_or_else(
            || cache_dir.display().to_string(),
            |h| format!("{} (HF_HOME override)", PathBuf::from(h).display()),
        );
        if verbose {
            eprintln!("clayers-search: model cache at {effective}");
        }

        let opts = TextInitOptions::new(model)
            .with_cache_dir(cache_dir.to_path_buf())
            .with_show_download_progress(verbose);
        let inner = TextEmbedding::try_new(opts)
            .context("failed to initialize TextEmbedding")?;
        Ok(Self { inner })
    }

    /// Embed a batch of documents. Returns one `Vec<f32>` per input.
    ///
    /// # Errors
    ///
    /// Returns an error if embedding fails.
    pub fn embed(&mut self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        self.inner.embed(texts, None).context("embed failed")
    }
}
