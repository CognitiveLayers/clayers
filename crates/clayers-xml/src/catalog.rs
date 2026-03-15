//! OASIS XML Catalog parsing (domain-agnostic).

use std::path::Path;

use xot::Xot;

/// A single entry from an OASIS XML Catalog `<uri>` element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    /// The namespace URI (`name` attribute).
    pub namespace: String,
    /// The relative file path (`uri` attribute).
    pub path: String,
}

/// Parse an OASIS XML Catalog file, returning namespace-to-path mappings.
///
/// Reads `<uri name="..." uri="..."/>` elements from the catalog.
///
/// # Errors
///
/// Returns an error if the file cannot be read or parsed as XML.
pub fn parse_catalog(catalog_path: &Path) -> Result<Vec<CatalogEntry>, crate::Error> {
    let content = std::fs::read_to_string(catalog_path)?;
    let mut xot = Xot::new();
    let doc = xot.parse(&content)?;
    let root = xot.document_element(doc)?;

    let catalog_ns = xot.add_namespace("urn:oasis:names:tc:entity:xmlns:xml:catalog");
    let uri_el = xot.add_name_ns("uri", catalog_ns);
    let name_attr = xot.add_name("name");
    let uri_attr = xot.add_name("uri");

    let mut entries = Vec::new();
    for child in xot.children(root) {
        if !xot.is_element(child) {
            continue;
        }
        let Some(el) = xot.element(child) else {
            continue;
        };
        if el.name() != uri_el {
            continue;
        }
        let Some(name) = el.get_attribute(name_attr) else {
            continue;
        };
        let Some(uri) = el.get_attribute(uri_attr) else {
            continue;
        };
        entries.push(CatalogEntry {
            namespace: name.to_string(),
            path: uri.to_string(),
        });
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_catalog_extracts_entries() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = r#"<?xml version="1.0" encoding="UTF-8"?>
<catalog xmlns="urn:oasis:names:tc:entity:xmlns:xml:catalog">
  <uri name="urn:test:foo" uri="foo.xsd"/>
  <uri name="urn:test:bar" uri="bar.xsd"/>
</catalog>"#;
        let path = dir.path().join("catalog.xml");
        std::fs::write(&path, catalog).unwrap();

        let entries = parse_catalog(&path).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].namespace, "urn:test:foo");
        assert_eq!(entries[0].path, "foo.xsd");
        assert_eq!(entries[1].namespace, "urn:test:bar");
        assert_eq!(entries[1].path, "bar.xsd");
    }

    #[test]
    fn parse_catalog_skips_non_uri_elements() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = r#"<?xml version="1.0" encoding="UTF-8"?>
<catalog xmlns="urn:oasis:names:tc:entity:xmlns:xml:catalog">
  <!-- A comment -->
  <uri name="urn:test:only" uri="only.xsd"/>
</catalog>"#;
        let path = dir.path().join("catalog.xml");
        std::fs::write(&path, catalog).unwrap();

        let entries = parse_catalog(&path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].namespace, "urn:test:only");
    }

    #[test]
    fn parse_catalog_empty_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = r#"<?xml version="1.0" encoding="UTF-8"?>
<catalog xmlns="urn:oasis:names:tc:entity:xmlns:xml:catalog">
</catalog>"#;
        let path = dir.path().join("catalog.xml");
        std::fs::write(&path, catalog).unwrap();

        let entries = parse_catalog(&path).unwrap();
        assert!(entries.is_empty());
    }
}
