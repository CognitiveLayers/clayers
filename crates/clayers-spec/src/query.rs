use std::path::Path;

use clayers_xml::query::{QueryMode as XmlQueryMode, QueryResult as XmlQueryResult};

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

    let mut all_file_paths = Vec::new();
    for index_path in &index_files {
        let file_paths = crate::discovery::discover_spec_files(index_path)?;
        all_file_paths.extend(file_paths);
    }

    let combined_xml = crate::assembly::assemble_combined_string(&all_file_paths)?;

    let xml_mode = match mode {
        QueryMode::Count => XmlQueryMode::Count,
        QueryMode::Text => XmlQueryMode::Text,
        QueryMode::Xml => XmlQueryMode::Xml,
    };

    let result = clayers_xml::query::evaluate_xpath(
        &combined_xml,
        xpath_expr,
        xml_mode,
        &[],
    )?;

    Ok(match result {
        XmlQueryResult::Count(n) => QueryResult::Count(n),
        XmlQueryResult::Text(t) => QueryResult::Text(t),
        XmlQueryResult::Xml(x) => QueryResult::Xml(x),
    })
}

/// Query output mode.
#[derive(Debug, Clone, Copy)]
pub enum QueryMode {
    Count,
    Text,
    Xml,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace;
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

    // --- XPath 3.1 feature tests ---

    #[test]
    fn query_absolute_path_terms() {
        // The combined document has <cmb:spec> as root, not <spec:clayers>.
        let result = execute_query(
            &spec_dir(),
            "/cmb:spec/trm:term/trm:name",
            QueryMode::Text,
        )
        .expect("absolute path query failed");
        if let QueryResult::Text(texts) = result {
            assert!(!texts.is_empty(), "should find term names via absolute path");
        } else {
            panic!("expected Text result");
        }
    }

    #[test]
    fn query_count_function() {
        let result = execute_query(
            &spec_dir(),
            "count(//trm:term)",
            QueryMode::Text,
        )
        .expect("count() query failed");
        if let QueryResult::Text(texts) = result {
            assert_eq!(texts.len(), 1, "count() should return one value");
            let count: f64 = texts[0].parse().expect("count should be numeric");
            assert!(count >= 15.0, "expected 15+ terms, got {count}");
        } else {
            panic!("expected Text result");
        }
    }

    #[test]
    fn query_starts_with_predicate() {
        let result = execute_query(
            &spec_dir(),
            "//trm:term[starts-with(@id, 'term-')]",
            QueryMode::Count,
        )
        .expect("starts-with query failed");
        if let QueryResult::Count(count) = result {
            assert!(count >= 15, "expected 15+ terms with 'term-' prefix, got {count}");
        } else {
            panic!("expected Count result");
        }
    }

    #[test]
    fn query_string_length_function() {
        let result = execute_query(
            &spec_dir(),
            "string-length(//trm:term[@id='term-layer']/trm:name)",
            QueryMode::Text,
        )
        .expect("string-length query failed");
        if let QueryResult::Text(texts) = result {
            assert_eq!(texts.len(), 1, "should return one value");
            let len: f64 = texts[0].parse().expect("should be numeric");
            assert!(len > 0.0, "term name should have non-zero length");
        } else {
            panic!("expected Text result");
        }
    }

    #[test]
    fn query_positional_predicate() {
        let result = execute_query(
            &spec_dir(),
            "(//trm:term)[1]/trm:name",
            QueryMode::Text,
        )
        .expect("positional predicate query failed");
        if let QueryResult::Text(texts) = result {
            assert_eq!(texts.len(), 1, "should return exactly 1 term name");
        } else {
            panic!("expected Text result");
        }
    }
}
