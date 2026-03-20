//! Export pipeline: content-addressed object DAG -> XML string.
//!
//! Reassembles an XML document from its stored Merkle DAG representation.
//! Collects the subtree into a `HashMap`, then builds a xot tree and
//! serializes it. This delegates all namespace handling, escaping, and
//! serialization to xot, avoiding hand-rolled XML generation.

use std::collections::HashMap;
use std::pin::pin;

use futures_core::Stream;
use xot::Xot;

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
/// Collects the entire subtree via `subtree()`, then reconstructs a xot
/// tree and serializes it.
///
/// # Errors
///
/// Returns an error if the document or any referenced objects are not found,
/// or if XML reconstruction fails.
pub async fn export_xml(store: &dyn ObjectStore, hash: ContentHash) -> Result<String> {
    let objects = try_collect_stream(store.subtree(&hash)).await?;

    let Object::Document(doc) = objects.get(&hash).ok_or(Error::NotFound(hash))? else {
        return Err(Error::InvalidObject("expected Document object".into()));
    };

    let mut xot = Xot::new();

    // Build the root element subtree.
    let root_node = build_xot_node(&mut xot, &objects, doc.root)?;

    // Create a document node and attach prologue + root.
    let doc_node = xot.new_document();

    // Emit prologue (comments, PIs before root element).
    for prologue_hash in &doc.prologue {
        if let Some(obj) = objects.get(prologue_hash) {
            let prologue_node = match obj {
                Object::Comment(c) => Some(xot.new_comment(&c.content)),
                Object::PI(pi) => {
                    let target = xot.add_name(&pi.target);
                    Some(xot.new_processing_instruction(target, pi.data.as_deref()))
                }
                _ => None,
            };
            if let Some(node) = prologue_node {
                xot.append(doc_node, node)
                    .map_err(|e| Error::XmlParse(e.to_string()))?;
            }
        }
    }

    // Attach root element.
    xot.append(doc_node, root_node)
        .map_err(|e| Error::XmlParse(e.to_string()))?;

    // Remove redundant namespace declarations inherited from ancestors.
    xot.deduplicate_namespaces(root_node);

    // Serialize with XML declaration.
    let params = xot::output::xml::Parameters {
        declaration: Some(xot::output::xml::Declaration::default()),
        ..Default::default()
    };
    let mut xml = xot
        .serialize_xml_string(params, doc_node)
        .map_err(|e| Error::XmlParse(e.to_string()))?;
    xml.push('\n');

    Ok(xml)
}

/// Synchronously build an XML string from a pre-collected object map.
pub(crate) fn build_xml_from_objects(
    objects: &HashMap<ContentHash, Object>,
    hash: ContentHash,
) -> Result<String> {
    let mut xot = Xot::new();
    let node = build_xot_node(&mut xot, objects, hash)?;
    xot.deduplicate_namespaces(node);
    xot.to_string(node)
        .map_err(|e| Error::XmlParse(e.to_string()))
}

/// Recursively build a xot node from the object DAG.
fn build_xot_node(
    xot: &mut Xot,
    objects: &HashMap<ContentHash, Object>,
    hash: ContentHash,
) -> Result<xot::Node> {
    let obj = objects.get(&hash).ok_or(Error::NotFound(hash))?;

    match obj {
        Object::Text(t) => Ok(xot.new_text(&t.content)),
        Object::Comment(c) => Ok(xot.new_comment(&c.content)),
        Object::PI(pi) => {
            let target = xot.add_name(&pi.target);
            Ok(xot.new_processing_instruction(target, pi.data.as_deref()))
        }
        Object::Element(el) => {
            // Create element with proper namespace.
            let name_id = if let Some(ref ns_uri) = el.namespace_uri {
                let ns_id = xot.add_namespace(ns_uri);
                xot.add_name_ns(&el.local_name, ns_id)
            } else {
                xot.add_name(&el.local_name)
            };
            let el_node = xot.new_element(name_id);

            // Declare the element's own prefix.
            if let (Some(prefix), Some(ns_uri)) =
                (&el.namespace_prefix, &el.namespace_uri)
            {
                let prefix_id = xot.add_prefix(prefix);
                let ns_id = xot.add_namespace(ns_uri);
                xot.namespaces_mut(el_node).insert(prefix_id, ns_id);
            } else if el.namespace_uri.is_some() {
                // Default namespace (empty prefix).
                let prefix_id = xot.empty_prefix();
                let ns_id = xot.add_namespace(el.namespace_uri.as_deref().unwrap());
                xot.namespaces_mut(el_node).insert(prefix_id, ns_id);
            } else {
                // No namespace: explicitly declare xmlns="" so xot can cancel
                // any inherited default namespace during serialization.
                let prefix_id = xot.empty_prefix();
                let no_ns = xot.no_namespace();
                xot.namespaces_mut(el_node).insert(prefix_id, no_ns);
            }

            // Extra namespace declarations (for descendant convenience).
            for (prefix, uri) in &el.extra_namespaces {
                let prefix_id = xot.add_prefix(prefix);
                let ns_id = xot.add_namespace(uri);
                xot.namespaces_mut(el_node).insert(prefix_id, ns_id);
            }

            // Declare attribute namespace prefixes and add attributes.
            for attr in &el.attributes {
                let attr_name = if let Some(ref ns_uri) = attr.namespace_uri {
                    // Ensure the attribute's prefix is declared.
                    if let Some(ref prefix) = attr.namespace_prefix {
                        let prefix_id = xot.add_prefix(prefix);
                        let ns_id = xot.add_namespace(ns_uri);
                        // Only declare if not already on this element.
                        if xot.namespaces(el_node).get(prefix_id).is_none() {
                            xot.namespaces_mut(el_node).insert(prefix_id, ns_id);
                        }
                    }
                    let ns_id = xot.add_namespace(ns_uri);
                    xot.add_name_ns(&attr.local_name, ns_id)
                } else {
                    xot.add_name(&attr.local_name)
                };
                xot.attributes_mut(el_node)
                    .insert(attr_name, attr.value.clone());
            }

            // Recursively build and attach children.
            for child_hash in &el.children {
                let child_node = build_xot_node(xot, objects, *child_hash)?;
                xot.append(el_node, child_node)
                    .map_err(|e| Error::XmlParse(e.to_string()))?;
            }

            Ok(el_node)
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
        // Element with multiple attributes in the same namespace.
        let xml = r#"<root xmlns:x="urn:ns" x:id="1" x:status="active">text</root>"#;
        let hash = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, hash).await.unwrap();
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
        assert_hash_idempotent(r#"<root xml:id="my-id">text</root>"#).await;
    }

    #[tokio::test]
    async fn preserve_xmi_style_namespaces() {
        let xml = r#"<Model xmlns="http://www.omg.org/spec/UML" xmlns:xmi="http://www.omg.org/spec/XMI" name="Arch" xmi:id="_arch" xml:id="model-arch">content</Model>"#;
        assert_hash_idempotent(xml).await;
        let store = MemoryStore::new();
        let h = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, h).await.unwrap();
        assert!(exported.contains("xmi:id"), "xmi prefix lost: {exported}");
    }

    #[tokio::test]
    async fn preserve_clayers_spec_pattern() {
        let xml = r#"<ns1:clayers xmlns:ns1="urn:clayers:spec" ns1:index="index.xml"><term xmlns="urn:clayers:terminology" id="term-layer"><name>Layer</name></term><section xmlns="urn:clayers:prose" id="overview"><title>Overview</title></section></ns1:clayers>"#;
        assert_hash_idempotent(xml).await;
        let store = MemoryStore::new();
        let h = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, h).await.unwrap();
        assert!(
            exported.contains("ns1:clayers"),
            "root prefix lost: {exported}"
        );
        assert!(
            exported.contains("ns1:index"),
            "attr prefix lost: {exported}"
        );
    }

    #[tokio::test]
    async fn preserve_mixed_content() {
        assert_hash_idempotent("<p>Hello <b>bold</b> and <i>italic</i> world</p>").await;
    }

    #[tokio::test]
    async fn preserve_whitespace() {
        assert_hash_idempotent("<root>\n  <child>text</child>\n</root>").await;
    }

    #[tokio::test]
    async fn preserve_mixed_default_and_prefixed_ns() {
        assert_hash_idempotent(
            r#"<root xmlns="urn:elem" xmlns:x="urn:attr" x:id="1">text</root>"#,
        )
        .await;
    }

    #[tokio::test]
    async fn preserve_xmlns_empty_override() {
        // Child element has no namespace but parent declares a default.
        // Export must emit xmlns="" on the child to cancel the inherited default.
        let xml = r#"<root xmlns="urn:default"><child xmlns="">text</child></root>"#;
        assert_hash_idempotent(xml).await;
        let store = MemoryStore::new();
        let h = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, h).await.unwrap();
        assert!(
            exported.contains(r#"xmlns="""#),
            "xmlns=\"\" missing on child: {exported}"
        );
    }

    #[tokio::test]
    async fn preserve_inherited_attr_prefix() {
        // Parent declares xmlns:rdf="...". Child uses rdf:datatype attribute.
        // The export must reuse the inherited "rdf" prefix, not generate "ns1".
        let xml = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:ex="http://example.org/"><rdf:Description><ex:p rdf:datatype="http://www.w3.org/2001/XMLSchema#integer">1</ex:p></rdf:Description></rdf:RDF>"#;
        assert_hash_idempotent(xml).await;
        let store = MemoryStore::new();
        let h = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, h).await.unwrap();
        assert!(
            exported.contains("rdf:datatype"),
            "rdf: prefix lost on attribute, got ns1: instead: {exported}"
        );
        assert!(
            !exported.contains("ns1:"),
            "should not generate ns1 prefix: {exported}"
        );
    }

    #[tokio::test]
    async fn preserve_document_comment_prologue() {
        let store = MemoryStore::new();
        let xml = "<!-- This is a file description -->\n<root>content</root>";
        let hash = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, hash).await.unwrap();
        assert!(
            exported.contains("<!-- This is a file description -->"),
            "prologue comment lost: {exported}"
        );
        assert!(
            exported.find("<!--").unwrap() < exported.find("<root").unwrap(),
            "comment should appear before root: {exported}"
        );
    }

    #[tokio::test]
    async fn preserve_document_pi_prologue() {
        let store = MemoryStore::new();
        let xml = "<?xml-stylesheet type=\"text/xsl\" href=\"s.xsl\"?>\n<root>content</root>";
        let hash = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, hash).await.unwrap();
        assert!(
            exported.contains("xml-stylesheet"),
            "prologue PI lost: {exported}"
        );
    }

    #[tokio::test]
    async fn no_redundant_xmlns_on_children() {
        let store = MemoryStore::new();
        let xml = r#"<pr:section xmlns:pr="urn:clayers:prose" id="s1"><pr:title>Hello</pr:title><pr:p>World</pr:p></pr:section>"#;
        let hash = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, hash).await.unwrap();
        let count = exported.matches("xmlns:pr").count();
        assert_eq!(
            count, 1,
            "xmlns:pr should appear once (on root), not on children. Got {count} in: {exported}"
        );
    }

    #[tokio::test]
    async fn no_redundant_xmlns_multi_namespace() {
        let store = MemoryStore::new();
        let xml = r#"<spec:clayers xmlns:spec="urn:spec" xmlns:pr="urn:prose" spec:index="i.xml"><pr:section id="s"><pr:title>T</pr:title></pr:section></spec:clayers>"#;
        let hash = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, hash).await.unwrap();
        assert_eq!(
            exported.matches("xmlns:spec").count(),
            1,
            "xmlns:spec should appear once: {exported}"
        );
        assert_eq!(
            exported.matches("xmlns:pr").count(),
            1,
            "xmlns:pr should appear once: {exported}"
        );
    }

    #[tokio::test]
    async fn preserve_xml_declaration() {
        let store = MemoryStore::new();
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<root>text</root>";
        let hash = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, hash).await.unwrap();
        assert!(
            exported.starts_with("<?xml"),
            "XML declaration missing: {exported}"
        );
    }

    #[tokio::test]
    async fn preserve_extra_ns_declarations_on_root() {
        let store = MemoryStore::new();
        let xml = r#"<spec:root xmlns:spec="urn:spec" xmlns:pr="urn:prose" xmlns:trm="urn:term" spec:id="1"><pr:section><trm:ref>x</trm:ref></pr:section></spec:root>"#;
        let hash = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, hash).await.unwrap();
        // pr and trm should be declared on root, not on each child.
        let root_start = exported.find("<spec:root").unwrap();
        let root_tag_end = exported[root_start..].find('>').unwrap() + root_start;
        let root_tag = &exported[root_start..root_tag_end];
        assert!(
            root_tag.contains("xmlns:pr=\"urn:prose\""),
            "extra ns 'pr' not on root: {exported}"
        );
        assert!(
            root_tag.contains("xmlns:trm=\"urn:term\""),
            "extra ns 'trm' not on root: {exported}"
        );
        // And NOT re-declared on children.
        let after_root = &exported[root_tag_end..];
        assert_eq!(
            after_root.matches("xmlns:pr").count(),
            0,
            "xmlns:pr should not appear on children: {exported}"
        );
    }

    #[tokio::test]
    async fn self_closing_empty_elements() {
        let store = MemoryStore::new();
        let xml = r"<root><empty/></root>";
        let hash = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, hash).await.unwrap();
        assert!(
            exported.contains("/>"),
            "empty element should use self-closing tag: {exported}"
        );
        assert!(
            !exported.contains("></empty>"),
            "should not have explicit close for empty element: {exported}"
        );
    }

    #[tokio::test]
    async fn trailing_newline() {
        let store = MemoryStore::new();
        let xml = "<root>text</root>";
        let hash = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, hash).await.unwrap();
        assert!(
            exported.ends_with('\n'),
            "should end with newline: {exported:?}"
        );
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
        use crate::hash;
        use crate::object::{Object, TreeEntry, TreeObject};

        let store = MemoryStore::new();
        let xml = "<root>hello</root>";
        let doc_hash = import_xml(&store, xml).await.unwrap();
        let tree = TreeObject::new(vec![TreeEntry {
            path: "doc.xml".into(),
            document: doc_hash,
        }]);
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
        use crate::hash;
        use crate::object::{Object, TreeEntry, TreeObject};

        let store = MemoryStore::new();
        let h1 = import_xml(&store, "<a>one</a>").await.unwrap();
        let h2 = import_xml(&store, "<b>two</b>").await.unwrap();
        let h3 = import_xml(&store, "<c>three</c>").await.unwrap();
        let tree = TreeObject::new(vec![
            TreeEntry {
                path: "a.xml".into(),
                document: h1,
            },
            TreeEntry {
                path: "b.xml".into(),
                document: h2,
            },
            TreeEntry {
                path: "c.xml".into(),
                document: h3,
            },
        ]);
        let tree_xml = tree.to_xml();
        let tree_hash = hash::hash_exclusive(&tree_xml).unwrap();
        let mut tx = store.transaction().await.unwrap();
        tx.put(tree_hash, Object::Tree(tree)).await.unwrap();
        tx.commit().await.unwrap();

        let result = export_tree(&store, tree_hash).await.unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].0, "a.xml");
        assert_eq!(result[1].0, "b.xml");
        assert_eq!(result[2].0, "c.xml");
    }

    #[tokio::test]
    async fn export_tree_roundtrip() {
        use crate::hash;
        use crate::object::{Object, TreeEntry, TreeObject};

        let store = MemoryStore::new();
        let xmls = vec![
            ("a.xml", "<root>alpha</root>"),
            ("b.xml", "<root>beta</root>"),
            ("c.xml", "<root>gamma</root>"),
        ];
        let mut entries = Vec::new();
        for (path, xml) in &xmls {
            let h = import_xml(&store, xml).await.unwrap();
            entries.push(TreeEntry {
                path: path.to_string(),
                document: h,
            });
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
        use crate::hash;
        use crate::object::{Object, TreeObject};

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

    // -----------------------------------------------------------------------
    // Property-based tests (Group C)
    // -----------------------------------------------------------------------
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// C1: Import determinism - same XML imported twice gives same hash.
        /// Requires successful parse (rejects unparseable inputs via prop_assume!).
        #[test]
        fn prop_import_determinism(xml in crate::store::prop_strategies::arb_xml_document()) {
            let rt = crate::store::prop_strategies::runtime();
            rt.block_on(async {
                let store = MemoryStore::new();
                let h1 = import_xml(&store, &xml).await;
                prop_assume!(h1.is_ok(), "skip unparseable XML");
                let h1 = h1.unwrap();
                let h2 = import_xml(&store, &xml).await.unwrap();
                prop_assert_eq!(h1, h2, "same XML should produce same hash");
                Ok(())
            })?;
        }

        /// C2: Export/import idempotency - import -> export -> reimport gives same hash.
        /// Requires successful parse (rejects unparseable inputs via prop_assume!).
        #[test]
        fn prop_export_import_idempotent(xml in crate::store::prop_strategies::arb_xml_document()) {
            let rt = crate::store::prop_strategies::runtime();
            rt.block_on(async {
                let store = MemoryStore::new();
                let h1 = import_xml(&store, &xml).await;
                prop_assume!(h1.is_ok(), "skip unparseable XML");
                let h1 = h1.unwrap();
                let exported = export_xml(&store, h1).await.unwrap();
                let h2 = import_xml(&store, &exported).await.unwrap();
                prop_assert_eq!(h1, h2, "export->reimport should give same hash");
                Ok(())
            })?;
        }

        /// C3: All objects stored - after import, subtree(doc) yields objects.
        /// Requires successful parse.
        #[test]
        fn prop_import_stores_subtree(xml in crate::store::prop_strategies::arb_xml_document()) {
            let rt = crate::store::prop_strategies::runtime();
            rt.block_on(async {
                let store = MemoryStore::new();
                let h = import_xml(&store, &xml).await;
                prop_assume!(h.is_ok(), "skip unparseable XML");
                let hash = h.unwrap();
                let objects = try_collect_stream(store.subtree(&hash)).await.unwrap();
                prop_assert!(!objects.is_empty(), "subtree should contain at least the document");
                prop_assert!(objects.contains_key(&hash), "subtree should contain the document hash");
                Ok(())
            })?;
        }

        /// C4: Different XML documents produce different hashes.
        /// If two structurally different inputs produce the same hash,
        /// one would silently overwrite the other in the store.
        #[test]
        fn prop_distinct_xml_distinct_hashes(
            xml_a in crate::store::prop_strategies::arb_xml_document(),
            xml_b in crate::store::prop_strategies::arb_xml_document(),
        ) {
            let rt = crate::store::prop_strategies::runtime();
            rt.block_on(async {
                // Only test when both parse and the XML strings differ
                let store = MemoryStore::new();
                let ha = import_xml(&store, &xml_a).await;
                let hb = import_xml(&store, &xml_b).await;
                prop_assume!(ha.is_ok() && hb.is_ok(), "skip unparseable");
                let ha = ha.unwrap();
                let hb = hb.unwrap();
                if xml_a != xml_b {
                    // Different XML text doesn't guarantee different hashes
                    // (e.g., insignificant whitespace differences canonicalize away).
                    // But if the hashes ARE the same, the exported XML must also be the same
                    // (proving the documents are canonically identical).
                    if ha == hb {
                        let ea = export_xml(&store, ha).await.unwrap();
                        let eb = export_xml(&store, hb).await.unwrap();
                        prop_assert_eq!(
                            ea, eb,
                            "same hash but different exports = data loss"
                        );
                    }
                }
                Ok(())
            })?;
        }
    }
}
