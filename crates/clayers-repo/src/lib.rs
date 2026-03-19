//! A git-like, offline-first version control system for structural XML.
//!
//! Operates on content-addressed Merkle DAGs of XML Infoset nodes rather
//! than text files. XML documents are decomposed into their constituent
//! nodes, each content-addressed by its Exclusive C14N hash, and stored
//! in a Merkle DAG.
//!
//! # Architecture
//!
//! - **Object model** (`object`): XML Infoset nodes + versioning objects,
//!   all content-addressed via SHA-256(ExclusiveC14N).
//! - **Storage** (`store`): Async traits for object store and ref store,
//!   with in-memory and optional `SQLite` backends.
//! - **Import/Export** (`import`, `export`): Bidirectional conversion
//!   between XML strings and the Merkle DAG.
//! - **Diff** (`diff`): Structural tree diff exploiting Merkle hashes.
//! - **Conflict** (`conflict`): Divergence elements for concurrent edits.
//! - **Repository** (`repo`): Porcelain API composing all components.

#![allow(clippy::module_name_repetitions)]

pub mod diff;
pub mod error;
pub mod export;
pub mod graph;
pub mod hash;
pub mod import;
pub mod conflict;
pub mod object;
pub mod query;
pub mod refs;
pub mod repo;
pub mod store;
pub mod sync;

pub use diff::FileChange;
pub use error::{Error, Result};
pub use object::{
    Attribute, Author, CommitObject, CommentObject, DocumentObject, ElementObject, Object,
    PIObject, TagObject, TextObject, TreeEntry, TreeObject, REPO_NS,
};
pub use repo::Repo;
pub use query::{QueryStore, QueryMode, QueryResult, DocumentQueryResult, NamespaceMap, resolve_revspec};
pub use store::{ObjectStore, RefStore, Transaction};
pub use store::memory::MemoryStore;
pub use sync::{FastForwardOnly, Overwrite, RefConflict, Reject};
#[cfg(feature = "sqlite")]
pub use store::sqlite::SqliteStore;
