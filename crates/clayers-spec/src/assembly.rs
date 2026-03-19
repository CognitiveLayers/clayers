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

    // Create <cmb:spec> root element with all namespace declarations
    let cmb_ns = xot.add_namespace(namespace::COMBINED);
    let spec_name = xot.add_name_ns("spec", cmb_ns);
    let root = xot.new_element(spec_name);

    for (prefix, uri) in namespace::PREFIX_MAP {
        let ns_id = xot.add_namespace(uri);
        let prefix_id = xot.add_prefix(prefix);
        xot.namespaces_mut(root).insert(prefix_id, ns_id);
    }

    // Parse each file and move its root element's children under <cmb:spec>
    for file_path in file_paths {
        let content = std::fs::read_to_string(file_path.as_ref())?;
        let doc = xot.parse(&content).map_err(xot::Error::from)?;
        let file_root = xot.document_element(doc)?;

        // Collect children first to avoid iterator invalidation during moves
        let children: Vec<_> = xot.children(file_root).collect();
        for child in children {
            xot.append(root, child)?;
        }
    }

    Ok((xot, root))
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
