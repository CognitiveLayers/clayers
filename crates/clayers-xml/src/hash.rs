use sha2::{Digest, Sha256};
use std::fmt;
use std::str::FromStr;

use crate::Error;

/// A SHA-256 content hash, stored as 32 raw bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    /// Create from raw bytes.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Compute a hash from canonical XML bytes.
    #[must_use]
    pub fn from_canonical(canonical_bytes: &[u8]) -> Self {
        let hash = Sha256::digest(canonical_bytes);
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&hash);
        Self(arr)
    }

    /// Hex representation (64 lowercase hex chars).
    #[must_use]
    pub fn to_hex(&self) -> String {
        use std::fmt::Write;
        self.0.iter().fold(String::with_capacity(64), |mut acc, b| {
            let _ = write!(acc, "{b:02x}");
            acc
        })
    }

    /// Prefixed form: `sha256:<hex>`.
    #[must_use]
    pub fn to_prefixed(&self) -> String {
        format!("sha256:{}", self.to_hex())
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sha256:{}", self.to_hex())
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({})", self.to_hex())
    }
}

impl FromStr for ContentHash {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s.strip_prefix("sha256:").ok_or_else(|| {
            Error::InvalidHashFormat(format!("expected sha256: prefix, got: {s}"))
        })?;

        if hex.len() != 64 {
            return Err(Error::InvalidHashFormat(format!(
                "expected 64 hex chars, got {}",
                hex.len()
            )));
        }

        let mut bytes = [0u8; 32];
        for i in 0..32 {
            bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| {
                Error::InvalidHashFormat(format!("invalid hex at position {}", i * 2))
            })?;
        }

        Ok(Self(bytes))
    }
}

// Public API surface (used by ast-grep for structural verification).
#[cfg(any())]
mod _api {
    use super::*;
    pub fn from_bytes(bytes: [u8; 32]) -> ContentHash;
    pub fn from_canonical(canonical_bytes: &[u8]) -> ContentHash;
    pub fn to_hex(&self) -> String;
    pub fn to_prefixed(&self) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_bytes_to_hex_roundtrip() {
        let bytes = [0xab; 32];
        let hash = ContentHash::from_bytes(bytes);
        assert_eq!(hash.to_hex().len(), 64);
        assert_eq!(hash.0, bytes);
    }

    #[test]
    fn to_prefixed_format() {
        let hash = ContentHash::from_bytes([0; 32]);
        let prefixed = hash.to_prefixed();
        assert!(prefixed.starts_with("sha256:"));
        assert_eq!(prefixed.len(), 7 + 64);
    }

    #[test]
    fn from_canonical_deterministic() {
        let data = b"<root>hello</root>";
        let h1 = ContentHash::from_canonical(data);
        let h2 = ContentHash::from_canonical(data);
        assert_eq!(h1, h2);
    }

    #[test]
    fn from_canonical_different_input_different_hash() {
        let h1 = ContentHash::from_canonical(b"<a/>");
        let h2 = ContentHash::from_canonical(b"<b/>");
        assert_ne!(h1, h2);
    }

    #[test]
    fn display_fromstr_roundtrip() {
        let hash = ContentHash::from_canonical(b"test data");
        let s = hash.to_string();
        let parsed: ContentHash = s.parse().expect("parse failed");
        assert_eq!(hash, parsed);
    }

    #[test]
    fn fromstr_rejects_missing_prefix() {
        let result = "abcd".parse::<ContentHash>();
        assert!(result.is_err());
    }

    #[test]
    fn fromstr_rejects_wrong_length() {
        let result = "sha256:abcd".parse::<ContentHash>();
        assert!(result.is_err());
    }
}
