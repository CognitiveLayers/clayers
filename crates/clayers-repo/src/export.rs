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

    let Object::Document(doc) = objects.get(&hash).ok_or(Error::NotFound(hash))? else {
        return Err(Error::InvalidObject("expected Document object".into()));
    };

    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");

    // Emit prologue (comments, PIs before root element).
    {
        use std::fmt::Write;
        for prologue_hash in &doc.prologue {
            if let Some(obj) = objects.get(prologue_hash) {
                match obj {
                    Object::Comment(c) => {
                        let _ = writeln!(xml, "<!--{}-->", c.content);
                    }
                    Object::PI(pi) => match &pi.data {
                        Some(d) if !d.is_empty() => {
                            let _ = writeln!(xml, "<?{} {}?>", pi.target, d);
                        }
                        _ => {
                            let _ = writeln!(xml, "<?{}?>", pi.target);
                        }
                    },
                    _ => {}
                }
            }
        }
    }

    // Build XML from the root element's object tree.
    xml.push_str(&build_xml_from_objects(&objects, doc.root)?);
    xml.push('\n');

    Ok(xml)
}

/// Synchronously build an XML string from a pre-collected object map.
fn build_xml_from_objects(
    objects: &HashMap<ContentHash, Object>,
    hash: ContentHash,
) -> Result<String> {
    let scope = HashMap::new(); // empty: no inherited namespaces at root
    build_xml_recursive(objects, hash, &scope)
}

/// The `xml` namespace is always predeclared and must use the `xml` prefix.
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

/// Build XML recursively, tracking namespace scope to avoid redundant declarations.
///
/// `parent_scope` maps prefix -> URI for namespaces already declared by ancestors.
#[allow(clippy::too_many_lines)]
fn build_xml_recursive(
    objects: &HashMap<ContentHash, Object>,
    hash: ContentHash,
    parent_scope: &HashMap<String, String>,
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
            let mut this_scope = parent_scope.clone();

            let elem_prefix = el.namespace_prefix.as_deref().unwrap_or("");

            // Opening tag.
            xml.push('<');
            if !elem_prefix.is_empty() {
                let _ = write!(xml, "{elem_prefix}:");
            }
            xml.push_str(&el.local_name);

            // Element namespace declaration (only if not already in scope).
            if let Some(ref ns) = el.namespace_uri {
                let already_declared = if elem_prefix.is_empty() {
                    parent_scope.get("") == Some(ns)
                } else {
                    parent_scope.get(elem_prefix) == Some(ns)
                };
                if !already_declared {
                    if elem_prefix.is_empty() {
                        let _ = write!(xml, " xmlns=\"{ns}\"");
                        this_scope.insert(String::new(), ns.clone());
                    } else {
                        let _ = write!(xml, " xmlns:{elem_prefix}=\"{ns}\"");
                        this_scope.insert(elem_prefix.to_string(), ns.clone());
                    }
                }
            }

            // Collect attribute namespace prefixes.
            let mut attr_ns_prefixes: HashMap<String, String> = HashMap::new();
            let mut used_prefixes: std::collections::HashSet<String> =
                this_scope.keys().cloned().collect();
            used_prefixes.insert("xml".to_string());
            attr_ns_prefixes.insert(XML_NS.to_string(), "xml".to_string());
            if !elem_prefix.is_empty() {
                used_prefixes.insert(elem_prefix.to_string());
                if let Some(ref ns) = el.namespace_uri {
                    attr_ns_prefixes.insert(ns.clone(), elem_prefix.to_string());
                }
            }
            let mut prefix_counter = 1u32;
            for attr in &el.attributes {
                if let Some(ref ns) = attr.namespace_uri {
                    attr_ns_prefixes.entry(ns.clone()).or_insert_with(|| {
                        let candidate = attr.namespace_prefix.clone().unwrap_or_else(|| loop {
                            let p = format!("ns{prefix_counter}");
                            prefix_counter += 1;
                            if !used_prefixes.contains(&p) {
                                return p;
                            }
                        });
                        if used_prefixes.contains(&candidate) {
                            loop {
                                let p = format!("ns{prefix_counter}");
                                prefix_counter += 1;
                                if !used_prefixes.contains(&p) {
                                    used_prefixes.insert(p.clone());
                                    return p;
                                }
                            }
                        } else {
                            used_prefixes.insert(candidate.clone());
                            candidate
                        }
                    });
                }
            }

            // Emit attribute namespace declarations (skip if in scope or xml ns).
            for (ns_uri, prefix) in &attr_ns_prefixes {
                if ns_uri == XML_NS {
                    continue;
                }
                if this_scope.get(prefix.as_str()) == Some(ns_uri) {
                    continue; // Already in scope from element or ancestor.
                }
                let _ = write!(xml, " xmlns:{prefix}=\"{ns_uri}\"");
                this_scope.insert(prefix.clone(), ns_uri.clone());
            }

            // Emit extra namespace declarations (for descendant convenience).
            for (prefix, uri) in &el.extra_namespaces {
                if this_scope.get(prefix.as_str()) == Some(uri) {
                    continue;
                }
                let _ = write!(xml, " xmlns:{prefix}=\"{uri}\"");
                this_scope.insert(prefix.clone(), uri.clone());
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

            if el.children.is_empty() {
                // Self-closing tag for empty elements.
                xml.push_str("/>");
            } else {
                xml.push('>');

                // Children (pass this element's scope down).
                for child_hash in &el.children {
                    let child_xml = build_xml_recursive(objects, *child_hash, &this_scope)?;
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
            }

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
    async fn preserve_document_comment_prologue() {
        // Comments before the root element must survive the round-trip.
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
        // Processing instructions before the root element must survive.
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
        // Children sharing the parent's namespace prefix must NOT re-declare it.
        let store = MemoryStore::new();
        let xml = r#"<pr:section xmlns:pr="urn:clayers:prose" id="s1"><pr:title>Hello</pr:title><pr:p>World</pr:p></pr:section>"#;
        let hash = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, hash).await.unwrap();
        // Count how many times xmlns:pr appears - should be exactly 1 (on the root).
        let count = exported.matches("xmlns:pr").count();
        assert_eq!(
            count, 1,
            "xmlns:pr should appear once (on root), not on children. Got {count} in: {exported}"
        );
    }

    #[tokio::test]
    async fn no_redundant_xmlns_multi_namespace() {
        // Root declares multiple prefixes. Children using those prefixes
        // must not re-declare them.
        let store = MemoryStore::new();
        let xml = r#"<spec:clayers xmlns:spec="urn:spec" xmlns:pr="urn:prose" spec:index="i.xml"><pr:section id="s"><pr:title>T</pr:title></pr:section></spec:clayers>"#;
        let hash = import_xml(&store, xml).await.unwrap();
        let exported = export_xml(&store, hash).await.unwrap();
        assert_eq!(
            exported.matches("xmlns:spec").count(), 1,
            "xmlns:spec should appear once: {exported}"
        );
        assert_eq!(
            exported.matches("xmlns:pr").count(), 1,
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
        // Root declares xmlns:pr and xmlns:trm for child convenience.
        // These must survive the round-trip even though root doesn't use them.
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
            after_root.matches("xmlns:pr").count(), 0,
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
