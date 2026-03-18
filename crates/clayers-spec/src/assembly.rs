use std::path::Path;

use xot::Xot;

use crate::namespace;

/// Assemble a combined document from spec files by merging all children
/// under a `<cmb:spec>` root element.
///
/// Each input file should be a `<spec:clayers>` document. Children of each
/// file's root are moved under the combined root, preserving all namespaces.
///
/// # Errors
///
/// Returns an error if any file cannot be read or parsed as XML.
pub fn assemble_combined(
    file_paths: &[impl AsRef<Path>],
) -> Result<(Xot, xot::Node), crate::Error> {
    let mut xot = Xot::new();

    // Register all namespace prefixes
    for (prefix, uri) in namespace::PREFIX_MAP {
        let ns_id = xot.add_namespace(uri);
        let prefix_id = xot.add_prefix(prefix);
        // We'll set these on the root element after creating it
        let _ = (ns_id, prefix_id);
    }

    // Build xmlns declarations string
    let xmlns_decls: Vec<String> = namespace::PREFIX_MAP
        .iter()
        .map(|(prefix, uri)| format!("xmlns:{prefix}=\"{uri}\""))
        .collect();

    // Collect inner XML from all files
    let mut inner_xml = String::new();
    for file_path in file_paths {
        let content = std::fs::read_to_string(file_path.as_ref())?;
        // Extract content between the root element's opening and closing tags
        if let Some(inner) = extract_inner_xml(&content) {
            inner_xml.push_str(inner);
            inner_xml.push('\n');
        }
    }

    // Build the combined document
    let combined_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<cmb:spec {}>\n{inner_xml}</cmb:spec>",
        xmlns_decls.join(" "),
    );

    let doc = xot.parse(&combined_xml).map_err(xot::Error::from)?;
    let root = xot.document_element(doc)?;

    Ok((xot, root))
}

/// Extract the inner XML content from a `<spec:clayers ...>` document.
///
/// Returns the content between the root element's opening and closing tags.
fn extract_inner_xml(xml: &str) -> Option<&str> {
    let trimmed = xml.trim();

    // Skip XML declaration if present
    let mut content = if trimmed.starts_with("<?xml") {
        let decl_end = trimmed.find("?>")?;
        trimmed[decl_end + 2..].trim()
    } else {
        trimmed
    };

    // Skip any comments before the root element
    while content.starts_with("<!--") {
        let comment_end = content.find("-->")?;
        content = content[comment_end + 3..].trim();
    }

    // Now content should start with the root element '<spec:clayers ...'
    if !content.starts_with('<') {
        return None;
    }

    // Find the end of the opening root tag
    let first_gt = content.find('>')?;

    // Check for self-closing tag
    if content[..first_gt].ends_with('/') {
        return Some("");
    }

    let inner_start = first_gt + 1;

    // Find the closing tag (last '</' in the document)
    let close_start = content.rfind("</")?;

    if close_start <= inner_start {
        return Some("");
    }

    Some(&content[inner_start..close_start])
}

/// Assemble combined document and return it as an XML string.
///
/// # Errors
///
/// Returns an error if any file cannot be read or parsed.
pub fn assemble_combined_string(file_paths: &[impl AsRef<Path>]) -> Result<String, crate::Error> {
    let (xot, root) = assemble_combined(file_paths)?;
    Ok(xot.to_string(root).unwrap_or_default())
}

// Public API surface (used by ast-grep for structural verification).
#[cfg(any())]
mod _api {
    use super::*;
    pub fn assemble_combined(
        file_paths: &[impl AsRef<Path>],
    ) -> Result<(Xot, xot::Node), crate::Error>;
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

    fn spec_files() -> Vec<PathBuf> {
        crate::discovery::discover_spec_files(&spec_dir().join("index.xml"))
            .expect("discovery failed")
    }

    #[test]
    fn assemble_shipped_spec_has_combined_root() {
        let files = spec_files();
        let (mut xot, root) = assemble_combined(&files).expect("assembly failed");

        let cmb_ns = xot.add_namespace(namespace::COMBINED);
        let spec_name = xot.add_name_ns("spec", cmb_ns);
        assert!(xot.element(root).is_some_and(|e| e.name() == spec_name));
    }

    #[test]
    fn combined_doc_has_elements_from_multiple_layers() {
        let files = spec_files();
        let (xot, root) = assemble_combined(&files).expect("assembly failed");
        let xml = xot.to_string(root).unwrap_or_default();

        // Should contain elements from at least prose, terminology, and relation layers
        assert!(xml.contains("urn:clayers:prose"), "missing prose namespace");
        assert!(
            xml.contains("urn:clayers:terminology"),
            "missing terminology namespace"
        );
        assert!(
            xml.contains("urn:clayers:relation"),
            "missing relation namespace"
        );
    }

    #[test]
    fn combined_doc_preserves_ids() {
        let files = spec_files();
        let (xot, root) = assemble_combined(&files).expect("assembly failed");
        let xml = xot.to_string(root).unwrap_or_default();

        // Known IDs from the self-referential spec
        assert!(xml.contains("\"term-layer\""), "missing term-layer id");
        assert!(
            xml.contains("\"layered-architecture\""),
            "missing layered-architecture id"
        );
    }

    #[test]
    fn assemble_single_file() {
        let spec = spec_dir();
        let overview = spec.join("overview.xml");
        let binding = [&overview];
        let (xot, root) = assemble_combined(&binding).expect("assembly failed");
        let xml = xot.to_string(root).unwrap_or_default();
        assert!(xml.contains("cmb:spec"), "missing combined root");
    }
}
