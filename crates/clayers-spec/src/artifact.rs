use std::path::{Path, PathBuf};

use clayers_xml::ContentHash;
use sha2::{Digest, Sha256};

use crate::namespace;

/// An artifact mapping parsed from spec XML.
#[derive(Debug)]
pub struct ArtifactMapping {
    pub id: String,
    pub spec_ref_node: String,
    pub spec_ref_revision: String,
    pub node_hash: Option<String>,
    pub artifact_path: String,
    pub artifact_repo: String,
    pub ranges: Vec<ArtifactRange>,
    pub coverage: String,
    /// The XML file this mapping was parsed from.
    pub source_file: PathBuf,
}

/// A range within an artifact file.
#[derive(Debug)]
pub struct ArtifactRange {
    pub hash: Option<String>,
    pub start_line: Option<u64>,
    pub end_line: Option<u64>,
    pub start_byte: Option<u64>,
    pub end_byte: Option<u64>,
}

/// Collect all artifact mappings from spec files.
///
/// # Errors
///
/// Returns an error if files cannot be read or parsed.
pub fn collect_artifact_mappings(
    file_paths: &[impl AsRef<Path>],
) -> Result<Vec<ArtifactMapping>, crate::Error> {
    let mut mappings = Vec::new();

    for file_path in file_paths {
        let content = std::fs::read_to_string(file_path.as_ref())?;
        let mut xot = xot::Xot::new();
        let doc = xot.parse(&content)?;
        let root = xot.document_element(doc)?;

        let art_ns = xot.add_namespace(namespace::ARTIFACT);
        let names = MappingNames {
            mapping_tag: xot.add_name_ns("mapping", art_ns),
            spec_ref_tag: xot.add_name_ns("spec-ref", art_ns),
            artifact_tag: xot.add_name_ns("artifact", art_ns),
            range_tag: xot.add_name_ns("range", art_ns),
            coverage_tag: xot.add_name_ns("coverage", art_ns),
            id_attr: xot.add_name("id"),
            node_attr: xot.add_name("node"),
            revision_attr: xot.add_name("revision"),
            node_hash_attr: xot.add_name("node-hash"),
            path_attr: xot.add_name("path"),
            repo_attr: xot.add_name("repo"),
            hash_attr: xot.add_name("hash"),
            start_line_attr: xot.add_name("start-line"),
            end_line_attr: xot.add_name("end-line"),
            start_byte_attr: xot.add_name("start-byte"),
            end_byte_attr: xot.add_name("end-byte"),
        };

        let start_idx = mappings.len();
        collect_mappings(&xot, root, &names, &mut mappings);

        // Set source_file on newly added mappings
        for mapping in &mut mappings[start_idx..] {
            mapping.source_file = file_path.as_ref().to_path_buf();
        }
    }

    Ok(mappings)
}

/// Interned name IDs for artifact mapping XML elements and attributes.
struct MappingNames {
    mapping_tag: xot::NameId,
    spec_ref_tag: xot::NameId,
    artifact_tag: xot::NameId,
    range_tag: xot::NameId,
    coverage_tag: xot::NameId,
    id_attr: xot::NameId,
    node_attr: xot::NameId,
    revision_attr: xot::NameId,
    node_hash_attr: xot::NameId,
    path_attr: xot::NameId,
    repo_attr: xot::NameId,
    hash_attr: xot::NameId,
    start_line_attr: xot::NameId,
    end_line_attr: xot::NameId,
    start_byte_attr: xot::NameId,
    end_byte_attr: xot::NameId,
}

fn collect_mappings(
    xot: &xot::Xot,
    node: xot::Node,
    names: &MappingNames,
    mappings: &mut Vec<ArtifactMapping>,
) {
    if xot.is_element(node)
        && xot
            .element(node)
            .is_some_and(|e| e.name() == names.mapping_tag)
    {
        mappings.push(parse_single_mapping(xot, node, names));
    }

    for child in xot.children(node) {
        collect_mappings(xot, child, names, mappings);
    }
}

fn parse_single_mapping(xot: &xot::Xot, node: xot::Node, names: &MappingNames) -> ArtifactMapping {
    let id = xot
        .element(node)
        .and_then(|e| e.get_attribute(names.id_attr))
        .unwrap_or("")
        .to_string();
    let mut spec_ref_node = String::new();
    let mut spec_ref_revision = String::new();
    let mut node_hash = None;
    let mut artifact_path = String::new();
    let mut artifact_repo = String::new();
    let mut ranges = Vec::new();
    let mut coverage = String::new();

    for child in xot.children(node) {
        if !xot.is_element(child) {
            continue;
        }
        let child_name = xot.element(child).map(xot::Element::name);
        if child_name == Some(names.spec_ref_tag) {
            let child_elem = xot.element(child);
            spec_ref_node = child_elem
                .and_then(|e| e.get_attribute(names.node_attr))
                .unwrap_or("")
                .to_string();
            spec_ref_revision = child_elem
                .and_then(|e| e.get_attribute(names.revision_attr))
                .unwrap_or("")
                .to_string();
            node_hash = child_elem
                .and_then(|e| e.get_attribute(names.node_hash_attr))
                .map(String::from);
        } else if child_name == Some(names.artifact_tag) {
            let child_elem = xot.element(child);
            artifact_path = child_elem
                .and_then(|e| e.get_attribute(names.path_attr))
                .unwrap_or("")
                .to_string();
            artifact_repo = child_elem
                .and_then(|e| e.get_attribute(names.repo_attr))
                .unwrap_or("")
                .to_string();

            for range_child in xot.children(child) {
                if xot.is_element(range_child)
                    && xot
                        .element(range_child)
                        .is_some_and(|e| e.name() == names.range_tag)
                {
                    let range_elem = xot.element(range_child);
                    ranges.push(ArtifactRange {
                        hash: range_elem
                            .and_then(|e| e.get_attribute(names.hash_attr))
                            .map(String::from),
                        start_line: range_elem
                            .and_then(|e| e.get_attribute(names.start_line_attr))
                            .and_then(|s| s.parse().ok()),
                        end_line: range_elem
                            .and_then(|e| e.get_attribute(names.end_line_attr))
                            .and_then(|s| s.parse().ok()),
                        start_byte: range_elem
                            .and_then(|e| e.get_attribute(names.start_byte_attr))
                            .and_then(|s| s.parse().ok()),
                        end_byte: range_elem
                            .and_then(|e| e.get_attribute(names.end_byte_attr))
                            .and_then(|s| s.parse().ok()),
                    });
                }
            }
        } else if child_name == Some(names.coverage_tag) {
            coverage = collect_text_content(xot, child);
        }
    }

    ArtifactMapping {
        id,
        spec_ref_node,
        spec_ref_revision,
        node_hash,
        artifact_path,
        artifact_repo,
        ranges,
        coverage,
        source_file: PathBuf::new(),
    }
}

fn collect_text_content(xot: &xot::Xot, node: xot::Node) -> String {
    let mut text = String::new();
    for child in xot.children(node) {
        if let Some(t) = xot.text_str(child) {
            text.push_str(t);
        }
    }
    text.trim().to_string()
}

/// Compute SHA-256 hash of a file (whole-file addressing).
///
/// # Errors
///
/// Returns an error if the file cannot be read.
pub fn hash_file(path: &Path) -> Result<ContentHash, crate::Error> {
    let content = std::fs::read(path)?;
    Ok(ContentHash::from_canonical(&Sha256::digest(&content)))
}

/// Extract content from a file using line-range addressing.
///
/// Lines are 1-based. Returns the content between `start_line` and `end_line` inclusive.
///
/// # Errors
///
/// Returns an error if the file cannot be read.
pub fn extract_line_range(
    path: &Path,
    start_line: u64,
    end_line: u64,
) -> Result<String, crate::Error> {
    let content = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();
    #[allow(clippy::cast_possible_truncation)]
    let start = start_line.saturating_sub(1) as usize;
    #[allow(clippy::cast_possible_truncation)]
    let end = std::cmp::min(end_line as usize, lines.len());

    if start >= lines.len() {
        return Ok(String::new());
    }

    Ok(lines[start..end].join("\n"))
}

/// Compute hash of a line range within a file.
///
/// # Errors
///
/// Returns an error if the file cannot be read.
pub fn hash_line_range(
    path: &Path,
    start_line: u64,
    end_line: u64,
) -> Result<ContentHash, crate::Error> {
    let text = extract_line_range(path, start_line, end_line)?;
    Ok(ContentHash::from_canonical(text.as_bytes()))
}

/// Find the repository root by walking up from a path looking for `.git`.
#[must_use]
pub fn find_repo_root(start: &Path) -> Option<PathBuf> {
    let dir = start.canonicalize().ok()?;
    let mut dir = dir.as_path();
    loop {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

/// Resolve an artifact path relative to the repo root or spec directory.
#[must_use]
pub fn resolve_artifact_path(
    artifact_path: &str,
    spec_dir: &Path,
    repo_root: Option<&Path>,
) -> PathBuf {
    if let Some(root) = repo_root {
        let candidate = root.join(artifact_path);
        if candidate.exists() {
            return candidate;
        }
    }
    // Walk up from spec_dir
    let mut dir = spec_dir.to_path_buf();
    loop {
        let candidate = dir.join(artifact_path);
        if candidate.exists() {
            return candidate;
        }
        if !dir.pop() {
            break;
        }
    }
    spec_dir.join(artifact_path)
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
    fn collect_mappings_from_shipped_spec() {
        let file_paths = crate::discovery::discover_spec_files(&spec_dir().join("index.xml"))
            .expect("discovery failed");
        let mappings = collect_artifact_mappings(&file_paths).expect("collection failed");
        assert!(
            !mappings.is_empty(),
            "shipped spec should have artifact mappings"
        );
        // At least one mapping should have valid structure
        let valid = mappings
            .iter()
            .any(|m| !m.spec_ref_node.is_empty() && !m.artifact_path.is_empty());
        assert!(
            valid,
            "at least one mapping should have spec-ref and artifact"
        );
    }

    #[test]
    fn hash_file_produces_consistent_hash() {
        let path = spec_dir().join("index.xml");
        let h1 = hash_file(&path).expect("hash failed");
        let h2 = hash_file(&path).expect("hash failed");
        assert_eq!(h1, h2);
    }

    #[test]
    fn extract_line_range_correct() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "line1\nline2\nline3\nline4\nline5\n").expect("write");

        let range = extract_line_range(&file, 2, 4).expect("extract failed");
        assert_eq!(range, "line2\nline3\nline4");
    }

    #[test]
    fn hash_line_range_differs_from_whole_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "line1\nline2\nline3\nline4\nline5\n").expect("write");

        let whole = hash_file(&file).expect("hash failed");
        let partial = hash_line_range(&file, 2, 4).expect("hash failed");
        assert_ne!(whole, partial);
    }
}
