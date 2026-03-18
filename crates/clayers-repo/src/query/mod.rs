//! `XPath` queries on repository objects.
//!
//! Provides `QueryStore` trait, default `XPath` evaluation via xot, revision
//! resolution, and cross-ref search.

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

// ---------------------------------------------------------------------------
// QueryStore trait
// ---------------------------------------------------------------------------

/// Trait for querying documents stored in the object store.
///
/// Backends can override this to use specialized indexing/search strategies.
/// The default implementation collects the subtree, builds a xot tree, and
/// evaluates `XPath`.
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

/// Default query implementation: collect subtree, build xot, evaluate `XPath`.
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

    // Build xot tree directly from objects.
    let (mut xot, xot_root) = build_xot_from_objects(&objects, root_hash)?;

    // Pre-collect predicate attribute names.
    let attr_names = collect_predicate_attr_names(xpath)?;

    // Evaluate `XPath`.
    match mode {
        QueryMode::Count => {
            let nodes = find_matching_nodes(&mut xot, xot_root, xpath, namespaces, &attr_names)?;
            Ok(QueryResult::Count(nodes.len()))
        }
        QueryMode::Text => {
            let nodes = find_matching_nodes(&mut xot, xot_root, xpath, namespaces, &attr_names)?;
            let texts = nodes.into_iter().map(|n| collect_all_text(&xot, n)).collect();
            Ok(QueryResult::Text(texts))
        }
        QueryMode::Xml => {
            let nodes = find_matching_nodes(&mut xot, xot_root, xpath, namespaces, &attr_names)?;
            let xmls = nodes
                .into_iter()
                .map(|n| xot.to_string(n).unwrap_or_default())
                .collect();
            Ok(QueryResult::Xml(xmls))
        }
    }
}

// ---------------------------------------------------------------------------
// Direct xot tree building from objects
// ---------------------------------------------------------------------------

/// Build a xot tree directly from the object `HashMap`.
///
/// Walks the object graph starting from `root_hash`, creating xot nodes
/// directly (no string serialization/reparsing round-trip).
fn build_xot_from_objects(
    objects: &HashMap<ContentHash, Object>,
    root_hash: ContentHash,
) -> Result<(xot::Xot, xot::Node)> {
    let mut xot = xot::Xot::new();
    let root_node = build_xot_node(&mut xot, objects, root_hash)?;
    let _doc_node = xot
        .new_document_with_element(root_node)
        .map_err(|e: xot::Error| Error::InvalidObject(e.to_string()))?;
    Ok((xot, root_node))
}

/// Recursively build a single xot node from the object map.
fn build_xot_node(
    xot: &mut xot::Xot,
    objects: &HashMap<ContentHash, Object>,
    hash: ContentHash,
) -> Result<xot::Node> {
    let obj = objects.get(&hash).ok_or(Error::NotFound(hash))?;

    match obj {
        Object::Text(t) => Ok(xot.new_text(&t.content)),
        Object::Comment(c) => Ok(xot.new_comment(&c.content)),
        Object::PI(pi) => {
            let target_name = xot.add_name(&pi.target);
            Ok(xot.new_processing_instruction(target_name, pi.data.as_deref()))
        }
        Object::Element(el) => {
            // Create element with namespace.
            let ns_uri = el.namespace_uri.as_deref().unwrap_or("");
            let ns = if ns_uri.is_empty() {
                xot.add_namespace("")
            } else {
                xot.add_namespace(ns_uri)
            };
            let name = xot.add_name_ns(&el.local_name, ns);
            let elem_node = xot.new_element(name);

            // Add namespace declaration so serialization works.
            if !ns_uri.is_empty() {
                let prefix = xot.add_prefix("");
                xot.namespaces_mut(elem_node).insert(prefix, ns);
            }

            // Set attributes.
            for attr in &el.attributes {
                let attr_ns = if let Some(ref attr_ns_uri) = attr.namespace_uri {
                    xot.add_namespace(attr_ns_uri)
                } else {
                    xot.add_namespace("")
                };
                let attr_name = xot.add_name_ns(&attr.local_name, attr_ns);
                xot.set_attribute(elem_node, attr_name, &attr.value);
            }

            // Recursively build children.
            for child_hash in &el.children {
                let child_node = build_xot_node(xot, objects, *child_hash)?;
                xot.append(elem_node, child_node)
                    .map_err(|e| Error::InvalidObject(e.to_string()))?;
            }

            Ok(elem_node)
        }
        _ => Err(Error::InvalidObject(
            "cannot build xot node from versioning object".into(),
        )),
    }
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
        let mut combined_count = 0usize;
        let mut combined_texts = Vec::new();
        let mut combined_xmls = Vec::new();
        for entry in &tree.entries {
            let result = query_store
                .query_document(entry.document, xpath, mode, namespaces)
                .await?;
            match result {
                QueryResult::Count(n) => combined_count += n,
                QueryResult::Text(ts) => combined_texts.extend(ts),
                QueryResult::Xml(xs) => combined_xmls.extend(xs),
            }
        }
        let combined = match mode {
            QueryMode::Count => QueryResult::Count(combined_count),
            QueryMode::Text => QueryResult::Text(combined_texts),
            QueryMode::Xml => QueryResult::Xml(combined_xmls),
        };
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

/// Convenience: resolve a revspec, then query all documents in the tree.
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
    let hash = resolve_revspec(ref_store, revspec).await?;
    let tree_hash = follow_to_tree(store, hash).await?;
    let tree_obj = store.get(&tree_hash).await?.ok_or(Error::NotFound(tree_hash))?;
    let Object::Tree(tree) = tree_obj else {
        return Err(Error::InvalidObject("expected Tree object".into()));
    };

    // Query each document in the tree and aggregate results.
    let mut combined_count = 0usize;
    let mut combined_texts = Vec::new();
    let mut combined_xmls = Vec::new();
    for entry in &tree.entries {
        let result = query_store
            .query_document(entry.document, xpath, mode, namespaces)
            .await?;
        match result {
            QueryResult::Count(n) => combined_count += n,
            QueryResult::Text(ts) => combined_texts.extend(ts),
            QueryResult::Xml(xs) => combined_xmls.extend(xs),
        }
    }
    Ok(match mode {
        QueryMode::Count => QueryResult::Count(combined_count),
        QueryMode::Text => QueryResult::Text(combined_texts),
        QueryMode::Xml => QueryResult::Xml(combined_xmls),
    })
}

// ---------------------------------------------------------------------------
// `XPath` evaluator (ported from clayers-spec/src/query.rs)
// ---------------------------------------------------------------------------

struct ParsedStep {
    prefix: String,
    local_name: String,
    predicate: Option<(String, String)>, // (attr_name, attr_value)
    is_descendant: bool,
}

struct ResolvedStep {
    prefix: String,
    local_name: String,
    pred_value: Option<String>,
    pred_name_id: Option<xot::NameId>,
    is_descendant: bool,
}

/// Collect predicate attribute names from an `XPath` expression.
fn collect_predicate_attr_names(xpath: &str) -> Result<Vec<String>> {
    let steps = parse_xpath_steps(xpath.trim())?;
    Ok(steps
        .into_iter()
        .filter_map(|s| s.predicate.map(|(name, _)| name))
        .collect())
}

fn parse_xpath_steps(xpath: &str) -> Result<Vec<ParsedStep>> {
    let xpath = xpath.trim();
    if !xpath.starts_with("//") {
        return Err(Error::InvalidObject(format!(
            "only //descendant XPath is supported, got: {xpath}"
        )));
    }

    let path = &xpath[2..];
    let mut steps = Vec::new();
    let mut first = true;

    for part in split_xpath_path(path) {
        let step = parse_single_step(part, first)?;
        steps.push(step);
        first = false;
    }

    Ok(steps)
}

fn split_xpath_path(path: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;

    for (i, c) in path.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            '/' if depth == 0 => {
                if i > start {
                    parts.push(&path[start..i]);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < path.len() {
        parts.push(&path[start..]);
    }
    parts
}

fn parse_single_step(step: &str, is_first: bool) -> Result<ParsedStep> {
    let (name_part, predicate) = if let Some(bracket_start) = step.find('[') {
        let bracket_end = step
            .rfind(']')
            .ok_or_else(|| Error::InvalidObject(format!("unmatched bracket in: {step}")))?;
        let pred_str = &step[bracket_start + 1..bracket_end];
        let pred = parse_predicate(pred_str)?;
        (&step[..bracket_start], Some(pred))
    } else {
        (step, None)
    };

    let (prefix, local_name) = if let Some(colon) = name_part.find(':') {
        (
            name_part[..colon].trim_start_matches('@').to_string(),
            name_part[colon + 1..].to_string(),
        )
    } else {
        (String::new(), name_part.to_string())
    };

    Ok(ParsedStep {
        prefix,
        local_name,
        predicate,
        is_descendant: is_first,
    })
}

fn parse_predicate(pred: &str) -> Result<(String, String)> {
    let pred = pred.trim();
    if !pred.starts_with('@') {
        return Err(Error::InvalidObject(format!(
            "only @attr=\"value\" predicates supported, got: {pred}"
        )));
    }

    let pred = &pred[1..];
    let eq_pos = pred
        .find('=')
        .ok_or_else(|| Error::InvalidObject(format!("missing = in predicate: {pred}")))?;

    let attr_name = pred[..eq_pos].to_string();
    let value_str = &pred[eq_pos + 1..];
    let value = value_str.trim_matches('"').trim_matches('\'').to_string();

    Ok((attr_name, value))
}

/// Resolve parsed `XPath` steps by interning attribute names into the Xot arena.
fn resolve_steps(
    xot: &mut xot::Xot,
    steps: Vec<ParsedStep>,
    attr_names: &[String],
) -> Vec<ResolvedStep> {
    let mut name_idx = 0;
    steps
        .into_iter()
        .map(|s| {
            let (pred_name_id, pred_value) = match s.predicate {
                Some((_, attr_value)) => {
                    let name_id = xot.add_name(&attr_names[name_idx]);
                    name_idx += 1;
                    (Some(name_id), Some(attr_value))
                }
                None => (None, None),
            };
            ResolvedStep {
                prefix: s.prefix,
                local_name: s.local_name,
                pred_value,
                pred_name_id,
                is_descendant: s.is_descendant,
            }
        })
        .collect()
}

/// Find matching nodes in the xot tree using caller-provided namespace map.
fn find_matching_nodes(
    xot: &mut xot::Xot,
    root: xot::Node,
    xpath: &str,
    namespaces: &NamespaceMap,
    attr_names: &[String],
) -> Result<Vec<xot::Node>> {
    let xpath = xpath.trim();
    let steps = parse_xpath_steps(xpath)?;
    let resolved = resolve_steps(xot, steps, attr_names);

    let mut results = Vec::new();
    find_nodes_by_steps(xot, root, &resolved, namespaces, 0, true, &mut results);
    Ok(results)
}

fn find_nodes_by_steps(
    xot: &xot::Xot,
    node: xot::Node,
    steps: &[ResolvedStep],
    namespaces: &NamespaceMap,
    step_idx: usize,
    search_descendants: bool,
    results: &mut Vec<xot::Node>,
) {
    if step_idx >= steps.len() {
        return;
    }

    let resolved = &steps[step_idx];
    let is_last = step_idx == steps.len() - 1;

    if !xot.is_element(node) {
        return;
    }

    // Check if this node matches the current step.
    if matches_step(xot, node, resolved, namespaces) {
        if is_last {
            results.push(node);
        } else {
            // Continue with next step on children.
            for child in xot.children(node) {
                find_nodes_by_steps(xot, child, steps, namespaces, step_idx + 1, false, results);
            }
        }
    }

    // If searching descendants (// axis), recurse into all children.
    if search_descendants || resolved.is_descendant {
        for child in xot.children(node) {
            find_nodes_by_steps(xot, child, steps, namespaces, step_idx, true, results);
        }
    }
}

/// Check if a node matches a resolved step, using caller-provided namespace map.
fn matches_step(
    xot: &xot::Xot,
    node: xot::Node,
    resolved: &ResolvedStep,
    namespaces: &NamespaceMap,
) -> bool {
    let Some(element) = xot.element(node) else {
        return false;
    };

    let name_id = element.name();
    let (local, ns_uri) = xot.name_ns_str(name_id);
    if local != resolved.local_name {
        return false;
    }

    // Check namespace prefix via caller-provided map.
    if !resolved.prefix.is_empty() {
        let expected_uri = namespaces
            .iter()
            .find(|(prefix, _)| prefix == &resolved.prefix)
            .map(|(_, uri)| uri.as_str());
        if let Some(expected) = expected_uri {
            if ns_uri != expected {
                return false;
            }
        } else {
            // Unknown prefix: no match.
            return false;
        }
    }

    // Check predicate.
    if let Some(ref attr_value) = resolved.pred_value {
        if let Some(attr_id) = resolved.pred_name_id {
            match xot.get_attribute(node, attr_id) {
                Some(val) if val == attr_value => {}
                _ => return false,
            }
        } else {
            return false;
        }
    }

    true
}

/// Collect all text content from a node and its descendants.
fn collect_all_text(xot: &xot::Xot, node: xot::Node) -> String {
    let mut text = String::new();
    collect_text_recursive(xot, node, &mut text);
    text.trim().to_string()
}

fn collect_text_recursive(xot: &xot::Xot, node: xot::Node, out: &mut String) {
    if let Some(t) = xot.text_str(node) {
        out.push_str(t);
    }
    for child in xot.children(node) {
        collect_text_recursive(xot, child, out);
    }
}
