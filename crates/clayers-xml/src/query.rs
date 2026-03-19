//! Shared `XPath` 3.1 evaluation via xee-xpath.
//!
//! All `!Send` xee-xpath types are created and dropped within [`evaluate_xpath`],
//! keeping them invisible to async callers.
//!
//! Namespace prefixes used in the `XPath` expression are automatically
//! discovered from the XML document's root element declarations. Callers
//! may supply additional bindings that override or supplement these.

use xee_xpath::context::StaticContextBuilder;
use xee_xpath::{Documents, Item, Queries, Query};

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

/// Evaluate an `XPath` 3.1 expression against an XML string.
///
/// Namespace prefixes are discovered automatically from the XML document's
/// root element. Additional `namespaces` bindings are merged on top
/// (overriding any conflicting prefix from the document).
///
/// # Errors
///
/// Returns an error if the XML cannot be parsed, the `XPath` cannot be compiled,
/// or execution fails.
pub fn evaluate_xpath(
    xml: &str,
    xpath_expr: &str,
    mode: QueryMode,
    namespaces: &[(&str, &str)],
) -> Result<QueryResult, crate::Error> {
    let mut documents = Documents::new();
    let doc = documents
        .add_string_without_uri(xml)
        .map_err(|e| crate::Error::Query(format!("XML parse error: {e}")))?;

    // Discover namespace declarations from all elements in the document.
    let doc_namespaces = {
        let xot = documents.xot();
        let doc_node = documents
            .document_node(doc)
            .ok_or_else(|| crate::Error::Query("missing document node".into()))?;
        let mut ns_map = std::collections::HashMap::<String, String>::new();
        collect_namespace_declarations(xot, doc_node, &mut ns_map);
        ns_map.into_iter().collect::<Vec<_>>()
    };

    // Build the static context: document namespaces first, caller overrides on top.
    let mut ctx = StaticContextBuilder::default();
    for (prefix, uri) in &doc_namespaces {
        if !prefix.is_empty() && !uri.is_empty() {
            ctx.add_namespace(prefix, uri);
        }
    }
    // Caller-provided namespaces override document ones.
    ctx.namespaces(namespaces.iter().copied());

    let queries = Queries::new(ctx);
    let q = queries
        .sequence(xpath_expr)
        .map_err(|e| crate::Error::Query(format!("XPath compile error: {e}")))?;
    let seq = q
        .execute(&mut documents, doc)
        .map_err(|e| crate::Error::Query(format!("XPath execution error: {e}")))?;

    match mode {
        QueryMode::Count => Ok(QueryResult::Count(seq.iter().count())),
        QueryMode::Text => {
            let xot = documents.xot();
            let texts = seq
                .iter()
                .map(|item| match item {
                    Item::Node(n) => Ok(collect_all_text(xot, n)),
                    _ => item
                        .string_value(xot)
                        .map_err(|e| crate::Error::Query(format!("string value error: {e}"))),
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(QueryResult::Text(texts))
        }
        QueryMode::Xml => {
            let xot = documents.xot();
            let xmls = seq
                .iter()
                .map(|item| match item {
                    Item::Node(n) => Ok(xot.to_string(n).unwrap_or_default()),
                    _ => item
                        .string_value(xot)
                        .map_err(|e| crate::Error::Query(format!("string value error: {e}"))),
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(QueryResult::Xml(xmls))
        }
    }
}

/// Recursively collect all namespace declarations from a node and its descendants.
///
/// First declaration wins: if a prefix is declared on multiple elements,
/// the one closest to the root is kept.
fn collect_namespace_declarations(
    xot: &xot::Xot,
    node: xot::Node,
    ns_map: &mut std::collections::HashMap<String, String>,
) {
    for (prefix_id, ns_id) in xot.namespaces(node).iter() {
        let prefix = xot.prefix_str(prefix_id);
        let uri = xot.namespace_str(*ns_id);
        ns_map.entry(prefix.to_owned()).or_insert_with(|| uri.to_owned());
    }
    for ch in xot.children(node) {
        collect_namespace_declarations(xot, ch, ns_map);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_discovers_namespaces() {
        let xml = r#"<root xmlns:app="urn:test:app"><app:item id="1">hello</app:item></root>"#;
        // No caller namespaces - should discover from document.
        let result = evaluate_xpath(xml, "//app:item", QueryMode::Count, &[]).unwrap();
        match result {
            QueryResult::Count(n) => assert_eq!(n, 1),
            _ => panic!("expected Count"),
        }
    }

    #[test]
    fn caller_namespace_overrides_document() {
        // XML declares xmlns:x="urn:a", caller maps x -> urn:b.
        // The XPath should use the caller's mapping.
        let xml = r#"<root xmlns:x="urn:a"><x:item>hello</x:item></root>"#;
        // With urn:a, should find 1.
        let result = evaluate_xpath(xml, "//x:item", QueryMode::Count, &[]).unwrap();
        assert!(matches!(result, QueryResult::Count(1)));
        // Override to urn:b - no elements match.
        let result =
            evaluate_xpath(xml, "//x:item", QueryMode::Count, &[("x", "urn:b")]).unwrap();
        assert!(matches!(result, QueryResult::Count(0)));
    }

    #[test]
    fn default_namespace_via_caller() {
        // XML uses default ns (no prefix), caller provides a prefix for it.
        let xml = r#"<root xmlns="urn:example"><entry id="1">hello</entry></root>"#;
        let ns = &[("ex", "urn:example")];
        let result = evaluate_xpath(xml, "//ex:entry", QueryMode::Count, ns).unwrap();
        assert!(matches!(result, QueryResult::Count(1)));
    }

    #[test]
    fn text_mode() {
        let xml = r"<root><item>hello</item></root>";
        let result = evaluate_xpath(xml, "//item", QueryMode::Text, &[]).unwrap();
        match result {
            QueryResult::Text(texts) => assert_eq!(texts, vec!["hello"]),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn discovers_namespaces_from_nested_elements() {
        // Only child elements declare the namespace, not the root.
        let xml = r#"<root><app:item xmlns:app="urn:test:app" id="1">hello</app:item></root>"#;
        let result = evaluate_xpath(xml, "//app:item", QueryMode::Count, &[]).unwrap();
        assert!(matches!(result, QueryResult::Count(1)));
    }

    #[test]
    fn count_function() {
        let xml = r"<root><a/><a/><a/></root>";
        let result = evaluate_xpath(xml, "count(//a)", QueryMode::Text, &[]).unwrap();
        match result {
            QueryResult::Text(texts) => {
                assert_eq!(texts.len(), 1);
                assert_eq!(texts[0], "3");
            }
            _ => panic!("expected Text"),
        }
    }
}
