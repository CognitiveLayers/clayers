use std::collections::HashMap;
use std::path::Path;

use xot::Xot;

use crate::namespace;

/// Result of running `validate_spec` over a spec directory: both the
/// hand-rolled structural checks (well-formedness, cross-file ID
/// uniqueness, cross-layer reference resolution) and the schema-driven
/// checks (via [`crate::xsd_validation`]). Errors from both layers are
/// merged into a single list.
#[derive(Debug)]
pub struct ValidationResult {
    pub spec_name: String,
    pub file_count: usize,
    pub errors: Vec<ValidationError>,
}

impl ValidationResult {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

/// A single validation finding. Messages carry enough context (file path,
/// line/column when available, a human-readable description) to be
/// printed directly by the CLI.
#[derive(Debug)]
pub struct ValidationError {
    pub message: String,
}

/// Validate a spec against its layer schemas.
///
/// Runs two layers of checks over every file reachable from the index(es):
///
/// 1. **Structural (hand-rolled)** — well-formedness, cross-file ID
///    uniqueness (XSD `xs:ID` only enforces within a single document;
///    clayers specs are multi-file so this fills that gap), and
///    cross-layer reference resolution (`rel:relation` `from`/`to`,
///    `art:artifact/@repo`).
/// 2. **Schema-driven (XSD 1.1)** — required attributes, pattern facets
///    (e.g. hash format), enumeration restrictions (e.g. `coverage`
///    values), content-model conformance, and strict
///    `xs:any namespace="##other"` wildcard resolution. Delegated to
///    [`crate::xsd_validation::validate_against_schemas`], which uses the
///    `uppsala` crate under the hood. Runs only when a schema directory
///    is reachable from `spec_dir`.
///
/// Both layers contribute to the same flat error list; the caller can
/// print them directly or test `is_valid()` to gate a build.
///
/// # Errors
///
/// Returns an error if spec files cannot be discovered or read, or if
/// the schema-driven validator cannot be constructed.
pub fn validate_spec(spec_dir: &Path) -> Result<ValidationResult, crate::Error> {
    let index_files = crate::discovery::find_index_files(spec_dir)?;

    if index_files.is_empty() {
        return Ok(ValidationResult {
            spec_name: spec_dir.display().to_string(),
            file_count: 0,
            errors: vec![ValidationError {
                message: "no index files found".into(),
            }],
        });
    }

    let mut all_errors = Vec::new();
    let mut total_files = 0;
    let mut spec_name = String::new();

    for index_path in &index_files {
        let file_paths = crate::discovery::discover_spec_files(index_path)?;
        total_files += file_paths.len();

        spec_name = index_path
            .parent()
            .and_then(|p| p.file_name())
            .map_or_else(|| "unknown".into(), |n| n.to_string_lossy().into_owned());

        // Check each file is well-formed XML
        for file_path in &file_paths {
            if let Err(e) = check_well_formed(file_path) {
                all_errors.push(ValidationError {
                    message: format!("{}: {e}", file_path.display()),
                });
            }
        }

        // Check ID uniqueness across all files
        let id_errors = check_id_uniqueness(&file_paths)?;
        all_errors.extend(id_errors);

        // Check cross-layer references
        let ref_errors = check_references(&file_paths)?;
        all_errors.extend(ref_errors);

        // Schema-driven validation: enforce required attributes, pattern
        // facets, enum restrictions, content models, etc. via uppsala.
        // Only runs if a schema directory is reachable from the spec dir.
        if let Some(schema_dir) = crate::discovery::find_schema_dir(spec_dir) {
            let xsd_errors =
                crate::xsd_validation::validate_against_schemas(&schema_dir, &file_paths)?;
            all_errors.extend(xsd_errors);
        }
    }

    Ok(ValidationResult {
        spec_name,
        file_count: total_files,
        errors: all_errors,
    })
}

fn check_well_formed(path: &Path) -> Result<(), String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut xot = Xot::new();
    xot.parse(&content).map_err(|e| e.to_string())?;
    Ok(())
}

fn check_id_uniqueness(
    file_paths: &[impl AsRef<Path>],
) -> Result<Vec<ValidationError>, crate::Error> {
    let mut seen: HashMap<String, String> = HashMap::new();
    let mut errors = Vec::new();

    for file_path in file_paths {
        let file_path = file_path.as_ref();
        let content = std::fs::read_to_string(file_path)?;
        let mut xot = Xot::new();
        let doc = xot.parse(&content).map_err(xot::Error::from)?;
        let root = xot.document_element(doc)?;
        let id_attr = xot.add_name("id");
        let xml_ns = xot.add_namespace(namespace::XML);
        let xml_id_attr = xot.add_name_ns("id", xml_ns);

        collect_ids(
            &xot,
            root,
            id_attr,
            xml_id_attr,
            file_path,
            &mut seen,
            &mut errors,
        );
    }

    Ok(errors)
}

fn collect_ids(
    xot: &Xot,
    node: xot::Node,
    id_attr: xot::NameId,
    xml_id_attr: xot::NameId,
    file_path: &Path,
    seen: &mut HashMap<String, String>,
    errors: &mut Vec<ValidationError>,
) {
    if xot.is_element(node) {
        // Collect bare @id
        if let Some(id) = xot.get_attribute(node, id_attr) {
            let id = id.to_string();
            let file_str = file_path.display().to_string();
            if let Some(prev_file) = seen.get(&id) {
                errors.push(ValidationError {
                    message: format!(
                        "duplicate id \"{id}\" (first in {prev_file}, also in {file_str})"
                    ),
                });
            } else {
                seen.insert(id, file_str);
            }
        }
        // Collect xml:id (W3C standard, used by XMI/UML elements)
        if let Some(xml_id) = xot.get_attribute(node, xml_id_attr) {
            let xml_id = xml_id.to_string();
            let file_str = file_path.display().to_string();
            if let Some(prev_file) = seen.get(&xml_id) {
                errors.push(ValidationError {
                    message: format!(
                        "duplicate id \"{xml_id}\" (first in {prev_file}, also in {file_str})"
                    ),
                });
            } else {
                seen.insert(xml_id, file_str);
            }
        }
    }
    for child in xot.children(node) {
        collect_ids(xot, child, id_attr, xml_id_attr, file_path, seen, errors);
    }
}

fn check_references(file_paths: &[impl AsRef<Path>]) -> Result<Vec<ValidationError>, crate::Error> {
    // Collect all known IDs (both bare @id and xml:id)
    let mut all_ids = std::collections::HashSet::new();
    let mut errors = Vec::new();

    for file_path in file_paths {
        let content = std::fs::read_to_string(file_path.as_ref())?;
        let mut xot = Xot::new();
        let doc = xot.parse(&content).map_err(xot::Error::from)?;
        let root = xot.document_element(doc)?;
        let id_attr = xot.add_name("id");
        let xml_ns = xot.add_namespace(namespace::XML);
        let xml_id_attr = xot.add_name_ns("id", xml_ns);
        collect_all_ids(&xot, root, id_attr, xml_id_attr, &mut all_ids);
    }

    // Check relation and artifact references
    for file_path in file_paths {
        let content = std::fs::read_to_string(file_path.as_ref())?;
        let mut xot = Xot::new();
        let doc = xot.parse(&content).map_err(xot::Error::from)?;
        let root = xot.document_element(doc)?;

        let relation_ns = xot.add_namespace(namespace::RELATION);
        let relation_tag = xot.add_name_ns("relation", relation_ns);
        let from_attr = xot.add_name("from");
        let to_attr = xot.add_name("to");
        let to_spec_attr = xot.add_name("to-spec");

        check_relation_refs(
            &xot,
            root,
            relation_tag,
            from_attr,
            to_attr,
            to_spec_attr,
            &all_ids,
            &mut errors,
        );

        // Check art:artifact/@repo references a known ID (typically vcs:git/@id)
        let art_ns = xot.add_namespace(namespace::ARTIFACT);
        let artifact_tag = xot.add_name_ns("artifact", art_ns);
        let repo_attr = xot.add_name("repo");

        check_artifact_repo_refs(
            &xot,
            root,
            artifact_tag,
            repo_attr,
            &all_ids,
            &mut errors,
        );
    }

    Ok(errors)
}

fn collect_all_ids(
    xot: &Xot,
    node: xot::Node,
    id_attr: xot::NameId,
    xml_id_attr: xot::NameId,
    ids: &mut std::collections::HashSet<String>,
) {
    if xot.is_element(node) {
        if let Some(id) = xot.get_attribute(node, id_attr) {
            ids.insert(id.to_string());
        }
        if let Some(xml_id) = xot.get_attribute(node, xml_id_attr) {
            ids.insert(xml_id.to_string());
        }
    }
    for child in xot.children(node) {
        collect_all_ids(xot, child, id_attr, xml_id_attr, ids);
    }
}

fn check_artifact_repo_refs(
    xot: &Xot,
    node: xot::Node,
    artifact_tag: xot::NameId,
    repo_attr: xot::NameId,
    all_ids: &std::collections::HashSet<String>,
    errors: &mut Vec<ValidationError>,
) {
    if xot.is_element(node)
        && xot.element(node).is_some_and(|e| e.name() == artifact_tag)
        && let Some(repo) = xot.get_attribute(node, repo_attr)
        && !all_ids.contains(repo)
    {
        errors.push(ValidationError {
            message: format!(
                "art:artifact repo=\"{repo}\" references unknown id \
                 (add a vcs:git or other element with id=\"{repo}\")"
            ),
        });
    }
    for child in xot.children(node) {
        check_artifact_repo_refs(xot, child, artifact_tag, repo_attr, all_ids, errors);
    }
}

#[allow(clippy::too_many_arguments)]
fn check_relation_refs(
    xot: &Xot,
    node: xot::Node,
    relation_tag: xot::NameId,
    from_attr: xot::NameId,
    to_attr: xot::NameId,
    to_spec_attr: xot::NameId,
    all_ids: &std::collections::HashSet<String>,
    errors: &mut Vec<ValidationError>,
) {
    if xot.is_element(node) && xot.element(node).is_some_and(|e| e.name() == relation_tag) {
        // Skip cross-spec relations
        if xot.get_attribute(node, to_spec_attr)
            .is_none()
        {
            if let Some(from) = xot.get_attribute(node, from_attr)
                && !all_ids.contains(from)
                && !from.starts_with("type-")
            {
                errors.push(ValidationError {
                    message: format!("relation from=\"{from}\" references nonexistent id"),
                });
            }
            if let Some(to) = xot.get_attribute(node, to_attr)
                && !all_ids.contains(to)
                && !to.starts_with("type-")
            {
                errors.push(ValidationError {
                    message: format!("relation to=\"{to}\" references nonexistent id"),
                });
            }
        }
    }
    for child in xot.children(node) {
        check_relation_refs(
            xot,
            child,
            relation_tag,
            from_attr,
            to_attr,
            to_spec_attr,
            all_ids,
            errors,
        );
    }
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
    fn shipped_spec_passes_validation() {
        let result = validate_spec(&spec_dir()).expect("validation failed");
        assert!(
            result.is_valid(),
            "shipped spec should be valid, got errors: {:?}",
            result.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn duplicate_id_detected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let xml = r#"<?xml version="1.0"?>
<spec:clayers xmlns:spec="urn:clayers:spec"
              xmlns:idx="urn:clayers:index"
              xmlns:pr="urn:clayers:prose">
  <idx:file href="content.xml"/>
</spec:clayers>"#;
        std::fs::write(dir.path().join("index.xml"), xml).expect("write");

        let content = r#"<?xml version="1.0"?>
<spec:clayers xmlns:spec="urn:clayers:spec"
              xmlns:pr="urn:clayers:prose"
              spec:index="index.xml">
  <pr:section id="dupe">first</pr:section>
  <pr:section id="dupe">second</pr:section>
</spec:clayers>"#;
        std::fs::write(dir.path().join("content.xml"), content).expect("write");

        let result = validate_spec(dir.path()).expect("validation failed");
        assert!(!result.is_valid(), "duplicate IDs should fail validation");
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.message.contains("duplicate")),
            "error message should mention duplicate"
        );
    }

    #[test]
    fn empty_dir_reports_no_index() {
        let dir = tempfile::tempdir().expect("tempdir");
        let result = validate_spec(dir.path()).expect("validation failed");
        assert!(!result.is_valid());
    }
}
