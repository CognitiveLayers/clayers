//! Pure XML diff engine.
//!
//! Compares two XML strings and produces a list of changes with XPath-like
//! location paths. No store dependency – works entirely on parsed XML trees.

use std::fmt;

use xot::Xot;

use crate::error::Error;

/// An XPath-like path to a node in an XML document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlPath {
    /// Path segments, e.g. `["root", "section[2]", "title"]`.
    pub segments: Vec<String>,
}

#[cfg(feature = "serde")]
impl serde::Serialize for XmlPath {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl XmlPath {
    fn child(&self, segment: &str) -> Self {
        let mut p = self.clone();
        p.segments.push(segment.to_string());
        p
    }
}

impl fmt::Display for XmlPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.segments.is_empty() {
            write!(f, "/")
        } else {
            for seg in &self.segments {
                write!(f, "/{seg}")?;
            }
            Ok(())
        }
    }
}

/// A single change between two XML documents.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "snake_case"))]
pub enum XmlChange {
    /// An element was added.
    ElementAdded {
        /// XPath-like location.
        path: XmlPath,
        /// Serialized XML content of the added element.
        content: String,
    },
    /// An element was removed.
    ElementRemoved {
        /// XPath-like location.
        path: XmlPath,
        /// Serialized XML content of the removed element.
        content: String,
    },
    /// An attribute value changed (added, removed, or modified).
    AttributeChanged {
        /// XPath-like location of the owning element.
        path: XmlPath,
        /// Attribute name.
        name: String,
        /// Old value (`None` if attribute was added).
        old: Option<String>,
        /// New value (`None` if attribute was removed).
        new: Option<String>,
    },
    /// Text content changed.
    TextChanged {
        /// XPath-like location of the parent element.
        path: XmlPath,
        /// Old text.
        old: String,
        /// New text.
        new: String,
    },
    /// Comment content changed.
    CommentChanged {
        /// XPath-like location of the parent element.
        path: XmlPath,
        /// Old comment text.
        old: String,
        /// New comment text.
        new: String,
    },
}

impl fmt::Display for XmlChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ElementAdded { path, .. } => write!(f, "  + {path}"),
            Self::ElementRemoved { path, .. } => write!(f, "  - {path}"),
            Self::AttributeChanged {
                path,
                name,
                old,
                new,
            } => {
                write!(f, "  ~ {path}/@{name}")?;
                match (old, new) {
                    (Some(o), Some(n)) => write!(f, ": \"{o}\" -> \"{n}\""),
                    (None, Some(n)) => write!(f, ": (added) \"{n}\""),
                    (Some(o), None) => write!(f, ": \"{o}\" (removed)"),
                    (None, None) => Ok(()),
                }
            }
            Self::TextChanged { path, old, new } => {
                writeln!(f, "  ~ {path}")?;
                write!(f, "    text: \"{old}\" -> \"{new}\"")
            }
            Self::CommentChanged { path, old, new } => {
                writeln!(f, "  ~ {path}")?;
                write!(f, "    comment: \"{old}\" -> \"{new}\"")
            }
        }
    }
}

/// The result of diffing two XML documents.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct XmlDiff {
    /// The individual changes.
    pub changes: Vec<XmlChange>,
}

impl XmlDiff {
    /// True if there are no changes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

impl fmt::Display for XmlDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for change in &self.changes {
            writeln!(f, "{change}")?;
        }
        Ok(())
    }
}

/// Context passed through the recursive diff to avoid repeated lookups.
struct DiffCtx {
    id_name: xot::NameId,
}

/// Diff two XML strings and produce a list of changes with XPath-like paths.
///
/// Both strings must be well-formed XML documents. The diff is position-based:
/// children at the same position are compared pairwise.
///
/// # Errors
///
/// Returns an error if either XML string cannot be parsed.
pub fn diff_xml(old: &str, new: &str) -> Result<XmlDiff, Error> {
    let mut xot = Xot::new();
    let id_name = xot.add_name("id");

    let doc_old = xot
        .parse(old)
        .map_err(|e| Error::XmlParse(e.to_string()))?;
    let doc_new = xot
        .parse(new)
        .map_err(|e| Error::XmlParse(e.to_string()))?;

    let root_old = xot
        .document_element(doc_old)
        .map_err(|e| Error::XmlParse(e.to_string()))?;
    let root_new = xot
        .document_element(doc_new)
        .map_err(|e| Error::XmlParse(e.to_string()))?;

    let ctx = DiffCtx { id_name };
    let mut changes = Vec::new();

    let old_name = xot.element(root_old).map(xot::Element::name);
    let new_name = xot.element(root_new).map(xot::Element::name);

    let seg = element_segment(&xot, root_new, &ctx);
    let path = XmlPath {
        segments: vec![seg],
    };

    if old_name == new_name {
        diff_elements(&xot, root_old, root_new, &path, &ctx, &mut changes);
    } else {
        changes.push(XmlChange::ElementRemoved {
            path: path.clone(),
            content: xot.to_string(root_old).unwrap_or_default(),
        });
        changes.push(XmlChange::ElementAdded {
            path,
            content: xot.to_string(root_new).unwrap_or_default(),
        });
    }

    Ok(XmlDiff { changes })
}

/// Get the path segment for an element (local name, optionally with `@id`).
fn element_segment(xot: &Xot, node: xot::Node, ctx: &DiffCtx) -> String {
    let Some(el) = xot.element(node) else {
        return "?".to_string();
    };
    let (local, _) = xot.name_ns_str(el.name());

    if let Some(id_val) = xot.get_attribute(node, ctx.id_name) {
        format!("{local}[@id=\"{id_val}\"]")
    } else {
        local.to_string()
    }
}

/// Build a path segment for a child element, with `@id` or positional `[N]`.
fn child_element_segment(
    xot: &Xot,
    parent: xot::Node,
    child: xot::Node,
    ctx: &DiffCtx,
) -> String {
    let Some(child_el) = xot.element(child) else {
        return "text()".to_string();
    };
    let child_name = child_el.name();
    let (local, _) = xot.name_ns_str(child_name);

    // Prefer @id if present.
    if let Some(id_val) = xot.get_attribute(child, ctx.id_name) {
        return format!("{local}[@id=\"{id_val}\"]");
    }

    // Count same-name siblings and find position.
    let mut count = 0usize;
    let mut position = 0usize;
    for sib in xot.children(parent) {
        if let Some(sib_el) = xot.element(sib)
            && sib_el.name() == child_name
        {
            count += 1;
            if sib == child {
                position = count;
            }
        }
    }

    if count > 1 {
        format!("{local}[{position}]")
    } else {
        local.to_string()
    }
}

/// Identity key for matching children across old/new trees.
///
/// Elements with `@id` are keyed by `(name, id)`. Elements without `@id`
/// are keyed by `(name, positional_index_among_same_name_no_id_siblings)`.
/// This prevents a single insertion or deletion from cascading mismatches
/// through all subsequent siblings.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ChildKey {
    /// Element with `@id` attribute.
    ElementById(xot::NameId, String),
    /// Element without `@id`, keyed by name + occurrence index.
    ElementByPos(xot::NameId, usize),
    /// Text node keyed by occurrence index among text siblings.
    Text(usize),
    /// Comment node keyed by occurrence index among comment siblings.
    Comment(usize),
    /// Other node types (PIs, etc.) keyed by raw position.
    Other(usize),
}

/// Assign a `ChildKey` to each child node.
fn key_children(xot: &Xot, parent: xot::Node, ctx: &DiffCtx) -> Vec<(ChildKey, xot::Node)> {
    let mut result = Vec::new();
    // Counters per element-name for positional disambiguation.
    let mut name_counts: std::collections::HashMap<xot::NameId, usize> =
        std::collections::HashMap::new();
    let mut text_idx = 0usize;
    let mut comment_idx = 0usize;
    let mut other_idx = 0usize;

    for child in xot.children(parent) {
        if let Some(el) = xot.element(child) {
            let name = el.name();
            if let Some(id_val) = xot.get_attribute(child, ctx.id_name) {
                result.push((ChildKey::ElementById(name, id_val.to_string()), child));
            } else {
                let idx = name_counts.entry(name).or_insert(0);
                result.push((ChildKey::ElementByPos(name, *idx), child));
                *idx += 1;
            }
        } else if xot.text_str(child).is_some() {
            result.push((ChildKey::Text(text_idx), child));
            text_idx += 1;
        } else if xot.comment_str(child).is_some() {
            result.push((ChildKey::Comment(comment_idx), child));
            comment_idx += 1;
        } else {
            result.push((ChildKey::Other(other_idx), child));
            other_idx += 1;
        }
    }
    result
}

/// Compare two elements that are known to have the same expanded name.
fn diff_elements(
    xot: &Xot,
    old: xot::Node,
    new: xot::Node,
    path: &XmlPath,
    ctx: &DiffCtx,
    changes: &mut Vec<XmlChange>,
) {
    diff_attributes(xot, old, new, path, changes);

    let old_keyed = key_children(xot, old, ctx);
    let new_keyed = key_children(xot, new, ctx);

    // Build lookup from key → node for old children.
    let old_map: std::collections::HashMap<&ChildKey, xot::Node> =
        old_keyed.iter().map(|(k, n)| (k, *n)).collect();
    // Track which old keys were matched.
    let mut matched_old: std::collections::HashSet<&ChildKey> =
        std::collections::HashSet::new();

    // Walk new children: match or report added.
    for (new_key, new_child) in &new_keyed {
        if let Some(&old_child) = old_map.get(new_key) {
            matched_old.insert(new_key);
            diff_matched_pair(xot, old, old_child, new, *new_child, path, ctx, changes);
        } else {
            // Added.
            if xot.element(*new_child).is_some() {
                let seg = child_element_segment(xot, new, *new_child, ctx);
                changes.push(XmlChange::ElementAdded {
                    path: path.child(&seg),
                    content: xot.to_string(*new_child).unwrap_or_default(),
                });
            } else if let Some(text) = xot.text_str(*new_child)
                && !text.trim().is_empty()
            {
                changes.push(XmlChange::TextChanged {
                    path: path.clone(),
                    old: String::new(),
                    new: text.to_string(),
                });
            }
        }
    }

    // Walk old children: report unmatched as removed.
    for (old_key, old_child) in &old_keyed {
        if !matched_old.contains(old_key) {
            if xot.element(*old_child).is_some() {
                let seg = child_element_segment(xot, old, *old_child, ctx);
                changes.push(XmlChange::ElementRemoved {
                    path: path.child(&seg),
                    content: xot.to_string(*old_child).unwrap_or_default(),
                });
            } else if let Some(text) = xot.text_str(*old_child)
                && !text.trim().is_empty()
            {
                changes.push(XmlChange::TextChanged {
                    path: path.clone(),
                    old: text.to_string(),
                    new: String::new(),
                });
            }
        }
    }
}

/// Compare two matched children (same key).
#[allow(clippy::too_many_arguments)]
fn diff_matched_pair(
    xot: &Xot,
    old_parent: xot::Node,
    old_child: xot::Node,
    new_parent: xot::Node,
    new_child: xot::Node,
    path: &XmlPath,
    ctx: &DiffCtx,
    changes: &mut Vec<XmlChange>,
) {
    let old_is_el = xot.element(old_child).is_some();
    let new_is_el = xot.element(new_child).is_some();

    match (old_is_el, new_is_el) {
        (true, true) => {
            let old_name = xot.element(old_child).unwrap().name();
            let new_name = xot.element(new_child).unwrap().name();

            if old_name == new_name {
                let seg = child_element_segment(xot, new_parent, new_child, ctx);
                let child_path = path.child(&seg);
                diff_elements(xot, old_child, new_child, &child_path, ctx, changes);
            } else {
                let old_seg = child_element_segment(xot, old_parent, old_child, ctx);
                changes.push(XmlChange::ElementRemoved {
                    path: path.child(&old_seg),
                    content: xot.to_string(old_child).unwrap_or_default(),
                });
                let new_seg = child_element_segment(xot, new_parent, new_child, ctx);
                changes.push(XmlChange::ElementAdded {
                    path: path.child(&new_seg),
                    content: xot.to_string(new_child).unwrap_or_default(),
                });
            }
        }
        (false, false) => {
            if let (Some(ot), Some(nt)) = (xot.text_str(old_child), xot.text_str(new_child)) {
                if ot != nt {
                    changes.push(XmlChange::TextChanged {
                        path: path.clone(),
                        old: ot.to_string(),
                        new: nt.to_string(),
                    });
                }
            } else if let (Some(oc), Some(nc)) =
                (xot.comment_str(old_child), xot.comment_str(new_child))
                && oc != nc
            {
                changes.push(XmlChange::CommentChanged {
                    path: path.clone(),
                    old: oc.to_string(),
                    new: nc.to_string(),
                });
            }
        }
        _ => {
            // Key matched but types differ (shouldn't happen with proper keying,
            // but handle gracefully).
            if old_is_el {
                let seg = child_element_segment(xot, old_parent, old_child, ctx);
                changes.push(XmlChange::ElementRemoved {
                    path: path.child(&seg),
                    content: xot.to_string(old_child).unwrap_or_default(),
                });
            }
            if new_is_el {
                let seg = child_element_segment(xot, new_parent, new_child, ctx);
                changes.push(XmlChange::ElementAdded {
                    path: path.child(&seg),
                    content: xot.to_string(new_child).unwrap_or_default(),
                });
            }
        }
    }
}

/// Compare attributes between two elements.
fn diff_attributes(
    xot: &Xot,
    old: xot::Node,
    new: xot::Node,
    path: &XmlPath,
    changes: &mut Vec<XmlChange>,
) {
    let old_attrs: Vec<(xot::NameId, String)> = xot
        .attributes(old)
        .iter()
        .map(|(name_id, value)| (name_id, value.clone()))
        .collect();
    let new_attrs: Vec<(xot::NameId, String)> = xot
        .attributes(new)
        .iter()
        .map(|(name_id, value)| (name_id, value.clone()))
        .collect();

    // Removed or changed attributes.
    for (old_name_id, old_value) in &old_attrs {
        let matching = new_attrs.iter().find(|(n, _)| n == old_name_id);
        let (local, _) = xot.name_ns_str(*old_name_id);
        match matching {
            Some((_, new_value)) if new_value != old_value => {
                changes.push(XmlChange::AttributeChanged {
                    path: path.clone(),
                    name: local.to_string(),
                    old: Some(old_value.clone()),
                    new: Some(new_value.clone()),
                });
            }
            None => {
                changes.push(XmlChange::AttributeChanged {
                    path: path.clone(),
                    name: local.to_string(),
                    old: Some(old_value.clone()),
                    new: None,
                });
            }
            _ => {}
        }
    }

    // Added attributes.
    for (new_name_id, new_value) in &new_attrs {
        if !old_attrs.iter().any(|(n, _)| n == new_name_id) {
            let (local, _) = xot.name_ns_str(*new_name_id);
            changes.push(XmlChange::AttributeChanged {
                path: path.clone(),
                name: local.to_string(),
                old: None,
                new: Some(new_value.clone()),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Count changes of a specific variant.
    fn count<F>(diff: &XmlDiff, pred: F) -> usize
    where
        F: Fn(&XmlChange) -> bool,
    {
        diff.changes.iter().filter(|c| pred(c)).count()
    }

    // -----------------------------------------------------------------
    // Identity / no-op
    // -----------------------------------------------------------------

    #[test]
    fn identical_xml_no_changes() {
        let xml = "<root><item>hello</item></root>";
        let diff = diff_xml(xml, xml).unwrap();
        assert!(diff.is_empty(), "identical XML should produce no changes");
        assert_eq!(diff.changes.len(), 0);
    }

    // -----------------------------------------------------------------
    // Text changes
    // -----------------------------------------------------------------

    #[test]
    fn text_content_change_exact() {
        let old = "<root><item>hello</item></root>";
        let new = "<root><item>world</item></root>";
        let diff = diff_xml(old, new).unwrap();
        assert_eq!(diff.changes.len(), 1, "exactly one change expected");
        assert!(matches!(
            &diff.changes[0],
            XmlChange::TextChanged { old, new, .. }
                if old == "hello" && new == "world"
        ));
    }

    #[test]
    fn whitespace_text_change() {
        let old = "<root><item> x </item></root>";
        let new = "<root><item>x</item></root>";
        let diff = diff_xml(old, new).unwrap();
        // Whitespace difference in text nodes is a real change.
        assert_eq!(
            count(&diff, |c| matches!(c, XmlChange::TextChanged { .. })),
            1,
            "whitespace-significant text difference should be reported"
        );
    }

    // -----------------------------------------------------------------
    // Attribute changes
    // -----------------------------------------------------------------

    #[test]
    fn attribute_value_change_exact() {
        let old = r#"<root><item id="1" class="old">x</item></root>"#;
        let new = r#"<root><item id="1" class="new">x</item></root>"#;
        let diff = diff_xml(old, new).unwrap();
        let attr_changes = count(&diff, |c| matches!(c, XmlChange::AttributeChanged { .. }));
        assert_eq!(attr_changes, 1, "only the 'class' attr changed");
        assert!(matches!(
            &diff.changes[0],
            XmlChange::AttributeChanged { name, old: Some(o), new: Some(n), .. }
                if name == "class" && o == "old" && n == "new"
        ));
    }

    #[test]
    fn attribute_added_exact() {
        let old = "<root><item>x</item></root>";
        let new = r#"<root><item color="red">x</item></root>"#;
        let diff = diff_xml(old, new).unwrap();
        assert_eq!(
            count(&diff, |c| matches!(c, XmlChange::AttributeChanged { .. })),
            1
        );
        assert!(matches!(
            &diff.changes[0],
            XmlChange::AttributeChanged { name, old: None, new: Some(n), .. }
                if name == "color" && n == "red"
        ));
    }

    #[test]
    fn attribute_removed_exact() {
        let old = r#"<root><item color="red">x</item></root>"#;
        let new = "<root><item>x</item></root>";
        let diff = diff_xml(old, new).unwrap();
        assert_eq!(
            count(&diff, |c| matches!(c, XmlChange::AttributeChanged { .. })),
            1
        );
        assert!(matches!(
            &diff.changes[0],
            XmlChange::AttributeChanged { name, old: Some(o), new: None, .. }
                if name == "color" && o == "red"
        ));
    }

    // -----------------------------------------------------------------
    // Element add / remove
    // -----------------------------------------------------------------

    #[test]
    fn element_added_exact() {
        let old = "<root><a>one</a></root>";
        let new = "<root><a>one</a><b>two</b></root>";
        let diff = diff_xml(old, new).unwrap();
        let added = count(&diff, |c| matches!(c, XmlChange::ElementAdded { .. }));
        assert_eq!(added, 1, "exactly one element added");
        assert!(matches!(
            &diff.changes[0],
            XmlChange::ElementAdded { path, .. }
                if path.to_string() == "/root/b"
        ));
    }

    #[test]
    fn element_removed_exact() {
        let old = "<root><a>one</a><b>two</b></root>";
        let new = "<root><a>one</a></root>";
        let diff = diff_xml(old, new).unwrap();
        let removed = count(&diff, |c| matches!(c, XmlChange::ElementRemoved { .. }));
        assert_eq!(removed, 1, "exactly one element removed");
        assert!(matches!(
            &diff.changes[0],
            XmlChange::ElementRemoved { path, .. }
                if path.to_string() == "/root/b"
        ));
    }

    // -----------------------------------------------------------------
    // Nested changes
    // -----------------------------------------------------------------

    #[test]
    fn nested_change_exact_path() {
        let old = "<root><section><title>Old</title></section></root>";
        let new = "<root><section><title>New</title></section></root>";
        let diff = diff_xml(old, new).unwrap();
        assert_eq!(diff.changes.len(), 1);
        assert!(matches!(
            &diff.changes[0],
            XmlChange::TextChanged { path, old, new }
                if path.to_string() == "/root/section/title"
                    && old == "Old" && new == "New"
        ));
    }

    // -----------------------------------------------------------------
    // @id path enrichment
    // -----------------------------------------------------------------

    #[test]
    fn path_includes_id_attribute() {
        let old = r#"<root><item id="x">old</item></root>"#;
        let new = r#"<root><item id="x">new</item></root>"#;
        let diff = diff_xml(old, new).unwrap();
        assert_eq!(diff.changes.len(), 1);
        if let XmlChange::TextChanged { path, .. } = &diff.changes[0] {
            assert_eq!(
                path.to_string(),
                r#"/root/item[@id="x"]"#,
                "path should use @id predicate"
            );
        } else {
            panic!("expected TextChanged, got {:?}", diff.changes[0]);
        }
    }

    // -----------------------------------------------------------------
    // Same-name sibling disambiguation ([N])
    // -----------------------------------------------------------------

    #[test]
    fn same_name_siblings_use_positional_index() {
        let old = "<root><item>a</item><item>b</item><item>c</item></root>";
        let new = "<root><item>a</item><item>CHANGED</item><item>c</item></root>";
        let diff = diff_xml(old, new).unwrap();
        assert_eq!(diff.changes.len(), 1, "only second item changed");
        if let XmlChange::TextChanged { path, old, new } = &diff.changes[0] {
            assert_eq!(path.to_string(), "/root/item[2]");
            assert_eq!(old, "b");
            assert_eq!(new, "CHANGED");
        } else {
            panic!("expected TextChanged, got {:?}", diff.changes[0]);
        }
    }

    #[test]
    fn same_name_siblings_id_preferred_over_position() {
        let old = r#"<root><item id="a">1</item><item id="b">2</item></root>"#;
        let new = r#"<root><item id="a">1</item><item id="b">CHANGED</item></root>"#;
        let diff = diff_xml(old, new).unwrap();
        assert_eq!(diff.changes.len(), 1);
        if let XmlChange::TextChanged { path, .. } = &diff.changes[0] {
            assert!(
                path.to_string().contains(r#"@id="b""#),
                "should use @id, not [2]: {path}"
            );
        } else {
            panic!("expected TextChanged");
        }
    }

    // -----------------------------------------------------------------
    // Comment changes
    // -----------------------------------------------------------------

    #[test]
    fn comment_change_detected() {
        let old = "<root><!-- old comment --></root>";
        let new = "<root><!-- new comment --></root>";
        let diff = diff_xml(old, new).unwrap();
        assert_eq!(
            count(&diff, |c| matches!(c, XmlChange::CommentChanged { .. })),
            1
        );
        assert!(matches!(
            &diff.changes[0],
            XmlChange::CommentChanged { old, new, .. }
                if old.contains("old") && new.contains("new")
        ));
    }

    #[test]
    fn identical_comments_no_change() {
        let xml = "<root><!-- same --></root>";
        let diff = diff_xml(xml, xml).unwrap();
        assert!(diff.is_empty());
    }

    // -----------------------------------------------------------------
    // Namespace handling
    // -----------------------------------------------------------------

    #[test]
    fn namespace_aware_same_uri_no_change() {
        // Same namespace URI, same local name → no change even if prefix differs
        // is NOT tested here because xot normalizes by namespace URI.
        let old = r#"<ns:root xmlns:ns="urn:test"><ns:item>x</ns:item></ns:root>"#;
        let new = r#"<ns:root xmlns:ns="urn:test"><ns:item>x</ns:item></ns:root>"#;
        let diff = diff_xml(old, new).unwrap();
        assert!(diff.is_empty(), "identical namespaced XML → no changes");
    }

    #[test]
    fn namespace_text_change() {
        let old = r#"<ns:root xmlns:ns="urn:test"><ns:item>old</ns:item></ns:root>"#;
        let new = r#"<ns:root xmlns:ns="urn:test"><ns:item>new</ns:item></ns:root>"#;
        let diff = diff_xml(old, new).unwrap();
        assert_eq!(diff.changes.len(), 1);
        assert!(matches!(
            &diff.changes[0],
            XmlChange::TextChanged { old, new, .. }
                if old == "old" && new == "new"
        ));
    }

    #[test]
    fn different_namespace_is_different_element() {
        let old = r#"<root xmlns:a="urn:a"><a:item>x</a:item></root>"#;
        let new = r#"<root xmlns:b="urn:b"><b:item>x</b:item></root>"#;
        let diff = diff_xml(old, new).unwrap();
        // Different namespace URI → different element → removed + added
        let removed = count(&diff, |c| matches!(c, XmlChange::ElementRemoved { .. }));
        let added = count(&diff, |c| matches!(c, XmlChange::ElementAdded { .. }));
        assert!(removed >= 1, "old namespaced element should be removed");
        assert!(added >= 1, "new namespaced element should be added");
    }

    // -----------------------------------------------------------------
    // Different root elements
    // -----------------------------------------------------------------

    #[test]
    fn different_root_elements() {
        let old = "<alpha>content</alpha>";
        let new = "<beta>content</beta>";
        let diff = diff_xml(old, new).unwrap();
        let removed = count(&diff, |c| matches!(c, XmlChange::ElementRemoved { .. }));
        let added = count(&diff, |c| matches!(c, XmlChange::ElementAdded { .. }));
        assert_eq!(removed, 1, "old root should be removed");
        assert_eq!(added, 1, "new root should be added");
    }

    // -----------------------------------------------------------------
    // Mixed content
    // -----------------------------------------------------------------

    #[test]
    fn mixed_content_text_change() {
        let old = "<p>Hello <b>world</b> end</p>";
        let new = "<p>Goodbye <b>world</b> end</p>";
        let diff = diff_xml(old, new).unwrap();
        assert_eq!(diff.changes.len(), 1);
        assert!(matches!(
            &diff.changes[0],
            XmlChange::TextChanged { old, new, .. }
                if old == "Hello " && new == "Goodbye "
        ));
    }

    #[test]
    fn mixed_content_element_and_text() {
        let old = "<p>text <em>a</em> more</p>";
        let new = "<p>text <em>a</em> more <strong>new</strong></p>";
        let diff = diff_xml(old, new).unwrap();
        let added = count(&diff, |c| matches!(c, XmlChange::ElementAdded { .. }));
        assert!(added >= 1, "added <strong> element should be detected");
    }

    // -----------------------------------------------------------------
    // Empty elements
    // -----------------------------------------------------------------

    #[test]
    fn empty_to_content() {
        let old = "<root><item/></root>";
        let new = "<root><item>text</item></root>";
        let diff = diff_xml(old, new).unwrap();
        // <item/> has no children; <item>text</item> has one text child → added text
        let text_changes = count(&diff, |c| matches!(c, XmlChange::TextChanged { .. }));
        assert!(text_changes >= 1, "should detect text added to empty element");
    }

    #[test]
    fn content_to_empty() {
        let old = "<root><item>text</item></root>";
        let new = "<root><item/></root>";
        let diff = diff_xml(old, new).unwrap();
        let text_changes = count(&diff, |c| matches!(c, XmlChange::TextChanged { .. }));
        assert!(text_changes >= 1, "should detect text removed from element");
    }

    // -----------------------------------------------------------------
    // Display formatting
    // -----------------------------------------------------------------

    #[test]
    fn display_format() {
        let old = "<root><item>old</item></root>";
        let new = "<root><item>new</item></root>";
        let diff = diff_xml(old, new).unwrap();
        let formatted = diff.to_string();
        assert!(formatted.contains('~'), "display should use ~ for changes");
        assert!(
            formatted.contains("text:"),
            "display should show text changes"
        );
    }

    #[test]
    fn display_element_added_uses_plus() {
        let old = "<root/>";
        let new = "<root><child>x</child></root>";
        let diff = diff_xml(old, new).unwrap();
        let formatted = diff.to_string();
        assert!(formatted.contains("+ /"), "added element display uses +");
    }

    #[test]
    fn display_element_removed_uses_minus() {
        let old = "<root><child>x</child></root>";
        let new = "<root/>";
        let diff = diff_xml(old, new).unwrap();
        let formatted = diff.to_string();
        assert!(formatted.contains("- /"), "removed element display uses -");
    }

    // -----------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------

    #[test]
    fn multiple_changes_in_one_diff() {
        let old = r#"<root><a>1</a><b x="old">2</b><c>3</c></root>"#;
        let new = r#"<root><a>CHANGED</a><b x="new">2</b><d>4</d></root>"#;
        let diff = diff_xml(old, new).unwrap();
        // a: text changed
        // b: attr changed
        // c removed, d added (position-based: c→d is a replacement)
        assert!(
            diff.changes.len() >= 3,
            "should detect text, attr, and element changes: got {}",
            diff.changes.len()
        );
        assert!(
            count(&diff, |c| matches!(c, XmlChange::TextChanged { .. })) >= 1,
            "text change in <a>"
        );
        assert!(
            count(&diff, |c| matches!(c, XmlChange::AttributeChanged { .. })) >= 1,
            "attr change in <b>"
        );
    }

    #[test]
    fn deeply_nested_change() {
        let old = "<a><b><c><d><e>old</e></d></c></b></a>";
        let new = "<a><b><c><d><e>new</e></d></c></b></a>";
        let diff = diff_xml(old, new).unwrap();
        assert_eq!(diff.changes.len(), 1);
        if let XmlChange::TextChanged { path, .. } = &diff.changes[0] {
            assert_eq!(path.to_string(), "/a/b/c/d/e");
        } else {
            panic!("expected TextChanged");
        }
    }

    #[test]
    fn parse_error_returns_err() {
        let result = diff_xml("<valid/>", "not xml at all");
        assert!(result.is_err(), "malformed XML should return Err");
    }

    // -----------------------------------------------------------------
    // Key-based matching (no cascade on insertion/deletion)
    // -----------------------------------------------------------------

    #[test]
    fn remove_middle_element_no_cascade() {
        // Removing B from [A, B, C, D] should report only B removed,
        // not a cascade of mismatches for C and D.
        let old = "<root><a>1</a><b>2</b><c>3</c><d>4</d></root>";
        let new = "<root><a>1</a><c>3</c><d>4</d></root>";
        let diff = diff_xml(old, new).unwrap();
        assert_eq!(
            diff.changes.len(),
            1,
            "only one removal, no cascade: {:#?}",
            diff.changes
        );
        assert!(matches!(
            &diff.changes[0],
            XmlChange::ElementRemoved { path, .. }
                if path.to_string() == "/root/b"
        ));
    }

    #[test]
    fn insert_middle_element_no_cascade() {
        let old = "<root><a>1</a><c>3</c><d>4</d></root>";
        let new = "<root><a>1</a><b>2</b><c>3</c><d>4</d></root>";
        let diff = diff_xml(old, new).unwrap();
        assert_eq!(
            diff.changes.len(),
            1,
            "only one addition, no cascade: {:#?}",
            diff.changes
        );
        assert!(matches!(
            &diff.changes[0],
            XmlChange::ElementAdded { path, .. }
                if path.to_string() == "/root/b"
        ));
    }

    #[test]
    fn remove_id_element_no_cascade() {
        // Same test but with @id attributes - the primary matching key.
        let old = r#"<root><item id="a">1</item><item id="b">2</item><item id="c">3</item></root>"#;
        let new = r#"<root><item id="a">1</item><item id="c">3</item></root>"#;
        let diff = diff_xml(old, new).unwrap();
        assert_eq!(
            diff.changes.len(),
            1,
            "only id=b removed, no cascade: {:#?}",
            diff.changes
        );
        assert!(matches!(
            &diff.changes[0],
            XmlChange::ElementRemoved { path, .. }
                if path.to_string().contains(r#"@id="b""#)
        ));
    }

    #[test]
    fn remove_and_modify_no_false_positives() {
        // Remove B, modify C's text. Should get exactly 2 changes.
        let old = "<root><a>1</a><b>2</b><c>old</c></root>";
        let new = "<root><a>1</a><c>new</c></root>";
        let diff = diff_xml(old, new).unwrap();
        let removed = count(&diff, |c| matches!(c, XmlChange::ElementRemoved { .. }));
        let text_changed = count(&diff, |c| matches!(c, XmlChange::TextChanged { .. }));
        assert_eq!(removed, 1, "b removed");
        assert_eq!(text_changed, 1, "c text changed");
        assert_eq!(diff.changes.len(), 2, "exactly 2 changes, no cascade");
    }
}
