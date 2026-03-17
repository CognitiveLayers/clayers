//! All clayers namespace URIs and prefix mappings.

pub const SPEC: &str = "urn:clayers:spec";
pub const INDEX: &str = "urn:clayers:index";
pub const REVISION: &str = "urn:clayers:revision";
pub const PROSE: &str = "urn:clayers:prose";
pub const TERMINOLOGY: &str = "urn:clayers:terminology";
pub const ORGANIZATION: &str = "urn:clayers:organization";
pub const RELATION: &str = "urn:clayers:relation";
pub const DECISION: &str = "urn:clayers:decision";
pub const SOURCE: &str = "urn:clayers:source";
pub const PLAN: &str = "urn:clayers:plan";
pub const ARTIFACT: &str = "urn:clayers:artifact";
pub const LLM: &str = "urn:clayers:llm";
pub const COMBINED: &str = "urn:clayers:combined";

// External standard namespaces (non-layer, used for XMI/UML model integration)
pub const XMI: &str = "http://www.omg.org/spec/XMI/20131001";
pub const UML: &str = "http://www.omg.org/spec/UML/20131001";
pub const XML: &str = "http://www.w3.org/XML/1998/namespace";
pub const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// All 12 layer URN constants (excluding combined).
pub const ALL_LAYERS: &[&str] = &[
    SPEC,
    INDEX,
    REVISION,
    PROSE,
    TERMINOLOGY,
    ORGANIZATION,
    RELATION,
    DECISION,
    SOURCE,
    PLAN,
    ARTIFACT,
    LLM,
];

/// Prefix-to-URI mapping for all namespaces (17 total: 12 layers + combined + 4 external).
pub const PREFIX_MAP: &[(&str, &str)] = &[
    ("spec", SPEC),
    ("idx", INDEX),
    ("rev", REVISION),
    ("pr", PROSE),
    ("trm", TERMINOLOGY),
    ("org", ORGANIZATION),
    ("rel", RELATION),
    ("dec", DECISION),
    ("src", SOURCE),
    ("pln", PLAN),
    ("art", ARTIFACT),
    ("llm", LLM),
    ("cmb", COMBINED),
    ("xmi", XMI),
    ("uml", UML),
    ("xml", XML),
    ("xsi", XSI),
];

/// Get the prefix for a given namespace URI.
#[must_use]
pub fn prefix_for(uri: &str) -> Option<&'static str> {
    PREFIX_MAP.iter().find(|(_, u)| *u == uri).map(|(p, _)| *p)
}

/// Get the URI for a given prefix.
#[must_use]
pub fn uri_for(prefix: &str) -> Option<&'static str> {
    PREFIX_MAP
        .iter()
        .find(|(p, _)| *p == prefix)
        .map(|(_, u)| *u)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn all_layer_urns_are_distinct() {
        let mut seen = HashSet::new();
        for urn in ALL_LAYERS {
            assert!(seen.insert(urn), "duplicate URN: {urn}");
        }
        // Also check COMBINED is distinct from all layers
        assert!(!seen.contains(&COMBINED));
    }

    #[test]
    fn prefix_map_covers_all_layers_plus_combined() {
        let map_uris: HashSet<&str> = PREFIX_MAP.iter().map(|(_, u)| *u).collect();
        for urn in ALL_LAYERS {
            assert!(map_uris.contains(urn), "prefix map missing {urn}");
        }
        assert!(map_uris.contains(COMBINED));
        assert_eq!(PREFIX_MAP.len(), 17);
    }

    #[test]
    fn prefix_for_known_uri() {
        assert_eq!(prefix_for(SPEC), Some("spec"));
        assert_eq!(prefix_for(TERMINOLOGY), Some("trm"));
        assert_eq!(prefix_for(COMBINED), Some("cmb"));
    }

    #[test]
    fn uri_for_known_prefix() {
        assert_eq!(uri_for("spec"), Some(SPEC));
        assert_eq!(uri_for("trm"), Some(TERMINOLOGY));
        assert_eq!(uri_for("cmb"), Some(COMBINED));
    }
}
