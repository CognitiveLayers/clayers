//! Schema-driven validation of spec files via the `uppsala` XSD validator.
//!
//! Complements the structural checks in [`crate::validate`] (well-formedness,
//! ID uniqueness, relation/artifact reference resolution) with full XSD 1.1
//! enforcement: required attributes, `HashType` and other pattern facets,
//! enumeration restrictions, content-model conformance, and strict
//! `xs:any namespace="##other"` wildcard checks.
//!
//! ## Approach
//!
//! - Every `*.xsd` in the spec's schema directory is collected and its
//!   `targetNamespace` attribute is read.
//! - A synthetic root schema is built that imports each layer XSD by
//!   `(namespace, schemaLocation)`. uppsala's
//!   `from_schema_with_base_path` then loads the imports relative to a
//!   virtual path inside `schemas/`.
//! - Each spec file is validated individually against the assembled
//!   validator. Per-file roots are `<spec:clayers>` (declared by `spec.xsd`),
//!   so validation has a starting point.
//!
//! ## Integration with the existing validator
//!
//! Findings are returned as a flat `Vec<ValidationError>` keyed to the
//! file they were found in. The caller (`validate::validate_spec`) merges
//! them with the structural-check errors. A spec passes overall validation
//! only when both check sets are clean.

use std::fmt::Write;
use std::path::Path;

use crate::validate::ValidationError;

/// Run schema validation against every file in `file_paths`.
///
/// Returns a flat list of validation errors. Errors include the file name
/// and uppsala's location/message so the caller can present them directly.
///
/// # Errors
///
/// Returns an error if the schema directory cannot be read, the synthetic
/// root schema cannot be parsed, or the validator cannot be built.
pub fn validate_against_schemas(
    schema_dir: &Path,
    file_paths: &[impl AsRef<Path>],
) -> Result<Vec<ValidationError>, crate::Error> {
    let layers = discover_layer_schemas(schema_dir)?;
    if layers.is_empty() {
        return Ok(Vec::new());
    }

    let root_schema = build_root_schema(&layers);
    let schema_doc = uppsala::parse(&root_schema).map_err(|e| {
        crate::Error::Validation(format!("synthetic root schema parse failed: {e}"))
    })?;

    // uppsala derives the include/import base directory from the parent of
    // the supplied path, so we feed it a virtual file path inside schemas/.
    let virtual_root = schema_dir.join("_clayers_root.xsd");
    let validator = uppsala::XsdValidator::from_schema_with_base_path(&schema_doc, Some(&virtual_root))
        .map_err(|e| crate::Error::Validation(format!("XSD validator build failed: {e}")))?;

    let mut errors = Vec::new();
    for file_path in file_paths {
        let path = file_path.as_ref();
        let content = std::fs::read_to_string(path)?;
        let doc = match uppsala::parse(&content) {
            Ok(d) => d,
            Err(e) => {
                // Surface as a validation error; well-formedness is also
                // checked by the structural pass, but uppsala may parse
                // slightly differently and surface its own diagnostic.
                errors.push(ValidationError {
                    message: format!("{}: uppsala parse: {e}", path.display()),
                });
                continue;
            }
        };
        for ve in validator.validate(&doc) {
            errors.push(ValidationError {
                message: format!("{}: {ve}", path.display()),
            });
        }
    }

    Ok(errors)
}

/// A discovered layer schema: its target namespace and its filename within
/// the schema directory.
struct LayerSchema {
    namespace: String,
    file_name: String,
}

/// Walk the schema directory, parse each `*.xsd`, and extract its
/// `targetNamespace` attribute. Schemas without a target namespace are
/// skipped (they can't be imported by namespace).
fn discover_layer_schemas(schema_dir: &Path) -> Result<Vec<LayerSchema>, crate::Error> {
    let mut layers = Vec::new();
    for entry in std::fs::read_dir(schema_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "xsd") {
            continue;
        }
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let content = std::fs::read_to_string(&path)?;
        let mut xot = xot::Xot::new();
        let Ok(doc) = xot.parse(&content) else {
            continue;
        };
        let Ok(root) = xot.document_element(doc) else {
            continue;
        };
        let target_ns_attr = xot.add_name("targetNamespace");
        let Some(ns) = xot.get_attribute(root, target_ns_attr) else {
            continue;
        };
        layers.push(LayerSchema {
            namespace: ns.to_string(),
            file_name,
        });
    }
    // Sort for stable schema-build output across runs.
    layers.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    Ok(layers)
}

/// Build a synthetic `<xs:schema>` root that imports every layer XSD by
/// `(namespace, schemaLocation)`. Each import's `schemaLocation` is the
/// bare filename, resolved by uppsala against the schema directory.
fn build_root_schema(layers: &[LayerSchema]) -> String {
    let mut s = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:clayers:internal:validation-root"
           elementFormDefault="qualified"
           version="1.1">
"#,
    );
    for layer in layers {
        writeln!(
            s,
            "  <xs:import namespace=\"{}\" schemaLocation=\"{}\"/>",
            layer.namespace, layer.file_name
        )
        .expect("write to String");
    }
    s.push_str("</xs:schema>\n");
    s
}

/// Internal: read the targetNamespace attribute from a schema document.
/// Used by tests to verify the discovery walk.
#[cfg(test)]
fn target_namespace_of(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut xot = xot::Xot::new();
    let doc = xot.parse(&content).ok()?;
    let root = xot.document_element(doc).ok()?;
    let attr = xot.add_name("targetNamespace");
    xot.get_attribute(root, attr).map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn schemas_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas")
            .canonicalize()
            .expect("resolve schemas/")
    }

    fn self_spec_files() -> Vec<PathBuf> {
        let spec = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../clayers/clayers")
            .canonicalize()
            .expect("resolve clayers/clayers/");
        crate::discovery::discover_spec_files(&spec.join("index.xml")).expect("discover")
    }

    #[test]
    fn discovers_every_shipped_layer_schema() {
        let layers = discover_layer_schemas(&schemas_dir()).expect("discover");
        assert!(
            layers.len() >= 17,
            "expected 17+ layer schemas, got {}",
            layers.len()
        );
        // Spot-check a few well-known ones
        let ns: Vec<&str> = layers.iter().map(|l| l.namespace.as_str()).collect();
        for required in &[
            "urn:clayers:spec",
            "urn:clayers:prose",
            "urn:clayers:terminology",
            "urn:clayers:artifact",
            "urn:clayers:relation",
        ] {
            assert!(ns.contains(required), "missing namespace {required}");
        }
    }

    #[test]
    fn root_schema_contains_imports_for_every_layer() {
        let layers = discover_layer_schemas(&schemas_dir()).expect("discover");
        let root = build_root_schema(&layers);
        for layer in &layers {
            let needle = format!(
                "<xs:import namespace=\"{}\" schemaLocation=\"{}\"/>",
                layer.namespace, layer.file_name
            );
            assert!(root.contains(&needle), "missing import for {needle}");
        }
    }

    #[test]
    fn validates_self_spec_returns_findings() {
        // Informational: we expect findings on the shipped self-spec until
        // the schema/spec mismatches surfaced by uppsala are fixed. For now
        // we only assert that validation runs end-to-end and produces a
        // structured result.
        let errs = validate_against_schemas(&schemas_dir(), &self_spec_files())
            .expect("validation runs");
        eprintln!("uppsala self-spec findings: {}", errs.len());
        for e in errs.iter().take(5) {
            eprintln!("  {}", e.message);
        }
    }

    #[test]
    fn target_ns_helper_reads_existing_schema() {
        let p = schemas_dir().join("spec.xsd");
        assert_eq!(
            target_namespace_of(&p).as_deref(),
            Some("urn:clayers:spec")
        );
    }
}
