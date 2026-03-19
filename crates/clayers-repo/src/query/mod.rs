//! `XPath` queries on repository objects.
//!
//! Provides `QueryStore` trait, default `XPath` evaluation via xee-xpath,
//! revision resolution, and cross-ref search.

#[cfg(test)]
pub(crate) mod tests;

use std::collections::HashMap;
use std::pin::pin;

use async_trait::async_trait;
use clayers_xml::ContentHash;
use futures_core::Stream;
use crate::error::{Error, Result};
use crate::object::Object;
use crate::refs;
use crate::store::{ObjectStore, RefStore};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Output mode for `XPath` queries.
#[derive(Debug, Clone, Copy)]
pub enum QueryMode {
    /// Return the count of matching nodes.
    Count,
    /// Return the text content of matching nodes.
    Text,
    /// Return the serialized XML of matching nodes.
    Xml,
}

/// Result of an `XPath` query.
#[derive(Debug)]
pub enum QueryResult {
    /// Node count.
    Count(usize),
    /// Text content of each matching node.
    Text(Vec<String>),
    /// Serialized XML of each matching node.
    Xml(Vec<String>),
}

/// Namespace prefix-to-URI map for `XPath` evaluation.
pub type NamespaceMap = Vec<(String, String)>;

/// Per-document query result, pairing a file path with its matches.
#[derive(Debug)]
pub struct DocumentQueryResult {
    /// File path within the tree (e.g., `overview.xml`).
    pub path: String,
    /// The query result for this document.
    pub result: QueryResult,
}

// ---------------------------------------------------------------------------
// QueryStore trait
// ---------------------------------------------------------------------------

/// Trait for querying documents stored in the object store.
///
/// Backends can override this to use specialized indexing/search strategies.
/// The default implementation collects the subtree, builds XML via export,
/// and evaluates `XPath` using xee-xpath.
#[async_trait]
pub trait QueryStore: Send + Sync {
    /// Query a document by its hash.
    async fn query_document(
        &self,
        doc_hash: ContentHash,
        xpath: &str,
        mode: QueryMode,
        namespaces: &NamespaceMap,
    ) -> Result<QueryResult>;
}

// ---------------------------------------------------------------------------
// Default implementation
// ---------------------------------------------------------------------------

/// Collect a stream into a `HashMap`.
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

/// Default query implementation: collect subtree, serialize to XML, evaluate
/// `XPath` via xee-xpath.
///
/// # Errors
///
/// Returns an error if the document cannot be loaded or the `XPath` is invalid.
pub async fn default_query_document(
    store: &dyn ObjectStore,
    doc_hash: ContentHash,
    xpath: &str,
    mode: QueryMode,
    namespaces: &NamespaceMap,
) -> Result<QueryResult> {
    let objects = try_collect_stream(store.subtree(&doc_hash)).await?;

    // Find the document root.
    let root_hash = match objects.get(&doc_hash) {
        Some(Object::Document(doc)) => doc.root,
        Some(_) => return Err(Error::InvalidObject("expected Document object".into())),
        None => return Err(Error::NotFound(doc_hash)),
    };

    // Serialize objects to XML string via the export module.
    let xml_string = crate::export::build_xml_from_objects(&objects, root_hash)?;

    // Evaluate XPath in a plain fn (all !Send xee-xpath types stay off the async stack).
    let ns_refs: Vec<(&str, &str)> = namespaces
        .iter()
        .map(|(p, u)| (p.as_str(), u.as_str()))
        .collect();
    let xml_mode = match mode {
        QueryMode::Count => clayers_xml::query::QueryMode::Count,
        QueryMode::Text => clayers_xml::query::QueryMode::Text,
        QueryMode::Xml => clayers_xml::query::QueryMode::Xml,
    };
    let result = clayers_xml::query::evaluate_xpath(&xml_string, xpath, xml_mode, &ns_refs)?;
    Ok(match result {
        clayers_xml::query::QueryResult::Count(n) => QueryResult::Count(n),
        clayers_xml::query::QueryResult::Text(t) => QueryResult::Text(t),
        clayers_xml::query::QueryResult::Xml(x) => QueryResult::Xml(x),
    })
}

// ---------------------------------------------------------------------------
// Revision resolution
// ---------------------------------------------------------------------------

/// Resolve a revspec string to a document `ContentHash`.
///
/// Handles: raw hex hash, `refs/heads/{name}`, `refs/tags/{name}`, `HEAD`,
/// bare branch/tag names. Follows commits through trees to reach a document
/// (uses the first tree entry's document for backwards compatibility).
///
/// # Errors
///
/// Returns an error if the revspec cannot be resolved.
pub async fn resolve_to_document(
    store: &dyn ObjectStore,
    ref_store: &dyn RefStore,
    revspec: &str,
) -> Result<ContentHash> {
    let hash = resolve_revspec(ref_store, revspec).await?;
    // Follow commits/tags to reach a document (via tree).
    follow_to_document(store, hash).await
}

/// Resolve a revspec string to a tree `ContentHash` and `TreeObject`.
///
/// # Errors
///
/// Returns an error if the revspec cannot be resolved or doesn't point to a tree.
pub async fn resolve_to_tree(
    store: &dyn ObjectStore,
    ref_store: &dyn RefStore,
    revspec: &str,
) -> Result<(ContentHash, crate::object::TreeObject)> {
    let hash = resolve_revspec(ref_store, revspec).await?;
    let tree_hash = follow_to_tree(store, hash).await?;
    let obj = store.get(&tree_hash).await?.ok_or(Error::NotFound(tree_hash))?;
    let Object::Tree(t) = obj else {
        return Err(Error::InvalidObject("expected Tree object".into()));
    };
    Ok((tree_hash, t))
}

/// Resolve a revspec string to a commit/tag/direct hash.
///
/// # Errors
///
/// Returns an error if the revspec cannot be resolved.
pub async fn resolve_revspec(
    ref_store: &dyn RefStore,
    revspec: &str,
) -> Result<ContentHash> {
    if let Ok(h) = try_parse_hash(revspec) {
        Ok(h)
    } else if revspec == "HEAD" {
        refs::resolve_head(ref_store)
            .await?
            .ok_or_else(|| Error::Ref("HEAD not set".into()))
    } else if revspec.starts_with("refs/") {
        ref_store
            .get_ref(revspec)
            .await?
            .ok_or_else(|| Error::Ref(format!("ref not found: {revspec}")))
    } else if let Some(h) = ref_store.get_ref(&refs::branch_ref(revspec)).await? {
        Ok(h)
    } else if let Some(h) = ref_store.get_ref(&refs::tag_ref(revspec)).await? {
        Ok(h)
    } else {
        Err(Error::Ref(format!("cannot resolve revspec: {revspec}")))
    }
}

/// Try to parse a hex string as a `ContentHash`.
fn try_parse_hash(s: &str) -> Result<ContentHash> {
    if s.len() != 64 {
        return Err(Error::Ref("not a valid hash".into()));
    }
    let bytes: Vec<u8> = (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect::<std::result::Result<Vec<u8>, _>>()
        .map_err(|_| Error::Ref("not a valid hex hash".into()))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| Error::Ref("not 32 bytes".into()))?;
    Ok(ContentHash(arr))
}

/// Follow Commit->tree->first entry document, Tag->target->recurse until we
/// reach a Document hash. For backwards compatibility with single-document queries.
async fn follow_to_document(
    store: &dyn ObjectStore,
    hash: ContentHash,
) -> Result<ContentHash> {
    let obj = store.get(&hash).await?.ok_or(Error::NotFound(hash))?;
    match obj {
        Object::Document(_) => Ok(hash),
        Object::Tree(t) => {
            // Return the first entry's document for backwards compatibility.
            t.entries.first()
                .map(|e| e.document)
                .ok_or_else(|| Error::InvalidObject("empty tree has no documents".into()))
        }
        Object::Commit(c) => Box::pin(follow_to_document(store, c.tree)).await,
        Object::Tag(t) => Box::pin(follow_to_document(store, t.target)).await,
        _ => Err(Error::InvalidObject(
            "revspec resolved to a non-versioning object".into(),
        )),
    }
}

/// Follow Commit->tree, Tag->target->recurse until we reach a Tree hash.
///
/// # Errors
///
/// Returns an error if objects cannot be loaded or the chain leads to a non-versioning object.
pub async fn follow_to_tree(
    store: &dyn ObjectStore,
    hash: ContentHash,
) -> Result<ContentHash> {
    let obj = store.get(&hash).await?.ok_or(Error::NotFound(hash))?;
    match obj {
        Object::Tree(_) => Ok(hash),
        Object::Commit(c) => Ok(c.tree),
        Object::Tag(t) => Box::pin(follow_to_tree(store, t.target)).await,
        _ => Err(Error::InvalidObject(
            "revspec resolved to a non-versioning object".into(),
        )),
    }
}

// ---------------------------------------------------------------------------
// Cross-ref search
// ---------------------------------------------------------------------------

/// Result of querying across multiple refs.
#[derive(Debug)]
pub struct RefQueryResult {
    /// The ref name (e.g., `refs/heads/main`).
    pub ref_name: String,
    /// The commit hash the ref points to.
    pub commit_hash: ContentHash,
    /// The document hash after following the commit.
    pub doc_hash: ContentHash,
    /// The query result for this document.
    pub result: QueryResult,
}

/// Query all refs matching a prefix, deduplicating on document hash.
///
/// # Errors
///
/// Returns an error if refs cannot be listed or queries fail.
pub async fn query_refs(
    store: &(dyn ObjectStore + Sync),
    query_store: &dyn QueryStore,
    ref_store: &dyn RefStore,
    prefix: &str,
    xpath: &str,
    mode: QueryMode,
    namespaces: &NamespaceMap,
) -> Result<Vec<RefQueryResult>> {
    let all_refs = ref_store.list_refs(prefix).await?;
    let mut results = Vec::new();
    let mut seen_docs = std::collections::HashSet::new();

    for (ref_name, commit_hash) in all_refs {
        let tree_hash = follow_to_tree(store, commit_hash).await?;
        if !seen_docs.insert(tree_hash) {
            continue; // Already queried this tree.
        }
        let tree_obj = store.get(&tree_hash).await?.ok_or(Error::NotFound(tree_hash))?;
        let Object::Tree(tree) = tree_obj else {
            return Err(Error::InvalidObject("expected Tree object".into()));
        };
        // Query each document in the tree, aggregate results.
        // Skip documents where XPath compilation fails (unknown prefix).
        let mut doc_results = Vec::new();
        for entry in &tree.entries {
            match query_store
                .query_document(entry.document, xpath, mode, namespaces)
                .await
            {
                Ok(result) => {
                    doc_results.push(DocumentQueryResult {
                        path: entry.path.clone(),
                        result,
                    });
                }
                Err(Error::Xml(ref e)) if e.to_string().contains("compile error") => {}
                Err(e) => return Err(e),
            }
        }
        let combined = aggregate_results(mode, doc_results);
        let doc_hash = tree.entries.first()
            .map_or(tree_hash, |e| e.document);
        results.push(RefQueryResult {
            ref_name,
            commit_hash,
            doc_hash,
            result: combined,
        });
    }

    Ok(results)
}

/// Resolve a revspec and query each document in the tree, returning
/// per-document results with file paths.
///
/// When `files` is non-empty, only documents whose path matches one of the
/// entries are queried (substring match on the tree entry path).
///
/// # Errors
///
/// Returns an error if resolution or query fails.
#[allow(clippy::too_many_arguments)]
pub async fn query_by_document(
    store: &(dyn ObjectStore + Sync),
    query_store: &dyn QueryStore,
    ref_store: &dyn RefStore,
    revspec: &str,
    xpath: &str,
    mode: QueryMode,
    namespaces: &NamespaceMap,
    files: &[String],
) -> Result<Vec<DocumentQueryResult>> {
    let hash = resolve_revspec(ref_store, revspec).await?;
    let tree_hash = follow_to_tree(store, hash).await?;
    let tree_obj = store.get(&tree_hash).await?.ok_or(Error::NotFound(tree_hash))?;
    let Object::Tree(tree) = tree_obj else {
        return Err(Error::InvalidObject("expected Tree object".into()));
    };

    let mut results = Vec::new();
    for entry in &tree.entries {
        // Apply file filter if specified.
        if !files.is_empty() && !files.iter().any(|f| entry.path.contains(f.as_str())) {
            continue;
        }

        match query_store
            .query_document(entry.document, xpath, mode, namespaces)
            .await
        {
            Ok(result) => {
                // Skip documents with zero matches.
                let has_matches = match &result {
                    QueryResult::Count(0) => false,
                    QueryResult::Count(_) => true,
                    QueryResult::Text(t) => !t.is_empty(),
                    QueryResult::Xml(x) => !x.is_empty(),
                };
                if has_matches {
                    results.push(DocumentQueryResult {
                        path: entry.path.clone(),
                        result,
                    });
                }
            }
            Err(Error::Xml(ref e)) if e.to_string().contains("compile error") => {
                // Document doesn't know the namespace prefix; skip it.
            }
            Err(e) => return Err(e),
        }
    }
    Ok(results)
}

/// Convenience: resolve a revspec, query all documents, aggregate results.
///
/// # Errors
///
/// Returns an error if resolution or query fails.
pub async fn query(
    store: &(dyn ObjectStore + Sync),
    query_store: &dyn QueryStore,
    ref_store: &dyn RefStore,
    revspec: &str,
    xpath: &str,
    mode: QueryMode,
    namespaces: &NamespaceMap,
) -> Result<QueryResult> {
    let docs = query_by_document(
        store, query_store, ref_store, revspec, xpath, mode, namespaces, &[],
    )
    .await?;
    Ok(aggregate_results(mode, docs))
}

/// Aggregate per-document results into a single combined result.
fn aggregate_results(mode: QueryMode, docs: Vec<DocumentQueryResult>) -> QueryResult {
    let mut combined_count = 0usize;
    let mut combined_texts = Vec::new();
    let mut combined_xmls = Vec::new();
    for doc in docs {
        match doc.result {
            QueryResult::Count(n) => combined_count += n,
            QueryResult::Text(ts) => combined_texts.extend(ts),
            QueryResult::Xml(xs) => combined_xmls.extend(xs),
        }
    }
    match mode {
        QueryMode::Count => QueryResult::Count(combined_count),
        QueryMode::Text => QueryResult::Text(combined_texts),
        QueryMode::Xml => QueryResult::Xml(combined_xmls),
    }
}

