use bergshamra_c14n::C14nMode;

use crate::{ContentHash, Error};

/// Canonicalization mode wrapping the W3C C14N algorithm variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalizationMode {
    /// Canonical XML 1.0 (inclusive).
    Inclusive,
    /// Canonical XML 1.0 with comments (inclusive).
    InclusiveWithComments,
    /// Canonical XML 1.1 (inclusive).
    Inclusive11,
    /// Canonical XML 1.1 with comments (inclusive).
    Inclusive11WithComments,
    /// Exclusive Canonical XML 1.0.
    Exclusive,
    /// Exclusive Canonical XML 1.0 with comments.
    ExclusiveWithComments,
}

impl CanonicalizationMode {
    fn to_bergshamra(self) -> C14nMode {
        match self {
            Self::Inclusive => C14nMode::Inclusive,
            Self::InclusiveWithComments => C14nMode::InclusiveWithComments,
            Self::Inclusive11 => C14nMode::Inclusive11,
            Self::Inclusive11WithComments => C14nMode::Inclusive11WithComments,
            Self::Exclusive => C14nMode::Exclusive,
            Self::ExclusiveWithComments => C14nMode::ExclusiveWithComments,
        }
    }
}

/// Canonicalize an XML string using the specified mode.
///
/// Bridge: xot tree -> serialize to string -> bergshamra C14N -> bytes.
///
/// # Errors
///
/// Returns `Error::Canonicalization` if the XML is malformed or C14N fails.
pub fn canonicalize(xml: &str, mode: CanonicalizationMode) -> Result<Vec<u8>, Error> {
    let prefixes: &[String] = &[];
    bergshamra_c14n::canonicalize(xml, mode.to_bergshamra(), None, prefixes)
        .map_err(|e| Error::Canonicalization(e.to_string()))
}

/// Canonicalize an XML string using inclusive C14N and return the raw bytes.
///
/// # Errors
///
/// Returns `Error::Canonicalization` if the XML is malformed or C14N fails.
pub fn canonicalize_str(xml: &str) -> Result<Vec<u8>, Error> {
    canonicalize(xml, CanonicalizationMode::Inclusive)
}

/// Canonicalize an XML string and compute its `ContentHash`.
///
/// # Errors
///
/// Returns `Error::Canonicalization` if the XML is malformed or C14N fails.
pub fn canonicalize_and_hash(xml: &str, mode: CanonicalizationMode) -> Result<ContentHash, Error> {
    let canonical = canonicalize(xml, mode)?;
    Ok(ContentHash::from_canonical(&canonical))
}

// Public API surface (used by ast-grep for structural verification).
#[cfg(any())]
mod _api {
    use super::*;
    pub fn canonicalize(xml: &str, mode: CanonicalizationMode) -> Result<Vec<u8>, Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_XML: &str = r#"<root xmlns:a="urn:a"><a:child>text</a:child></root>"#;

    #[test]
    fn canonicalize_str_deterministic() {
        let c1 = canonicalize_str(SIMPLE_XML).expect("c14n failed");
        let c2 = canonicalize_str(SIMPLE_XML).expect("c14n failed");
        assert_eq!(c1, c2);
    }

    #[test]
    fn inclusive_vs_exclusive_differ_for_namespaced() {
        let inc = canonicalize(SIMPLE_XML, CanonicalizationMode::Inclusive).expect("inc failed");
        let exc = canonicalize(SIMPLE_XML, CanonicalizationMode::Exclusive).expect("exc failed");
        // Inclusive and exclusive produce different output for namespaced XML
        // because exclusive only renders visibly-used namespaces per element
        assert_ne!(inc, exc);
    }

    #[test]
    fn canonicalize_and_hash_returns_valid_hash() {
        let hash = canonicalize_and_hash(SIMPLE_XML, CanonicalizationMode::Inclusive)
            .expect("hash failed");
        let prefixed = hash.to_prefixed();
        assert!(prefixed.starts_with("sha256:"));
        assert_eq!(prefixed.len(), 71);
    }

    #[test]
    fn all_six_modes_dont_panic() {
        let modes = [
            CanonicalizationMode::Inclusive,
            CanonicalizationMode::InclusiveWithComments,
            CanonicalizationMode::Inclusive11,
            CanonicalizationMode::Inclusive11WithComments,
            CanonicalizationMode::Exclusive,
            CanonicalizationMode::ExclusiveWithComments,
        ];
        let xml = "<root>hello</root>";
        for mode in modes {
            let result = canonicalize(xml, mode);
            assert!(result.is_ok(), "mode {mode:?} failed: {result:?}");
        }
    }
}
