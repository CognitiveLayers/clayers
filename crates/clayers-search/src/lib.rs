//! Semantic search for clayers specs.
//!
//! Two-pass chunking over a combined-document spec, a 256-bit structural
//! fingerprint, `HuggingFace` text embeddings via `fastembed`, and a
//! [`usearch`] index with a custom metric
//! (`alpha * cosine(text) + beta * tanimoto(struct)`).
//!
//! This crate is a normal workspace member; it is pulled into the
//! top-level `clayers` binary only when the `semantic-search` Cargo
//! feature is enabled there.

pub mod embedder;
pub mod fingerprint;
pub mod index;
pub mod meta;
pub mod query;
