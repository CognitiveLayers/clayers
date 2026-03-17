use std::collections::HashMap;
use std::path::Path;

use crate::artifact;

/// Coverage strength classification based on line count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageStrength {
    Precise,
    Moderate,
    Broad,
}

impl std::fmt::Display for CoverageStrength {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Precise => write!(f, "precise"),
            Self::Moderate => write!(f, "moderate"),
            Self::Broad => write!(f, "broad"),
        }
    }
}

/// Classify coverage strength by line count.
#[must_use]
pub fn classify_strength(line_count: usize) -> CoverageStrength {
    if line_count <= 30 {
        CoverageStrength::Precise
    } else if line_count <= 100 {
        CoverageStrength::Moderate
    } else {
        CoverageStrength::Broad
    }
}

/// Spec node coverage status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecCoverage {
    /// Node has a direct artifact mapping.
    Direct,
    /// Node inherits coverage from a parent/related node.
    Inherited,
    /// Node is explicitly exempt from coverage.
    Exempt,
    /// Node has no artifact mapping.
    Unmapped,
}

/// Coverage report for a spec.
#[derive(Debug)]
pub struct CoverageReport {
    pub spec_name: String,
    pub total_nodes: usize,
    pub mapped_nodes: usize,
    pub exempt_nodes: usize,
    pub unmapped_nodes: Vec<String>,
    pub artifact_coverages: Vec<ArtifactCoverage>,
    /// Per-file code coverage (code→spec direction).
    pub file_coverages: Vec<FileCoverage>,
}

/// Per-file code coverage analysis.
#[derive(Debug)]
pub struct FileCoverage {
    pub file_path: String,
    pub total_lines: usize,
    pub covered_lines: usize,
    pub coverage_percent: f64,
    pub covered_ranges: Vec<CoveredRange>,
    pub uncovered_ranges: Vec<UncoveredRange>,
}

/// A range of lines covered by an artifact mapping.
#[derive(Debug)]
pub struct CoveredRange {
    pub start_line: u64,
    pub end_line: u64,
    pub mapping_ids: Vec<String>,
}

/// A contiguous range of uncovered non-whitespace lines.
#[derive(Debug)]
pub struct UncoveredRange {
    pub start_line: usize,
    pub end_line: usize,
    pub line_count: usize,
}

/// Coverage info for a single artifact mapping.
#[derive(Debug)]
pub struct ArtifactCoverage {
    pub mapping_id: String,
    pub artifact_path: String,
    pub strength: CoverageStrength,
    pub line_count: usize,
}

/// Analyze spec and code coverage.
///
/// # Errors
///
/// Returns an error if spec files cannot be read.
pub fn analyze_coverage(
    spec_dir: &Path,
    code_path_filter: Option<&str>,
) -> Result<CoverageReport, crate::Error> {
    let index_files = crate::discovery::find_index_files(spec_dir)?;
    let spec_name = spec_dir
        .file_name()
        .map_or_else(|| "unknown".into(), |n| n.to_string_lossy().into_owned());

    let mut all_node_ids = std::collections::HashSet::new();
    let mut mapped_node_ids = std::collections::HashSet::new();
    let mut exempt_node_ids = std::collections::HashSet::new();
    let mut artifact_coverages = Vec::new();
    let mut artifact_coverages_raw = Vec::new();

    for index_path in &index_files {
        let file_paths = crate::discovery::discover_spec_files(index_path)?;

        // Collect all node IDs and exempt declarations
        for file_path in &file_paths {
            let content = std::fs::read_to_string(file_path)?;
            let mut xot = xot::Xot::new();
            let doc = xot.parse(&content)?;
            let root = xot.document_element(doc)?;
            let id_attr = xot.add_name("id");
            let xml_ns = xot.add_namespace(crate::namespace::XML);
            let xml_id_attr = xot.add_name_ns("id", xml_ns);
            let art_ns = xot.add_namespace(crate::namespace::ARTIFACT);
            collect_node_ids(&xot, root, id_attr, xml_id_attr, art_ns, &mut all_node_ids);

            let exempt_tag = xot.add_name_ns("exempt", art_ns);
            let node_attr = xot.add_name("node");
            collect_exempt_nodes(&xot, root, exempt_tag, node_attr, &mut exempt_node_ids);
        }

        // Collect artifact mappings
        let mappings = artifact::collect_artifact_mappings(&file_paths)?;
        for mapping in &mappings {
            if !mapping.spec_ref_node.is_empty() {
                mapped_node_ids.insert(mapping.spec_ref_node.clone());
            }

            #[allow(clippy::cast_possible_truncation)]
            let line_count: usize = mapping
                .ranges
                .iter()
                .map(|r| match (r.start_line, r.end_line) {
                    (Some(s), Some(e)) => (e.saturating_sub(s) + 1) as usize,
                    _ => 0,
                })
                .sum();

            artifact_coverages.push(ArtifactCoverage {
                mapping_id: mapping.id.clone(),
                artifact_path: mapping.artifact_path.clone(),
                strength: classify_strength(line_count),
                line_count,
            });
        }
        artifact_coverages_raw.extend(mappings);
    }

    // Exempt nodes count as covered (not unmapped)
    let covered: std::collections::HashSet<_> =
        mapped_node_ids.union(&exempt_node_ids).cloned().collect();
    let unmapped: Vec<String> = all_node_ids.difference(&covered).cloned().collect();

    // Code→spec direction: per-file line coverage
    let repo_root = artifact::find_repo_root(spec_dir);
    let file_coverages = compute_file_coverages(
        &artifact_coverages_raw,
        spec_dir,
        repo_root.as_deref(),
        code_path_filter,
    );

    // Count exempt nodes that are not also mapped (avoid double-counting)
    let exempt_only: usize = exempt_node_ids.difference(&mapped_node_ids).count();

    Ok(CoverageReport {
        spec_name,
        total_nodes: all_node_ids.len(),
        mapped_nodes: mapped_node_ids.len(),
        exempt_nodes: exempt_only,
        unmapped_nodes: unmapped,
        artifact_coverages,
        file_coverages,
    })
}

fn collect_node_ids(
    xot: &xot::Xot,
    node: xot::Node,
    id_attr: xot::NameId,
    xml_id_attr: xot::NameId,
    art_ns: xot::NamespaceId,
    ids: &mut std::collections::HashSet<String>,
) {
    if xot.is_element(node) {
        // Skip artifact-namespace elements (mapping, exempt, spec-ref, etc.)
        // — they are traceability infrastructure, not content nodes.
        let is_artifact = xot
            .element(node)
            .is_some_and(|e| xot.namespace_for_name(e.name()) == art_ns);
        if !is_artifact {
            if let Some(id) = xot.element(node).and_then(|e| e.get_attribute(id_attr)) {
                ids.insert(id.to_string());
            }
            if let Some(xml_id) = xot.element(node).and_then(|e| e.get_attribute(xml_id_attr)) {
                ids.insert(xml_id.to_string());
            }
        }
    }
    for child in xot.children(node) {
        collect_node_ids(xot, child, id_attr, xml_id_attr, art_ns, ids);
    }
}

fn collect_exempt_nodes(
    xot: &xot::Xot,
    node: xot::Node,
    exempt_tag: xot::NameId,
    node_attr: xot::NameId,
    exempt_ids: &mut std::collections::HashSet<String>,
) {
    if xot.is_element(node)
        && xot
            .element(node)
            .is_some_and(|e| e.name() == exempt_tag)
        && let Some(ref_node) = xot.element(node).and_then(|e| e.get_attribute(node_attr))
    {
        exempt_ids.insert(ref_node.to_string());
    }
    for child in xot.children(node) {
        collect_exempt_nodes(xot, child, exempt_tag, node_attr, exempt_ids);
    }
}

/// Filter out whitespace-only lines from a range and return the non-whitespace line count.
#[must_use]
pub fn count_non_whitespace_lines(text: &str) -> usize {
    text.lines().filter(|l| !l.trim().is_empty()).count()
}

/// Intermediate structure for building per-file coverage maps.
struct FileRangeEntry {
    start_line: u64,
    end_line: u64,
    mapping_id: String,
}

/// Build coverage maps from artifact mappings: ranged and whole-file.
fn build_coverage_maps(
    mappings: &[artifact::ArtifactMapping],
    code_path_filter: Option<&str>,
) -> (
    HashMap<String, Vec<FileRangeEntry>>,
    HashMap<String, Vec<String>>,
) {
    let mut file_map: HashMap<String, Vec<FileRangeEntry>> = HashMap::new();
    let mut whole_file_mappings: HashMap<String, Vec<String>> = HashMap::new();

    for mapping in mappings {
        if mapping.artifact_path.is_empty() {
            continue;
        }
        if let Some(filter) = code_path_filter
            && !mapping.artifact_path.contains(filter)
        {
            continue;
        }

        let has_line_ranges = mapping
            .ranges
            .iter()
            .any(|r| r.start_line.is_some() && r.end_line.is_some());

        if has_line_ranges {
            for range in &mapping.ranges {
                if let (Some(start), Some(end)) = (range.start_line, range.end_line) {
                    file_map
                        .entry(mapping.artifact_path.clone())
                        .or_default()
                        .push(FileRangeEntry {
                            start_line: start,
                            end_line: end,
                            mapping_id: mapping.id.clone(),
                        });
                }
            }
        } else {
            whole_file_mappings
                .entry(mapping.artifact_path.clone())
                .or_default()
                .push(mapping.id.clone());
        }
    }

    (file_map, whole_file_mappings)
}

/// Compute line-level coverage for a single file from its range entries.
fn compute_single_file_coverage(
    artifact_path: &str,
    lines: &[&str],
    ranges: Option<&[FileRangeEntry]>,
) -> FileCoverage {
    let total_lines = lines.len();
    let mut covered = vec![false; total_lines];
    let mut covered_range_list: Vec<CoveredRange> = Vec::new();

    if let Some(ranges) = ranges {
        for entry in ranges {
            #[allow(clippy::cast_possible_truncation)]
            let start = (entry.start_line as usize).saturating_sub(1);
            #[allow(clippy::cast_possible_truncation)]
            let end = std::cmp::min(entry.end_line as usize, total_lines);
            for c in &mut covered[start..end] {
                *c = true;
            }
            covered_range_list.push(CoveredRange {
                start_line: entry.start_line,
                end_line: entry.end_line,
                mapping_ids: vec![entry.mapping_id.clone()],
            });
        }
    }

    covered_range_list.sort_by_key(|r| (r.start_line, r.end_line));

    let covered_count = covered
        .iter()
        .enumerate()
        .filter(|(i, is_covered)| **is_covered && !lines[*i].trim().is_empty())
        .count();

    let non_ws_total = lines.iter().filter(|l| !l.trim().is_empty()).count();

    #[allow(clippy::cast_precision_loss)]
    let coverage_percent = if non_ws_total > 0 {
        (covered_count as f64 / non_ws_total as f64) * 100.0
    } else {
        100.0
    };

    let uncovered_ranges = find_uncovered_ranges(&covered, lines);

    FileCoverage {
        file_path: artifact_path.to_string(),
        total_lines,
        covered_lines: covered_count,
        coverage_percent,
        covered_ranges: covered_range_list,
        uncovered_ranges,
    }
}

/// Compute per-file code coverage from artifact mappings.
fn compute_file_coverages(
    mappings: &[artifact::ArtifactMapping],
    spec_dir: &Path,
    repo_root: Option<&Path>,
    code_path_filter: Option<&str>,
) -> Vec<FileCoverage> {
    let (file_map, whole_file_mappings) = build_coverage_maps(mappings, code_path_filter);

    let all_paths: std::collections::HashSet<&String> = file_map
        .keys()
        .chain(whole_file_mappings.keys())
        .collect();

    let mut coverages: Vec<FileCoverage> = Vec::new();

    for artifact_path in all_paths {
        let resolved = artifact::resolve_artifact_path(artifact_path, spec_dir, repo_root);
        let Ok(content) = std::fs::read_to_string(&resolved) else {
            continue;
        };
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            continue;
        }

        // Whole-file mapping with no ranged entries: 100% covered
        if let Some(wf_ids) = whole_file_mappings.get(artifact_path.as_str())
            && !file_map.contains_key(artifact_path.as_str())
        {
            coverages.push(FileCoverage {
                file_path: artifact_path.clone(),
                total_lines: lines.len(),
                covered_lines: lines.len(),
                coverage_percent: 100.0,
                covered_ranges: vec![CoveredRange {
                    start_line: 1,
                    end_line: lines.len() as u64,
                    mapping_ids: wf_ids.clone(),
                }],
                uncovered_ranges: Vec::new(),
            });
            continue;
        }

        coverages.push(compute_single_file_coverage(
            artifact_path,
            &lines,
            file_map.get(artifact_path.as_str()).map(Vec::as_slice),
        ));
    }

    coverages.sort_by(|a, b| a.file_path.cmp(&b.file_path));
    coverages
}

/// Find contiguous ranges of uncovered non-whitespace lines.
fn find_uncovered_ranges(covered: &[bool], lines: &[&str]) -> Vec<UncoveredRange> {
    let mut ranges = Vec::new();
    let mut range_start: Option<usize> = None;

    for (i, is_covered) in covered.iter().enumerate() {
        let is_whitespace = lines[i].trim().is_empty();

        if !is_covered && !is_whitespace {
            if range_start.is_none() {
                range_start = Some(i + 1); // 1-indexed
            }
        } else if let Some(start) = range_start {
            // End of an uncovered range (we hit a covered or whitespace line)
            let end = i; // last uncovered was i-1, so end = i (exclusive), 1-indexed end = i
            let count = end + 1 - start;
            ranges.push(UncoveredRange {
                start_line: start,
                end_line: end, // 1-indexed inclusive
                line_count: count,
            });
            range_start = None;
        }
    }

    // Close final range
    if let Some(start) = range_start {
        let end = covered.len(); // 1-indexed
        let count = end + 1 - start;
        ranges.push(UncoveredRange {
            start_line: start,
            end_line: end,
            line_count: count,
        });
    }

    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strength_precise() {
        assert_eq!(classify_strength(1), CoverageStrength::Precise);
        assert_eq!(classify_strength(30), CoverageStrength::Precise);
    }

    #[test]
    fn strength_moderate() {
        assert_eq!(classify_strength(31), CoverageStrength::Moderate);
        assert_eq!(classify_strength(100), CoverageStrength::Moderate);
    }

    #[test]
    fn strength_broad() {
        assert_eq!(classify_strength(101), CoverageStrength::Broad);
        assert_eq!(classify_strength(1000), CoverageStrength::Broad);
    }

    #[test]
    fn whitespace_lines_excluded() {
        let text = "line1\n  \nline3\n\nline5\n";
        assert_eq!(count_non_whitespace_lines(text), 3);
    }

    #[test]
    fn empty_text_zero_lines() {
        assert_eq!(count_non_whitespace_lines(""), 0);
        assert_eq!(count_non_whitespace_lines("  \n  \n"), 0);
    }

    #[test]
    fn find_uncovered_ranges_basic() {
        // 5 lines, lines 2-3 covered, rest uncovered (0-indexed)
        let covered = vec![false, true, true, false, false];
        let lines = vec!["fn a() {", "  let x = 1;", "  let y = 2;", "  z()", "}"];
        let ranges = find_uncovered_ranges(&covered, &lines);
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0].start_line, 1);
        assert_eq!(ranges[0].end_line, 1);
        assert_eq!(ranges[1].start_line, 4);
        assert_eq!(ranges[1].end_line, 5);
    }

    #[test]
    fn find_uncovered_ranges_skips_whitespace() {
        // Whitespace-only uncovered lines don't start new ranges
        let covered = vec![true, false, false, true];
        let lines = vec!["code", "", "  ", "code"];
        let ranges = find_uncovered_ranges(&covered, &lines);
        // Both uncovered lines are whitespace, so no uncovered ranges
        assert!(ranges.is_empty());
    }

    #[test]
    fn find_uncovered_ranges_all_covered() {
        let covered = vec![true, true, true];
        let lines = vec!["a", "b", "c"];
        let ranges = find_uncovered_ranges(&covered, &lines);
        assert!(ranges.is_empty());
    }

    #[test]
    fn compute_single_file_coverage_basic() {
        let lines = vec!["fn main() {", "  println!(\"hi\");", "}"];
        let ranges = vec![FileRangeEntry {
            start_line: 1,
            end_line: 2,
            mapping_id: "map-1".to_string(),
        }];
        let fc = compute_single_file_coverage("test.rs", &lines, Some(&ranges));
        assert_eq!(fc.total_lines, 3);
        assert_eq!(fc.covered_lines, 2); // lines 1-2 are non-whitespace & covered
        assert!(!fc.uncovered_ranges.is_empty()); // line 3 not covered
        assert!(fc.coverage_percent < 100.0);
        assert!(fc.coverage_percent > 50.0);
    }

    #[test]
    fn compute_single_file_coverage_full() {
        let lines = vec!["a", "b", "c"];
        let ranges = vec![FileRangeEntry {
            start_line: 1,
            end_line: 3,
            mapping_id: "map-all".to_string(),
        }];
        let fc = compute_single_file_coverage("test.rs", &lines, Some(&ranges));
        assert_eq!(fc.covered_lines, 3);
        assert!((fc.coverage_percent - 100.0).abs() < f64::EPSILON);
        assert!(fc.uncovered_ranges.is_empty());
    }

    #[test]
    fn build_coverage_maps_filters_by_path() {
        let mappings = vec![
            artifact::ArtifactMapping {
                id: "m1".into(),
                spec_ref_node: "n1".into(),
                spec_ref_revision: "r1".into(),
                node_hash: None,
                artifact_path: "src/foo.rs".into(),
                artifact_repo: "repo".into(),
                ranges: vec![artifact::ArtifactRange {
                    hash: None,
                    start_line: Some(1),
                    end_line: Some(10),
                    start_byte: None,
                    end_byte: None,
                }],
                coverage: "full".into(),
                source_file: std::path::PathBuf::new(),
            },
            artifact::ArtifactMapping {
                id: "m2".into(),
                spec_ref_node: "n2".into(),
                spec_ref_revision: "r1".into(),
                node_hash: None,
                artifact_path: "src/bar.rs".into(),
                artifact_repo: "repo".into(),
                ranges: vec![artifact::ArtifactRange {
                    hash: None,
                    start_line: Some(5),
                    end_line: Some(20),
                    start_byte: None,
                    end_byte: None,
                }],
                coverage: "full".into(),
                source_file: std::path::PathBuf::new(),
            },
        ];

        // No filter: both files
        let (map, _) = build_coverage_maps(&mappings, None);
        assert_eq!(map.len(), 2);

        // Filter to "foo": only foo.rs
        let (map, _) = build_coverage_maps(&mappings, Some("foo"));
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("src/foo.rs"));
    }

    #[test]
    fn build_coverage_maps_whole_file_vs_ranged() {
        let mappings = vec![
            artifact::ArtifactMapping {
                id: "m-ranged".into(),
                spec_ref_node: "n1".into(),
                spec_ref_revision: "r1".into(),
                node_hash: None,
                artifact_path: "ranged.rs".into(),
                artifact_repo: "repo".into(),
                ranges: vec![artifact::ArtifactRange {
                    hash: None,
                    start_line: Some(1),
                    end_line: Some(10),
                    start_byte: None,
                    end_byte: None,
                }],
                coverage: "full".into(),
                source_file: std::path::PathBuf::new(),
            },
            artifact::ArtifactMapping {
                id: "m-whole".into(),
                spec_ref_node: "n2".into(),
                spec_ref_revision: "r1".into(),
                node_hash: None,
                artifact_path: "whole.rs".into(),
                artifact_repo: "repo".into(),
                ranges: vec![artifact::ArtifactRange {
                    hash: None,
                    start_line: None,
                    end_line: None,
                    start_byte: None,
                    end_byte: None,
                }],
                coverage: "full".into(),
                source_file: std::path::PathBuf::new(),
            },
        ];

        let (file_map, whole_map) = build_coverage_maps(&mappings, None);
        assert!(file_map.contains_key("ranged.rs"));
        assert!(!file_map.contains_key("whole.rs"));
        assert!(whole_map.contains_key("whole.rs"));
        assert!(!whole_map.contains_key("ranged.rs"));
    }

    #[test]
    fn shipped_spec_has_file_coverages() {
        let spec_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../clayers/clayers")
            .canonicalize()
            .expect("clayers/clayers/ not found");
        let report = analyze_coverage(&spec_dir, None).expect("coverage failed");
        assert!(
            !report.file_coverages.is_empty(),
            "shipped spec should have file coverages"
        );
        // Every file coverage should have non-zero total lines
        for fc in &report.file_coverages {
            assert!(fc.total_lines > 0, "file {} has 0 total lines", fc.file_path);
        }
    }
}
