//! Per-node C14N content hashing.
//!
//! Public wrapper around the path used by `artifact --drift`
//! (find-by-id → serialize → inclusive C14N → `SHA-256`). Reused by
//! `clayers-search`'s incremental-rebuild cache so the search index
//! and drift detector agree on what "changed" means.

use clayers_xml::{CanonicalizationMode, ContentHash, canonicalize_and_hash};

use crate::namespace;

/// Compute the inclusive-C14N `SHA-256` hash of the element under `root`
/// whose `@id` (or `xml:id`) matches `id`.
///
/// Returns `None` if no such element exists, or if serialization or
/// hashing fails.
#[must_use]
pub fn compute_node_hash(
    xot: &mut xot::Xot,
    root: xot::Node,
    id: &str,
) -> Option<ContentHash> {
    let id_attr = xot.add_name("id");
    let xml_ns = xot.add_namespace(namespace::XML);
    let xml_id_attr = xot.add_name_ns("id", xml_ns);
    let node = crate::fix::find_node_by_id(xot, root, id_attr, xml_id_attr, id)?;
    let xml_str = xot.to_string(node).ok()?;
    canonicalize_and_hash(&xml_str, CanonicalizationMode::Inclusive).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(xml: &str) -> (xot::Xot, xot::Node) {
        let mut xot = xot::Xot::new();
        let doc = xot.parse(xml).unwrap();
        let root = xot.document_element(doc).unwrap();
        (xot, root)
    }

    #[test]
    fn compute_node_hash_hashes_existing_id() {
        let (mut xot, root) = parse(
            r#"<root xmlns="urn:test"><section id="foo"><p>hello</p></section></root>"#,
        );
        let hash = compute_node_hash(&mut xot, root, "foo");
        assert!(hash.is_some());
    }

    #[test]
    fn compute_node_hash_missing_id_returns_none() {
        let (mut xot, root) = parse(r"<root><p>hello</p></root>");
        assert!(compute_node_hash(&mut xot, root, "missing").is_none());
    }

    #[test]
    fn compute_node_hash_deterministic_across_runs() {
        let xml =
            r#"<root xmlns="urn:test"><section id="foo"><p>content</p></section></root>"#;
        let (mut x1, r1) = parse(xml);
        let h1 = compute_node_hash(&mut x1, r1, "foo").unwrap();
        let (mut x2, r2) = parse(xml);
        let h2 = compute_node_hash(&mut x2, r2, "foo").unwrap();
        assert_eq!(h1.to_prefixed(), h2.to_prefixed());
    }

    #[test]
    fn compute_node_hash_different_content_different_hash() {
        let (mut x1, r1) = parse(
            r#"<root xmlns="urn:test"><section id="foo"><p>A</p></section></root>"#,
        );
        let (mut x2, r2) = parse(
            r#"<root xmlns="urn:test"><section id="foo"><p>B</p></section></root>"#,
        );
        let h1 = compute_node_hash(&mut x1, r1, "foo").unwrap();
        let h2 = compute_node_hash(&mut x2, r2, "foo").unwrap();
        assert_ne!(h1.to_prefixed(), h2.to_prefixed());
    }
}
