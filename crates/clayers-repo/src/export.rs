//! Export pipeline: content-addressed object DAG -> XML string.
//!
//! Reassembles an XML document from its stored Merkle DAG representation.
//! Collects the subtree into a `HashMap`, then builds XML synchronously.

use std::collections::HashMap;
use std::pin::pin;

use futures_core::Stream;

use crate::error::{Error, Result};
use crate::object::Object;
use crate::store::ObjectStore;

use clayers_xml::ContentHash;

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

/// Export a document from the object store as a canonical XML string.
///
/// Collects the entire subtree via `subtree()`, then builds the XML
/// string synchronously from the in-memory `HashMap`.
///
/// # Errors
///
/// Returns an error if the document or any referenced objects are not found,
/// or if XML reconstruction fails.
pub async fn export_xml(store: &dyn ObjectStore, hash: ContentHash) -> Result<String> {
    let objects = try_collect_stream(store.subtree(&hash)).await?;

    let root_hash = match objects.get(&hash).ok_or(Error::NotFound(hash))? {
        Object::Document(doc) => doc.root,
        _ => return Err(Error::InvalidObject("expected Document object".into())),
    };

    // Build XML synchronously from the collected objects.
    // Each element carries its own namespace declarations, so the output
    // is already well-formed without whole-document C14N. Skipping C14N
    // here is critical: the import hashes each element's subtree individually
    // (including its namespace context), so whole-document C14N would strip
    // inherited namespace declarations from child elements, breaking the
    // export -> reimport hash idempotency.
    build_xml_from_objects(&objects, root_hash)
}

/// Synchronously build an XML string from a pre-collected object map.
#[allow(clippy::too_many_lines)]
fn build_xml_from_objects(
    objects: &HashMap<ContentHash, Object>,
    hash: ContentHash,
) -> Result<String> {
    let obj = objects.get(&hash).ok_or(Error::NotFound(hash))?;

    match obj {
        Object::Text(t) => Ok(xml_escape_text(&t.content)),
        Object::Comment(c) => Ok(format!("<!--{}-->", c.content)),
        Object::PI(pi) => match &pi.data {
            Some(d) if !d.is_empty() => Ok(format!("<?{} {}?>", pi.target, d)),
            _ => Ok(format!("<?{}?>", pi.target)),
        },
        Object::Element(el) => {
            use std::fmt::Write;
            let mut xml = String::new();

            // Opening tag with namespace, using stored prefix if available.
            xml.push('<');
            let elem_prefix = el.namespace_prefix.as_deref().unwrap_or("");
            if !elem_prefix.is_empty() {
                let _ = write!(xml, "{elem_prefix}:");
            }
            xml.push_str(&el.local_name);
            if let Some(ref ns) = el.namespace_uri {
                if elem_prefix.is_empty() {
                    let _ = write!(xml, " xmlns=\"{ns}\"");
                } else {
                    let _ = write!(xml, " xmlns:{elem_prefix}=\"{ns}\"");
                }
            }

            // Collect unique namespace URIs from attributes and assign prefixes.
            // If an attribute's namespace matches the element's, reuse the element's prefix.
            let mut attr_ns_prefixes: HashMap<String, String> = HashMap::new();
            let mut used_prefixes: std::collections::HashSet<String> = std::collections::HashSet::new();
            if !elem_prefix.is_empty() {
                used_prefixes.insert(elem_prefix.to_string());
                // Pre-register element namespace so attributes in the same NS reuse its prefix.
                if let Some(ref ns) = el.namespace_uri {
                    attr_ns_prefixes.insert(ns.clone(), elem_prefix.to_string());
                }
            }
            let mut prefix_counter = 1u32;
            for attr in &el.attributes {
                if let Some(ref ns) = attr.namespace_uri {
                    attr_ns_prefixes.entry(ns.clone()).or_insert_with(|| {
                        let candidate = attr.namespace_prefix.clone()
                            .unwrap_or_else(|| {
                                loop {
                                    let p = format!("ns{prefix_counter}");
                                    prefix_counter += 1;
                                    if !used_prefixes.contains(&p) {
                                        return p;
                                    }
                                }
                            });
                        // Ensure uniqueness.
                        if used_prefixes.contains(&candidate) {
                            loop {
                                let p = format!("ns{prefix_counter}");
                                prefix_counter += 1;
                                if !used_prefixes.contains(&p) {
                                    used_prefixes.insert(p.clone());
                                    return p;
                                }
                            }
                        }
                        used_prefixes.insert(candidate.clone());
                        candidate
                    });
                }
            }

            // Emit namespace declarations for attribute namespaces
            // (skip if already declared by the element).
            for (ns_uri, prefix) in &attr_ns_prefixes {
                if el.namespace_uri.as_deref() == Some(ns_uri.as_str())
                    && !elem_prefix.is_empty()
                {
                    continue; // Already declared by the element.
                }
                let _ = write!(xml, " xmlns:{prefix}=\"{ns_uri}\"");
            }

            // Attributes.
            for attr in &el.attributes {
                xml.push(' ');
                if let Some(ref ns) = attr.namespace_uri {
                    let prefix = &attr_ns_prefixes[ns];
                    let _ = write!(
                        xml,
                        "{prefix}:{}=\"{}\"",
                        attr.local_name,
                        xml_escape_attr(&attr.value)
                    );
                } else {
                    let _ = write!(
                        xml,
                        "{}=\"{}\"",
                        attr.local_name,
                        xml_escape_attr(&attr.value)
                    );
                }
            }
            xml.push('>');

            // Children (synchronous recursion).
            for child_hash in &el.children {
                let child_xml = build_xml_from_objects(objects, *child_hash)?;
                xml.push_str(&child_xml);
            }

            // Closing tag.
            xml.push_str("</");
            if !elem_prefix.is_empty() {
                xml.push_str(elem_prefix);
                xml.push(':');
            }
            xml.push_str(&el.local_name);
            xml.push('>');

            Ok(xml)
        }
        _ => Err(Error::InvalidObject(
            "cannot export versioning object as XML content".into(),
        )),
    }
}

/// Export all documents in a tree as `(path, canonical_xml)` pairs.
///
/// # Errors
///
/// Returns an error if the tree or any referenced objects are not found.
pub async fn export_tree(
    store: &dyn ObjectStore,
    tree_hash: ContentHash,
) -> Result<Vec<(String, String)>> {
    let tree_obj = store
        .get(&tree_hash)
        .await?
        .ok_or(Error::NotFound(tree_hash))?;
    let Object::Tree(tree) = tree_obj else {
        return Err(Error::InvalidObject("expected Tree object".into()));
    };

    let mut results = Vec::with_capacity(tree.entries.len());
    for entry in &tree.entries {
        let xml = export_xml(store, entry.document).await?;
        results.push((entry.path.clone(), xml));
    }
    Ok(results)
}

/// Escape text content for XML.
fn xml_escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Escape attribute values for XML.
fn xml_escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::import_xml;
    use crate::store::memory::MemoryStore;

    #[tokio::test]
    async fn roundtrip_simple() {
        let store = MemoryStore::new();
        let xml = "<root>hello</root>";
        let hash = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, hash).await.unwrap();
        assert!(exported.contains("hello"), "content missing: {exported}");
    }

    #[tokio::test]
    async fn roundtrip_namespaced() {
        let store = MemoryStore::new();
        let xml = r#"<root xmlns="urn:test"><child>text</child></root>"#;
        let hash = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, hash).await.unwrap();
        assert!(exported.contains("text"), "content missing: {exported}");
        assert!(exported.contains("urn:test"), "namespace missing: {exported}");
    }

    /// Export -> reimport must produce the same document hash.
    /// Tests the idempotency of the export pipeline.
    #[tokio::test]
    async fn roundtrip_hash_idempotent() {
        let store = MemoryStore::new();
        let xml = r#"<root xmlns:app="urn:test:app"><app:item id="1">text</app:item></root>"#;
        let hash1 = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, hash1).await.unwrap();
        let hash2 = import_xml(&store, &exported).await.unwrap();
        assert_eq!(hash1, hash2, "export->reimport must produce same hash");
    }

    /// Child elements inheriting parent's namespace must round-trip.
    #[tokio::test]
    async fn roundtrip_inherited_namespace_hash_idempotent() {
        let store = MemoryStore::new();
        // Children inherit the default namespace from parent. The export must
        // re-declare it on each child so that individual element hashes match.
        let xml = r#"<root xmlns="urn:test"><child>one</child><child>two</child></root>"#;
        let hash1 = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, hash1).await.unwrap();
        let hash2 = import_xml(&store, &exported).await.unwrap();
        assert_eq!(
            hash1, hash2,
            "inherited namespace round-trip failed.\nOriginal: {xml}\nExported: {exported}"
        );
    }

    /// Multi-namespace XML like clayers spec files must round-trip.
    #[tokio::test]
    async fn roundtrip_multi_namespace_hash_idempotent() {
        let store = MemoryStore::new();
        let xml = r#"<spec:clayers xmlns:spec="urn:clayers:spec" xmlns:pr="urn:clayers:prose" xmlns:trm="urn:clayers:terminology" spec:index="index.xml"><pr:section id="overview"><pr:title>Overview</pr:title><pr:p>Some <trm:ref target="term-node">node</trm:ref> text.</pr:p></pr:section><trm:term id="term-node"><trm:name>Node</trm:name><trm:definition>A unit.</trm:definition></trm:term></spec:clayers>"#;
        let hash1 = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, hash1).await.unwrap();
        let hash2 = import_xml(&store, &exported).await.unwrap();
        assert_eq!(
            hash1, hash2,
            "multi-namespace export->reimport must produce same hash.\n\
             Original XML:\n{xml}\n\nExported:\n{exported}"
        );
    }

    #[tokio::test]
    async fn roundtrip_namespaced_attributes() {
        let store = MemoryStore::new();
        // Element with multiple attributes in the same namespace - the export
        // must assign a single prefix for that namespace, not duplicate ns1:.
        let xml = r#"<root xmlns:x="urn:ns" x:id="1" x:status="active">text</root>"#;
        let hash = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, hash).await.unwrap();
        // Should not error (was crashing with "Duplicate attribute: ns1:id").
        assert!(exported.contains("text"), "content missing: {exported}");
    }

    #[tokio::test]
    async fn roundtrip_multiple_attr_namespaces() {
        let store = MemoryStore::new();
        // Two attributes from different namespaces need different prefixes.
        let xml = r#"<root xmlns:a="urn:a" xmlns:b="urn:b" a:x="1" b:y="2">text</root>"#;
        let hash = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, hash).await.unwrap();
        assert!(exported.contains("text"), "content missing: {exported}");
        // Both attribute values must survive.
        assert!(exported.contains("\"1\""), "attr a:x missing: {exported}");
        assert!(exported.contains("\"2\""), "attr b:y missing: {exported}");
    }

    // -----------------------------------------------------------------------
    // XML preservation: hash idempotency for patterns that caused real bugs
    // -----------------------------------------------------------------------

    /// Helper: verify import→export→reimport produces the same hash.
    async fn assert_hash_idempotent(xml: &str) {
        let store = MemoryStore::new();
        let h1 = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, h1).await.unwrap();
        let h2 = import_xml(&store, &exported).await.unwrap();
        assert_eq!(
            h1, h2,
            "hash not idempotent.\nOriginal:  {xml}\nExported: {exported}"
        );
    }

    #[tokio::test]
    async fn preserve_xml_id_attribute() {
        // xml:id uses the implicit xml namespace - no xmlns:xml declaration
        // exists, so the import must handle it specially.
        assert_hash_idempotent(r#"<root xml:id="my-id">text</root>"#).await;
    }

    #[tokio::test]
    async fn preserve_xmi_style_namespaces() {
        // Multiple ns-prefixed attributes on one element including xml:id.
        // This was the pattern that caused "Duplicate attribute: xmlns:ns2".
        let xml = r#"<Model xmlns="http://www.omg.org/spec/UML" xmlns:xmi="http://www.omg.org/spec/XMI" name="Arch" xmi:id="_arch" xml:id="model-arch">content</Model>"#;
        assert_hash_idempotent(xml).await;
        let store = MemoryStore::new();
        let h = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, h).await.unwrap();
        assert!(exported.contains("xmi:id"), "xmi prefix lost: {exported}");
    }

    #[tokio::test]
    async fn preserve_clayers_spec_pattern() {
        // Real-world pattern: prefixed root with attribute in same namespace,
        // children in different default namespaces.
        let xml = r#"<ns1:clayers xmlns:ns1="urn:clayers:spec" ns1:index="index.xml"><term xmlns="urn:clayers:terminology" id="term-layer"><name>Layer</name></term><section xmlns="urn:clayers:prose" id="overview"><title>Overview</title></section></ns1:clayers>"#;
        assert_hash_idempotent(xml).await;
        let store = MemoryStore::new();
        let h = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, h).await.unwrap();
        assert!(exported.contains("ns1:clayers"), "root prefix lost: {exported}");
        assert!(exported.contains("ns1:index"), "attr prefix lost: {exported}");
    }

    #[tokio::test]
    async fn preserve_mixed_content() {
        // Text interleaved with elements - easy to lose text nodes.
        assert_hash_idempotent("<p>Hello <b>bold</b> and <i>italic</i> world</p>").await;
    }

    #[tokio::test]
    async fn preserve_whitespace() {
        // Whitespace-only text nodes between elements.
        assert_hash_idempotent("<root>\n  <child>text</child>\n</root>").await;
    }

    #[tokio::test]
    async fn preserve_mixed_default_and_prefixed_ns() {
        // Element uses default ns, attribute uses prefixed ns for a different URI.
        assert_hash_idempotent(
            r#"<root xmlns="urn:elem" xmlns:x="urn:attr" x:id="1">text</root>"#,
        ).await;
    }

    #[tokio::test]
    async fn not_found_error() {
        let store = MemoryStore::new();
        let bad_hash = ContentHash::from_canonical(b"nonexistent");
        let result = export_xml(&store, bad_hash).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn export_tree_single() {
        use crate::object::{TreeEntry, TreeObject, Object};
        use crate::hash;

        let store = MemoryStore::new();
        let xml = "<root>hello</root>";
        let doc_hash = import_xml(&store, xml).await.unwrap();
        let tree = TreeObject::new(vec![
            TreeEntry { path: "doc.xml".into(), document: doc_hash },
        ]);
        let tree_xml = tree.to_xml();
        let tree_hash = hash::hash_exclusive(&tree_xml).unwrap();
        let mut tx = store.transaction().await.unwrap();
        tx.put(tree_hash, Object::Tree(tree)).await.unwrap();
        tx.commit().await.unwrap();

        let result = export_tree(&store, tree_hash).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "doc.xml");
        assert!(result[0].1.contains("hello"));
    }

    #[tokio::test]
    async fn export_tree_multi() {
        use crate::object::{TreeEntry, TreeObject, Object};
        use crate::hash;

        let store = MemoryStore::new();
        let h1 = import_xml(&store, "<a>one</a>").await.unwrap();
        let h2 = import_xml(&store, "<b>two</b>").await.unwrap();
        let h3 = import_xml(&store, "<c>three</c>").await.unwrap();
        let tree = TreeObject::new(vec![
            TreeEntry { path: "a.xml".into(), document: h1 },
            TreeEntry { path: "b.xml".into(), document: h2 },
            TreeEntry { path: "c.xml".into(), document: h3 },
        ]);
        let tree_xml = tree.to_xml();
        let tree_hash = hash::hash_exclusive(&tree_xml).unwrap();
        let mut tx = store.transaction().await.unwrap();
        tx.put(tree_hash, Object::Tree(tree)).await.unwrap();
        tx.commit().await.unwrap();

        let result = export_tree(&store, tree_hash).await.unwrap();
        assert_eq!(result.len(), 3);
        // Sorted by path.
        assert_eq!(result[0].0, "a.xml");
        assert_eq!(result[1].0, "b.xml");
        assert_eq!(result[2].0, "c.xml");
    }

    #[tokio::test]
    async fn export_tree_roundtrip() {
        use crate::object::{TreeEntry, TreeObject, Object};
        use crate::hash;

        let store = MemoryStore::new();
        let xmls = vec![
            ("a.xml", "<root>alpha</root>"),
            ("b.xml", "<root>beta</root>"),
            ("c.xml", "<root>gamma</root>"),
        ];
        let mut entries = Vec::new();
        for (path, xml) in &xmls {
            let h = import_xml(&store, xml).await.unwrap();
            entries.push(TreeEntry { path: path.to_string(), document: h });
        }
        let tree = TreeObject::new(entries);
        let tree_xml = tree.to_xml();
        let tree_hash = hash::hash_exclusive(&tree_xml).unwrap();
        let mut tx = store.transaction().await.unwrap();
        tx.put(tree_hash, Object::Tree(tree)).await.unwrap();
        tx.commit().await.unwrap();

        let result = export_tree(&store, tree_hash).await.unwrap();
        assert_eq!(result.len(), 3);
        assert!(result[0].1.contains("alpha"));
        assert!(result[1].1.contains("beta"));
        assert!(result[2].1.contains("gamma"));
    }

    #[tokio::test]
    async fn export_tree_empty() {
        use crate::object::{TreeObject, Object};
        use crate::hash;

        let store = MemoryStore::new();
        let tree = TreeObject::new(vec![]);
        let tree_xml = tree.to_xml();
        let tree_hash = hash::hash_exclusive(&tree_xml).unwrap();
        let mut tx = store.transaction().await.unwrap();
        tx.put(tree_hash, Object::Tree(tree)).await.unwrap();
        tx.commit().await.unwrap();

        let result = export_tree(&store, tree_hash).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn export_tree_not_a_tree() {
        let store = MemoryStore::new();
        let doc_hash = import_xml(&store, "<r/>").await.unwrap();
        let result = export_tree(&store, doc_hash).await;
        assert!(result.is_err());
    }
}
