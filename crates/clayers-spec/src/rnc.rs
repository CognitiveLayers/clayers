//! LLM description fusing and RNC export API.
//!
//! Calls `clayers_xml::rnc::xsd_to_rnc()` to auto-discover schemas,
//! then enriches the result by extracting `llm:describe` annotations from
//! XSD appinfo and injecting them as `description` fields on the RNC structs.

use std::collections::HashMap;
use std::path::Path;

use xot::Xot;

use clayers_xml::rnc::RncSchema;

/// Export all schemas as RNC with `llm:describe` annotations as comments.
///
/// # Errors
///
/// Returns an error if schema files cannot be read or parsed.
pub fn export_rnc(schema_dir: &Path) -> Result<RncSchema, crate::Error> {
    let mut schema = clayers_xml::rnc::xsd_to_rnc(schema_dir, &[])?;
    fuse_descriptions(schema_dir, &mut schema)?;
    Ok(schema)
}

/// Export specific layers (by prefix) as RNC with `llm:describe` comments.
///
/// # Errors
///
/// Returns an error if schema files cannot be read or parsed.
pub fn export_rnc_filtered(
    schema_dir: &Path,
    prefixes: &[&str],
) -> Result<RncSchema, crate::Error> {
    let mut schema = export_rnc(schema_dir)?;
    schema
        .layers
        .retain(|layer| prefixes.contains(&layer.prefix.as_str()));
    Ok(schema)
}

/// Get the local part of a possibly-prefixed type reference.
fn split_type_local(type_ref: &str) -> &str {
    type_ref.rsplit_once(':').map_or(type_ref, |(_, l)| l)
}

/// Extract `llm:describe` text from a schema root or type element.
fn extract_llm_describe(xot: &mut Xot, node: xot::Node, llm_uri: &str) -> Option<String> {
    let xs_ns = xot.add_namespace("http://www.w3.org/2001/XMLSchema");
    let annotation = xot.add_name_ns("annotation", xs_ns);
    let appinfo = xot.add_name_ns("appinfo", xs_ns);
    let llm_ns = xot.add_namespace(llm_uri);
    let describe = xot.add_name_ns("describe", llm_ns);

    for ann_child in xot.children(node) {
        if !xot.is_element(ann_child)
            || xot.element(ann_child).is_none_or(|e| e.name() != annotation)
        {
            continue;
        }
        for app_child in xot.children(ann_child) {
            if !xot.is_element(app_child)
                || xot
                    .element(app_child)
                    .is_none_or(|e| e.name() != appinfo)
            {
                continue;
            }
            for desc_child in xot.children(app_child) {
                if xot.is_element(desc_child)
                    && xot
                        .element(desc_child)
                        .is_some_and(|e| e.name() == describe)
                {
                    let text = xot.text_content_str(desc_child).unwrap_or("").trim().to_string();
                    if !text.is_empty() {
                        // Normalize whitespace.
                        let normalized: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
                        return Some(normalized);
                    }
                }
            }
        }
    }
    None
}

/// Walk XSD files and inject `llm:describe` text into matching RNC structs.
fn fuse_descriptions(schema_dir: &Path, schema: &mut RncSchema) -> Result<(), crate::Error> {
    let mut xsd_paths: Vec<_> = std::fs::read_dir(schema_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "xsd"))
        .collect();
    xsd_paths.sort();

    let uri_to_prefix: HashMap<String, String> = schema
        .namespaces
        .iter()
        .map(|ns| (ns.uri.clone(), ns.prefix.clone()))
        .collect();

    // Find the LLM namespace URI from auto-discovered namespaces.
    let llm_uri = schema
        .namespaces
        .iter()
        .find(|ns| ns.uri == "urn:clayers:llm")
        .map(|ns| ns.uri.clone());

    // We need a persistent Xot so we can use add_namespace (takes &mut).
    // But extract_llm_describe also calls add_namespace. To work around this,
    // parse all files first, collect the descriptions, then apply them.
    let mut layer_descs: HashMap<String, String> = HashMap::new();
    let mut type_descs: HashMap<(String, String), String> = HashMap::new();
    let mut elem_descs: HashMap<(String, String), String> = HashMap::new();

    // If no LLM namespace was discovered, skip description extraction entirely.
    let Some(llm_uri) = llm_uri else {
        return Ok(());
    };

    for xsd_path in &xsd_paths {
        let content = std::fs::read_to_string(xsd_path)?;
        let mut xot = Xot::new();
        let doc = xot.parse(&content).map_err(xot::Error::from)?;
        let root = xot.document_element(doc)?;

        let tns_attr = xot.add_name("targetNamespace");
        let tns = xot.get_attribute(root, tns_attr)
            .unwrap_or("")
            .to_string();
        // Skip XSD files whose namespace was not discovered (not in the schema).
        let Some(pfx) = uri_to_prefix.get(&tns).cloned() else {
            continue;
        };

        // Schema-level llm:describe -> layer description.
        if let Some(desc) = extract_llm_describe(&mut xot, root, &llm_uri) {
            layer_descs.insert(pfx.clone(), desc);
        }

        // complexType-level llm:describe -> pattern/element type descriptions.
        let xs_ns = xot.add_namespace("http://www.w3.org/2001/XMLSchema");
        let complex_type = xot.add_name_ns("complexType", xs_ns);
        let element_tag = xot.add_name_ns("element", xs_ns);
        let name_attr = xot.add_name("name");
        let type_attr = xot.add_name("type");

        // Collect child info first to avoid borrow conflicts with extract_llm_describe.
        let child_info: Vec<(xot::Node, xot::NameId, Option<String>, Option<String>)> = xot
            .children(root)
            .filter(|c| xot.is_element(*c))
            .filter_map(|c| {
                let el = xot.element(c)?;
                let cn = el.name();
                let nm = xot.get_attribute(c, name_attr).map(String::from);
                let tr = xot.get_attribute(c, type_attr).map(String::from);
                Some((c, cn, nm, tr))
            })
            .collect();

        for (child, child_name, name_val, type_ref_val) in child_info {
            let Some(n) = name_val else { continue };
            if child_name == complex_type {
                if let Some(desc) = extract_llm_describe(&mut xot, child, &llm_uri) {
                    type_descs.insert((pfx.clone(), n), desc);
                }
            } else if child_name == element_tag {
                if let Some(desc) = extract_llm_describe(&mut xot, child, &llm_uri) {
                    elem_descs.insert((pfx.clone(), n.clone()), desc);
                }
                let key = (pfx.clone(), n.clone());
                if !elem_descs.contains_key(&key)
                    && let Some(type_ref) = &type_ref_val
                {
                    let local = split_type_local(type_ref);
                    if let Some(desc) = type_descs.get(&(pfx.clone(), local.to_string())) {
                        elem_descs.insert(key, desc.clone());
                    }
                }
            }
        }
    }

    // Apply descriptions to the schema.
    for layer in &mut schema.layers {
        if let Some(desc) = layer_descs.get(&layer.prefix) {
            layer.description = Some(desc.clone());
        }
        for pat in &mut layer.patterns {
            let key = (layer.prefix.clone(), pat.name.clone());
            if let Some(desc) = type_descs.get(&key) {
                pat.description = Some(desc.clone());
            }
        }
        for elem in &mut layer.elements {
            let key = (layer.prefix.clone(), elem.name.clone());
            if let Some(desc) = elem_descs.get(&key) {
                elem.description = Some(desc.clone());
            }
        }
    }

    Ok(())
}

/// Format an `RncSchema` as a string (convenience wrapper around `Display`).
///
/// This produces the same output as `schema.to_string()` but makes the
/// intent explicit.
#[must_use]
pub fn render(schema: &RncSchema) -> String {
    schema.to_string()
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn schemas_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas")
            .canonicalize()
            .expect("schemas/ directory not found")
    }

    #[test]
    fn export_rnc_produces_output_with_namespaces() {
        let schema = export_rnc(&schemas_dir()).expect("export_rnc failed");
        let output = schema.to_string();
        assert!(output.len() > 100, "Output too short: {}", output.len());
        assert!(
            output.contains("namespace"),
            "Missing namespace declarations"
        );
        // Should have multiple layers.
        assert!(
            schema.layers.len() >= 10,
            "Expected 10+ layers, got {}",
            schema.layers.len()
        );
    }

    #[test]
    fn export_rnc_has_llm_describe_comments() {
        let schema = export_rnc(&schemas_dir()).expect("export_rnc failed");
        let output = schema.to_string();
        // prose.xsd has llm:describe on the schema root and on SectionType.
        assert!(
            output.contains("# The prose schema provides"),
            "Missing prose layer llm:describe comment in output:\n{output}"
        );
    }

    #[test]
    fn export_rnc_filtered_returns_single_layer() {
        let schema =
            export_rnc_filtered(&schemas_dir(), &["pr"]).expect("export_rnc_filtered failed");
        assert_eq!(schema.layers.len(), 1);
        assert_eq!(schema.layers[0].prefix, "pr");
    }

    #[test]
    fn export_rnc_recursive_types_are_named_patterns() {
        let schema = export_rnc(&schemas_dir()).expect("export_rnc failed");
        let output = schema.to_string();
        // SectionType in prose is recursive (section contains section).
        assert!(
            output.contains("SectionType ="),
            "SectionType should be a named pattern: {output}"
        );
    }

    #[test]
    fn export_rnc_topicref_recursive() {
        let schema = export_rnc(&schemas_dir()).expect("export_rnc failed");
        let output = schema.to_string();
        // TopicRefType in organization is recursive.
        assert!(
            output.contains("TopicRefType ="),
            "TopicRefType should be a named pattern: {output}"
        );
    }

    #[test]
    fn render_produces_same_as_display() {
        let schema = export_rnc(&schemas_dir()).expect("export_rnc failed");
        assert_eq!(render(&schema), schema.to_string());
    }
}
