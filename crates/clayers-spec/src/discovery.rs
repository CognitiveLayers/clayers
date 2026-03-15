use std::path::{Path, PathBuf};

use crate::namespace;

/// Find all index.xml files reachable from the target path.
///
/// Scans all XML files. A file is an index if its root element has
/// children in the `urn:clayers:index` namespace with `href` attributes.
/// Non-index files with a `spec:index` attribute pointing to an index
/// are also resolved.
///
/// # Errors
///
/// Returns an error if the path cannot be read.
pub fn find_index_files(target: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let xml_files = find_xml_files(target);
    let mut index_paths = std::collections::BTreeSet::new();

    for xml_path in &xml_files {
        if is_index_file(xml_path) {
            if let Ok(canonical) = xml_path.canonicalize() {
                index_paths.insert(canonical);
            }
        } else if let Some(index_ref) = resolve_index(xml_path)
            && index_ref.exists()
            && let Ok(canonical) = index_ref.canonicalize()
        {
            index_paths.insert(canonical);
        }
    }

    Ok(index_paths.into_iter().collect())
}

/// Discover all spec files referenced from an index document.
///
/// Parses the index XML, finds all `idx:file` elements with `href`
/// attributes, and resolves them relative to the index's parent directory.
/// The index file itself is always included in the result.
///
/// # Errors
///
/// Returns an error if the index file cannot be read or parsed.
pub fn discover_spec_files(index_path: &Path) -> Result<Vec<PathBuf>, crate::Error> {
    let spec_dir = index_path
        .parent()
        .ok_or_else(|| crate::Error::Discovery("index has no parent dir".into()))?;

    let content = std::fs::read_to_string(index_path)?;
    let mut xot = xot::Xot::new();
    let doc = xot.parse(&content)?;
    let root = xot.document_element(doc)?;

    let idx_ns = xot.add_namespace(namespace::INDEX);
    let file_name = xot.add_name_ns("file", idx_ns);
    let href_name = xot.add_name("href");

    let mut file_paths = Vec::new();
    collect_file_refs(&xot, root, file_name, href_name, spec_dir, &mut file_paths);

    // Always include the index itself
    if let Ok(canonical) = index_path.canonicalize()
        && !file_paths.contains(&canonical)
    {
        file_paths.push(canonical);
    }

    Ok(file_paths)
}

fn collect_file_refs(
    xot: &xot::Xot,
    node: xot::Node,
    file_name: xot::NameId,
    href_name: xot::NameId,
    spec_dir: &Path,
    out: &mut Vec<PathBuf>,
) {
    if xot.is_element(node)
        && xot.element(node).is_some_and(|e| e.name() == file_name)
        && let Some(href) = xot.element(node).and_then(|e| e.get_attribute(href_name))
    {
        let resolved = spec_dir.join(href);
        if let Ok(canonical) = resolved.canonicalize() {
            out.push(canonical);
        }
    }
    for child in xot.children(node) {
        collect_file_refs(xot, child, file_name, href_name, spec_dir, out);
    }
}

fn find_xml_files(target: &Path) -> Vec<PathBuf> {
    if target.is_file() {
        if target.extension().is_some_and(|ext| ext == "xml") {
            return vec![target.to_path_buf()];
        }
        return vec![];
    }
    let mut files = Vec::new();
    if let Ok(entries) = walkdir(target) {
        for entry in entries {
            if entry.extension().is_some_and(|ext| ext == "xml") {
                files.push(entry);
            }
        }
    }
    files.sort();
    files
}

fn walkdir(dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut result = Vec::new();
    walkdir_inner(dir, &mut result)?;
    Ok(result)
}

fn walkdir_inner(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                walkdir_inner(&path, out)?;
            } else {
                out.push(path);
            }
        }
    }
    Ok(())
}

fn is_index_file(xml_path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(xml_path) else {
        return false;
    };
    let mut xot = xot::Xot::new();
    let Ok(doc) = xot.parse(&content) else {
        return false;
    };
    let Ok(root) = xot.document_element(doc) else {
        return false;
    };

    let spec_ns = xot.add_namespace(namespace::SPEC);
    let spec_tag = xot.add_name_ns("clayers", spec_ns);
    if xot.element(root).is_none_or(|e| e.name() != spec_tag) {
        return false;
    }

    let idx_ns = xot.add_namespace(namespace::INDEX);
    let file_name = xot.add_name_ns("file", idx_ns);
    xot.children(root)
        .any(|child| xot.element(child).is_some_and(|e| e.name() == file_name))
}

fn resolve_index(xml_path: &Path) -> Option<PathBuf> {
    let content = std::fs::read_to_string(xml_path).ok()?;
    let mut xot = xot::Xot::new();
    let doc = xot.parse(&content).ok()?;
    let root = xot.document_element(doc).ok()?;

    let spec_ns = xot.add_namespace(namespace::SPEC);
    let index_attr = xot.add_name_ns("index", spec_ns);

    let index_ref = xot
        .element(root)
        .and_then(|e| e.get_attribute(index_attr))?;
    let parent = xml_path.parent()?;
    Some(parent.join(index_ref))
}

/// Find the schema directory by walking up from the given path.
///
/// Checks for `schemas/` and `.clayers/schemas/` at each level.
/// Returns the first directory found, or `None` if neither exists.
#[must_use]
pub fn find_schema_dir(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        let schemas = dir.join("schemas");
        if schemas.is_dir() {
            return Some(schemas);
        }
        let dot_clayers = dir.join(".clayers").join("schemas");
        if dot_clayers.is_dir() {
            return Some(dot_clayers);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../clayers/clayers")
            .canonicalize()
            .expect("clayers/clayers/ not found")
    }

    #[test]
    fn discover_spec_files_from_shipped_spec() {
        let index = spec_dir().join("index.xml");
        let files = discover_spec_files(&index).expect("discovery failed");
        // 12 content files + 1 index = 13
        assert!(files.len() >= 13, "expected 13+ files, got {}", files.len());
    }

    #[test]
    fn all_discovered_files_exist() {
        let index = spec_dir().join("index.xml");
        let files = discover_spec_files(&index).expect("discovery failed");
        for f in &files {
            assert!(f.exists(), "discovered file doesn't exist: {}", f.display());
        }
    }

    #[test]
    fn find_index_files_finds_shipped_spec() {
        let specs_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../clayers")
            .canonicalize()
            .expect("clayers/ not found");
        let indices = find_index_files(&specs_root).expect("find failed");
        assert!(
            !indices.is_empty(),
            "should find at least one index.xml in clayers/"
        );
    }
}
