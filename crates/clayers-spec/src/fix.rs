use std::collections::HashMap;
use std::path::{Path, PathBuf};

use clayers_xml::c14n;

use crate::artifact;

/// Result of a single hash fix operation.
#[derive(Debug)]
pub struct FixResult {
    pub mapping_id: String,
    pub old_hash: String,
    pub new_hash: String,
}

/// Report from a fix operation.
#[derive(Debug)]
pub struct FixReport {
    pub spec_name: String,
    pub total_mappings: usize,
    pub fixed_count: usize,
    pub results: Vec<FixResult>,
}

/// Fix artifact-side hashes by recomputing from current file content.
///
/// For each artifact mapping, reads the referenced file (with line-range
/// addressing if specified), computes SHA-256, and updates the `hash`
/// attribute on `<art:range>` elements in-place.
///
/// # Errors
///
/// Returns an error if spec files cannot be read or artifact files are missing.
pub fn fix_artifact_hashes(spec_dir: &Path) -> Result<FixReport, crate::Error> {
    let spec_name = spec_dir
        .file_name()
        .map_or_else(|| "unknown".into(), |n| n.to_string_lossy().into_owned());

    let index_files = crate::discovery::find_index_files(spec_dir)?;
    let mut all_file_paths = Vec::new();
    for index_path in &index_files {
        let file_paths = crate::discovery::discover_spec_files(index_path)?;
        all_file_paths.extend(file_paths);
    }

    let repo_root = artifact::find_repo_root(spec_dir);
    let mappings = artifact::collect_artifact_mappings(&all_file_paths)?;
    let total_mappings = mappings.len();

    let mut results = Vec::new();
    let mut file_changes: HashMap<PathBuf, Vec<RangeChange>> = HashMap::new();

    for mapping in &mappings {
        for range in &mapping.ranges {
            let artifact_path = artifact::resolve_artifact_path(
                &mapping.artifact_path,
                spec_dir,
                repo_root.as_deref(),
            );

            if !artifact_path.exists() {
                continue;
            }

            let new_hash =
                if let (Some(start), Some(end)) = (range.start_line, range.end_line) {
                    artifact::hash_line_range(&artifact_path, start, end)?
                } else {
                    artifact::hash_file(&artifact_path)?
                };

            let new_hash_str = new_hash.to_prefixed();
            let old_hash_str = range.hash.clone().unwrap_or_default();

            if old_hash_str != new_hash_str {
                results.push(FixResult {
                    mapping_id: mapping.id.clone(),
                    old_hash: old_hash_str.clone(),
                    new_hash: new_hash_str.clone(),
                });

                file_changes
                    .entry(mapping.source_file.clone())
                    .or_default()
                    .push(RangeChange {
                        mapping_id: mapping.id.clone(),
                        old_hash: old_hash_str,
                        new_hash: new_hash_str,
                        start_line: range.start_line,
                        end_line: range.end_line,
                    });
            }
        }
    }

    for (file_path, changes) in &file_changes {
        apply_range_changes(file_path, changes)?;
    }

    let fixed_count = results.len();
    Ok(FixReport {
        spec_name,
        total_mappings,
        fixed_count,
        results,
    })
}

/// Fix node-side hashes by recomputing C14N hash from current spec content.
///
/// Assembles the combined document, finds each referenced spec node,
/// serializes it, applies inclusive C14N via bergshamra, and computes
/// SHA-256. Updates `node-hash` attributes on `<art:spec-ref>` in-place.
///
/// # Errors
///
/// Returns an error if spec files cannot be read or assembled.
pub fn fix_node_hashes(spec_dir: &Path) -> Result<FixReport, crate::Error> {
    let spec_name = spec_dir
        .file_name()
        .map_or_else(|| "unknown".into(), |n| n.to_string_lossy().into_owned());

    let index_files = crate::discovery::find_index_files(spec_dir)?;
    let mut all_file_paths = Vec::new();
    for index_path in &index_files {
        let file_paths = crate::discovery::discover_spec_files(index_path)?;
        all_file_paths.extend(file_paths);
    }

    let (mut xot, root) = crate::assembly::assemble_combined(&all_file_paths)?;
    let mappings = artifact::collect_artifact_mappings(&all_file_paths)?;
    let total_mappings = mappings.len();

    let id_attr = xot.add_name("id");
    let xml_ns = xot.add_namespace(crate::namespace::XML);
    let xml_id_attr = xot.add_name_ns("id", xml_ns);

    let mut results = Vec::new();
    let mut file_changes: HashMap<PathBuf, Vec<NodeHashChange>> = HashMap::new();

    for mapping in &mappings {
        let old_hash = match &mapping.node_hash {
            Some(h) => h.clone(),
            None => continue,
        };

        if mapping.spec_ref_node.is_empty() {
            continue;
        }

        let Some(node) =
            find_node_by_id(&xot, root, id_attr, xml_id_attr, &mapping.spec_ref_node)
        else {
            continue;
        };

        let xml_str = xot.to_string(node).unwrap_or_default();
        let Ok(new_hash) =
            c14n::canonicalize_and_hash(&xml_str, c14n::CanonicalizationMode::Inclusive)
        else {
            continue;
        };

        let new_hash_str = new_hash.to_prefixed();
        if old_hash != new_hash_str {
            results.push(FixResult {
                mapping_id: mapping.id.clone(),
                old_hash: old_hash.clone(),
                new_hash: new_hash_str.clone(),
            });

            file_changes
                .entry(mapping.source_file.clone())
                .or_default()
                .push(NodeHashChange {
                    mapping_id: mapping.id.clone(),
                    old_hash,
                    new_hash: new_hash_str,
                });
        }
    }

    for (file_path, changes) in &file_changes {
        apply_node_hash_changes(file_path, changes)?;
    }

    let fixed_count = results.len();
    Ok(FixReport {
        spec_name,
        total_mappings,
        fixed_count,
        results,
    })
}

// --- Internal types ---

struct RangeChange {
    mapping_id: String,
    old_hash: String,
    new_hash: String,
    start_line: Option<u64>,
    end_line: Option<u64>,
}

struct NodeHashChange {
    mapping_id: String,
    old_hash: String,
    new_hash: String,
}

// --- Helpers ---

pub(crate) fn find_node_by_id(
    xot: &xot::Xot,
    node: xot::Node,
    id_attr: xot::NameId,
    xml_id_attr: xot::NameId,
    target_id: &str,
) -> Option<xot::Node> {
    if xot.is_element(node) {
        // Check bare @id
        if xot.get_attribute(node, id_attr)
            .is_some_and(|id| id == target_id)
        {
            return Some(node);
        }
        // Check xml:id
        if xot.get_attribute(node, xml_id_attr)
            .is_some_and(|id| id == target_id)
        {
            return Some(node);
        }
    }
    for child in xot.children(node) {
        if let Some(found) = find_node_by_id(xot, child, id_attr, xml_id_attr, target_id) {
            return Some(found);
        }
    }
    None
}

fn apply_range_changes(file_path: &Path, changes: &[RangeChange]) -> Result<(), crate::Error> {
    let mut content = std::fs::read_to_string(file_path)?;

    for change in changes {
        content = replace_hash_in_mapping(
            &content,
            &change.mapping_id,
            "hash",
            &change.old_hash,
            &change.new_hash,
            change.start_line,
            change.end_line,
        );
    }

    std::fs::write(file_path, content)?;
    Ok(())
}

fn apply_node_hash_changes(
    file_path: &Path,
    changes: &[NodeHashChange],
) -> Result<(), crate::Error> {
    let mut content = std::fs::read_to_string(file_path)?;

    for change in changes {
        content = replace_hash_in_mapping(
            &content,
            &change.mapping_id,
            "node-hash",
            &change.old_hash,
            &change.new_hash,
            None,
            None,
        );
    }

    std::fs::write(file_path, content)?;
    Ok(())
}

/// Replace a hash attribute value within a specific mapping block.
///
/// Uses the mapping ID as an anchor to locate the correct mapping block,
/// then uses `start-line`/`end-line` attributes (if present) to anchor
/// the replacement to the correct `<art:range>` element.
fn replace_hash_in_mapping(
    content: &str,
    mapping_id: &str,
    attr_name: &str,
    old_hash: &str,
    new_hash: &str,
    start_line: Option<u64>,
    end_line: Option<u64>,
) -> String {
    let id_marker = format!("id=\"{mapping_id}\"");
    let Some(mapping_start) = content.find(&id_marker) else {
        return content.to_string();
    };
    let Some(rel_end) = content[mapping_start..].find("</art:mapping>") else {
        return content.to_string();
    };
    let mapping_end = mapping_start + rel_end + "</art:mapping>".len();

    let block = &content[mapping_start..mapping_end];
    // Use a space boundary to avoid matching "node-hash" when attr_name is "hash"
    let old_attr = format!(" {attr_name}=\"{old_hash}\"");
    let new_attr = format!(" {attr_name}=\"{new_hash}\"");

    if let (Some(sl), Some(_el)) = (start_line, end_line) {
        // Find the range tag anchored by start-line/end-line
        let anchor = format!("start-line=\"{sl}\"");
        if let Some(anchor_pos) = block.find(&anchor) {
            let tag_start = block[..anchor_pos]
                .rfind("<art:range")
                .unwrap_or(anchor_pos);
            let tag_end = block[anchor_pos..]
                .find("/>")
                .map_or(block.len(), |p| anchor_pos + p + 2);
            let tag = &block[tag_start..tag_end];

            if tag.contains(&old_attr) {
                let new_tag = tag.replace(&old_attr, &new_attr);
                let abs_start = mapping_start + tag_start;
                let abs_end = mapping_start + tag_end;
                return format!(
                    "{}{}{}",
                    &content[..abs_start],
                    new_tag,
                    &content[abs_end..]
                );
            }
        }
    } else if let Some(pos) = block.find(&old_attr) {
        // No line anchor: replace the first occurrence of the attribute in the block
        let abs_pos = mapping_start + pos;
        return format!(
            "{}{}{}",
            &content[..abs_pos],
            new_attr,
            &content[abs_pos + old_attr.len()..]
        );
    }

    content.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_hash_node_hash() {
        let content = r#"<art:mapping id="map-foo">
  <art:spec-ref node="foo" node-hash="sha256:oldoldold"/>
</art:mapping>"#;
        let result = replace_hash_in_mapping(
            content,
            "map-foo",
            "node-hash",
            "sha256:oldoldold",
            "sha256:newnewnew",
            None,
            None,
        );
        assert!(result.contains("node-hash=\"sha256:newnewnew\""));
        assert!(!result.contains("sha256:oldoldold"));
    }

    #[test]
    fn replace_hash_range_with_line_anchor() {
        let content = r#"<art:mapping id="map-bar">
  <art:spec-ref node="bar" node-hash="sha256:aaa"/>
  <art:artifact path="src/foo.rs">
    <art:range hash="sha256:old1" start-line="10" end-line="20"/>
    <art:range hash="sha256:old2" start-line="30" end-line="40"/>
  </art:artifact>
</art:mapping>"#;
        // Replace only the second range
        let result = replace_hash_in_mapping(
            content,
            "map-bar",
            "hash",
            "sha256:old2",
            "sha256:new2",
            Some(30),
            Some(40),
        );
        assert!(result.contains("hash=\"sha256:old1\""), "first range should be unchanged");
        assert!(result.contains("hash=\"sha256:new2\""), "second range should be updated");
    }

    #[test]
    fn replace_hash_whole_file_range() {
        let content = r#"<art:mapping id="map-baz">
  <art:spec-ref node="baz" node-hash="sha256:aaa"/>
  <art:artifact path="README.md">
    <art:range hash="sha256:old"/>
  </art:artifact>
</art:mapping>"#;
        let result = replace_hash_in_mapping(
            content,
            "map-baz",
            "hash",
            "sha256:old",
            "sha256:new",
            None,
            None,
        );
        assert!(result.contains("hash=\"sha256:new\""));
    }

    #[test]
    fn replace_does_not_affect_other_mappings() {
        let content = r#"<art:mapping id="map-a">
  <art:spec-ref node="a" node-hash="sha256:same"/>
</art:mapping>
<art:mapping id="map-b">
  <art:spec-ref node="b" node-hash="sha256:same"/>
</art:mapping>"#;
        let result = replace_hash_in_mapping(
            content,
            "map-a",
            "node-hash",
            "sha256:same",
            "sha256:changed",
            None,
            None,
        );
        // map-a should be changed, map-b should be untouched
        let pos_a = result.find("map-a").unwrap();
        let pos_b = result.find("map-b").unwrap();
        let between = &result[pos_a..pos_b];
        assert!(between.contains("sha256:changed"));
        let after_b = &result[pos_b..];
        assert!(after_b.contains("sha256:same"));
    }

    #[test]
    fn replace_hash_missing_mapping_returns_unchanged() {
        let content = r#"<art:mapping id="map-exists">
  <art:spec-ref node="x" node-hash="sha256:aaa"/>
</art:mapping>"#;
        let result = replace_hash_in_mapping(
            content,
            "map-nonexistent",
            "node-hash",
            "sha256:aaa",
            "sha256:bbb",
            None,
            None,
        );
        assert_eq!(result, content, "content should be unchanged for missing mapping");
    }

    #[test]
    fn replace_hash_missing_attribute_returns_unchanged() {
        let content = r#"<art:mapping id="map-foo">
  <art:spec-ref node="foo" node-hash="sha256:aaa"/>
</art:mapping>"#;
        // Try to replace a hash that doesn't exist in this mapping
        let result = replace_hash_in_mapping(
            content,
            "map-foo",
            "node-hash",
            "sha256:nonexistent",
            "sha256:bbb",
            None,
            None,
        );
        assert_eq!(result, content, "content should be unchanged when old_hash not found");
    }

    #[test]
    fn replace_hash_first_range_with_line_anchor() {
        let content = r#"<art:mapping id="map-bar">
  <art:spec-ref node="bar" node-hash="sha256:aaa"/>
  <art:artifact path="src/foo.rs">
    <art:range hash="sha256:old1" start-line="10" end-line="20"/>
    <art:range hash="sha256:old2" start-line="30" end-line="40"/>
  </art:artifact>
</art:mapping>"#;
        // Replace only the first range
        let result = replace_hash_in_mapping(
            content,
            "map-bar",
            "hash",
            "sha256:old1",
            "sha256:new1",
            Some(10),
            Some(20),
        );
        assert!(result.contains("hash=\"sha256:new1\""), "first range should be updated");
        assert!(result.contains("hash=\"sha256:old2\""), "second range should be unchanged");
    }

    #[test]
    fn replace_preserves_surrounding_content() {
        let content = r#"<?xml version="1.0"?>
<spec:clayers xmlns:art="urn:clayers:artifact">
  <art:mapping id="map-test">
    <art:spec-ref node="test" node-hash="sha256:old"/>
  </art:mapping>
  <!-- trailing content -->
</spec:clayers>"#;
        let result = replace_hash_in_mapping(
            content,
            "map-test",
            "node-hash",
            "sha256:old",
            "sha256:new",
            None,
            None,
        );
        assert!(result.starts_with("<?xml version=\"1.0\"?>"));
        assert!(result.contains("<!-- trailing content -->"));
        assert!(result.contains("</spec:clayers>"));
        assert!(result.contains("node-hash=\"sha256:new\""));
    }

    // --- Synthetic spec helpers ---

    fn create_synthetic_spec(dir: &Path, artifact_content: &str) {
        // Create index.xml
        let index = r#"<?xml version="1.0" encoding="UTF-8"?>
<idx:index xmlns:idx="urn:clayers:index">
  <idx:file href="content.xml"/>
</idx:index>"#;
        std::fs::write(dir.join("index.xml"), index).unwrap();

        // Create content.xml with an artifact mapping
        let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<spec:clayers xmlns:spec="urn:clayers:spec"
              xmlns:pr="urn:clayers:prose"
              xmlns:art="urn:clayers:artifact"
              spec:index="index.xml">
  <pr:section id="test-node">
    <pr:title>Test Node</pr:title>
    <pr:p>Some content here.</pr:p>
  </pr:section>
  <art:mapping id="map-test">
    <art:spec-ref node="test-node" node-hash="sha256:placeholder"/>
    <art:artifact repo="test" path="artifact.txt">
      <art:range hash="sha256:placeholder"/>
    </art:artifact>
  </art:mapping>
</spec:clayers>"#;
        std::fs::write(dir.join("content.xml"), content).unwrap();

        // Create the artifact file
        std::fs::write(dir.join("artifact.txt"), artifact_content).unwrap();
    }

    fn create_synthetic_spec_with_ranges(dir: &Path) {
        let index = r#"<?xml version="1.0" encoding="UTF-8"?>
<idx:index xmlns:idx="urn:clayers:index">
  <idx:file href="content.xml"/>
</idx:index>"#;
        std::fs::write(dir.join("index.xml"), index).unwrap();

        let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<spec:clayers xmlns:spec="urn:clayers:spec"
              xmlns:pr="urn:clayers:prose"
              xmlns:art="urn:clayers:artifact"
              spec:index="index.xml">
  <pr:section id="test-node">
    <pr:title>Test Node</pr:title>
    <pr:p>Content.</pr:p>
  </pr:section>
  <art:mapping id="map-ranges">
    <art:spec-ref node="test-node" node-hash="sha256:placeholder"/>
    <art:artifact repo="test" path="code.rs">
      <art:range hash="sha256:placeholder" start-line="2" end-line="4"/>
      <art:range hash="sha256:placeholder" start-line="6" end-line="8"/>
    </art:artifact>
  </art:mapping>
</spec:clayers>"#;
        std::fs::write(dir.join("content.xml"), content).unwrap();

        let code = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\n";
        std::fs::write(dir.join("code.rs"), code).unwrap();
    }

    #[test]
    fn fix_artifact_hashes_updates_whole_file_hash() {
        let tmp = tempfile::tempdir().expect("tempdir");
        create_synthetic_spec(tmp.path(), "hello world\n");

        let report = fix_artifact_hashes(tmp.path()).expect("fix failed");
        assert_eq!(report.total_mappings, 1);
        assert_eq!(report.fixed_count, 1);
        assert_eq!(report.results[0].mapping_id, "map-test");
        assert_eq!(report.results[0].old_hash, "sha256:placeholder");
        assert!(
            report.results[0].new_hash.starts_with("sha256:"),
            "new hash should be sha256 prefixed"
        );
        assert_ne!(report.results[0].new_hash, "sha256:placeholder");

        // Verify the artifact hash was updated on disk (node-hash is not touched)
        let xml = std::fs::read_to_string(tmp.path().join("content.xml")).unwrap();
        let new_hash = &report.results[0].new_hash;
        assert!(
            xml.contains(&format!("hash=\"{new_hash}\"")),
            "artifact range hash should be updated on disk"
        );
        // node-hash is untouched by fix_artifact_hashes
        assert!(
            xml.contains("node-hash=\"sha256:placeholder\""),
            "node-hash should remain unchanged"
        );
    }

    #[test]
    fn fix_artifact_hashes_idempotent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        create_synthetic_spec(tmp.path(), "hello world\n");

        let first = fix_artifact_hashes(tmp.path()).expect("first fix failed");
        assert_eq!(first.fixed_count, 1);

        // Second run should find nothing to fix
        let second = fix_artifact_hashes(tmp.path()).expect("second fix failed");
        assert_eq!(second.fixed_count, 0, "second run should be a no-op");
    }

    #[test]
    fn fix_artifact_hashes_with_line_ranges() {
        let tmp = tempfile::tempdir().expect("tempdir");
        create_synthetic_spec_with_ranges(tmp.path());

        let report = fix_artifact_hashes(tmp.path()).expect("fix failed");
        assert_eq!(report.total_mappings, 1);
        // Two ranges, both with placeholder hashes
        assert_eq!(report.fixed_count, 2);

        // The two ranges should have different hashes (different content)
        let hashes: Vec<&str> = report.results.iter().map(|r| r.new_hash.as_str()).collect();
        assert_ne!(hashes[0], hashes[1], "different line ranges should produce different hashes");

        // Verify range hashes updated on disk
        let xml = std::fs::read_to_string(tmp.path().join("content.xml")).unwrap();
        for h in &hashes {
            assert!(xml.contains(h), "range hash {h} should appear on disk");
        }
        // node-hash is untouched
        assert!(xml.contains("node-hash=\"sha256:placeholder\""));
    }

    #[test]
    fn fix_artifact_hashes_skips_missing_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        create_synthetic_spec(tmp.path(), "content");

        // Delete the artifact file
        std::fs::remove_file(tmp.path().join("artifact.txt")).unwrap();

        let report = fix_artifact_hashes(tmp.path()).expect("fix should succeed");
        assert_eq!(report.total_mappings, 1);
        assert_eq!(report.fixed_count, 0, "should skip missing artifact files");
    }

    #[test]
    fn fix_node_hashes_updates_hash() {
        let tmp = tempfile::tempdir().expect("tempdir");
        create_synthetic_spec(tmp.path(), "irrelevant");

        let report = fix_node_hashes(tmp.path()).expect("fix failed");
        assert_eq!(report.total_mappings, 1);
        assert_eq!(report.fixed_count, 1);
        assert_eq!(report.results[0].mapping_id, "map-test");
        assert!(report.results[0].new_hash.starts_with("sha256:"));
        assert_ne!(report.results[0].new_hash, "sha256:placeholder");

        // Verify the XML was updated on disk
        let xml = std::fs::read_to_string(tmp.path().join("content.xml")).unwrap();
        assert!(!xml.contains("node-hash=\"sha256:placeholder\""));
        assert!(xml.contains(&format!("node-hash=\"{}\"", report.results[0].new_hash)));
    }

    #[test]
    fn fix_node_hashes_idempotent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        create_synthetic_spec(tmp.path(), "irrelevant");

        let first = fix_node_hashes(tmp.path()).expect("first fix failed");
        assert_eq!(first.fixed_count, 1);

        let second = fix_node_hashes(tmp.path()).expect("second fix failed");
        assert_eq!(second.fixed_count, 0, "second run should be a no-op");
    }

    #[test]
    fn fix_node_hashes_deterministic() {
        // Create two identical specs and verify they produce the same node hash
        let tmp1 = tempfile::tempdir().expect("tempdir1");
        let tmp2 = tempfile::tempdir().expect("tempdir2");
        create_synthetic_spec(tmp1.path(), "a");
        create_synthetic_spec(tmp2.path(), "b"); // different artifact, same spec content

        let r1 = fix_node_hashes(tmp1.path()).expect("fix1 failed");
        let r2 = fix_node_hashes(tmp2.path()).expect("fix2 failed");

        assert_eq!(
            r1.results[0].new_hash, r2.results[0].new_hash,
            "same spec node content should produce same node hash regardless of artifact content"
        );
    }

    #[test]
    fn fix_artifact_hashes_different_content_different_hash() {
        let tmp1 = tempfile::tempdir().expect("tempdir1");
        let tmp2 = tempfile::tempdir().expect("tempdir2");
        create_synthetic_spec(tmp1.path(), "content A\n");
        create_synthetic_spec(tmp2.path(), "content B\n");

        let r1 = fix_artifact_hashes(tmp1.path()).expect("fix1 failed");
        let r2 = fix_artifact_hashes(tmp2.path()).expect("fix2 failed");

        assert_ne!(
            r1.results[0].new_hash, r2.results[0].new_hash,
            "different file content should produce different hashes"
        );
    }

    #[test]
    fn fix_both_hashes_on_synthetic_spec() {
        let tmp = tempfile::tempdir().expect("tempdir");
        create_synthetic_spec(tmp.path(), "test content\n");

        // Fix node hashes first, then artifact hashes
        let node_report = fix_node_hashes(tmp.path()).expect("fix_node failed");
        assert_eq!(node_report.fixed_count, 1);

        let art_report = fix_artifact_hashes(tmp.path()).expect("fix_artifact failed");
        assert_eq!(art_report.fixed_count, 1);

        // Verify both hashes are now real (not placeholder)
        let xml = std::fs::read_to_string(tmp.path().join("content.xml")).unwrap();
        assert!(!xml.contains("sha256:placeholder"));

        // Both should be idempotent now
        let node2 = fix_node_hashes(tmp.path()).expect("fix_node2 failed");
        let art2 = fix_artifact_hashes(tmp.path()).expect("fix_artifact2 failed");
        assert_eq!(node2.fixed_count, 0);
        assert_eq!(art2.fixed_count, 0);
    }

    #[test]
    fn fix_artifact_hashes_on_shipped_spec() {
        let spec_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../clayers/clayers")
            .canonicalize()
            .expect("clayers/clayers/ not found");

        let tmp = tempfile::tempdir().expect("tempdir");
        copy_dir_all(&spec_dir, tmp.path()).expect("copy");

        let report = fix_artifact_hashes(tmp.path()).expect("fix_artifact_hashes failed");
        assert!(
            report.total_mappings > 0,
            "should find mappings in copied spec"
        );
    }

    #[test]
    fn fix_node_hashes_on_shipped_spec() {
        let spec_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../clayers/clayers")
            .canonicalize()
            .expect("clayers/clayers/ not found");

        let tmp = tempfile::tempdir().expect("tempdir");
        copy_dir_all(&spec_dir, tmp.path()).expect("copy");

        let report = fix_node_hashes(tmp.path()).expect("fix_node_hashes failed");
        assert!(
            report.total_mappings > 0,
            "should find mappings in copied spec"
        );
    }

    #[test]
    fn fix_node_hashes_shipped_spec_idempotent() {
        let spec_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../clayers/clayers")
            .canonicalize()
            .expect("clayers/clayers/ not found");

        let tmp = tempfile::tempdir().expect("tempdir");
        copy_dir_all(&spec_dir, tmp.path()).expect("copy");

        // First run computes hashes (may or may not change depending on
        // whether the shipped spec has correct hashes already)
        let first = fix_node_hashes(tmp.path()).expect("first fix failed");
        assert!(first.total_mappings > 0, "should find mappings");

        // Second run must be a no-op regardless
        let second = fix_node_hashes(tmp.path()).expect("second fix failed");
        assert_eq!(
            second.fixed_count, 0,
            "second run should find nothing to fix (idempotent)"
        );
    }

    fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            let dest_path = dst.join(entry.file_name());
            if ty.is_dir() {
                copy_dir_all(&entry.path(), &dest_path)?;
            } else {
                std::fs::copy(entry.path(), &dest_path)?;
            }
        }
        Ok(())
    }
}
