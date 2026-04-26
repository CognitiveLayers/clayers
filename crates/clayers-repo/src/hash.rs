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

fn hash_leaf(kind: &[u8], parts: &[&[u8]]) -> ContentHash {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"clayers-repo:leaf:v1\0");
    bytes.extend_from_slice(kind);
    bytes.push(0);
    for part in parts {
        bytes.extend_from_slice(&(part.len() as u64).to_be_bytes());
        bytes.extend_from_slice(part);
    }
    ContentHash::from_canonical(&bytes)
}

/// Hash raw text bytes.
///
/// Text uses the original raw-payload hash domain for repository
/// compatibility. Other leaf node kinds use separate domains so they cannot
/// alias text with the same payload bytes.
#[must_use]
pub fn hash_text(text: &str) -> ContentHash {
    ContentHash::from_canonical(text.as_bytes())
}

/// Hash a comment node with a node-kind domain separator.
#[must_use]
pub fn hash_comment(comment: &str) -> ContentHash {
    hash_leaf(b"comment", &[comment.as_bytes()])
}

/// Hash a processing instruction.
///
/// Processing instructions have no namespace context, so Exclusive and
/// Inclusive hashes are identical. The target and data are hashed in a typed
/// leaf domain so they cannot alias with text or comments.
#[must_use]
pub fn hash_pi(target: &str, data: Option<&str>) -> ContentHash {
    let data = data.unwrap_or("");
    hash_leaf(b"pi", &[target.as_bytes(), data.as_bytes()])
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
    fn text_hash_uses_legacy_raw_payload_domain() {
        assert_eq!(hash_text("hello"), ContentHash::from_canonical(b"hello"));
    }

    #[test]
    fn different_text_different_hash() {
        assert_ne!(hash_text("hello"), hash_text("world"));
    }

    #[test]
    fn text_and_comment_same_payload_do_not_collide() {
        assert_ne!(hash_text("x"), hash_comment("x"));
    }

    #[test]
    fn pi_and_text_same_serialized_payload_do_not_collide() {
        assert_ne!(hash_pi("x", None), hash_text("<?x?>"));
    }

    #[test]
    fn comment_and_pi_same_payload_do_not_collide() {
        assert_ne!(hash_comment("x"), hash_pi("x", None));
    }

    #[test]
    fn pi_target_data_boundaries_are_length_delimited() {
        assert_ne!(hash_pi("ab", Some("c")), hash_pi("a", Some("bc")));
    }

    #[test]
    fn pi_missing_data_matches_empty_data() {
        assert_eq!(hash_pi("target", None), hash_pi("target", Some("")));
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
