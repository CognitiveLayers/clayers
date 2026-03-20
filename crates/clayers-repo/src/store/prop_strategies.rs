//! Proptest `Strategy` implementations for property-based testing.
//!
//! Provides strategies for every object type in the Merkle DAG, plus
//! ref names, XML document generation, DAG topologies, and operation
//! sequences.

#![allow(clippy::needless_pass_by_value)]
#![allow(dead_code, unused_imports)]

use std::fmt::Write as _;

use chrono::{DateTime, TimeZone, Utc};
use clayers_xml::ContentHash;
use proptest::prelude::*;
use proptest::collection::vec as pvec;

use crate::object::{
    Attribute, Author, CommitObject, CommentObject, DocumentObject, ElementObject,
    Object, PIObject, TagObject, TextObject, TreeEntry, TreeObject,
};

// ---------------------------------------------------------------------------
// Primitive strategies
// ---------------------------------------------------------------------------

/// Arbitrary `ContentHash` via `from_canonical` on 1..64 random bytes.
pub(crate) fn arb_content_hash() -> impl Strategy<Value = ContentHash> {
    pvec(any::<u8>(), 1..64).prop_map(|bytes| ContentHash::from_canonical(&bytes))
}

/// Arbitrary `ContentHash` via `from_bytes` on a uniform 32-byte array.
pub(crate) fn arb_content_hash_raw() -> impl Strategy<Value = ContentHash> {
    proptest::array::uniform32(any::<u8>()).prop_map(ContentHash::from_bytes)
}

/// Arbitrary `Author` with a 1-20 character alphabetic name and a simple email.
pub(crate) fn arb_author() -> impl Strategy<Value = Author> {
    (
        "[a-zA-Z]{1,20}",
        "[a-z]{1,8}",
        "[a-z]{1,6}",
    )
        .prop_map(|(name, user, domain)| Author {
            name,
            email: format!("{user}@{domain}.com"),
        })
}

/// Arbitrary `DateTime<Utc>` in the 2020-2030 range.
pub(crate) fn arb_timestamp() -> impl Strategy<Value = DateTime<Utc>> {
    // 2020-01-01T00:00:00Z .. 2030-01-01T00:00:00Z
    (1_577_836_800_i64..1_893_456_000_i64).prop_map(|secs| {
        Utc.timestamp_opt(secs, 0)
            .single()
            .expect("timestamp in valid range")
    })
}

/// Create a single-threaded Tokio runtime for use in proptest closures.
pub(crate) fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().expect("failed to build tokio runtime")
}

// ---------------------------------------------------------------------------
// Object strategies
// ---------------------------------------------------------------------------

/// Arbitrary `TextObject` (any string, including Unicode).
pub(crate) fn arb_text_object() -> impl Strategy<Value = TextObject> {
    ".*".prop_map(|content| TextObject { content })
}

/// Arbitrary `CommentObject`. XML comments must not contain `--`.
pub(crate) fn arb_comment_object() -> impl Strategy<Value = CommentObject> {
    "[^-]{0,50}(-[^-][^-]{0,10}){0,5}"
        .prop_map(|content| CommentObject { content })
}

/// Arbitrary `PIObject` with a valid PI target (letter start, no colons)
/// and optional data.
pub(crate) fn arb_pi_object() -> impl Strategy<Value = PIObject> {
    (
        "[a-zA-Z][a-zA-Z0-9._]{0,15}",
        proptest::option::of(".{0,60}"),
    )
        .prop_map(|(target, data)| PIObject { target, data })
}

/// Arbitrary `Attribute` with a valid XML local name, optional namespace,
/// and a string value.
pub(crate) fn arb_attribute() -> impl Strategy<Value = Attribute> {
    (
        "[a-zA-Z_][a-zA-Z0-9_]{0,10}",
        proptest::option::of("urn:[a-z]{2,8}:[a-z]{2,8}"),
        proptest::option::of("[a-z]{1,5}"),
        ".{0,30}",
    )
        .prop_map(|(local_name, namespace_uri, namespace_prefix, value)| Attribute {
            local_name,
            namespace_uri,
            namespace_prefix,
            value,
        })
}

/// Arbitrary `ElementObject` with 0-5 attributes, 0-3 random child hashes,
/// and an inclusive hash.
pub(crate) fn arb_element_object() -> impl Strategy<Value = ElementObject> {
    (
        "[a-zA-Z][a-zA-Z0-9]{0,10}",
        proptest::option::of("urn:[a-z]{2,8}:[a-z]{2,8}"),
        proptest::option::of("[a-z]{1,5}"),
        pvec(
            ("[a-z]{1,4}", "urn:[a-z]{2,6}:[a-z]{2,6}")
                .prop_map(|(prefix, uri)| (prefix, uri)),
            0..3,
        ),
        pvec(arb_attribute(), 0..5),
        pvec(arb_content_hash(), 0..3),
        arb_content_hash(),
    )
        .prop_map(
            |(local_name, namespace_uri, namespace_prefix, extra_namespaces, attributes, children, inclusive_hash)| {
                ElementObject {
                    local_name,
                    namespace_uri,
                    namespace_prefix,
                    extra_namespaces,
                    attributes,
                    children,
                    inclusive_hash,
                }
            },
        )
}

/// Arbitrary `DocumentObject` with a root hash and 0-3 prologue hashes.
pub(crate) fn arb_document_object() -> impl Strategy<Value = DocumentObject> {
    (arb_content_hash(), pvec(arb_content_hash(), 0..3))
        .prop_map(|(root, prologue)| DocumentObject { root, prologue })
}

/// Arbitrary `TreeObject` with 0-10 entries having unique paths.
pub(crate) fn arb_tree_object() -> impl Strategy<Value = TreeObject> {
    let paths: Vec<&str> = vec![
        "a.xml", "b.xml", "c.xml", "d.xml", "e.xml",
        "f.xml", "g.xml", "h.xml", "i.xml", "j.xml",
    ];
    (0..=10_usize, pvec(arb_content_hash(), 10))
        .prop_map(move |(count, hashes)| {
            let count = count.min(paths.len());
            let entries: Vec<TreeEntry> = paths[..count]
                .iter()
                .zip(hashes.iter())
                .map(|(path, hash)| TreeEntry {
                    path: (*path).to_string(),
                    document: *hash,
                })
                .collect();
            TreeObject::new(entries)
        })
}

/// Arbitrary `CommitObject` with a tree hash, 0-3 parents, author,
/// timestamp, and message.
pub(crate) fn arb_commit_object() -> impl Strategy<Value = CommitObject> {
    (
        arb_content_hash(),
        pvec(arb_content_hash(), 0..3),
        arb_author(),
        arb_timestamp(),
        ".{0,80}",
    )
        .prop_map(|(tree, parents, author, timestamp, message)| CommitObject {
            tree,
            parents,
            author,
            timestamp,
            message,
        })
}

/// Arbitrary `TagObject` with target, name, tagger, timestamp, and message.
pub(crate) fn arb_tag_object() -> impl Strategy<Value = TagObject> {
    (
        arb_content_hash(),
        "[a-zA-Z0-9._-]{1,20}",
        arb_author(),
        arb_timestamp(),
        ".{0,80}",
    )
        .prop_map(|(target, name, tagger, timestamp, message)| TagObject {
            target,
            name,
            tagger,
            timestamp,
            message,
        })
}

/// Arbitrary `Object` chosen uniformly from all 8 variants, paired with a
/// `ContentHash`.
pub(crate) fn arb_object() -> BoxedStrategy<(ContentHash, Object)> {
    prop_oneof![
        arb_element_object().prop_map(Object::Element),
        arb_text_object().prop_map(Object::Text),
        arb_comment_object().prop_map(Object::Comment),
        arb_pi_object().prop_map(Object::PI),
        arb_document_object().prop_map(Object::Document),
        arb_tree_object().prop_map(Object::Tree),
        arb_commit_object().prop_map(Object::Commit),
        arb_tag_object().prop_map(Object::Tag),
    ]
    .prop_flat_map(|obj| arb_content_hash().prop_map(move |h| (h, obj.clone())))
    .boxed()
}

// ---------------------------------------------------------------------------
// Ref name strategies
// ---------------------------------------------------------------------------

/// Realistic ref names: branches, tags, and HEAD.
pub(crate) fn arb_ref_name() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z]{1,10}".prop_map(|name| format!("refs/heads/{name}")),
        "[0-9]{1,2}\\.[0-9]{1,2}".prop_map(|ver| format!("refs/tags/v{ver}")),
        Just("HEAD".to_string()),
    ]
}

/// Adversarial ref names containing SQL LIKE wildcards (`%`, `_`).
pub(crate) fn arb_ref_name_adversarial() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z]{1,5}".prop_map(|name| format!("refs/heads/feat%{name}")),
        "[a-z]{1,5}".prop_map(|name| format!("refs/heads/my_{name}")),
        "[a-z]{1,5}".prop_map(|name| format!("refs/tags/%{name}%")),
        "[a-z]{1,3}_[a-z]{1,3}".prop_map(|name| format!("refs/heads/{name}")),
    ]
}

/// An adversarial ref scenario: an adversarial prefix containing `%` or `_`,
/// plus a decoy ref that `SQLite` LIKE would match but `starts_with` would not.
///
/// Example: prefix=`"refs/heads/f%x"`, decoy=`"refs/heads/foox"`
/// LIKE `'refs/heads/f%x%'` matches `"refs/heads/foox"` (% expands to "oo")
/// but `starts_with` returns false.
#[derive(Debug, Clone)]
pub(crate) struct AdversarialRefScenario {
    /// The adversarial ref name (contains % or _)
    pub adversarial_name: String,
    /// A decoy ref that LIKE would match but `starts_with` would not
    pub decoy_name: String,
}

pub(crate) fn arb_adversarial_ref_scenario() -> impl Strategy<Value = AdversarialRefScenario> {
    // Generate scenarios where LIKE's wildcard interpretation causes false matches.
    // Pattern: "refs/heads/<before>%<after>" with decoy "refs/heads/<before><filler><after>"
    (
        "[a-z]{1,3}",  // before
        "[a-z]{1,3}",  // after
        "[a-z]{1,5}",  // filler (what % would expand to in LIKE)
    )
        .prop_map(|(before, after, filler)| {
            let adversarial_name = format!("refs/heads/{before}%{after}");
            // This decoy does NOT start with "refs/heads/<before>%<after>"
            // but LIKE 'refs/heads/<before>%<after>%' WOULD match it
            // because the first % expands to <filler>.
            let decoy_name = format!("refs/heads/{before}{filler}{after}");
            AdversarialRefScenario {
                adversarial_name,
                decoy_name,
            }
        })
}

// ---------------------------------------------------------------------------
// XML generation strategy
// ---------------------------------------------------------------------------

/// A generated XML attribute for serialization.
#[derive(Debug, Clone)]
struct XmlAttr {
    name: String,
    prefix: Option<String>,
    ns_uri: Option<String>,
    value: String,
}

/// A generated XML node tree for serialization.
#[derive(Debug, Clone)]
enum XmlNode {
    Element {
        name: String,
        prefix: Option<String>,
        ns_uri: Option<String>,
        extra_ns: Vec<(String, String)>,
        attrs: Vec<XmlAttr>,
        children: Vec<XmlNode>,
    },
    Text(String),
    Comment(String),
    PI {
        target: String,
        data: Option<String>,
    },
}

impl XmlNode {
    fn to_xml_string(&self) -> String {
        let mut buf = String::new();
        self.write_to(&mut buf);
        buf
    }

    fn write_to(&self, buf: &mut String) {
        match self {
            XmlNode::Text(text) => {
                buf.push_str(&xml_escape_text(text));
            }
            XmlNode::Comment(text) => {
                buf.push_str("<!--");
                buf.push_str(text);
                buf.push_str("-->");
            }
            XmlNode::PI { target, data } => {
                buf.push_str("<?");
                buf.push_str(target);
                if let Some(d) = data {
                    buf.push(' ');
                    buf.push_str(d);
                }
                buf.push_str("?>");
            }
            XmlNode::Element {
                name,
                prefix,
                ns_uri,
                extra_ns,
                attrs,
                children,
            } => {
                buf.push('<');
                let qualified = match prefix {
                    Some(p) => format!("{p}:{name}"),
                    None => name.clone(),
                };
                buf.push_str(&qualified);

                // Namespace declarations
                if let Some(uri) = ns_uri {
                    match prefix {
                        Some(p) => {
                            let _ = write!(buf, " xmlns:{p}=\"{}\"", xml_escape_attr(uri));
                        }
                        None => {
                            let _ = write!(buf, " xmlns=\"{}\"", xml_escape_attr(uri));
                        }
                    }
                }
                for (pfx, uri) in extra_ns {
                    if pfx.is_empty() {
                        let _ = write!(buf, " xmlns=\"{}\"", xml_escape_attr(uri));
                    } else {
                        let _ = write!(buf, " xmlns:{pfx}=\"{}\"", xml_escape_attr(uri));
                    }
                }

                // Attributes
                for attr in attrs {
                    let attr_qualified = match &attr.prefix {
                        Some(p) => format!("{p}:{}", attr.name),
                        None => attr.name.clone(),
                    };
                    let _ = write!(
                        buf,
                        " {attr_qualified}=\"{}\"",
                        xml_escape_attr(&attr.value)
                    );
                }

                if children.is_empty() {
                    buf.push_str("/>");
                } else {
                    buf.push('>');
                    for child in children {
                        child.write_to(buf);
                    }
                    let _ = write!(buf, "</{qualified}>");
                }
            }
        }
    }
}

fn xml_escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn xml_escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A shared namespace pool for generating XML with real inheritance patterns.
/// Elements pick from this pool so parent/child share URIs.
const NS_POOL: &[(&str, &str)] = &[
    ("pr", "urn:clayers:prose"),
    ("trm", "urn:clayers:terminology"),
    ("spec", "urn:clayers:spec"),
    ("app", "urn:example:app"),
];

/// A default namespace URI for testing xmlns="..." declarations.
const DEFAULT_NS: &str = "urn:clayers:default";

/// An XML generation scenario that controls which hard pattern to exercise.
#[derive(Debug, Clone, Copy)]
enum XmlScenario {
    /// Simple: no namespaces, just nested elements with text
    Plain,
    /// Root declares prefixed namespaces; children inherit and use them
    InheritedPrefixes,
    /// Root declares default xmlns; children inherit or cancel with xmlns=""
    DefaultNsCancellation,
    /// Attributes from multiple namespaces on the same element
    MultiNsAttributes,
    /// Mixed content: text + elements + comments + PIs interleaved
    MixedContent,
    /// Clayers-style: root with spec: prefix, children with pr: and trm:
    ClayersPattern,
}

fn arb_xml_scenario() -> impl Strategy<Value = XmlScenario> {
    prop_oneof![
        Just(XmlScenario::Plain),
        Just(XmlScenario::InheritedPrefixes),
        Just(XmlScenario::DefaultNsCancellation),
        Just(XmlScenario::MultiNsAttributes),
        Just(XmlScenario::MixedContent),
        Just(XmlScenario::ClayersPattern),
    ]
}

fn arb_xml_attr_from_pool() -> impl Strategy<Value = XmlAttr> {
    (
        "[a-z][a-z0-9]{0,6}",
        proptest::option::of(0..NS_POOL.len()),
        "[a-zA-Z0-9 ]{0,20}",
    )
        .prop_map(|(name, ns_idx, value)| {
            let (prefix, ns_uri) = match ns_idx {
                Some(i) => (
                    Some(NS_POOL[i].0.to_string()),
                    Some(NS_POOL[i].1.to_string()),
                ),
                None => (None, None),
            };
            XmlAttr {
                name,
                prefix,
                ns_uri,
                value,
            }
        })
}

fn arb_xml_leaf() -> BoxedStrategy<XmlNode> {
    prop_oneof![
        3 => "[a-zA-Z0-9 .,!?]{1,30}".prop_map(XmlNode::Text),
        1 => "[a-zA-Z0-9 ]{1,20}".prop_map(XmlNode::Comment),
        1 => (
            "[a-zA-Z][a-zA-Z0-9]{0,8}",
            proptest::option::of("[a-zA-Z0-9 =]{1,15}")
        )
            .prop_map(|(target, data)| XmlNode::PI { target, data }),
    ]
    .boxed()
}

/// Build a child element that optionally inherits or cancels parent's namespace.
fn arb_xml_child(depth: u32, parent_has_default_ns: bool) -> BoxedStrategy<XmlNode> {
    if depth == 0 {
        return arb_xml_leaf();
    }
    let children = pvec(arb_xml_child(depth - 1, parent_has_default_ns), 0..4);
    (
        "[a-z][a-z0-9]{0,6}",
        // Pick from the NS pool for prefix
        proptest::option::of(0..NS_POOL.len()),
        // Cancel parent's default namespace?
        proptest::bool::weighted(if parent_has_default_ns { 0.4 } else { 0.0 }),
        pvec(arb_xml_attr_from_pool(), 0..3),
        children,
    )
        .prop_map(move |(name, ns_idx, cancel_ns, attrs, children)| {
            let (prefix, ns_uri) = match ns_idx {
                Some(i) => (
                    Some(NS_POOL[i].0.to_string()),
                    Some(NS_POOL[i].1.to_string()),
                ),
                None => (None, None),
            };
            let extra_ns = if cancel_ns && prefix.is_none() && ns_uri.is_none() {
                // xmlns="" cancels inherited default namespace
                vec![(String::new(), String::new())]
            } else {
                vec![]
            };
            XmlNode::Element {
                name,
                prefix,
                ns_uri,
                extra_ns,
                attrs,
                children,
            }
        })
        .boxed()
}

/// Build a root element according to a scenario.
#[allow(clippy::too_many_lines)]
fn arb_xml_root(scenario: XmlScenario, depth: u32) -> BoxedStrategy<XmlNode> {
    match scenario {
        XmlScenario::Plain => {
            let children = pvec(arb_xml_child(depth.saturating_sub(1), false), 1..4);
            ("[a-z][a-z0-9]{0,6}", children)
                .prop_map(|(name, children)| XmlNode::Element {
                    name,
                    prefix: None,
                    ns_uri: None,
                    extra_ns: vec![],
                    attrs: vec![],
                    children,
                })
                .boxed()
        }
        XmlScenario::InheritedPrefixes => {
            // Root declares 2-3 prefixed namespaces; children inherit them.
            let children = pvec(arb_xml_child(depth.saturating_sub(1), false), 1..4);
            (
                "[a-z][a-z0-9]{0,6}",
                0..NS_POOL.len(),
                pvec(0..NS_POOL.len(), 1..3),
                children,
            )
                .prop_map(|(name, root_ns_idx, extra_idxs, children)| {
                    let extra_ns: Vec<(String, String)> = extra_idxs
                        .iter()
                        .filter(|&&i| i != root_ns_idx)
                        .map(|&i| (NS_POOL[i].0.to_string(), NS_POOL[i].1.to_string()))
                        .collect();
                    XmlNode::Element {
                        name,
                        prefix: Some(NS_POOL[root_ns_idx].0.to_string()),
                        ns_uri: Some(NS_POOL[root_ns_idx].1.to_string()),
                        extra_ns,
                        attrs: vec![],
                        children,
                    }
                })
                .boxed()
        }
        XmlScenario::DefaultNsCancellation => {
            // Root declares xmlns="urn:...", children cancel with xmlns=""
            let children = pvec(arb_xml_child(depth.saturating_sub(1), true), 1..4);
            ("[a-z][a-z0-9]{0,6}", children)
                .prop_map(|(name, children)| XmlNode::Element {
                    name,
                    prefix: None,
                    ns_uri: Some(DEFAULT_NS.to_string()),
                    extra_ns: vec![],
                    attrs: vec![],
                    children,
                })
                .boxed()
        }
        XmlScenario::MultiNsAttributes => {
            // Element with attributes from 2+ different namespaces.
            let children = pvec(arb_xml_child(depth.saturating_sub(1), false), 0..3);
            (
                "[a-z][a-z0-9]{0,6}",
                pvec(arb_xml_attr_from_pool(), 2..5),
                children,
            )
                .prop_map(|(name, attrs, children)| {
                    // Collect all namespace prefixes used by attrs so root declares them
                    let extra_ns: Vec<(String, String)> = attrs
                        .iter()
                        .filter_map(|a| {
                            a.prefix
                                .as_ref()
                                .map(|p| (p.clone(), a.ns_uri.clone().unwrap_or_default()))
                        })
                        .collect::<std::collections::HashSet<_>>()
                        .into_iter()
                        .collect();
                    XmlNode::Element {
                        name,
                        prefix: None,
                        ns_uri: None,
                        extra_ns,
                        attrs,
                        children,
                    }
                })
                .boxed()
        }
        XmlScenario::MixedContent => {
            // Interleaved text + elements + comments + PIs
            let child = prop_oneof![
                3 => "[a-zA-Z0-9 ]{1,20}".prop_map(XmlNode::Text),
                2 => arb_xml_child(depth.saturating_sub(1), false),
                1 => "[a-zA-Z0-9 ]{1,15}".prop_map(XmlNode::Comment),
                1 => "[a-zA-Z][a-zA-Z0-9]{0,6}"
                    .prop_map(|t| XmlNode::PI { target: t, data: None }),
            ];
            ("[a-z][a-z0-9]{0,6}", pvec(child, 2..6))
                .prop_map(|(name, children)| XmlNode::Element {
                    name,
                    prefix: None,
                    ns_uri: None,
                    extra_ns: vec![],
                    attrs: vec![],
                    children,
                })
                .boxed()
        }
        XmlScenario::ClayersPattern => {
            // Mimics real clayers specs: spec:clayers root with pr: and trm: children
            let child = prop_oneof![
                arb_xml_child(depth.saturating_sub(1), false),
                "[a-zA-Z0-9 ]{1,30}".prop_map(XmlNode::Text),
            ];
            (
                pvec(child, 1..5),
                "[a-z][a-z0-9]{0,6}", // spec:index attr value
            )
                .prop_map(|(children, idx_val)| XmlNode::Element {
                    name: "clayers".to_string(),
                    prefix: Some("spec".to_string()),
                    ns_uri: Some("urn:clayers:spec".to_string()),
                    extra_ns: vec![
                        ("pr".to_string(), "urn:clayers:prose".to_string()),
                        ("trm".to_string(), "urn:clayers:terminology".to_string()),
                    ],
                    attrs: vec![XmlAttr {
                        name: "index".to_string(),
                        prefix: Some("spec".to_string()),
                        ns_uri: Some("urn:clayers:spec".to_string()),
                        value: format!("{idx_val}.xml"),
                    }],
                    children,
                })
                .boxed()
        }
    }
}

/// Arbitrary well-formed XML document string with rich namespace patterns.
///
/// Uses a scenario-based approach: each scenario targets a specific pattern
/// that historically caused bugs (namespace inheritance, cancellation,
/// multi-namespace attributes, mixed content, clayers-style documents).
pub(crate) fn arb_xml_document() -> impl Strategy<Value = String> {
    (
        arb_xml_scenario(),
        1..=4_u32,
        proptest::option::of(prop_oneof![
            "[a-zA-Z0-9 ]{1,20}".prop_map(|c| format!("<!--{c}-->")),
            "[a-zA-Z][a-zA-Z0-9]{0,8}"
                .prop_map(|t| format!("<?{t}?>")),
        ]),
    )
        .prop_flat_map(|(scenario, depth, prologue)| {
            arb_xml_root(scenario, depth).prop_map(move |root| {
                let mut doc = String::new();
                if let Some(ref p) = prologue {
                    doc.push_str(p);
                }
                doc.push_str(&root.to_xml_string());
                doc
            })
        })
}

// ---------------------------------------------------------------------------
// DAG topology strategies
// ---------------------------------------------------------------------------

/// Generate a DAG of objects forming a valid document tree.
///
/// Returns `(objects, document_hash)` where objects is a `Vec<(ContentHash, Object)>`
/// suitable for insertion into a store.
pub(crate) fn arb_object_dag() -> impl Strategy<Value = (Vec<(ContentHash, Object)>, ContentHash)> {
    // 1-5 leaf objects, then 1-3 elements referencing them, then a document.
    (
        pvec(
            prop_oneof![
                arb_text_object().prop_map(Object::Text),
                arb_comment_object().prop_map(Object::Comment),
                arb_pi_object().prop_map(Object::PI),
            ],
            1..=5,
        ),
        pvec(arb_content_hash(), 5),
        pvec(arb_content_hash(), 3),
        arb_content_hash(),
        arb_content_hash(),
        // For element local names
        pvec("[a-z]{1,6}", 3),
    )
        .prop_map(|(leaf_objs, leaf_hashes, elem_hashes, root_elem_hash, doc_hash, elem_names)| {
            let mut objects: Vec<(ContentHash, Object)> = Vec::new();

            // Store leaf objects
            let leaf_count = leaf_objs.len();
            for (i, obj) in leaf_objs.into_iter().enumerate() {
                objects.push((leaf_hashes[i], obj));
            }

            // Build 1-3 element objects referencing leaf hashes
            let mut element_hashes_used = Vec::new();
            let elems_to_make = elem_hashes.len().min(3);
            for i in 0..elems_to_make {
                // Each element gets a slice of the leaf hashes as children
                let start = (i * leaf_count) / elems_to_make;
                let end = ((i + 1) * leaf_count) / elems_to_make;
                let children: Vec<ContentHash> = (start..end)
                    .map(|j| leaf_hashes[j])
                    .collect();

                let elem = ElementObject {
                    local_name: elem_names[i].clone(),
                    namespace_uri: None,
                    namespace_prefix: None,
                    extra_namespaces: vec![],
                    attributes: vec![],
                    children,
                    inclusive_hash: elem_hashes[i],
                };
                objects.push((elem_hashes[i], Object::Element(elem)));
                element_hashes_used.push(elem_hashes[i]);
            }

            // Build a root element that references all inner elements
            let root_elem = ElementObject {
                local_name: "root".to_string(),
                namespace_uri: None,
                namespace_prefix: None,
                extra_namespaces: vec![],
                attributes: vec![],
                children: element_hashes_used,
                inclusive_hash: root_elem_hash,
            };
            objects.push((root_elem_hash, Object::Element(root_elem)));

            // Wrap in a document
            let doc = DocumentObject {
                root: root_elem_hash,
                prologue: vec![],
            };
            objects.push((doc_hash, Object::Document(doc)));

            (objects, doc_hash)
        })
}

/// Generate a DAG with commits, trees, and documents.
///
/// Returns `(objects, tip_commit_hash)`.
pub(crate) fn arb_commit_dag()
    -> impl Strategy<Value = (Vec<(ContentHash, Object)>, ContentHash)>
{
    (
        // 1-3 document DAGs
        pvec(arb_object_dag(), 1..=3),
        // Hashes for trees and commits
        pvec(arb_content_hash(), 6),
        arb_author(),
        arb_timestamp(),
        pvec(".{1,30}", 3),
    )
        .prop_map(|(doc_dags, extra_hashes, author, timestamp, messages)| {
            let mut objects: Vec<(ContentHash, Object)> = Vec::new();
            let mut doc_hash_to_path: Vec<(String, ContentHash)> = Vec::new();

            // Collect all document objects
            for (i, (dag_objects, doc_hash)) in doc_dags.into_iter().enumerate() {
                for obj in dag_objects {
                    objects.push(obj);
                }
                doc_hash_to_path.push((format!("doc{i}.xml"), doc_hash));
            }

            // Build a tree
            let tree_hash = extra_hashes[0];
            let entries: Vec<TreeEntry> = doc_hash_to_path
                .iter()
                .map(|(path, hash)| TreeEntry {
                    path: path.clone(),
                    document: *hash,
                })
                .collect();
            let tree = TreeObject::new(entries);
            objects.push((tree_hash, Object::Tree(tree)));

            // Build 1-3 commits in a chain
            let commit1_hash = extra_hashes[1];
            let commit1 = CommitObject {
                tree: tree_hash,
                parents: vec![],
                author: author.clone(),
                timestamp,
                message: messages[0].clone(),
            };
            objects.push((commit1_hash, Object::Commit(commit1)));

            let commit2_hash = extra_hashes[2];
            let commit2 = CommitObject {
                tree: tree_hash,
                parents: vec![commit1_hash],
                author: author.clone(),
                timestamp,
                message: messages[1].clone(),
            };
            objects.push((commit2_hash, Object::Commit(commit2)));

            // Third commit merges (has two parents to test branching)
            let commit3_hash = extra_hashes[3];
            let commit3 = CommitObject {
                tree: tree_hash,
                parents: vec![commit2_hash, commit1_hash],
                author,
                timestamp,
                message: messages[2].clone(),
            };
            objects.push((commit3_hash, Object::Commit(commit3)));

            (objects, commit3_hash)
        })
}

// ---------------------------------------------------------------------------
// Operation sequence strategy
// ---------------------------------------------------------------------------

/// A single store operation for sequential testing.
#[derive(Debug, Clone)]
pub(crate) enum StoreOp {
    Put(ContentHash, Object),
    CommitTx,
    RollbackTx,
    SetRef(String, ContentHash),
    DeleteRef(String),
    CasRef(String, Option<ContentHash>, ContentHash),
    /// Verify `list_refs` returns the expected set for a given prefix.
    ListRefs(String),
}

/// Arbitrary single store operation.
pub(crate) fn arb_store_op() -> BoxedStrategy<StoreOp> {
    prop_oneof![
        4 => arb_object().prop_map(|(h, o)| StoreOp::Put(h, o)),
        2 => Just(StoreOp::CommitTx),
        1 => Just(StoreOp::RollbackTx),
        2 => (arb_ref_name(), arb_content_hash())
            .prop_map(|(name, hash)| StoreOp::SetRef(name, hash)),
        1 => arb_ref_name().prop_map(StoreOp::DeleteRef),
        2 => (
            arb_ref_name(),
            proptest::option::of(arb_content_hash()),
            arb_content_hash(),
        )
            .prop_map(|(name, expected, new)| StoreOp::CasRef(name, expected, new)),
        1 => prop_oneof![
            Just("refs/heads/".to_string()),
            Just("refs/tags/".to_string()),
            Just(String::new()),
        ].prop_map(StoreOp::ListRefs),
    ]
    .boxed()
}

/// Arbitrary sequence of 10..50 store operations.
pub(crate) fn arb_op_sequence() -> impl Strategy<Value = Vec<StoreOp>> {
    pvec(arb_store_op(), 10..50)
}
