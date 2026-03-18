use std::path::Path;

/// Schema annotation discovery: finds content-elements and keyrefs
/// declared via `spec:content-element` and `spec:keyref` appinfo annotations.
///
/// A content element discovered from schema annotations.
#[derive(Debug, Clone)]
pub struct ContentElement {
    pub prefix: String,
    pub element_name: String,
    pub namespace: String,
}

/// A keyref discovered from schema annotations.
#[derive(Debug, Clone)]
pub struct Keyref {
    pub name: String,
    pub selector: String,
    pub field: String,
}

/// Discover acyclic relation types from relation.xsd annotations.
///
/// # Errors
///
/// Returns an error if the schema file cannot be read or parsed.
pub fn discover_acyclic_types(
    schema_dir: &Path,
) -> Result<std::collections::HashSet<String>, crate::Error> {
    let rel_xsd = schema_dir.join("relation.xsd");
    let mut acyclic = std::collections::HashSet::new();

    if !rel_xsd.exists() {
        return Ok(acyclic);
    }

    let content = std::fs::read_to_string(&rel_xsd)?;

    // Simple text-based extraction: find enumeration values with acyclic="true"
    // This mirrors the Python logic without needing full XSD parsing
    let mut xot = xot::Xot::new();
    let doc = xot.parse(&content).map_err(xot::Error::from)?;
    let root = xot.document_element(doc)?;

    let xs_ns = xot.add_namespace("http://www.w3.org/2001/XMLSchema");
    let enum_name = xot.add_name_ns("enumeration", xs_ns);
    let value_attr = xot.add_name("value");

    let relation_ns = xot.add_namespace(crate::namespace::RELATION);
    let acyclic_name = xot.add_name_ns("acyclic", relation_ns);
    let acyclic_value_attr = xot.add_name("value");

    collect_acyclic_types(
        &xot,
        root,
        enum_name,
        value_attr,
        acyclic_name,
        acyclic_value_attr,
        &mut acyclic,
    );

    Ok(acyclic)
}

fn collect_acyclic_types(
    xot: &xot::Xot,
    node: xot::Node,
    enum_name: xot::NameId,
    value_attr: xot::NameId,
    acyclic_name: xot::NameId,
    acyclic_value_attr: xot::NameId,
    acyclic: &mut std::collections::HashSet<String>,
) {
    if xot.is_element(node)
        && xot.element(node).is_some_and(|e| e.name() == enum_name)
        && let Some(value) = xot.get_attribute(node, value_attr)
    {
        let value = value.to_string();
        if has_acyclic_true(xot, node, acyclic_name, acyclic_value_attr) {
            acyclic.insert(value);
        }
    }
    for child in xot.children(node) {
        collect_acyclic_types(
            xot,
            child,
            enum_name,
            value_attr,
            acyclic_name,
            acyclic_value_attr,
            acyclic,
        );
    }
}

fn has_acyclic_true(
    xot: &xot::Xot,
    node: xot::Node,
    acyclic_name: xot::NameId,
    value_attr: xot::NameId,
) -> bool {
    for child in xot.children(node) {
        if xot.is_element(child)
            && xot.element(child).is_some_and(|e| e.name() == acyclic_name)
            && let Some(v) = xot.get_attribute(child, value_attr)
            && v == "true"
        {
            return true;
        }
        if has_acyclic_true(xot, child, acyclic_name, value_attr) {
            return true;
        }
    }
    false
}

/// Discover content elements from schema annotations.
///
/// Scans all `.xsd` files for global elements annotated with `spec:content-element`.
///
/// # Errors
///
/// Returns an error if schema files cannot be read.
pub fn discover_content_elements(schema_dir: &Path) -> Result<Vec<ContentElement>, crate::Error> {
    let mut elements = Vec::new();

    for entry in std::fs::read_dir(schema_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "xsd")
            && let Ok(content) = std::fs::read_to_string(&path)
        {
            discover_from_xsd(&content, &mut elements);
        }
    }

    Ok(elements)
}

fn discover_from_xsd(content: &str, elements: &mut Vec<ContentElement>) {
    let mut xot = xot::Xot::new();
    let Ok(doc) = xot.parse(content) else { return };
    let Ok(root) = xot.document_element(doc) else {
        return;
    };

    let xs_ns = xot.add_namespace("http://www.w3.org/2001/XMLSchema");
    let element_tag = xot.add_name_ns("element", xs_ns);
    let spec_ns = xot.add_namespace(crate::namespace::SPEC);
    let content_element_tag = xot.add_name_ns("content-element", spec_ns);
    let name_attr = xot.add_name("name");
    let target_ns_attr = xot.add_name("targetNamespace");

    let target_ns = xot.get_attribute(root, target_ns_attr)
        .unwrap_or("")
        .to_string();

    // Find prefix for this namespace
    let prefix = crate::namespace::prefix_for(&target_ns)
        .unwrap_or("")
        .to_string();

    collect_content_elements(
        &xot,
        root,
        element_tag,
        content_element_tag,
        name_attr,
        &prefix,
        &target_ns,
        elements,
    );
}

#[allow(clippy::too_many_arguments)]
fn collect_content_elements(
    xot: &xot::Xot,
    node: xot::Node,
    element_tag: xot::NameId,
    content_element_tag: xot::NameId,
    name_attr: xot::NameId,
    prefix: &str,
    namespace: &str,
    elements: &mut Vec<ContentElement>,
) {
    if xot.is_element(node)
        && xot.element(node).is_some_and(|e| e.name() == element_tag)
        && let Some(name) = xot.get_attribute(node, name_attr)
    {
        let name = name.to_string();
        if has_content_element_annotation(xot, node, content_element_tag) && !prefix.is_empty() {
            elements.push(ContentElement {
                prefix: prefix.to_string(),
                element_name: name,
                namespace: namespace.to_string(),
            });
        }
    }
    for child in xot.children(node) {
        collect_content_elements(
            xot,
            child,
            element_tag,
            content_element_tag,
            name_attr,
            prefix,
            namespace,
            elements,
        );
    }
}

fn has_content_element_annotation(xot: &xot::Xot, node: xot::Node, tag: xot::NameId) -> bool {
    for child in xot.children(node) {
        if xot.is_element(child) && xot.element(child).is_some_and(|e| e.name() == tag) {
            return true;
        }
        if has_content_element_annotation(xot, child, tag) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn schema_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas")
            .canonicalize()
            .expect("schemas/ not found")
    }

    #[test]
    fn discover_content_elements_from_shipped_schemas() {
        let elements = discover_content_elements(&schema_dir()).expect("discovery failed");
        assert!(
            elements.len() >= 5,
            "expected 5+ content elements, got {}",
            elements.len()
        );
    }

    #[test]
    fn discover_acyclic_types_from_relation_xsd() {
        let acyclic = discover_acyclic_types(&schema_dir()).expect("discovery failed");
        assert!(
            acyclic.contains("depends-on"),
            "depends-on should be acyclic"
        );
    }
}
