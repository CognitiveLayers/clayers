//! Structural tree diff operating on the Merkle DAG.
//!
//! Compares two content-addressed trees by exploiting the Merkle property:
//! if hashes are equal, the subtrees are identical (short-circuit).

use clayers_xml::ContentHash;

use crate::error::{Error, Result};
use crate::object::Object;
use crate::store::ObjectStore;

/// A structural diff between two trees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeDiff {
    /// The individual node-level changes.
    pub changes: Vec<NodeChange>,
}

impl TreeDiff {
    /// True if there are no changes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

/// A single change in the tree diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeChange {
    /// A node was added to a parent at a given position.
    Added {
        /// Parent element hash.
        parent: ContentHash,
        /// Position among siblings.
        position: usize,
        /// Hash of the added node.
        node: ContentHash,
    },
    /// A node was removed from a parent at a given position.
    Removed {
        /// Parent element hash.
        parent: ContentHash,
        /// Position among siblings.
        position: usize,
        /// Hash of the removed node.
        node: ContentHash,
    },
    /// A node was modified (hash changed, subtree differs).
    Modified {
        /// Hash before the change.
        hash_before: ContentHash,
        /// Hash after the change.
        hash_after: ContentHash,
        /// Recursive diff of the subtree.
        inner: Box<TreeDiff>,
    },
    /// An attribute on an element was changed.
    AttributeChanged {
        /// The element's hash (after change).
        element: ContentHash,
        /// The attribute name.
        attr: String,
        /// The old value (None if attribute was added).
        old: Option<String>,
        /// The new value (None if attribute was removed).
        new: Option<String>,
    },
    /// A text node's content changed.
    TextChanged {
        /// The old text object hash.
        old: ContentHash,
        /// The new text object hash.
        new: ContentHash,
    },
}

/// Compute a structural diff between two content-addressed trees.
///
/// Uses the Merkle property to short-circuit comparison of unchanged subtrees.
///
/// # Errors
///
/// Returns an error if referenced objects cannot be loaded from the store.
pub async fn diff(
    store: &dyn ObjectStore,
    a: ContentHash,
    b: ContentHash,
) -> Result<TreeDiff> {
    // Short-circuit: identical hashes mean identical content.
    if a == b {
        return Ok(TreeDiff {
            changes: Vec::new(),
        });
    }

    let obj_a = store.get(&a).await?.ok_or(Error::NotFound(a))?;
    let obj_b = store.get(&b).await?.ok_or(Error::NotFound(b))?;

    let mut changes = Vec::new();

    match (&obj_a, &obj_b) {
        (Object::Text(_), Object::Text(_)) => {
            changes.push(NodeChange::TextChanged { old: a, new: b });
        }
        (Object::Element(el_a), Object::Element(el_b)) => {
            // Diff attributes.
            diff_attributes(&mut changes, b, el_a, el_b);

            // Diff children using position-based matching.
            diff_children(store, &mut changes, a, &el_a.children, &el_b.children).await?;
        }
        _ => {
            // Different node types: treat as complete replacement.
            changes.push(NodeChange::Removed {
                parent: a,
                position: 0,
                node: a,
            });
            changes.push(NodeChange::Added {
                parent: b,
                position: 0,
                node: b,
            });
        }
    }

    Ok(TreeDiff { changes })
}

/// Diff attributes between two elements.
fn diff_attributes(
    changes: &mut Vec<NodeChange>,
    element_hash: ContentHash,
    el_a: &crate::object::ElementObject,
    el_b: &crate::object::ElementObject,
) {
    // Check for removed or changed attributes.
    for attr_a in &el_a.attributes {
        let matching = el_b.attributes.iter().find(|ab| {
            ab.local_name == attr_a.local_name && ab.namespace_uri == attr_a.namespace_uri
        });
        match matching {
            Some(attr_b) if attr_b.value != attr_a.value => {
                changes.push(NodeChange::AttributeChanged {
                    element: element_hash,
                    attr: attr_a.local_name.clone(),
                    old: Some(attr_a.value.clone()),
                    new: Some(attr_b.value.clone()),
                });
            }
            None => {
                changes.push(NodeChange::AttributeChanged {
                    element: element_hash,
                    attr: attr_a.local_name.clone(),
                    old: Some(attr_a.value.clone()),
                    new: None,
                });
            }
            _ => {}
        }
    }

    // Check for added attributes.
    for attr_b in &el_b.attributes {
        let exists_in_a = el_a.attributes.iter().any(|aa| {
            aa.local_name == attr_b.local_name && aa.namespace_uri == attr_b.namespace_uri
        });
        if !exists_in_a {
            changes.push(NodeChange::AttributeChanged {
                element: element_hash,
                attr: attr_b.local_name.clone(),
                old: None,
                new: Some(attr_b.value.clone()),
            });
        }
    }
}

/// Diff ordered child lists using position-based matching with hash short-circuit.
async fn diff_children(
    store: &dyn ObjectStore,
    changes: &mut Vec<NodeChange>,
    parent: ContentHash,
    children_a: &[ContentHash],
    children_b: &[ContentHash],
) -> Result<()> {
    let len_a = children_a.len();
    let len_b = children_b.len();
    let min_len = len_a.min(len_b);

    // Match by position.
    for i in 0..min_len {
        if children_a[i] != children_b[i] {
            // Subtrees differ: recurse.
            let inner = Box::pin(diff(store, children_a[i], children_b[i])).await?;
            changes.push(NodeChange::Modified {
                hash_before: children_a[i],
                hash_after: children_b[i],
                inner: Box::new(inner),
            });
        }
    }

    // Removed children (a has more than b).
    for (i, child) in children_a.iter().enumerate().skip(min_len) {
        changes.push(NodeChange::Removed {
            parent,
            position: i,
            node: *child,
        });
    }

    // Added children (b has more than a).
    for (i, child) in children_b.iter().enumerate().skip(min_len) {
        changes.push(NodeChange::Added {
            parent,
            position: i,
            node: *child,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{ElementObject, TextObject};
    use crate::store::memory::MemoryStore;

    async fn store_text(store: &MemoryStore, text: &str) -> ContentHash {
        let hash = ContentHash::from_canonical(text.as_bytes());
        let mut tx = store.transaction().await.unwrap();
        tx.put(
            hash,
            Object::Text(TextObject {
                content: text.into(),
            }),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        hash
    }

    #[tokio::test]
    async fn identical_hashes_no_changes() {
        let store = MemoryStore::new();
        let h = store_text(&store, "same").await;
        let d = diff(&store, h, h).await.unwrap();
        assert!(d.is_empty());
    }

    #[tokio::test]
    async fn text_content_change() {
        let store = MemoryStore::new();
        let h1 = store_text(&store, "old").await;
        let h2 = store_text(&store, "new").await;
        let d = diff(&store, h1, h2).await.unwrap();
        assert_eq!(d.changes.len(), 1);
        assert!(matches!(&d.changes[0], NodeChange::TextChanged { .. }));
    }

    #[tokio::test]
    async fn attribute_change_detected() {
        let store = MemoryStore::new();
        let h1 = ContentHash::from_canonical(b"el1");
        let h2 = ContentHash::from_canonical(b"el2");

        let mut tx = store.transaction().await.unwrap();
        tx.put(
            h1,
            Object::Element(ElementObject {
                local_name: "div".into(),
                namespace_uri: None,
                namespace_prefix: None,
                attributes: vec![crate::object::Attribute {
                    local_name: "class".into(),
                    namespace_uri: None,
                    namespace_prefix: None,
                    value: "old".into(),
                }],
                children: vec![],
                inclusive_hash: h1,
            }),
        )
        .await
        .unwrap();
        tx.put(
            h2,
            Object::Element(ElementObject {
                local_name: "div".into(),
                namespace_uri: None,
                namespace_prefix: None,
                attributes: vec![crate::object::Attribute {
                    local_name: "class".into(),
                    namespace_uri: None,
                    namespace_prefix: None,
                    value: "new".into(),
                }],
                children: vec![],
                inclusive_hash: h2,
            }),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let d = diff(&store, h1, h2).await.unwrap();
        assert!(d.changes.iter().any(|c| matches!(
            c,
            NodeChange::AttributeChanged {
                attr,
                old: Some(_),
                new: Some(_),
                ..
            } if attr == "class"
        )));
    }
}
