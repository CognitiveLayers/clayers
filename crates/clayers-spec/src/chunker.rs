//! Two-pass chunker: one [`Chunk`] per `@id`-bearing element.
//!
//! Pass 1 walks each spec file independently with
//! [`xot::Xot::parse_with_span_info`] to capture byte offsets for every
//! `@id` element, then converts them to 1-indexed line numbers.
//!
//! Pass 2 assembles the combined document via
//! [`crate::assembly::assemble_combined`] and walks it to collect each
//! node's layer, namespace, ancestor chain, and relation incidence.
//! The two passes are joined on the string value of `@id`.
//!
//! See `clayers/clayers/search.xml#search-chunker-strategy` for the
//! design rationale.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use clayers_xml::{CanonicalizationMode, canonicalize_and_hash};
use serde::Serialize;
use xot::{SpanInfoKey, Xot};

use crate::namespace;

#[allow(dead_code)]
fn _namespace_ref() -> &'static str {
    namespace::XML
}

/// A single chunk emitted by the chunker.
#[derive(Debug, Clone, Serialize)]
pub struct Chunk {
    pub id: String,
    pub file: PathBuf,
    pub line_start: usize,
    pub line_end: usize,
    /// Long-form layer name from `urn:clayers:<NAME>` (`prose`,
    /// `terminology`, …), or the raw namespace URI for non-clayers
    /// elements.
    pub layer: String,
    pub namespace: String,
    pub element_name: String,
    /// Context-prepended text: `[layer=X path=A>B>C]\n<content>`.
    pub text: String,
    /// `@id` attribute values of ancestor elements (empty ids are
    /// skipped). Useful for chunk-level display and rel graph joins.
    pub ancestor_ids: Vec<String>,
    /// Element local-names of ancestor elements, root → self-excluded.
    /// Used by the structural fingerprint's path-n-gram segment.
    pub ancestor_local_names: Vec<String>,
    pub outgoing_relation_types: Vec<String>,
    pub incoming_relation_types: Vec<String>,
    /// Inclusive-C14N `sha256:<hex>` of the element.
    pub node_hash: String,
}

/// Extract all chunks from a spec directory.
///
/// # Errors
///
/// Returns an error if the spec's `index.xml` cannot be located, any
/// spec file cannot be parsed, or combined-document assembly fails.
pub fn extract_chunks(spec_dir: &Path) -> Result<Vec<Chunk>, crate::Error> {
    let index = crate::discovery::find_index_files(spec_dir)?
        .into_iter()
        .next()
        .ok_or_else(|| {
            crate::Error::Discovery(format!("no index.xml under {}", spec_dir.display()))
        })?;
    let files = crate::discovery::discover_spec_files(&index)?;

    let spans = pass1_spans(&files)?;
    let (mut xot, root) = crate::assembly::assemble_combined(&files)?;
    Ok(pass2_combined(&mut xot, root, &spans))
}

/// Side-table produced by pass 1, keyed on `@id` string value.
type SpanMap = HashMap<String, (PathBuf, usize, usize)>;

/// Adjacency maps produced during pass 2.
type RelMaps = (HashMap<String, Vec<String>>, HashMap<String, Vec<String>>);

/// Pass 1: per-file span scan.
fn pass1_spans(files: &[PathBuf]) -> Result<SpanMap, crate::Error> {
    let mut out = HashMap::new();
    for file in files {
        let content = std::fs::read_to_string(file)?;
        let mut xot = Xot::new();
        let (doc, span_info) = xot
            .parse_with_span_info(&content)
            .map_err(xot::Error::from)?;
        let id_attr = xot.add_name("id");
        let xml_ns = xot.add_namespace(namespace::XML);
        let xml_id_attr = xot.add_name_ns("id", xml_ns);

        let mut stack: Vec<xot::Node> = vec![doc];
        while let Some(node) = stack.pop() {
            for child in xot.children(node) {
                stack.push(child);
            }
            if !xot.is_element(node) {
                continue;
            }
            let id_val = xot
                .get_attribute(node, id_attr)
                .or_else(|| xot.get_attribute(node, xml_id_attr));
            let Some(id_val) = id_val else { continue };
            let Some(start_span) = span_info.get(SpanInfoKey::ElementStart(node)) else {
                continue;
            };
            let line_start = byte_to_line(&content, start_span.start);
            let line_end = span_info
                .get(SpanInfoKey::ElementEnd(node))
                .map_or(line_start, |s| byte_to_line(&content, s.end));
            out.insert(
                id_val.to_owned(),
                (file.clone(), line_start, line_end),
            );
        }
    }
    Ok(out)
}

fn byte_to_line(src: &str, pos: usize) -> usize {
    src[..pos.min(src.len())].bytes().filter(|&b| b == b'\n').count() + 1
}

/// Pass 2: walk the combined document, collect metadata, emit chunks.
fn pass2_combined(xot: &mut Xot, root: xot::Node, spans: &SpanMap) -> Vec<Chunk> {
    let id_attr = xot.add_name("id");
    let xml_ns = xot.add_namespace(namespace::XML);
    let xml_id_attr = xot.add_name_ns("id", xml_ns);
    let (outgoing, incoming) = collect_relations(xot, root);

    let mut chunks = Vec::new();
    let mut ancestors: Vec<(String, String)> = Vec::new();
    walk_chunks(
        xot,
        root,
        id_attr,
        xml_id_attr,
        spans,
        &outgoing,
        &incoming,
        &mut ancestors,
        &mut chunks,
    );
    chunks
}

#[allow(clippy::too_many_arguments)]
fn walk_chunks(
    xot: &Xot,
    node: xot::Node,
    id_attr: xot::NameId,
    xml_id_attr: xot::NameId,
    spans: &SpanMap,
    outgoing: &HashMap<String, Vec<String>>,
    incoming: &HashMap<String, Vec<String>>,
    ancestors: &mut Vec<(String, String)>,
    chunks: &mut Vec<Chunk>,
) {
    if !xot.is_element(node) {
        return;
    }
    let Some(el) = xot.element(node) else { return };
    let (local_name, ns_uri) = xot.name_ns_str(el.name());
    let local_name = local_name.to_owned();
    let ns_uri = ns_uri.to_owned();

    let id_val = xot
        .get_attribute(node, id_attr)
        .or_else(|| xot.get_attribute(node, xml_id_attr))
        .map(str::to_owned);

    if let Some(id) = &id_val
        && let Some((file, line_start, line_end)) = spans.get(id)
    {
        let layer = layer_from_namespace(&ns_uri);
        let path = build_ancestor_path(ancestors, &local_name);
        let raw_text = collect_text(xot, node);
        let text = format!(
            "[layer={} path={}]\n{}",
            if layer.is_empty() { "?" } else { &layer },
            path,
            raw_text.trim()
        );
        let ancestor_ids: Vec<String> = ancestors
            .iter()
            .filter(|(_, id)| !id.is_empty())
            .map(|(_, id)| id.clone())
            .collect();
        let ancestor_local_names: Vec<String> =
            ancestors.iter().map(|(name, _)| name.clone()).collect();
        let out_types = outgoing.get(id).cloned().unwrap_or_default();
        let in_types = incoming.get(id).cloned().unwrap_or_default();
        let xml_str = xot.to_string(node).unwrap_or_default();
        let node_hash = canonicalize_and_hash(&xml_str, CanonicalizationMode::Inclusive)
            .map(|h| h.to_prefixed())
            .unwrap_or_default();

        chunks.push(Chunk {
            id: id.clone(),
            file: file.clone(),
            line_start: *line_start,
            line_end: *line_end,
            layer,
            namespace: ns_uri.clone(),
            element_name: local_name.clone(),
            text,
            ancestor_ids,
            ancestor_local_names,
            outgoing_relation_types: out_types,
            incoming_relation_types: in_types,
            node_hash,
        });
    }

    ancestors.push((local_name, id_val.unwrap_or_default()));
    for child in xot.children(node) {
        walk_chunks(
            xot, child, id_attr, xml_id_attr, spans, outgoing, incoming, ancestors, chunks,
        );
    }
    ancestors.pop();
}

/// Map a clayers namespace URI to its long-form layer name
/// (`"urn:clayers:terminology"` → `"terminology"`). Non-clayers
/// namespaces return the raw URI.
fn layer_from_namespace(ns_uri: &str) -> String {
    ns_uri
        .strip_prefix("urn:clayers:")
        .map_or_else(|| ns_uri.to_owned(), str::to_owned)
}

fn build_ancestor_path(ancestors: &[(String, String)], self_name: &str) -> String {
    let mut parts: Vec<&str> =
        ancestors.iter().map(|(n, _)| n.as_str()).collect();
    parts.push(self_name);
    parts.join(">")
}

fn collect_text(xot: &Xot, node: xot::Node) -> String {
    let mut out = String::new();
    collect_text_rec(xot, node, &mut out);
    out
}

fn collect_text_rec(xot: &Xot, node: xot::Node, out: &mut String) {
    if let Some(t) = xot.text_str(node) {
        out.push_str(t);
        return;
    }
    for child in xot.children(node) {
        collect_text_rec(xot, child, out);
    }
}

/// Walk the tree for `<rel:relation type="..." from="..." to="..."/>`
/// elements. Returns `(outgoing, incoming)` adjacency maps keyed on
/// id.
fn collect_relations(xot: &mut Xot, root: xot::Node) -> RelMaps {
    let rel_ns = xot.add_namespace(namespace::RELATION);
    let relation_name = xot.add_name_ns("relation", rel_ns);
    let type_attr = xot.add_name("type");
    let from_attr = xot.add_name("from");
    let to_attr = xot.add_name("to");

    let mut outgoing: HashMap<String, Vec<String>> = HashMap::new();
    let mut incoming: HashMap<String, Vec<String>> = HashMap::new();

    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        for child in xot.children(node) {
            stack.push(child);
        }
        if !xot.is_element(node) {
            continue;
        }
        let Some(el) = xot.element(node) else { continue };
        if el.name() != relation_name {
            continue;
        }
        let Some(ty) = xot.get_attribute(node, type_attr) else { continue };
        let from = xot.get_attribute(node, from_attr).unwrap_or("").to_owned();
        let to = xot.get_attribute(node, to_attr).unwrap_or("").to_owned();
        let ty = ty.to_owned();
        if !from.is_empty() {
            outgoing.entry(from).or_default().push(ty.clone());
        }
        if !to.is_empty() {
            incoming.entry(to).or_default().push(ty);
        }
    }
    (outgoing, incoming)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Build a minimal 2-file spec under a tempdir and return the dir
    /// path. The spec has terms + a relation between them.
    fn build_mini_spec() -> TempDir {
        let dir = TempDir::new().unwrap();
        let index = dir.path().join("index.xml");
        let main = dir.path().join("main.xml");
        std::fs::write(
            &index,
            r#"<?xml version="1.0"?>
<spec:clayers xmlns:spec="urn:clayers:spec" xmlns="urn:clayers:index" spec:spec="t" spec:version="0.0.0">
  <file href="main.xml"/>
</spec:clayers>"#,
        )
        .unwrap();
        std::fs::write(
            &main,
            r#"<?xml version="1.0"?>
<spec:clayers xmlns:spec="urn:clayers:spec"
       xmlns:trm="urn:clayers:terminology"
       xmlns:pr="urn:clayers:prose"
       xmlns:rel="urn:clayers:relation"
       spec:index="index.xml">
  <trm:term id="term-alpha">
    <trm:name>Alpha</trm:name>
    <trm:definition>A multi-line
    definition that spans
    several lines of text.</trm:definition>
  </trm:term>
  <trm:term id="term-beta">
    <trm:name>Beta</trm:name>
    <trm:definition>Beta definition.</trm:definition>
  </trm:term>
  <rel:relation type="depends-on" from="term-beta" to="term-alpha"/>
  <rel:relation type="refines" from="term-beta" to="term-alpha"/>
  <pr:section id="sec-empty"/>
</spec:clayers>"#,
        )
        .unwrap();
        dir
    }

    #[test]
    fn byte_to_line_counts_newlines() {
        let s = "line1\nline2\nline3";
        assert_eq!(byte_to_line(s, 0), 1);
        assert_eq!(byte_to_line(s, 6), 2);
        assert_eq!(byte_to_line(s, 12), 3);
    }

    #[test]
    fn layer_from_namespace_strips_prefix() {
        assert_eq!(layer_from_namespace("urn:clayers:terminology"), "terminology");
        assert_eq!(layer_from_namespace("urn:clayers:prose"), "prose");
        // Non-clayers URIs pass through untouched.
        assert_eq!(
            layer_from_namespace("http://example.com/ns"),
            "http://example.com/ns"
        );
    }

    #[test]
    fn chunker_captures_multi_line_spans() {
        let dir = build_mini_spec();
        let chunks = extract_chunks(dir.path()).unwrap();
        let alpha = chunks.iter().find(|c| c.id == "term-alpha").unwrap();
        assert!(
            alpha.line_end > alpha.line_start,
            "multi-line element should span >1 lines: {}-{}",
            alpha.line_start,
            alpha.line_end,
        );
    }

    #[test]
    fn chunker_captures_relation_incidence() {
        let dir = build_mini_spec();
        let chunks = extract_chunks(dir.path()).unwrap();
        let beta = chunks.iter().find(|c| c.id == "term-beta").unwrap();
        let alpha = chunks.iter().find(|c| c.id == "term-alpha").unwrap();
        assert!(beta.outgoing_relation_types.contains(&"depends-on".into()));
        assert!(beta.outgoing_relation_types.contains(&"refines".into()));
        assert!(alpha.incoming_relation_types.contains(&"depends-on".into()));
        assert!(alpha.incoming_relation_types.contains(&"refines".into()));
    }

    #[test]
    fn chunker_handles_empty_text_elements() {
        let dir = build_mini_spec();
        let chunks = extract_chunks(dir.path()).unwrap();
        let sec = chunks.iter().find(|c| c.id == "sec-empty");
        // Empty-text elements MUST still produce a chunk (the text
        // field is effectively just the context header).
        assert!(sec.is_some(), "empty section skipped");
    }

    #[test]
    fn chunker_populates_ancestor_local_names_from_walk() {
        let dir = build_mini_spec();
        let chunks = extract_chunks(dir.path()).unwrap();
        let term = chunks.iter().find(|c| c.id == "term-alpha").unwrap();
        // The combined doc wraps files under <cmb:spec> (local-name
        // "spec"). So every top-level element's first ancestor is "spec".
        assert_eq!(
            term.ancestor_local_names.first().map(String::as_str),
            Some("spec")
        );
    }

    #[test]
    fn chunker_on_spec_with_zero_id_elements_returns_empty() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("index.xml"),
            r#"<?xml version="1.0"?>
<spec:clayers xmlns:spec="urn:clayers:spec" xmlns="urn:clayers:index"
       spec:spec="t" spec:version="0.0.0">
  <file href="main.xml"/>
</spec:clayers>"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("main.xml"),
            r#"<?xml version="1.0"?>
<spec:clayers xmlns:spec="urn:clayers:spec"
       xmlns:pr="urn:clayers:prose"
       spec:index="index.xml">
  <pr:p>Paragraph without an @id attribute.</pr:p>
</spec:clayers>"#,
        )
        .unwrap();
        let chunks = extract_chunks(dir.path()).unwrap();
        assert!(chunks.is_empty(), "expected 0 chunks, got {}", chunks.len());
    }

    #[test]
    fn chunker_extracts_from_example_spec() {
        // Run against the self-referential spec; at least a few chunks
        // should be produced and carry valid fields.
        let workspace = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let spec_dir = std::path::Path::new(&workspace)
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("clayers/clayers");
        if !spec_dir.join("index.xml").exists() {
            eprintln!("skipping: self-ref spec not found at {}", spec_dir.display());
            return;
        }
        let chunks = extract_chunks(&spec_dir).expect("extract");
        assert!(
            chunks.len() > 50,
            "expected plenty of chunks, got {}",
            chunks.len()
        );
        for c in chunks.iter().take(3) {
            assert!(!c.id.is_empty());
            assert!(c.line_start >= 1);
            assert!(!c.text.is_empty(), "empty text for {}", c.id);
            assert!(c.node_hash.starts_with("sha256:"), "bad hash for {}", c.id);
        }
    }
}
