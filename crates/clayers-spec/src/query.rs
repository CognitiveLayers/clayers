use std::path::Path;

use crate::namespace;

/// Result of an `XPath` query.
#[derive(Debug)]
pub enum QueryResult {
    /// Node count (--count mode).
    Count(usize),
    /// Text content (--text mode).
    Text(Vec<String>),
    /// Raw XML output (default mode).
    Xml(Vec<String>),
}

/// Execute an `XPath` query against a spec's combined document.
///
/// Assembles a combined document from the spec files, registers all
/// clayers namespace prefixes, and evaluates the `XPath` expression.
///
/// # Errors
///
/// Returns an error if the spec cannot be assembled or the `XPath` is invalid.
pub fn execute_query(
    spec_dir: &Path,
    xpath_expr: &str,
    mode: QueryMode,
) -> Result<QueryResult, crate::Error> {
    let index_files = crate::discovery::find_index_files(spec_dir)?;
    if index_files.is_empty() {
        return Err(crate::Error::Discovery("no specs found".into()));
    }

    // Collect all file paths from all indices
    let mut all_file_paths = Vec::new();
    for index_path in &index_files {
        let file_paths = crate::discovery::discover_spec_files(index_path)?;
        all_file_paths.extend(file_paths);
    }

    // Assemble combined document as string
    let combined_xml = crate::assembly::assemble_combined_string(&all_file_paths)?;

    // Pre-collect predicate attribute names so they outlive the Xot instance.
    // xot.add_name() requires &'a str where 'a is Xot's lifetime parameter,
    // so these strings must live at least as long as the Xot.
    let attr_names = collect_predicate_attr_names(xpath_expr)?;

    // Parse into xot and evaluate XPath
    let mut xot = xot::Xot::new();
    let doc = xot.parse(&combined_xml).map_err(xot::Error::from)?;
    let root = xot.document_element(doc)?;

    // For XPath evaluation, we use a simple tree-walking approach
    // since xee-xpath may not be available. This handles the most
    // common query patterns used by the spec tooling.
    match mode {
        QueryMode::Count => {
            let count = evaluate_xpath_count(&mut xot, root, xpath_expr, &attr_names)?;
            Ok(QueryResult::Count(count))
        }
        QueryMode::Text => {
            let texts = evaluate_xpath_text(&mut xot, root, xpath_expr, &attr_names)?;
            Ok(QueryResult::Text(texts))
        }
        QueryMode::Xml => {
            let xmls = evaluate_xpath_xml(&mut xot, root, xpath_expr, &attr_names)?;
            Ok(QueryResult::Xml(xmls))
        }
    }
}

/// Query output mode.
#[derive(Debug, Clone, Copy)]
pub enum QueryMode {
    Count,
    Text,
    Xml,
}

/// Simple `XPath` evaluator for the subset of `XPath` used by the spec tooling.
///
/// Supports: `//prefix:element`, `//prefix:element[@attr="value"]`,
/// `//prefix:element[@attr="value"]/prefix:child`
fn evaluate_xpath_count(
    xot: &mut xot::Xot,
    root: xot::Node,
    xpath: &str,
    attr_names: &[String],
) -> Result<usize, crate::Error> {
    let nodes = find_matching_nodes(xot, root, xpath, attr_names)?;
    Ok(nodes.len())
}

fn evaluate_xpath_text(
    xot: &mut xot::Xot,
    root: xot::Node,
    xpath: &str,
    attr_names: &[String],
) -> Result<Vec<String>, crate::Error> {
    let nodes = find_matching_nodes(xot, root, xpath, attr_names)?;
    Ok(nodes
        .into_iter()
        .map(|n| collect_all_text(xot, n))
        .collect())
}

fn evaluate_xpath_xml(
    xot: &mut xot::Xot,
    root: xot::Node,
    xpath: &str,
    attr_names: &[String],
) -> Result<Vec<String>, crate::Error> {
    let nodes = find_matching_nodes(xot, root, xpath, attr_names)?;
    let mut results = Vec::new();
    for node in nodes {
        results.push(xot.to_string(node).unwrap_or_default());
    }
    Ok(results)
}

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

/// Resolve parsed `XPath` steps by interning attribute names into the Xot arena.
///
/// The `attr_names` vector must outlive the `Xot` instance since `add_name`
/// borrows the string for the Xot's lifetime.
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

/// Parse a simplified `XPath` and find matching nodes.
///
/// Supported patterns:
/// - `//prefix:element` - descendant elements
/// - `//prefix:element[@attr="value"]` - with attribute predicate
/// - `//prefix:element[@attr="value"]/prefix:child` - with child step
/// - `//prefix:element/prefix:child` - parent/child
fn find_matching_nodes(
    xot: &mut xot::Xot,
    root: xot::Node,
    xpath: &str,
    attr_names: &[String],
) -> Result<Vec<xot::Node>, crate::Error> {
    let xpath = xpath.trim();
    let steps = parse_xpath_steps(xpath)?;
    let resolved = resolve_steps(xot, steps, attr_names);

    let mut results = Vec::new();
    find_nodes_by_steps(xot, root, &resolved, 0, true, &mut results);
    Ok(results)
}

/// Collect predicate attribute names from an `XPath` expression.
fn collect_predicate_attr_names(xpath: &str) -> Result<Vec<String>, crate::Error> {
    let steps = parse_xpath_steps(xpath.trim())?;
    Ok(steps
        .into_iter()
        .filter_map(|s| s.predicate.map(|(name, _)| name))
        .collect())
}

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

fn parse_xpath_steps(xpath: &str) -> Result<Vec<ParsedStep>, crate::Error> {
    let xpath = xpath.trim();
    if !xpath.starts_with("//") {
        return Err(crate::Error::Query(format!(
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

fn parse_single_step(step: &str, is_first: bool) -> Result<ParsedStep, crate::Error> {
    let (name_part, predicate) = if let Some(bracket_start) = step.find('[') {
        let bracket_end = step
            .rfind(']')
            .ok_or_else(|| crate::Error::Query(format!("unmatched bracket in: {step}")))?;
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

fn parse_predicate(pred: &str) -> Result<(String, String), crate::Error> {
    // Parse @attr="value"
    let pred = pred.trim();
    if !pred.starts_with('@') {
        return Err(crate::Error::Query(format!(
            "only @attr=\"value\" predicates supported, got: {pred}"
        )));
    }

    let pred = &pred[1..];
    let eq_pos = pred
        .find('=')
        .ok_or_else(|| crate::Error::Query(format!("missing = in predicate: {pred}")))?;

    let attr_name = pred[..eq_pos].to_string();
    let value_str = &pred[eq_pos + 1..];
    let value = value_str.trim_matches('"').trim_matches('\'').to_string();

    Ok((attr_name, value))
}

fn find_nodes_by_steps(
    xot: &xot::Xot,
    node: xot::Node,
    steps: &[ResolvedStep],
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

    // Check if this node matches the current step
    if matches_step(xot, node, resolved) {
        if is_last {
            results.push(node);
        } else {
            // Continue with next step on children
            for child in xot.children(node) {
                find_nodes_by_steps(xot, child, steps, step_idx + 1, false, results);
            }
        }
    }

    // If searching descendants (// axis), recurse into all children
    if search_descendants || resolved.is_descendant {
        for child in xot.children(node) {
            find_nodes_by_steps(xot, child, steps, step_idx, true, results);
        }
    }
}

fn matches_step(xot: &xot::Xot, node: xot::Node, resolved: &ResolvedStep) -> bool {
    let Some(element) = xot.element(node) else {
        return false;
    };

    let name_id = element.name();
    let (local, ns_uri) = xot.name_ns_str(name_id);
    if local != resolved.local_name {
        return false;
    }

    // Check namespace prefix
    if !resolved.prefix.is_empty()
        && let Some(expected) = namespace::uri_for(&resolved.prefix)
        && ns_uri != expected
    {
        return false;
    }

    // Check predicate
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn spec_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../clayers/clayers")
            .canonicalize()
            .expect("clayers/clayers/ not found")
    }

    #[test]
    fn query_count_terms() {
        let result =
            execute_query(&spec_dir(), "//trm:term", QueryMode::Count).expect("query failed");
        if let QueryResult::Count(count) = result {
            assert!(count >= 15, "expected 15+ terms, got {count}");
        } else {
            panic!("expected Count result");
        }
    }

    #[test]
    fn query_count_depends_on_relations() {
        let result = execute_query(
            &spec_dir(),
            "//rel:relation[@type=\"depends-on\"]",
            QueryMode::Count,
        )
        .expect("query failed");
        if let QueryResult::Count(count) = result {
            assert!(count >= 20, "expected 20+ depends-on, got {count}");
        } else {
            panic!("expected Count result");
        }
    }

    #[test]
    fn query_text_term_definition() {
        let result = execute_query(
            &spec_dir(),
            "//trm:term[@id=\"term-layer\"]/trm:definition",
            QueryMode::Text,
        )
        .expect("query failed");
        if let QueryResult::Text(texts) = result {
            assert!(!texts.is_empty(), "should find term-layer definition");
            let text = &texts[0];
            assert!(
                text.len() > 10,
                "definition should have meaningful text: {text}"
            );
            assert!(!text.contains("<trm:"), "text should not contain XML tags");
        } else {
            panic!("expected Text result");
        }
    }

    #[test]
    fn query_xml_output() {
        let result = execute_query(
            &spec_dir(),
            "//trm:term[@id=\"term-layer\"]",
            QueryMode::Xml,
        )
        .expect("query failed");
        if let QueryResult::Xml(xmls) = result {
            assert!(!xmls.is_empty(), "should find term-layer");
            let xml = &xmls[0];
            assert!(xml.contains('<'), "should contain XML");
            let old_urn = ["living", "spec"].concat();
            assert!(!xml.contains(&old_urn), "should not contain old URN");
        } else {
            panic!("expected Xml result");
        }
    }

    #[test]
    fn all_namespace_prefixes_resolve() {
        // Verify all 13 prefixes are usable
        let prefixes = [
            "pr", "trm", "org", "rel", "art", "llm", "rev", "spec", "cmb", "idx", "dec", "src",
            "pln",
        ];
        for prefix in prefixes {
            assert!(
                namespace::uri_for(prefix).is_some(),
                "prefix {prefix} should resolve to a URI"
            );
        }
    }
}
