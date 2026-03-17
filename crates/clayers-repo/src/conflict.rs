//! Conflict element generation, detection, and listing.
//!
//! Conflicts are represented as `<repo:divergence>` XML elements in the
//! `urn:clayers:repository` namespace, making them first-class, queryable,
//! and resolvable through normal XML editing.

use std::collections::HashMap;
use std::pin::pin;

use clayers_xml::ContentHash;
use futures_core::Stream;

use crate::error::{Error, Result};
use crate::object::{Object, REPO_NS};
use crate::store::ObjectStore;

/// Information about a single divergence (concurrent edit conflict).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictInfo {
    /// XPath-like path to the conflicting node.
    pub path: String,
    /// The ancestor version's hash.
    pub ancestor: ContentHash,
    /// The conflicting sides: (commit hash, ref name, content hash).
    pub sides: Vec<ConflictSide>,
}

/// One side of a conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictSide {
    /// The commit hash that introduced this version.
    pub commit: ContentHash,
    /// The ref name (e.g., `refs/heads/feature-a`).
    pub ref_name: String,
    /// Hash of the content on this side.
    pub content: ContentHash,
}

/// Generate the XML for a `<repo:divergence>` conflict element.
///
/// Each side and the ancestor redeclare all necessary namespaces for
/// portability and self-containment.
#[must_use]
pub fn generate_divergence_xml(
    path: &str,
    ancestor_commit: ContentHash,
    ancestor_xml: &str,
    sides: &[(ContentHash, &str, &str)], // (commit, ref, content_xml)
) -> String {
    use std::fmt::Write;
    let mut xml = format!(
        "<repo:divergence xmlns:repo=\"{REPO_NS}\" path=\"{path}\">"
    );
    let _ = write!(
        xml,
        "<repo:ancestor commit=\"{ancestor_commit}\">{ancestor_xml}</repo:ancestor>"
    );
    for (commit, ref_name, content_xml) in sides {
        let _ = write!(
            xml,
            "<repo:side commit=\"{commit}\" ref=\"{ref_name}\">{content_xml}</repo:side>"
        );
    }
    xml.push_str("</repo:divergence>");
    xml
}

/// Collect a stream of `Result<(K, V)>` into a `HashMap`, short-circuiting on error.
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

/// Detect whether a document tree contains any unresolved conflicts.
///
/// Collects the subtree, then scans synchronously for `<repo:divergence>` elements.
///
/// # Errors
///
/// Returns an error if objects cannot be loaded from the store.
pub async fn has_conflicts(store: &dyn ObjectStore, document: ContentHash) -> Result<bool> {
    let objects = try_collect_stream(store.subtree(&document)).await?;

    let doc_obj = objects.get(&document).ok_or(Error::NotFound(document))?;
    let root_hash = match doc_obj {
        Object::Document(doc) => doc.root,
        _ => return Err(Error::InvalidObject("expected Document object".into())),
    };
    Ok(check_conflicts_sync(&objects, root_hash))
}

/// Synchronously check for divergence elements in the collected object map.
fn check_conflicts_sync(
    objects: &HashMap<ContentHash, Object>,
    hash: ContentHash,
) -> bool {
    let Some(obj) = objects.get(&hash) else { return false };
    if let Object::Element(el) = obj {
        if el.local_name == "divergence"
            && el.namespace_uri.as_deref() == Some(REPO_NS)
        {
            return true;
        }
        for child in &el.children {
            if check_conflicts_sync(objects, *child) {
                return true;
            }
        }
    }
    false
}

/// List all conflict elements in a document tree.
///
/// # Errors
///
/// Returns an error if objects cannot be loaded.
pub async fn list_conflicts(
    store: &dyn ObjectStore,
    document: ContentHash,
) -> Result<Vec<ConflictInfo>> {
    let objects = try_collect_stream(store.subtree(&document)).await?;

    let doc_obj = objects.get(&document).ok_or(Error::NotFound(document))?;
    let root_hash = match doc_obj {
        Object::Document(doc) => doc.root,
        _ => return Err(Error::InvalidObject("expected Document object".into())),
    };
    let mut conflicts = Vec::new();
    collect_conflicts_sync(&objects, root_hash, &mut conflicts);
    Ok(conflicts)
}

/// Synchronously collect conflict info from the object map.
fn collect_conflicts_sync(
    objects: &HashMap<ContentHash, Object>,
    hash: ContentHash,
    conflicts: &mut Vec<ConflictInfo>,
) {
    let Some(obj) = objects.get(&hash) else { return };
    if let Object::Element(el) = obj {
        if el.local_name == "divergence"
            && el.namespace_uri.as_deref() == Some(REPO_NS)
        {
            let path = el
                .attributes
                .iter()
                .find(|a| a.local_name == "path")
                .map(|a| a.value.clone())
                .unwrap_or_default();

            conflicts.push(ConflictInfo {
                path,
                ancestor: hash,
                sides: Vec::new(),
            });
        }
        for child in &el.children {
            collect_conflicts_sync(objects, *child, conflicts);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divergence_xml_contains_repository_namespace() {
        let hash = ContentHash::from_canonical(b"test");
        let xml = generate_divergence_xml(
            "/root/child",
            hash,
            "<p>original</p>",
            &[(hash, "refs/heads/a", "<p>alice</p>")],
        );
        assert!(xml.contains(REPO_NS));
        assert!(xml.contains("divergence"));
        assert!(xml.contains("repo:ancestor"));
        assert!(xml.contains("repo:side"));
    }
}
