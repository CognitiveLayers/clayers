//! Hashing functions using Exclusive and Inclusive C14N via `clayers-xml`.
//!
//! All hashing is delegated to `clayers-xml`; there is no direct `sha2`
//! dependency in this crate.

use clayers_xml::{CanonicalizationMode, ContentHash, canonicalize};

use crate::error::Result;

/// Compute both Exclusive (identity) and Inclusive (drift detection) C14N
/// hashes for an XML string representing an element subtree.
///
/// # Errors
///
/// Returns an error if C14N fails (malformed XML).
pub fn hash_element_xml(xml: &str) -> Result<(ContentHash, ContentHash)> {
    let exclusive_bytes = canonicalize(xml, CanonicalizationMode::Exclusive)?;
    let inclusive_bytes = canonicalize(xml, CanonicalizationMode::Inclusive)?;
    Ok((
        ContentHash::from_canonical(&exclusive_bytes),
        ContentHash::from_canonical(&inclusive_bytes),
    ))
}

/// Compute the Exclusive C14N identity hash for an XML string.
///
/// Used for versioning objects (commits, tags, documents) that only need
/// the identity hash.
///
/// # Errors
///
/// Returns an error if C14N fails.
pub fn hash_exclusive(xml: &str) -> Result<ContentHash> {
    let bytes = canonicalize(xml, CanonicalizationMode::Exclusive)?;
    Ok(ContentHash::from_canonical(&bytes))
}

/// Hash raw text bytes (for `TextObject` and `CommentObject`).
///
/// Both identity and inclusive hashes are the same for text content.
#[must_use]
pub fn hash_text(text: &str) -> ContentHash {
    ContentHash::from_canonical(text.as_bytes())
}

/// Hash a processing instruction.
///
/// PIs have no namespace context, so Exclusive and Inclusive hashes are
/// identical. We hash the canonical serialized form.
#[must_use]
pub fn hash_pi(target: &str, data: Option<&str>) -> ContentHash {
    let serialized = match data {
        Some(d) if !d.is_empty() => format!("<?{target} {d}?>"),
        _ => format!("<?{target}?>"),
    };
    ContentHash::from_canonical(serialized.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_hash_deterministic() {
        let h1 = hash_text("hello");
        let h2 = hash_text("hello");
        assert_eq!(h1, h2);
    }

    #[test]
    fn different_text_different_hash() {
        assert_ne!(hash_text("hello"), hash_text("world"));
    }

    #[test]
    fn pi_hash_with_data() {
        let h1 = hash_pi("xml-stylesheet", Some("type=\"text/css\""));
        let h2 = hash_pi("xml-stylesheet", Some("type=\"text/css\""));
        assert_eq!(h1, h2);
    }

    #[test]
    fn pi_hash_without_data() {
        let h = hash_pi("target", None);
        assert_ne!(h, hash_pi("other", None));
    }

    #[test]
    fn element_xml_dual_hash() {
        let xml = "<root xmlns=\"urn:test\"><child>text</child></root>";
        let (exclusive, inclusive) = hash_element_xml(xml).expect("hash failed");
        // Both hashes should be valid (non-zero). For standalone subtrees,
        // Exclusive and Inclusive C14N may produce identical output since
        // there are no inherited ancestor namespaces to differ on.
        assert_ne!(exclusive, ContentHash::from_canonical(b""));
        assert_ne!(inclusive, ContentHash::from_canonical(b""));
    }

    #[test]
    fn exclusive_hash_consistent() {
        let xml = "<root>hello</root>";
        let h1 = hash_exclusive(xml).expect("hash failed");
        let h2 = hash_exclusive(xml).expect("hash failed");
        assert_eq!(h1, h2);
    }
}
