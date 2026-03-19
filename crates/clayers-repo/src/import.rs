//! Import pipeline: XML string -> content-addressed object DAG.
//!
//! Parses an XML document with `xot`, decomposes it into its constituent
//! nodes via post-order traversal, hashes each one, and stores everything
//! in a single transaction.

use clayers_xml::ContentHash;
use xot::Xot;

use crate::error::{Error, Result};
use crate::hash;
use crate::object::{
    Attribute, CommentObject, DocumentObject, ElementObject, Object, PIObject, TextObject,
};
use crate::store::ObjectStore;

/// Collected object ready to be stored in a transaction.
struct CollectedObject {
    hash: ContentHash,
    object: Object,
}

/// Import an XML string into the object store, returning the document hash.
///
/// The XML is decomposed into its constituent Infoset nodes, each
/// content-addressed by its Exclusive C14N hash, and stored as a Merkle DAG.
///
/// # Errors
///
/// Returns an error if the XML is malformed, has no root element, or
/// the storage transaction fails.
pub async fn import_xml(store: &dyn ObjectStore, xml: &str) -> Result<ContentHash> {
    let mut xot = Xot::new();
    let doc = xot.parse(xml).map_err(xot::Error::from)?;
    let root = xot
        .document_element(doc)
        .map_err(|e| Error::XmlParse(e.to_string()))?;

    // Collect prologue: comments and PIs before the root element.
    let mut objects = Vec::new();
    let mut prologue_hashes = Vec::new();
    let doc_children: Vec<_> = xot.children(doc).collect();
    for child in doc_children {
        if child == root {
            break;
        }
        if xot.comment_str(child).is_some() || xot.processing_instruction(child).is_some() {
            let h = collect_node(&mut xot, child, &mut objects)?;
            prologue_hashes.push(h);
        }
    }

    // Collect all objects via post-order traversal (sync, no async needed).
    let root_hash = collect_node(&mut xot, root, &mut objects)?;

    // Create the document object with prologue.
    let doc_obj = DocumentObject {
        root: root_hash,
        prologue: prologue_hashes,
    };
    let doc_xml = doc_obj.to_xml();
    let doc_hash = hash::hash_exclusive(&doc_xml)?;
    objects.push(CollectedObject {
        hash: doc_hash,
        object: Object::Document(doc_obj),
    });

    // Batch-insert into a single transaction.
    let mut tx = store.transaction().await?;
    for entry in objects {
        tx.put(entry.hash, entry.object).await?;
    }
    tx.commit().await?;

    Ok(doc_hash)
}

/// Recursively collect objects from a xot node tree (post-order).
#[allow(clippy::too_many_lines)]
fn collect_node(
    xot: &mut Xot,
    node: xot::Node,
    objects: &mut Vec<CollectedObject>,
) -> Result<ContentHash> {
    // Text node
    if let Some(text) = xot.text_str(node) {
        let text = text.to_string();
        let h = hash::hash_text(&text);
        objects.push(CollectedObject {
            hash: h,
            object: Object::Text(TextObject { content: text }),
        });
        return Ok(h);
    }

    // Comment node
    if let Some(comment) = xot.comment_str(node) {
        let comment = comment.to_string();
        let h = hash::hash_text(&comment);
        objects.push(CollectedObject {
            hash: h,
            object: Object::Comment(CommentObject { content: comment }),
        });
        return Ok(h);
    }

    // Processing instruction
    if let Some(pi) = xot.processing_instruction(node) {
        let target = xot.local_name_str(pi.target()).to_string();
        let data = pi.data().map(String::from);
        let h = hash::hash_pi(&target, data.as_deref());
        objects.push(CollectedObject {
            hash: h,
            object: Object::PI(PIObject { target, data }),
        });
        return Ok(h);
    }

    // Element node: process children first (post-order), then this element.
    if xot.is_element(node) {
        let mut child_hashes = Vec::new();
        let children: Vec<_> = xot.children(node).collect();
        for child in children {
            let child_hash = collect_node(xot, child, objects)?;
            child_hashes.push(child_hash);
        }

        // Serialize this element's subtree to XML for hashing.
        // Use clone_with_prefixes so inherited namespace declarations are included.
        let clone = xot.clone_with_prefixes(node);
        let xml_str = xot
            .to_string(clone)
            .map_err(|e| Error::XmlParse(e.to_string()))?;

        // Extract prefix map from the clone's namespace declarations (xot API).
        // The clone has inherited prefixes from ancestors, so this gives us
        // a complete picture of all prefixes in scope for this element.
        let prefix_map: Vec<(String, String)> = xot
            .namespaces(clone)
            .iter()
            .map(|(prefix_id, ns_id)| {
                (
                    xot.prefix_str(prefix_id).to_string(),
                    xot.namespace_str(*ns_id).to_string(),
                )
            })
            .collect();

        xot.remove(clone)
            .map_err(|e| Error::XmlParse(e.to_string()))?;
        let (identity_hash, inclusive_hash) = hash::hash_element_xml(&xml_str)?;

        // Extract structural fields.
        let element = xot
            .element(node)
            .ok_or_else(|| Error::InvalidObject("expected element data".into()))?;
        let name_id = element.name();
        let (local_name, ns_str) = xot.name_ns_str(name_id);
        let local_name = local_name.to_string();
        let namespace_uri = if ns_str.is_empty() {
            None
        } else {
            Some(ns_str.to_string())
        };

        // Find element prefix: look up which prefix maps to the element's namespace.
        let namespace_prefix = namespace_uri.as_ref().and_then(|uri| {
            prefix_map.iter().find_map(|(pfx, ns)| {
                if ns == uri && !pfx.is_empty() {
                    Some(pfx.clone())
                } else {
                    None
                }
            })
        });

        // Extract attributes with their prefixes.
        let mut attributes = Vec::new();
        for (attr_name_id, attr_value) in xot.attributes(node).iter() {
            let (attr_local, attr_ns) = xot.name_ns_str(attr_name_id);
            let attr_ns_uri = if attr_ns.is_empty() {
                None
            } else {
                Some(attr_ns.to_string())
            };
            // Find which prefix maps to this attribute's namespace.
            let attr_prefix = attr_ns_uri.as_ref().and_then(|uri| {
                prefix_map.iter().find_map(|(pfx, ns)| {
                    if ns == uri && !pfx.is_empty() {
                        Some(pfx.clone())
                    } else {
                        None
                    }
                })
            });
            attributes.push(Attribute {
                local_name: attr_local.to_string(),
                namespace_uri: attr_ns_uri,
                namespace_prefix: attr_prefix,
                value: attr_value.clone(),
            });
        }

        // Collect extra namespace declarations (declared on this element
        // but not used by its name or attributes - convenience for descendants).
        let mut used_uris: std::collections::HashSet<&str> = std::collections::HashSet::new();
        if let Some(ref uri) = namespace_uri {
            used_uris.insert(uri);
        }
        for attr in &attributes {
            if let Some(ref uri) = attr.namespace_uri {
                used_uris.insert(uri);
            }
        }
        let extra_namespaces: Vec<(String, String)> = prefix_map
            .iter()
            .filter(|(pfx, uri)| !pfx.is_empty() && !used_uris.contains(uri.as_str()))
            .map(|(pfx, uri)| (pfx.clone(), uri.clone()))
            .collect();

        objects.push(CollectedObject {
            hash: identity_hash,
            object: Object::Element(ElementObject {
                local_name,
                namespace_uri,
                namespace_prefix,
                extra_namespaces,
                attributes,
                children: child_hashes,
                inclusive_hash,
            }),
        });
        return Ok(identity_hash);
    }

    // Skip document nodes, whitespace-only text, etc.
    // For document nodes, recurse into children.
    let mut last_hash = None;
    let children: Vec<_> = xot.children(node).collect();
    for child in children {
        last_hash = Some(collect_node(xot, child, objects)?);
    }
    last_hash.ok_or(Error::EmptyDocument)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::memory::MemoryStore;

    #[tokio::test]
    async fn import_simple_element() {
        let store = MemoryStore::new();
        let xml = "<root>hello</root>";
        let hash = import_xml(&store, xml).await.unwrap();
        // The document should be stored.
        assert!(store.contains(&hash).await.unwrap());
    }

    #[tokio::test]
    async fn import_nested_elements() {
        let store = MemoryStore::new();
        let xml = r#"<root xmlns="urn:test"><child>text</child></root>"#;
        let hash = import_xml(&store, xml).await.unwrap();
        assert!(store.contains(&hash).await.unwrap());
    }

    #[tokio::test]
    async fn import_deterministic() {
        let store = MemoryStore::new();
        let xml = "<root><a>1</a><b>2</b></root>";
        let h1 = import_xml(&store, xml).await.unwrap();
        let h2 = import_xml(&store, xml).await.unwrap();
        assert_eq!(h1, h2);
    }

    #[tokio::test]
    async fn import_mixed_content() {
        let store = MemoryStore::new();
        let xml = "<p>Hello <b>world</b>!</p>";
        let hash = import_xml(&store, xml).await.unwrap();
        assert!(store.contains(&hash).await.unwrap());
    }
}
