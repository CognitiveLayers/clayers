use std::collections::HashMap;
use std::path::Path;

use clayers_xml::c14n;

use crate::artifact::{self, ArtifactMapping};

/// Result of drift detection for a single artifact mapping.
#[derive(Debug)]
pub enum DriftStatus {
    /// Both spec and artifact hashes match stored values.
    Clean,
    /// The spec node's content has changed.
    SpecDrifted {
        stored_hash: String,
        current_hash: String,
    },
    /// The artifact file's content has changed.
    ArtifactDrifted {
        stored_hash: String,
        current_hash: String,
        artifact_path: String,
    },
    /// Cannot check drift (missing file, missing hash, etc.).
    Unavailable { reason: String },
}

/// Result of drift detection for a single mapping.
#[derive(Debug)]
pub struct MappingDrift {
    pub mapping_id: String,
    pub status: DriftStatus,
}

/// Overall drift report for a spec.
#[derive(Debug)]
pub struct DriftReport {
    pub spec_name: String,
    pub total_mappings: usize,
    pub drifted_count: usize,
    pub mapping_drifts: Vec<MappingDrift>,
}

/// Check for drift across all artifact mappings in a spec.
///
/// Compares stored hashes against current content for both spec nodes
/// and artifact files. Reports which mappings have drifted.
///
/// # Errors
///
/// Returns an error if spec files cannot be read.
pub fn check_drift(spec_dir: &Path, repo_root: Option<&Path>) -> Result<DriftReport, crate::Error> {
    let index_files = crate::discovery::find_index_files(spec_dir)?;
    let spec_name = spec_dir
        .file_name()
        .map_or_else(|| "unknown".into(), |n| n.to_string_lossy().into_owned());

    let mut all_mappings = Vec::new();
    let mut all_file_paths = Vec::new();

    for index_path in &index_files {
        let file_paths = crate::discovery::discover_spec_files(index_path)?;
        let mappings = artifact::collect_artifact_mappings(&file_paths)?;
        all_mappings.extend(mappings);
        all_file_paths.extend(file_paths);
    }

    // Compute current C14N hashes for each mapped spec node by assembling
    // the combined document once and serializing each referenced node.
    let current_node_hashes = collect_current_node_hashes(&all_file_paths, &all_mappings);

    let mut mapping_drifts = Vec::new();
    let mut drifted_count = 0;

    for mapping in &all_mappings {
        let drift = check_single_mapping(mapping, &current_node_hashes, repo_root, spec_dir);
        if matches!(
            drift.status,
            DriftStatus::SpecDrifted { .. } | DriftStatus::ArtifactDrifted { .. }
        ) {
            drifted_count += 1;
        }
        mapping_drifts.push(drift);
    }

    Ok(DriftReport {
        spec_name,
        total_mappings: all_mappings.len(),
        drifted_count,
        mapping_drifts,
    })
}

fn check_single_mapping(
    mapping: &ArtifactMapping,
    current_node_hashes: &HashMap<String, String>,
    repo_root: Option<&Path>,
    spec_dir: &Path,
) -> MappingDrift {
    let id = mapping.id.clone();

    // Check spec-side node hash. Placeholders and missing hashes are skipped
    // so freshly-authored mappings without `--fix-node-hash` don't false-positive.
    if let Some(stored_hash) = &mapping.node_hash
        && stored_hash.starts_with("sha256:")
        && stored_hash != "sha256:placeholder"
        && let Some(current_hash) = current_node_hashes.get(&mapping.spec_ref_node)
        && current_hash != stored_hash
    {
        return MappingDrift {
            mapping_id: id,
            status: DriftStatus::SpecDrifted {
                stored_hash: stored_hash.clone(),
                current_hash: current_hash.clone(),
            },
        };
    }

    // Check artifact hash
    for range in &mapping.ranges {
        if let Some(ref stored_hash) = range.hash {
            if !stored_hash.starts_with("sha256:") || stored_hash == "sha256:placeholder" {
                continue;
            }

            let artifact_path =
                artifact::resolve_artifact_path(&mapping.artifact_path, spec_dir, repo_root);

            if !artifact_path.exists() {
                return MappingDrift {
                    mapping_id: id,
                    status: DriftStatus::Unavailable {
                        reason: format!("artifact file not found: {}", mapping.artifact_path),
                    },
                };
            }

            let current_hash_result =
                if let (Some(start), Some(end)) = (range.start_line, range.end_line) {
                    artifact::hash_line_range(&artifact_path, start, end)
                } else {
                    artifact::hash_file(&artifact_path)
                };

            match current_hash_result {
                Ok(current_hash) => {
                    let current_str = current_hash.to_prefixed();
                    if &current_str != stored_hash {
                        return MappingDrift {
                            mapping_id: id,
                            status: DriftStatus::ArtifactDrifted {
                                stored_hash: stored_hash.clone(),
                                current_hash: current_str,
                                artifact_path: mapping.artifact_path.clone(),
                            },
                        };
                    }
                }
                Err(e) => {
                    return MappingDrift {
                        mapping_id: id,
                        status: DriftStatus::Unavailable {
                            reason: format!("hash computation failed: {e}"),
                        },
                    };
                }
            }
        }
    }

    MappingDrift {
        mapping_id: id,
        status: DriftStatus::Clean,
    }
}

/// Compute current C14N hashes for every spec node referenced by a mapping.
///
/// Builds the combined document once, then for each unique `spec_ref_node`
/// looks up the node, serializes it, and applies inclusive C14N + SHA-256.
/// Returns a map `node_id -> "sha256:<hex>"` for nodes that were found and
/// hashed successfully. Nodes that don't exist or fail to hash are simply
/// absent from the map; callers must handle that case as "no drift signal".
fn collect_current_node_hashes(
    file_paths: &[std::path::PathBuf],
    mappings: &[ArtifactMapping],
) -> HashMap<String, String> {
    let mut hashes = HashMap::new();
    let Ok((mut xot, root)) = crate::assembly::assemble_combined(file_paths) else {
        return hashes;
    };
    let id_attr = xot.add_name("id");
    let xml_ns = xot.add_namespace(crate::namespace::XML);
    let xml_id_attr = xot.add_name_ns("id", xml_ns);

    for mapping in mappings {
        if mapping.spec_ref_node.is_empty() || hashes.contains_key(&mapping.spec_ref_node) {
            continue;
        }
        let Some(node) =
            crate::fix::find_node_by_id(&xot, root, id_attr, xml_id_attr, &mapping.spec_ref_node)
        else {
            continue;
        };
        let xml_str = xot.to_string(node).unwrap_or_default();
        let Ok(hash) = c14n::canonicalize_and_hash(&xml_str, c14n::CanonicalizationMode::Inclusive)
        else {
            continue;
        };
        hashes.insert(mapping.spec_ref_node.clone(), hash.to_prefixed());
    }

    hashes
}

/// Compare two hashes and return whether they match.
#[must_use]
pub fn hashes_match(stored: &str, current: &str) -> bool {
    stored == current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_hashes_no_drift() {
        assert!(hashes_match(
            "sha256:abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
            "sha256:abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
        ));
    }

    #[test]
    fn different_hashes_drift_detected() {
        assert!(!hashes_match(
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        ));
    }

    #[test]
    fn drift_report_on_shipped_spec() {
        let spec_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../clayers/clayers")
            .canonicalize()
            .expect("clayers/clayers/ not found");
        let report = check_drift(&spec_dir, None).expect("drift check failed");
        // Should report some mappings (even if they're all drifted or unavailable)
        assert!(
            report.total_mappings > 0,
            "shipped spec should have artifact mappings"
        );
    }

    /// When a mapped spec node's content changes after `fix_node_hashes` has
    /// recorded its C14N hash, `check_drift` must report that mapping as
    /// `SpecDrifted`. Without this, the documented spec-side drift workflow
    /// is silently broken.
    #[test]
    fn spec_node_edit_is_reported_as_spec_drifted() {
        let dir = tempfile::tempdir().expect("tempdir");

        let index_xml = r#"<?xml version="1.0"?>
<spec:clayers xmlns:spec="urn:clayers:spec"
              xmlns:idx="urn:clayers:index"
              spec:spec="drift-test"
              spec:version="0.1.0">
  <idx:file href="content.xml"/>
  <idx:file href="revision.xml"/>
</spec:clayers>"#;
        std::fs::write(dir.path().join("index.xml"), index_xml).expect("write index");

        let revision_xml = r#"<?xml version="1.0"?>
<spec:clayers xmlns:spec="urn:clayers:spec"
              xmlns:rev="urn:clayers:revision"
              spec:index="index.xml">
  <rev:revision name="draft-1"/>
</spec:clayers>"#;
        std::fs::write(dir.path().join("revision.xml"), revision_xml).expect("write revision");

        let content_xml = r#"<?xml version="1.0"?>
<spec:clayers xmlns:spec="urn:clayers:spec"
              xmlns:pr="urn:clayers:prose"
              xmlns:art="urn:clayers:artifact"
              xmlns:vcs="urn:clayers:vcs"
              spec:index="index.xml">
  <vcs:git id="repo-test" remote="https://example.com/test.git" default-branch="main"/>
  <pr:section id="sec-tracked">
    <pr:title>Tracked Section</pr:title>
    <pr:p>Original content.</pr:p>
  </pr:section>
  <art:mapping id="map-tracked">
    <art:spec-ref node="sec-tracked"
                  revision="draft-1"
                  node-hash="sha256:placeholder"/>
    <art:artifact repo="repo-test" repo-revision="HEAD" path="README.md"/>
    <art:coverage>full</art:coverage>
  </art:mapping>
</spec:clayers>"#;
        std::fs::write(dir.path().join("content.xml"), content_xml).expect("write content");

        // First, compute and record the correct node hash via the fixer.
        let fix_report = crate::fix::fix_node_hashes(dir.path()).expect("fix failed");
        assert!(
            fix_report.fixed_count >= 1,
            "fixer should record at least one node hash, got {}",
            fix_report.fixed_count
        );

        // Drift check immediately after fixing should be clean.
        let clean = check_drift(dir.path(), None).expect("clean drift check failed");
        let map_status = clean
            .mapping_drifts
            .iter()
            .find(|m| m.mapping_id == "map-tracked")
            .expect("map-tracked missing from clean report");
        assert!(
            matches!(map_status.status, DriftStatus::Clean),
            "expected Clean before edit, got {:?}",
            map_status.status
        );

        // Edit the prose paragraph inside the tracked section. Read the file
        // back from disk first because `fix_node_hashes` rewrote it with the
        // computed hash; mutating the original string would also wipe the hash.
        let on_disk = std::fs::read_to_string(dir.path().join("content.xml")).expect("read");
        let edited = on_disk.replace("Original content.", "Edited content.");
        assert_ne!(on_disk, edited, "edit should change file content");
        std::fs::write(dir.path().join("content.xml"), edited).expect("rewrite content");

        // Drift check should now report SpecDrifted for map-tracked.
        let report = check_drift(dir.path(), None).expect("drift check failed");
        let drifted = report
            .mapping_drifts
            .iter()
            .find(|m| m.mapping_id == "map-tracked")
            .expect("map-tracked missing from report");

        match &drifted.status {
            DriftStatus::SpecDrifted { stored_hash, current_hash } => {
                assert_ne!(stored_hash, current_hash,
                    "stored and current hashes should differ after edit");
            }
            other => panic!("expected SpecDrifted after edit, got {other:?}"),
        }

        assert_eq!(
            report.drifted_count, 1,
            "drifted_count should reflect the spec-side drift"
        );
    }
}
