//! Three-way merge framework with pluggable strategies.
//!
//! Provides file-level and element-level three-way merge for XML documents
//! in the content-addressed Merkle DAG. Strategies (Ours, Theirs, `AutoMerge`,
//! Manual) are composable and selectable per-file via [`MergePolicy`].

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use clayers_xml::ContentHash;

use crate::conflict;
use crate::error::{Error, Result};
use crate::export;
use crate::hash;
use crate::import;
use crate::object::{
    Attribute, DocumentObject, ElementObject, Object, TreeEntry, TreeObject,
};
use crate::store::ObjectStore;

// -----------------------------------------------------------------------
// Result types
// -----------------------------------------------------------------------

/// Outcome of a merge operation.
pub enum MergeOutcome {
    /// Target branch was ahead; HEAD fast-forwarded.
    FastForward {
        /// The commit that HEAD now points to.
        commit: ContentHash,
    },
    /// A merge commit was created (may contain conflicts).
    Merged {
        /// The merge commit hash.
        commit: ContentHash,
        /// Details about the merge.
        result: MergeResult,
    },
    /// HEAD already contains the target; nothing to do.
    UpToDate,
    /// Branches share no common history.
    NoCommonAncestor,
}

/// Details of a completed three-way merge.
pub struct MergeResult {
    /// Hash of the merged tree.
    pub tree: ContentHash,
    /// Paths that were auto-merged without conflicts.
    pub auto_merged: Vec<String>,
    /// Paths with unresolved conflicts (contain divergence elements).
    pub conflicts: Vec<FileConflict>,
    /// Paths changed only on our side.
    pub ours_only: Vec<String>,
    /// Paths changed only on their side.
    pub theirs_only: Vec<String>,
}

/// A file with unresolved conflicts.
pub struct FileConflict {
    /// Original file path in the tree.
    pub path: String,
    /// Path to the divergence document in the tree.
    pub divergence_path: String,
    /// Human-readable description of the conflict kind.
    pub description: String,
}

/// Path prefix for divergence sidecar documents in the tree.
pub const DIVERGENCE_PREFIX: &str = ".clayers/divergence/";

/// Check whether a tree contains unresolved divergences.
#[must_use]
pub fn tree_has_divergences(tree: &TreeObject) -> bool {
    tree.entries
        .iter()
        .any(|e| e.path.starts_with(DIVERGENCE_PREFIX))
}

/// List divergence entries in a tree.
#[must_use]
pub fn list_divergence_entries(tree: &TreeObject) -> Vec<&TreeEntry> {
    tree.entries
        .iter()
        .filter(|e| e.path.starts_with(DIVERGENCE_PREFIX))
        .collect()
}

/// Compute the sidecar path for a divergence document.
///
/// Encodes a hash of the three sides into the filename to allow multiple
/// divergences for the same file to coexist.
fn divergence_entry_path(
    path: &str,
    ancestor: Option<ContentHash>,
    ours: ContentHash,
    theirs: ContentHash,
) -> String {
    let mut input = Vec::with_capacity(96);
    if let Some(a) = ancestor {
        input.extend_from_slice(&a.0);
    }
    input.extend_from_slice(&ours.0);
    input.extend_from_slice(&theirs.0);
    let combined = ContentHash::from_canonical(&input);
    let short = combined.0.iter().take(8).fold(String::new(), |mut s, b| {
        use std::fmt::Write;
        write!(s, "{b:02x}").unwrap();
        s
    });
    format!("{DIVERGENCE_PREFIX}{path}.{short}")
}

// -----------------------------------------------------------------------
// Strategy types
// -----------------------------------------------------------------------

/// How a file-level conflict was resolved by a strategy.
pub enum Resolution {
    /// Use this document hash as the merged content.
    Resolved(ContentHash),
    /// Generate `<repo:divergence>` markers for manual resolution.
    Divergence,
}

/// A file-level conflict presented to a [`MergeStrategy`].
pub struct MergeConflict {
    /// File path in the tree.
    pub path: String,
    /// Ancestor (common base) document hash (`None` if both sides added).
    pub ancestor: Option<ContentHash>,
    /// Our version's document hash.
    pub ours: ContentHash,
    /// Their version's document hash.
    pub theirs: ContentHash,
    /// Our ref name (e.g. `refs/heads/main`).
    pub ours_ref: String,
    /// Their ref name (e.g. `refs/heads/feature`).
    pub theirs_ref: String,
    /// True if our side deleted the file.
    pub ours_deleted: bool,
    /// True if their side deleted the file.
    pub theirs_deleted: bool,
}

/// A pluggable strategy for resolving file-level merge conflicts.
#[async_trait]
pub trait MergeStrategy: Send + Sync {
    /// Attempt to resolve a file-level conflict.
    async fn resolve(
        &self,
        store: &dyn ObjectStore,
        conflict: &MergeConflict,
    ) -> Result<Resolution>;
}

// -----------------------------------------------------------------------
// Policy
// -----------------------------------------------------------------------

/// Dispatches merge strategies per file path.
pub struct MergePolicy {
    /// Default strategy for all files without a specific override.
    pub default: Box<dyn MergeStrategy>,
    /// Per-file overrides: `(path_suffix, strategy)`.
    pub file_overrides: Vec<(String, Box<dyn MergeStrategy>)>,
}

impl MergePolicy {
    fn strategy_for(&self, path: &str) -> &dyn MergeStrategy {
        for (pattern, strategy) in &self.file_overrides {
            if path == pattern || path.ends_with(pattern) {
                return strategy.as_ref();
            }
        }
        self.default.as_ref()
    }
}

// -----------------------------------------------------------------------
// Built-in strategies
// -----------------------------------------------------------------------

/// Always take our version.
pub struct Ours;

/// Always take their version.
pub struct Theirs;

/// Always produce divergence markers for manual resolution.
pub struct Manual;

/// Attempt element-level three-way merge; divergence on conflict.
pub struct AutoMerge;

#[async_trait]
impl MergeStrategy for Ours {
    async fn resolve(
        &self,
        _store: &dyn ObjectStore,
        conflict: &MergeConflict,
    ) -> Result<Resolution> {
        // Delete-vs-modify is ambiguous even for "ours": produce divergence.
        if conflict.ours_deleted || conflict.theirs_deleted {
            return Ok(Resolution::Divergence);
        }
        Ok(Resolution::Resolved(conflict.ours))
    }
}

#[async_trait]
impl MergeStrategy for Theirs {
    async fn resolve(
        &self,
        _store: &dyn ObjectStore,
        conflict: &MergeConflict,
    ) -> Result<Resolution> {
        if conflict.ours_deleted || conflict.theirs_deleted {
            return Ok(Resolution::Divergence);
        }
        Ok(Resolution::Resolved(conflict.theirs))
    }
}

#[async_trait]
impl MergeStrategy for Manual {
    async fn resolve(
        &self,
        _store: &dyn ObjectStore,
        _conflict: &MergeConflict,
    ) -> Result<Resolution> {
        Ok(Resolution::Divergence)
    }
}

#[async_trait]
impl MergeStrategy for AutoMerge {
    async fn resolve(
        &self,
        store: &dyn ObjectStore,
        conflict: &MergeConflict,
    ) -> Result<Resolution> {
        // Delete-vs-modify is always a conflict for auto-merge.
        if conflict.ours_deleted || conflict.theirs_deleted {
            return Ok(Resolution::Divergence);
        }
        let Some(ancestor) = conflict.ancestor else {
            // Both sides added: cannot auto-merge without an ancestor.
            return Ok(Resolution::Divergence);
        };
        match merge_documents(store, ancestor, conflict.ours, conflict.theirs).await? {
            Some(merged_hash) => Ok(Resolution::Resolved(merged_hash)),
            None => Ok(Resolution::Divergence),
        }
    }
}

// -----------------------------------------------------------------------
// Element identity (ChildKey)
// -----------------------------------------------------------------------

/// Identity key for a child node in the Merkle DAG.
///
/// Used for three-way merge to correctly match children across ancestor,
/// ours, and theirs versions — preventing cascading mismatches when
/// children are inserted or deleted on different sides.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChildKey {
    /// Element with an `@id` attribute: `(local_name, id_value)`.
    ElementById(String, String),
    /// Element without `@id`, keyed by `(local_name, occurrence_index)`.
    ElementByPos(String, usize),
    /// Text node keyed by occurrence index among text siblings.
    Text(usize),
    /// Comment node keyed by occurrence index.
    Comment(usize),
    /// Other node type (PI, etc.) keyed by position.
    Other(usize),
}

/// Assign identity keys to children by loading objects from the store.
///
/// For each child hash, loads the object and determines its key based on
/// type and `@id` attribute presence.
///
/// # Errors
///
/// Returns an error if objects cannot be loaded from the store.
pub async fn key_children(
    store: &dyn ObjectStore,
    children: &[ContentHash],
) -> Result<Vec<(ChildKey, ContentHash)>> {
    let mut result = Vec::with_capacity(children.len());
    let mut name_counts: HashMap<String, usize> = HashMap::new();
    let mut text_idx = 0usize;
    let mut comment_idx = 0usize;
    let mut other_idx = 0usize;

    for &child_hash in children {
        let obj = store
            .get(&child_hash)
            .await?
            .ok_or(Error::NotFound(child_hash))?;
        let key = match &obj {
            Object::Element(el) => {
                let id_val = el
                    .attributes
                    .iter()
                    .find(|a| {
                        a.local_name == "id"
                            && (a.namespace_uri.is_none()
                                || a.namespace_uri.as_deref()
                                    == Some("http://www.w3.org/XML/1998/namespace"))
                    })
                    .map(|a| a.value.clone());
                if let Some(id) = id_val {
                    ChildKey::ElementById(el.local_name.clone(), id)
                } else {
                    let count = name_counts.entry(el.local_name.clone()).or_insert(0);
                    let key = ChildKey::ElementByPos(el.local_name.clone(), *count);
                    *count += 1;
                    key
                }
            }
            Object::Text(_) => {
                let key = ChildKey::Text(text_idx);
                text_idx += 1;
                key
            }
            Object::Comment(_) => {
                let key = ChildKey::Comment(comment_idx);
                comment_idx += 1;
                key
            }
            _ => {
                let key = ChildKey::Other(other_idx);
                other_idx += 1;
                key
            }
        };
        result.push((key, child_hash));
    }

    Ok(result)
}

// -----------------------------------------------------------------------
// Three-way document merge
// -----------------------------------------------------------------------

/// Attempt a three-way merge of two document versions against their common
/// ancestor. Returns `Some(merged_hash)` on clean merge, `None` on conflict.
async fn merge_documents(
    store: &dyn ObjectStore,
    ancestor: ContentHash,
    ours: ContentHash,
    theirs: ContentHash,
) -> Result<Option<ContentHash>> {
    // Short-circuits (Merkle property).
    if ours == theirs {
        return Ok(Some(ours));
    }
    if ours == ancestor {
        return Ok(Some(theirs));
    }
    if theirs == ancestor {
        return Ok(Some(ours));
    }

    // Load documents.
    let anc_obj = store
        .get(&ancestor)
        .await?
        .ok_or(Error::NotFound(ancestor))?;
    let ours_obj = store.get(&ours).await?.ok_or(Error::NotFound(ours))?;
    let theirs_obj = store
        .get(&theirs)
        .await?
        .ok_or(Error::NotFound(theirs))?;

    let (Object::Document(anc_doc), Object::Document(ours_doc), Object::Document(theirs_doc)) =
        (&anc_obj, &ours_obj, &theirs_obj)
    else {
        return Ok(None);
    };

    // Merge root elements.
    let Some(merged_root) =
        merge_elements(store, anc_doc.root, ours_doc.root, theirs_doc.root).await?
    else {
        return Ok(None);
    };

    // Use ours' prologue (pre-root comments/PIs — rarely conflicting).
    let merged_doc = DocumentObject {
        root: merged_root,
        prologue: ours_doc.prologue.clone(),
    };
    let doc_xml = merged_doc.to_xml();
    let doc_hash = hash::hash_exclusive(&doc_xml)?;

    let mut tx = store.transaction().await?;
    tx.put(doc_hash, Object::Document(merged_doc)).await?;
    tx.commit().await?;

    Ok(Some(doc_hash))
}

// -----------------------------------------------------------------------
// Three-way element merge
// -----------------------------------------------------------------------

/// Recursive three-way merge of elements in the Merkle DAG.
///
/// Returns `Some(merged_hash)` if the merge is clean, `None` if there is
/// an unresolvable conflict.
///
/// # Errors
///
/// Returns an error if objects cannot be loaded or stored.
#[allow(clippy::too_many_lines)]
pub async fn merge_elements(
    store: &dyn ObjectStore,
    ancestor: ContentHash,
    ours: ContentHash,
    theirs: ContentHash,
) -> Result<Option<ContentHash>> {
    let mut cache = HashMap::new();
    merge_elements_cached(store, ancestor, ours, theirs, &mut cache).await
}

/// Inner recursive merge with a shared object cache to avoid redundant
/// store loads when computing element hashes.
#[allow(clippy::too_many_lines)]
async fn merge_elements_cached(
    store: &dyn ObjectStore,
    ancestor: ContentHash,
    ours: ContentHash,
    theirs: ContentHash,
    cache: &mut HashMap<ContentHash, Object>,
) -> Result<Option<ContentHash>> {
    // Short-circuits.
    if ours == theirs {
        return Ok(Some(ours));
    }
    if ours == ancestor {
        return Ok(Some(theirs));
    }
    if theirs == ancestor {
        return Ok(Some(ours));
    }

    // Load all three.
    let anc_obj = store
        .get(&ancestor)
        .await?
        .ok_or(Error::NotFound(ancestor))?;
    let ours_obj = store.get(&ours).await?.ok_or(Error::NotFound(ours))?;
    let theirs_obj = store
        .get(&theirs)
        .await?
        .ok_or(Error::NotFound(theirs))?;

    // Non-element nodes with both sides changed differently: conflict.
    let (Object::Element(anc_el), Object::Element(ours_el), Object::Element(theirs_el)) =
        (&anc_obj, &ours_obj, &theirs_obj)
    else {
        return Ok(None);
    };

    // Element name/namespace must match across all three sides.
    if anc_el.local_name != ours_el.local_name
        || anc_el.local_name != theirs_el.local_name
        || anc_el.namespace_uri != ours_el.namespace_uri
        || anc_el.namespace_uri != theirs_el.namespace_uri
    {
        return Ok(None);
    }

    // Merge attributes.
    let Some(merged_attrs) = merge_attributes(
        &anc_el.attributes,
        &ours_el.attributes,
        &theirs_el.attributes,
    ) else {
        return Ok(None);
    };

    // Key children on all three sides.
    let anc_keyed = key_children(store, &anc_el.children).await?;
    let ours_keyed = key_children(store, &ours_el.children).await?;
    let theirs_keyed = key_children(store, &theirs_el.children).await?;

    let anc_map: HashMap<ChildKey, ContentHash> =
        anc_keyed.iter().map(|(k, h)| (k.clone(), *h)).collect();
    let ours_map: HashMap<ChildKey, ContentHash> =
        ours_keyed.iter().map(|(k, h)| (k.clone(), *h)).collect();
    let theirs_map: HashMap<ChildKey, ContentHash> =
        theirs_keyed.iter().map(|(k, h)| (k.clone(), *h)).collect();

    // Build merged child list: ours order as base, theirs-only appended.
    let mut merged_children: Vec<ContentHash> = Vec::new();
    let mut processed: HashSet<ChildKey> = HashSet::new();

    // Process ours' children (preserving order).
    for (key, _) in &ours_keyed {
        processed.insert(key.clone());
        let a = anc_map.get(key).copied();
        let o = ours_map.get(key).copied();
        let t = theirs_map.get(key).copied();

        match (a, o, t) {
            (Some(ah), Some(oh), Some(th)) => {
                if oh == th {
                    merged_children.push(oh);
                } else if th == ah {
                    // Only ours changed.
                    merged_children.push(oh);
                } else if oh == ah {
                    // Only theirs changed.
                    merged_children.push(th);
                } else {
                    // Both changed: recurse.
                    match Box::pin(merge_elements_cached(store, ah, oh, th, cache)).await? {
                        Some(merged) => merged_children.push(merged),
                        None => return Ok(None),
                    }
                }
            }
            // In ancestor and ours, deleted by theirs.
            (Some(ah), Some(oh), None) => {
                if oh == ah {
                    // Unchanged on ours, deleted by theirs: delete.
                } else {
                    // Modified on ours, deleted by theirs: conflict.
                    return Ok(None);
                }
            }
            // Added by both sides.
            (None, Some(oh), Some(th)) => {
                if oh == th {
                    merged_children.push(oh);
                } else {
                    return Ok(None);
                }
            }
            // Added by ours only.
            (None, Some(oh), None) => {
                merged_children.push(oh);
            }
            _ => {}
        }
    }

    // Append theirs-only children.
    for (key, _) in &theirs_keyed {
        if processed.contains(key) {
            continue;
        }
        let a = anc_map.get(key).copied();
        let t = theirs_map.get(key).copied();

        match (a, t) {
            (Some(ah), Some(th)) => {
                if th == ah {
                    // Unchanged on theirs, deleted by ours: delete.
                } else {
                    // Modified on theirs, deleted by ours: conflict.
                    return Ok(None);
                }
            }
            (None, Some(th)) => {
                // Added by theirs only.
                merged_children.push(th);
            }
            _ => {}
        }
    }

    // Build the merged ElementObject.
    let merged_el = ElementObject {
        local_name: ours_el.local_name.clone(),
        namespace_uri: ours_el.namespace_uri.clone(),
        namespace_prefix: ours_el.namespace_prefix.clone(),
        extra_namespaces: ours_el.extra_namespaces.clone(),
        attributes: merged_attrs,
        children: merged_children.clone(),
        inclusive_hash: ContentHash::from_canonical(b"placeholder"),
    };

    // Compute hash by building the full XML via the export pipeline.
    // Reuse the shared cache to avoid redundant store loads across recursion.
    for &child_hash in &merged_children {
        collect_subtree(store, child_hash, cache).await?;
    }
    let temp_hash = ContentHash::from_canonical(b"__merge_temp__");
    cache.insert(temp_hash, Object::Element(merged_el.clone()));

    let merged_xml = export::build_xml_from_objects(cache, temp_hash)?;
    let (identity_hash, inclusive_hash) = hash::hash_element_xml(&merged_xml)?;

    // Remove the temp entry so it doesn't pollute the cache.
    cache.remove(&temp_hash);

    let final_el = ElementObject {
        inclusive_hash,
        ..merged_el
    };
    let mut tx = store.transaction().await?;
    tx.put(identity_hash, Object::Element(final_el)).await?;
    tx.commit().await?;

    Ok(Some(identity_hash))
}

/// Recursively collect all objects in a subtree into a map.
async fn collect_subtree(
    store: &dyn ObjectStore,
    hash: ContentHash,
    map: &mut HashMap<ContentHash, Object>,
) -> Result<()> {
    if map.contains_key(&hash) {
        return Ok(());
    }
    let obj = store.get(&hash).await?.ok_or(Error::NotFound(hash))?;
    if let Object::Element(ref el) = obj {
        for &child in &el.children {
            Box::pin(collect_subtree(store, child, map)).await?;
        }
    }
    map.insert(hash, obj);
    Ok(())
}

// -----------------------------------------------------------------------
// Attribute merge
// -----------------------------------------------------------------------

/// Three-way merge of attribute lists.
///
/// Returns `Some(merged)` if clean, `None` if any attribute conflicts.
fn merge_attributes(
    ancestor: &[Attribute],
    ours: &[Attribute],
    theirs: &[Attribute],
) -> Option<Vec<Attribute>> {
    type Key = (String, Option<String>);

    let anc: HashMap<Key, &Attribute> = ancestor
        .iter()
        .map(|a| ((a.local_name.clone(), a.namespace_uri.clone()), a))
        .collect();
    let our: HashMap<Key, &Attribute> = ours
        .iter()
        .map(|a| ((a.local_name.clone(), a.namespace_uri.clone()), a))
        .collect();
    let their: HashMap<Key, &Attribute> = theirs
        .iter()
        .map(|a| ((a.local_name.clone(), a.namespace_uri.clone()), a))
        .collect();

    // Deterministic order: ours first (preserving order), then theirs-only
    // (preserving order), then ancestor-only (preserving order).
    let mut ordered_keys: Vec<Key> = Vec::new();
    let mut seen: HashSet<Key> = HashSet::new();
    for a in ours {
        let key = (a.local_name.clone(), a.namespace_uri.clone());
        if seen.insert(key.clone()) {
            ordered_keys.push(key);
        }
    }
    for a in theirs {
        let key = (a.local_name.clone(), a.namespace_uri.clone());
        if seen.insert(key.clone()) {
            ordered_keys.push(key);
        }
    }
    for a in ancestor {
        let key = (a.local_name.clone(), a.namespace_uri.clone());
        if seen.insert(key.clone()) {
            ordered_keys.push(key);
        }
    }

    let mut merged = Vec::new();

    for key in &ordered_keys {
        let a = anc.get(key);
        let o = our.get(key);
        let t = their.get(key);

        match (a, o, t) {
            // All three present.
            (Some(av), Some(ov), Some(tv)) => {
                if ov.value == tv.value || tv.value == av.value {
                    merged.push((*ov).clone());
                } else if ov.value == av.value {
                    merged.push((*tv).clone());
                } else {
                    return None;
                }
            }
            // Added on one side only.
            (None, Some(ov), None) | (None, None, Some(ov)) => {
                merged.push((*ov).clone());
            }
            // Added on both with same value.
            (None, Some(ov), Some(tv)) if ov.value == tv.value => {
                merged.push((*ov).clone());
            }
            // Deleted on one side, unchanged on other.
            (Some(av), None, Some(tv)) if tv.value == av.value => {}
            (Some(av), Some(ov), None) if ov.value == av.value => {}
            // Deleted on both.
            (Some(_) | None, None, None) => {}
            // All remaining cases are conflicts.
            _ => return None,
        }
    }

    Some(merged)
}

// -----------------------------------------------------------------------
// File-level three-way merge
// -----------------------------------------------------------------------

/// Perform a file-level three-way merge between two trees and their
/// common ancestor, using the given policy to resolve conflicts.
///
/// # Errors
///
/// Returns an error if objects cannot be loaded or stored.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub async fn merge_trees(
    store: &dyn ObjectStore,
    ancestor_tree: &TreeObject,
    ours_tree: &TreeObject,
    theirs_tree: &TreeObject,
    policy: &MergePolicy,
    ours_ref: &str,
    theirs_ref: &str,
    ours_commit: ContentHash,
    theirs_commit: ContentHash,
    ancestor_commit: ContentHash,
) -> Result<MergeResult> {
    let anc: HashMap<&str, ContentHash> = ancestor_tree
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.document))
        .collect();
    let ours: HashMap<&str, ContentHash> = ours_tree
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.document))
        .collect();
    let theirs: HashMap<&str, ContentHash> = theirs_tree
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.document))
        .collect();

    let mut all_paths: HashSet<&str> = HashSet::new();
    all_paths.extend(anc.keys());
    all_paths.extend(ours.keys());
    all_paths.extend(theirs.keys());

    let mut entries: Vec<TreeEntry> = Vec::new();
    let mut auto_merged = Vec::new();
    let mut conflicts = Vec::new();
    let mut ours_only = Vec::new();
    let mut theirs_only = Vec::new();

    for &path in &all_paths {
        let a = anc.get(path).copied();
        let o = ours.get(path).copied();
        let t = theirs.get(path).copied();

        match (a, o, t) {
            // Both present, convergent or unchanged.
            (Some(_), Some(oh), Some(th)) if oh == th => {
                entries.push(TreeEntry {
                    path: path.into(),
                    document: oh,
                });
            }
            // Only ours changed.
            (Some(ah), Some(oh), Some(th)) if th == ah => {
                entries.push(TreeEntry {
                    path: path.into(),
                    document: oh,
                });
                ours_only.push(path.into());
            }
            // Only theirs changed.
            (Some(ah), Some(oh), Some(th)) if oh == ah => {
                entries.push(TreeEntry {
                    path: path.into(),
                    document: th,
                });
                theirs_only.push(path.into());
            }
            // Both changed differently → strategy.
            (Some(ah), Some(oh), Some(th)) => {
                resolve_file_conflict(
                    store,
                    path,
                    Some(ah),
                    oh,
                    th,
                    policy,
                    ancestor_commit,
                    ours_commit,
                    theirs_commit,
                    ours_ref,
                    theirs_ref,
                    false,
                    false,
                    "both sides modified",
                    &mut entries,
                    &mut auto_merged,
                    &mut conflicts,
                )
                .await?;
            }
            // Added on ours only.
            (None, Some(oh), None) => {
                entries.push(TreeEntry {
                    path: path.into(),
                    document: oh,
                });
                ours_only.push(path.into());
            }
            // Added on theirs only.
            (None, None, Some(th)) => {
                entries.push(TreeEntry {
                    path: path.into(),
                    document: th,
                });
                theirs_only.push(path.into());
            }
            // Added on both, same content.
            (None, Some(oh), Some(th)) if oh == th => {
                entries.push(TreeEntry {
                    path: path.into(),
                    document: oh,
                });
                auto_merged.push(path.into());
            }
            // Added on both, different content.
            (None, Some(oh), Some(th)) => {
                resolve_file_conflict(
                    store,
                    path,
                    None,
                    oh,
                    th,
                    policy,
                    ancestor_commit,
                    ours_commit,
                    theirs_commit,
                    ours_ref,
                    theirs_ref,
                    false,
                    false,
                    "both sides added with different content",
                    &mut entries,
                    &mut auto_merged,
                    &mut conflicts,
                )
                .await?;
            }
            // Deleted on ours, unchanged on theirs.
            (Some(ah), None, Some(th)) if th == ah => {
                ours_only.push(path.into());
            }
            // Deleted on theirs, unchanged on ours.
            (Some(ah), Some(oh), None) if oh == ah => {
                theirs_only.push(path.into());
            }
            // Deleted on both or absent everywhere.
            (Some(_) | None, None, None) => {}
            // Delete vs modify: ours deleted, theirs modified.
            (Some(ah), None, Some(th)) => {
                resolve_file_conflict(
                    store,
                    path,
                    Some(ah),
                    ah, // deleted: ancestor as placeholder
                    th,
                    policy,
                    ancestor_commit,
                    ours_commit,
                    theirs_commit,
                    ours_ref,
                    theirs_ref,
                    true,
                    false,
                    "deleted on ours, modified on theirs",
                    &mut entries,
                    &mut auto_merged,
                    &mut conflicts,
                )
                .await?;
            }
            // Delete vs modify: theirs deleted, ours modified.
            (Some(ah), Some(oh), None) => {
                resolve_file_conflict(
                    store,
                    path,
                    Some(ah),
                    oh,
                    ah, // deleted: ancestor as placeholder
                    policy,
                    ancestor_commit,
                    ours_commit,
                    theirs_commit,
                    ours_ref,
                    theirs_ref,
                    false,
                    true,
                    "modified on ours, deleted on theirs",
                    &mut entries,
                    &mut auto_merged,
                    &mut conflicts,
                )
                .await?;
            }
        }
    }

    // Build merged tree.
    let tree = TreeObject::new(entries);
    let tree_xml = tree.to_xml();
    let tree_hash = hash::hash_exclusive(&tree_xml)?;

    let mut tx = store.transaction().await?;
    tx.put(tree_hash, Object::Tree(tree)).await?;
    tx.commit().await?;

    Ok(MergeResult {
        tree: tree_hash,
        auto_merged,
        conflicts,
        ours_only,
        theirs_only,
    })
}

/// Resolve a single file-level conflict using the policy's strategy.
#[allow(clippy::too_many_arguments)]
async fn resolve_file_conflict(
    store: &dyn ObjectStore,
    path: &str,
    ancestor: Option<ContentHash>,
    ours: ContentHash,
    theirs: ContentHash,
    policy: &MergePolicy,
    ancestor_commit: ContentHash,
    ours_commit: ContentHash,
    theirs_commit: ContentHash,
    ours_ref: &str,
    theirs_ref: &str,
    ours_deleted: bool,
    theirs_deleted: bool,
    description: &str,
    entries: &mut Vec<TreeEntry>,
    auto_merged: &mut Vec<String>,
    conflicts: &mut Vec<FileConflict>,
) -> Result<()> {
    let mc = MergeConflict {
        path: path.into(),
        ancestor,
        ours,
        theirs,
        ours_ref: ours_ref.into(),
        theirs_ref: theirs_ref.into(),
        ours_deleted,
        theirs_deleted,
    };
    let strategy = policy.strategy_for(path);
    match strategy.resolve(store, &mc).await? {
        Resolution::Resolved(hash) => {
            entries.push(TreeEntry {
                path: path.into(),
                document: hash,
            });
            auto_merged.push(path.into());
        }
        Resolution::Divergence => {
            let div_hash = generate_divergence_doc(
                store,
                path,
                ancestor,
                ours,
                theirs,
                ancestor_commit,
                ours_commit,
                theirs_commit,
                ours_ref,
                theirs_ref,
            )
            .await?;
            // Keep ours at the original path (document stays valid).
            entries.push(TreeEntry {
                path: path.into(),
                document: ours,
            });
            // Store divergence as a sidecar document.
            let div_path = divergence_entry_path(path, ancestor, ours, theirs);
            entries.push(TreeEntry {
                path: div_path.clone(),
                document: div_hash,
            });
            conflicts.push(FileConflict {
                path: path.into(),
                divergence_path: div_path,
                description: description.into(),
            });
        }
    }
    Ok(())
}

/// Generate a `<repo:divergence>` document and import it into the store.
#[allow(clippy::too_many_arguments)]
async fn generate_divergence_doc(
    store: &dyn ObjectStore,
    path: &str,
    ancestor: Option<ContentHash>,
    ours: ContentHash,
    theirs: ContentHash,
    ancestor_commit: ContentHash,
    ours_commit: ContentHash,
    theirs_commit: ContentHash,
    ours_ref: &str,
    theirs_ref: &str,
) -> Result<ContentHash> {
    let ancestor_xml = if let Some(a) = ancestor {
        export::export_xml(store, a).await?
    } else {
        "<empty/>".to_string()
    };
    let ours_xml = export::export_xml(store, ours).await?;
    let theirs_xml = export::export_xml(store, theirs).await?;

    let div_xml = conflict::generate_divergence_xml(
        path,
        ancestor_commit,
        &ancestor_xml,
        &[
            (ours_commit, ours_ref, &ours_xml),
            (theirs_commit, theirs_ref, &theirs_xml),
        ],
    );

    let full_xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{div_xml}");
    import::import_xml(store, &full_xml).await
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::Author;
    use crate::store::memory::MemoryStore;

    // --- Test helpers for structural DAG traversal ---

    /// Extract all text content from a subtree in the DAG.
    async fn collect_text(store: &MemoryStore, hash: ContentHash) -> String {
        let obj = store.get(&hash).await.unwrap().unwrap();
        match obj {
            Object::Text(t) => t.content,
            Object::Element(el) => {
                let mut buf = String::new();
                for child in &el.children {
                    buf.push_str(&Box::pin(collect_text(store, *child)).await);
                }
                buf
            }
            _ => String::new(),
        }
    }

    /// Load a document's root element from the store.
    async fn doc_root_element(store: &MemoryStore, doc_hash: ContentHash) -> ElementObject {
        let obj = store.get(&doc_hash).await.unwrap().unwrap();
        let Object::Document(doc) = obj else {
            panic!("expected Document");
        };
        let root_obj = store.get(&doc.root).await.unwrap().unwrap();
        let Object::Element(el) = root_obj else {
            panic!("expected Element");
        };
        el
    }

    /// Find the first child element by name.
    async fn find_child_element(
        store: &MemoryStore,
        parent: &ElementObject,
        name: &str,
    ) -> Option<(ContentHash, ElementObject)> {
        for &child_hash in &parent.children {
            let obj = store.get(&child_hash).await.unwrap().unwrap();
            if let Object::Element(el) = obj
                && el.local_name == name
            {
                return Some((child_hash, el));
            }
        }
        None
    }

    /// Collect all child elements with a given name.
    async fn find_child_elements(
        store: &MemoryStore,
        parent: &ElementObject,
        name: &str,
    ) -> Vec<(ContentHash, ElementObject)> {
        let mut result = Vec::new();
        for &child_hash in &parent.children {
            let obj = store.get(&child_hash).await.unwrap().unwrap();
            if let Object::Element(el) = obj
                && el.local_name == name
            {
                result.push((child_hash, el));
            }
        }
        result
    }

    /// Get the value of an attribute by local name.
    fn attr_value<'a>(el: &'a ElementObject, name: &str) -> Option<&'a str> {
        el.attributes
            .iter()
            .find(|a| a.local_name == name)
            .map(|a| a.value.as_str())
    }

    #[tokio::test]
    async fn ours_strategy_takes_ours() {
        let store = MemoryStore::new();
        let h1 = import::import_xml(&store, "<a>ours</a>").await.unwrap();
        let h2 = import::import_xml(&store, "<a>theirs</a>").await.unwrap();
        let mc = MergeConflict {
            path: "f.xml".into(),
            ancestor: None,
            ours: h1,
            theirs: h2,
            ours_ref: "main".into(),
            theirs_ref: "feature".into(),
            ours_deleted: false,
            theirs_deleted: false,
        };
        let res = Ours.resolve(&store, &mc).await.unwrap();
        assert!(matches!(res, Resolution::Resolved(h) if h == h1));
    }

    #[tokio::test]
    async fn theirs_strategy_takes_theirs() {
        let store = MemoryStore::new();
        let h1 = import::import_xml(&store, "<a>ours</a>").await.unwrap();
        let h2 = import::import_xml(&store, "<a>theirs</a>").await.unwrap();
        let mc = MergeConflict {
            path: "f.xml".into(),
            ancestor: None,
            ours: h1,
            theirs: h2,
            ours_ref: "main".into(),
            theirs_ref: "feature".into(),
            ours_deleted: false,
            theirs_deleted: false,
        };
        let res = Theirs.resolve(&store, &mc).await.unwrap();
        assert!(matches!(res, Resolution::Resolved(h) if h == h2));
    }

    #[tokio::test]
    async fn manual_strategy_produces_divergence() {
        let store = MemoryStore::new();
        let h = import::import_xml(&store, "<a/>").await.unwrap();
        let mc = MergeConflict {
            path: "f.xml".into(),
            ancestor: None,
            ours: h,
            theirs: h,
            ours_ref: "main".into(),
            theirs_ref: "feature".into(),
            ours_deleted: false,
            theirs_deleted: false,
        };
        let res = Manual.resolve(&store, &mc).await.unwrap();
        assert!(matches!(res, Resolution::Divergence));
    }

    #[tokio::test]
    async fn key_children_by_id() {
        let store = MemoryStore::new();
        let doc = import::import_xml(
            &store,
            r#"<root><child id="a">one</child><child id="b">two</child></root>"#,
        )
        .await
        .unwrap();

        let doc_obj = store.get(&doc).await.unwrap().unwrap();
        let root_hash = match doc_obj {
            Object::Document(d) => d.root,
            _ => panic!("expected document"),
        };
        let root_obj = store.get(&root_hash).await.unwrap().unwrap();
        let children = match root_obj {
            Object::Element(e) => e.children,
            _ => panic!("expected element"),
        };

        let keyed = key_children(&store, &children).await.unwrap();
        assert_eq!(keyed.len(), 2);
        assert!(
            matches!(&keyed[0].0, ChildKey::ElementById(n, id) if n == "child" && id == "a")
        );
        assert!(
            matches!(&keyed[1].0, ChildKey::ElementById(n, id) if n == "child" && id == "b")
        );
    }

    #[tokio::test]
    async fn merge_documents_short_circuit_equal() {
        let store = MemoryStore::new();
        let h = import::import_xml(&store, "<root>same</root>")
            .await
            .unwrap();
        let result = merge_documents(&store, h, h, h).await.unwrap();
        assert_eq!(result, Some(h));
    }

    #[tokio::test]
    async fn merge_documents_one_side_changed() {
        let store = MemoryStore::new();
        let anc = import::import_xml(&store, "<root>base</root>")
            .await
            .unwrap();
        let modified = import::import_xml(&store, "<root>modified</root>")
            .await
            .unwrap();

        let r1 = merge_documents(&store, anc, modified, anc).await.unwrap();
        assert_eq!(r1, Some(modified));

        let r2 = merge_documents(&store, anc, anc, modified).await.unwrap();
        assert_eq!(r2, Some(modified));
    }

    #[tokio::test]
    async fn merge_trees_non_overlapping() {
        let store = MemoryStore::new();
        let a1 = import::import_xml(&store, "<a>original</a>").await.unwrap();
        let b1 = import::import_xml(&store, "<b>original</b>").await.unwrap();
        let a2 = import::import_xml(&store, "<a>ours</a>").await.unwrap();
        let b2 = import::import_xml(&store, "<b>theirs</b>").await.unwrap();

        let ancestor = TreeObject::new(vec![
            TreeEntry {
                path: "a.xml".into(),
                document: a1,
            },
            TreeEntry {
                path: "b.xml".into(),
                document: b1,
            },
        ]);
        let ours_tree = TreeObject::new(vec![
            TreeEntry {
                path: "a.xml".into(),
                document: a2,
            },
            TreeEntry {
                path: "b.xml".into(),
                document: b1,
            },
        ]);
        let theirs_tree = TreeObject::new(vec![
            TreeEntry {
                path: "a.xml".into(),
                document: a1,
            },
            TreeEntry {
                path: "b.xml".into(),
                document: b2,
            },
        ]);

        let policy = MergePolicy {
            default: Box::new(AutoMerge),
            file_overrides: vec![],
        };
        let ch = ContentHash::from_canonical(b"commit");

        let result = merge_trees(
            &store,
            &ancestor,
            &ours_tree,
            &theirs_tree,
            &policy,
            "main",
            "feature",
            ch,
            ch,
            ch,
        )
        .await
        .unwrap();

        assert!(result.conflicts.is_empty());
        assert_eq!(result.ours_only.len(), 1);
        assert_eq!(result.theirs_only.len(), 1);
    }

    // --- merge_attributes tests ---

    #[test]
    fn attr_merge_both_changed_same_value() {
        let anc = vec![Attribute {
            local_name: "x".into(),
            namespace_uri: None,
            namespace_prefix: None,
            value: "old".into(),
        }];
        let ours = vec![Attribute {
            local_name: "x".into(),
            namespace_uri: None,
            namespace_prefix: None,
            value: "new".into(),
        }];
        let theirs = ours.clone();
        let merged = merge_attributes(&anc, &ours, &theirs).unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].value, "new");
    }

    #[test]
    fn attr_merge_both_changed_different_values_is_conflict() {
        let anc = vec![Attribute {
            local_name: "x".into(),
            namespace_uri: None,
            namespace_prefix: None,
            value: "old".into(),
        }];
        let ours = vec![Attribute {
            local_name: "x".into(),
            namespace_uri: None,
            namespace_prefix: None,
            value: "A".into(),
        }];
        let theirs = vec![Attribute {
            local_name: "x".into(),
            namespace_uri: None,
            namespace_prefix: None,
            value: "B".into(),
        }];
        assert!(merge_attributes(&anc, &ours, &theirs).is_none());
    }

    #[test]
    fn attr_merge_one_side_adds() {
        let anc = vec![];
        let ours = vec![Attribute {
            local_name: "x".into(),
            namespace_uri: None,
            namespace_prefix: None,
            value: "new".into(),
        }];
        let merged = merge_attributes(&anc, &ours, &anc).unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].value, "new");
    }

    #[test]
    fn attr_merge_one_side_deletes_unchanged() {
        let anc = vec![Attribute {
            local_name: "x".into(),
            namespace_uri: None,
            namespace_prefix: None,
            value: "v".into(),
        }];
        let merged = merge_attributes(&anc, &[], &anc).unwrap();
        assert!(merged.is_empty());
    }

    #[test]
    fn attr_merge_delete_vs_modify_is_conflict() {
        let anc = vec![Attribute {
            local_name: "x".into(),
            namespace_uri: None,
            namespace_prefix: None,
            value: "v".into(),
        }];
        let theirs = vec![Attribute {
            local_name: "x".into(),
            namespace_uri: None,
            namespace_prefix: None,
            value: "modified".into(),
        }];
        assert!(merge_attributes(&anc, &[], &theirs).is_none());
    }

    #[test]
    fn attr_merge_ordering_is_deterministic() {
        let anc = vec![];
        let ours = vec![
            Attribute {
                local_name: "b".into(),
                namespace_uri: None,
                namespace_prefix: None,
                value: "1".into(),
            },
            Attribute {
                local_name: "a".into(),
                namespace_uri: None,
                namespace_prefix: None,
                value: "2".into(),
            },
        ];
        let theirs = vec![Attribute {
            local_name: "c".into(),
            namespace_uri: None,
            namespace_prefix: None,
            value: "3".into(),
        }];
        let m1 = merge_attributes(&anc, &ours, &theirs).unwrap();
        let m2 = merge_attributes(&anc, &ours, &theirs).unwrap();
        // Same order every time.
        assert_eq!(m1[0].local_name, m2[0].local_name);
        assert_eq!(m1[1].local_name, m2[1].local_name);
        assert_eq!(m1[2].local_name, m2[2].local_name);
        // Ours order first, then theirs.
        assert_eq!(m1[0].local_name, "b");
        assert_eq!(m1[1].local_name, "a");
        assert_eq!(m1[2].local_name, "c");
    }

    // --- merge_elements tests ---

    #[tokio::test]
    async fn merge_elements_non_overlapping_children() {
        let store = MemoryStore::new();
        let anc = import::import_xml(
            &store,
            r#"<root><a id="a">base</a><b id="b">base</b></root>"#,
        )
        .await
        .unwrap();
        let ours = import::import_xml(
            &store,
            r#"<root><a id="a">ours</a><b id="b">base</b></root>"#,
        )
        .await
        .unwrap();
        let theirs = import::import_xml(
            &store,
            r#"<root><a id="a">base</a><b id="b">theirs</b></root>"#,
        )
        .await
        .unwrap();
        let result = merge_documents(&store, anc, ours, theirs).await.unwrap();
        assert!(result.is_some(), "non-overlapping edits should auto-merge");

        // Structurally verify: the merged root should have child "a" with
        // text "ours" and child "b" with text "theirs".
        let root = doc_root_element(&store, result.unwrap()).await;
        let (a_hash, _) = find_child_element(&store, &root, "a")
            .await
            .expect("merged doc should have <a>");
        assert_eq!(
            collect_text(&store, a_hash).await,
            "ours",
            "child <a> should have ours' text"
        );
        let (b_hash, _) = find_child_element(&store, &root, "b")
            .await
            .expect("merged doc should have <b>");
        assert_eq!(
            collect_text(&store, b_hash).await,
            "theirs",
            "child <b> should have theirs' text"
        );
    }

    #[tokio::test]
    async fn merge_elements_both_changed_same_child_is_conflict() {
        let store = MemoryStore::new();
        let anc = import::import_xml(&store, r#"<root><p id="p">base</p></root>"#)
            .await
            .unwrap();
        let ours = import::import_xml(&store, r#"<root><p id="p">ours</p></root>"#)
            .await
            .unwrap();
        let theirs = import::import_xml(&store, r#"<root><p id="p">theirs</p></root>"#)
            .await
            .unwrap();
        let result = merge_documents(&store, anc, ours, theirs).await.unwrap();
        assert!(
            result.is_none(),
            "both changing same text child should conflict"
        );
    }

    #[tokio::test]
    async fn merge_elements_child_added_on_theirs_only() {
        let store = MemoryStore::new();
        let anc = import::import_xml(&store, r#"<root><a id="a">x</a></root>"#)
            .await
            .unwrap();
        let ours = anc; // unchanged
        let theirs = import::import_xml(
            &store,
            r#"<root><a id="a">x</a><b id="b">new</b></root>"#,
        )
        .await
        .unwrap();
        let result = merge_documents(&store, anc, ours, theirs).await.unwrap();
        assert!(result.is_some());

        // Structurally verify: merged root should have child <b id="b">
        // with text "new" from theirs' addition.
        let root = doc_root_element(&store, result.unwrap()).await;
        let (b_hash, b_el) = find_child_element(&store, &root, "b")
            .await
            .expect("merged doc should have <b> from theirs");
        assert_eq!(attr_value(&b_el, "id"), Some("b"));
        assert_eq!(
            collect_text(&store, b_hash).await,
            "new",
            "child <b> should have theirs' text"
        );
    }

    #[tokio::test]
    async fn merge_elements_name_mismatch_is_conflict() {
        let store = MemoryStore::new();
        // All three sides have different root element names.
        let anc = import::import_xml(&store, "<root/>").await.unwrap();
        let ours = import::import_xml(&store, "<alpha/>").await.unwrap();
        let theirs = import::import_xml(&store, "<beta/>").await.unwrap();
        let anc_doc = store.get(&anc).await.unwrap().unwrap();
        let ours_doc = store.get(&ours).await.unwrap().unwrap();
        let theirs_doc = store.get(&theirs).await.unwrap().unwrap();
        let (Object::Document(ad), Object::Document(od), Object::Document(td)) =
            (&anc_doc, &ours_doc, &theirs_doc)
        else {
            panic!("expected documents");
        };
        let r = merge_elements(&store, ad.root, od.root, td.root)
            .await
            .unwrap();
        assert!(r.is_none(), "different element names should conflict");
    }

    // --- AutoMerge delete-vs-modify tests ---

    #[tokio::test]
    async fn auto_merge_delete_vs_modify_is_divergence() {
        let store = MemoryStore::new();
        let anc = import::import_xml(&store, "<a>base</a>").await.unwrap();
        let modified = import::import_xml(&store, "<a>changed</a>").await.unwrap();
        let mc = MergeConflict {
            path: "f.xml".into(),
            ancestor: Some(anc),
            ours: anc,
            theirs: modified,
            ours_ref: "main".into(),
            theirs_ref: "feature".into(),
            ours_deleted: true,
            theirs_deleted: false,
        };
        let res = AutoMerge.resolve(&store, &mc).await.unwrap();
        assert!(
            matches!(res, Resolution::Divergence),
            "delete-vs-modify should produce divergence"
        );
    }

    #[tokio::test]
    async fn ours_delete_vs_modify_is_divergence() {
        let store = MemoryStore::new();
        let anc = import::import_xml(&store, "<a>base</a>").await.unwrap();
        let modified = import::import_xml(&store, "<a>changed</a>").await.unwrap();
        let mc = MergeConflict {
            path: "f.xml".into(),
            ancestor: Some(anc),
            ours: anc,
            theirs: modified,
            ours_ref: "main".into(),
            theirs_ref: "feature".into(),
            ours_deleted: true,
            theirs_deleted: false,
        };
        // Ours strategy must NOT silently return the ancestor hash
        // (which would resurrect the deleted file). It should diverge.
        let res = Ours.resolve(&store, &mc).await.unwrap();
        assert!(
            matches!(res, Resolution::Divergence),
            "ours strategy on delete-vs-modify should produce divergence, not resurrect"
        );
    }

    #[tokio::test]
    async fn theirs_delete_vs_modify_is_divergence() {
        let store = MemoryStore::new();
        let anc = import::import_xml(&store, "<a>base</a>").await.unwrap();
        let modified = import::import_xml(&store, "<a>changed</a>").await.unwrap();
        let mc = MergeConflict {
            path: "f.xml".into(),
            ancestor: Some(anc),
            ours: modified,
            theirs: anc,
            ours_ref: "main".into(),
            theirs_ref: "feature".into(),
            ours_deleted: false,
            theirs_deleted: true,
        };
        let res = Theirs.resolve(&store, &mc).await.unwrap();
        assert!(
            matches!(res, Resolution::Divergence),
            "theirs strategy on delete-vs-modify should produce divergence, not resurrect"
        );
    }

    // --- key_children tests ---

    #[tokio::test]
    async fn key_children_positional() {
        let store = MemoryStore::new();
        let doc = import::import_xml(&store, "<root><p>one</p><p>two</p></root>")
            .await
            .unwrap();
        let doc_obj = store.get(&doc).await.unwrap().unwrap();
        let Object::Document(d) = doc_obj else {
            panic!("expected document");
        };
        let root_obj = store.get(&d.root).await.unwrap().unwrap();
        let Object::Element(el) = root_obj else {
            panic!("expected element");
        };
        let keyed = key_children(&store, &el.children).await.unwrap();
        assert_eq!(keyed.len(), 2);
        assert!(matches!(
            &keyed[0].0,
            ChildKey::ElementByPos(n, 0) if n == "p"
        ));
        assert!(matches!(
            &keyed[1].0,
            ChildKey::ElementByPos(n, 1) if n == "p"
        ));
    }

    #[tokio::test]
    async fn key_children_mixed_types() {
        let store = MemoryStore::new();
        let doc = import::import_xml(
            &store,
            r#"<root>text<!-- comment --><child id="c"/></root>"#,
        )
        .await
        .unwrap();
        let doc_obj = store.get(&doc).await.unwrap().unwrap();
        let Object::Document(d) = doc_obj else {
            panic!("expected document");
        };
        let root_obj = store.get(&d.root).await.unwrap().unwrap();
        let Object::Element(el) = root_obj else {
            panic!("expected element");
        };
        let keyed = key_children(&store, &el.children).await.unwrap();
        assert_eq!(keyed.len(), 3);
        assert!(matches!(&keyed[0].0, ChildKey::Text(0)));
        assert!(matches!(&keyed[1].0, ChildKey::Comment(0)));
        assert!(matches!(
            &keyed[2].0,
            ChildKey::ElementById(n, id) if n == "child" && id == "c"
        ));
    }

    // --- Repo::merge integration tests ---

    #[tokio::test]
    async fn repo_merge_fast_forward() {
        let store = MemoryStore::new();
        let repo = crate::repo::Repo::init(store);
        let author = Author {
            name: "T".into(),
            email: "t@t".into(),
        };

        let h1 = repo.import_xml("<root>v1</root>").await.unwrap();
        let t1 = repo.build_tree(vec![("f.xml".into(), h1)]).await.unwrap();
        let _c1 = repo.commit("main", t1, &author, "init").await.unwrap();

        // Create feature branch, add a commit.
        let branches = repo.list_branches().await.unwrap();
        let main_tip = branches.iter().find(|(n, _)| n == "main").unwrap().1;
        repo.create_branch("feature", main_tip).await.unwrap();

        let h2 = repo.import_xml("<root>v2</root>").await.unwrap();
        let t2 = repo.build_tree(vec![("f.xml".into(), h2)]).await.unwrap();
        let c2 = repo.commit("feature", t2, &author, "feat").await.unwrap();

        let policy = MergePolicy {
            default: Box::new(AutoMerge),
            file_overrides: vec![],
        };
        let outcome = repo
            .merge("main", "feature", &author, "merge", &policy)
            .await
            .unwrap();
        assert!(
            matches!(outcome, MergeOutcome::FastForward { commit } if commit == c2)
        );
    }

    #[tokio::test]
    async fn repo_merge_creates_two_parent_commit() {
        let store = MemoryStore::new();
        let repo = crate::repo::Repo::init(store);
        let author = Author {
            name: "T".into(),
            email: "t@t".into(),
        };

        // Initial commit on main.
        let h1 = repo.import_xml("<a>base</a>").await.unwrap();
        let h2 = repo.import_xml("<b>base</b>").await.unwrap();
        let t = repo
            .build_tree(vec![("a.xml".into(), h1), ("b.xml".into(), h2)])
            .await
            .unwrap();
        let _c1 = repo.commit("main", t, &author, "init").await.unwrap();

        // Branch.
        let branches = repo.list_branches().await.unwrap();
        let main_tip = branches.iter().find(|(n, _)| n == "main").unwrap().1;
        repo.create_branch("feature", main_tip).await.unwrap();

        // Diverge: ours changes a.xml.
        let ours_doc = repo.import_xml("<a>ours</a>").await.unwrap();
        let t_ours = repo
            .build_tree(vec![("a.xml".into(), ours_doc), ("b.xml".into(), h2)])
            .await
            .unwrap();
        let c_ours = repo.commit("main", t_ours, &author, "ours").await.unwrap();

        // Diverge: theirs changes b.xml.
        let theirs_doc = repo.import_xml("<b>theirs</b>").await.unwrap();
        let t_theirs = repo
            .build_tree(vec![("a.xml".into(), h1), ("b.xml".into(), theirs_doc)])
            .await
            .unwrap();
        let c_theirs = repo
            .commit("feature", t_theirs, &author, "theirs")
            .await
            .unwrap();

        let policy = MergePolicy {
            default: Box::new(AutoMerge),
            file_overrides: vec![],
        };
        let outcome = repo
            .merge("main", "feature", &author, "merge", &policy)
            .await
            .unwrap();

        if let MergeOutcome::Merged { commit, result } = outcome {
            assert!(result.conflicts.is_empty());
            // Verify merge commit has two parents.
            let (_, obj) = repo
                .log(commit, Some(1))
                .await
                .unwrap()
                .into_iter()
                .next()
                .unwrap();
            assert_eq!(obj.parents.len(), 2);
            assert!(obj.parents.contains(&c_ours));
            assert!(obj.parents.contains(&c_theirs));
        } else {
            panic!("expected Merged outcome");
        }
    }

    #[tokio::test]
    async fn repo_merge_up_to_date() {
        let store = MemoryStore::new();
        let repo = crate::repo::Repo::init(store);
        let author = Author {
            name: "T".into(),
            email: "t@t".into(),
        };
        let h = repo.import_xml("<r/>").await.unwrap();
        let t = repo.build_tree(vec![("f.xml".into(), h)]).await.unwrap();
        let _c = repo.commit("main", t, &author, "init").await.unwrap();
        let branches = repo.list_branches().await.unwrap();
        let tip = branches.iter().find(|(n, _)| n == "main").unwrap().1;
        repo.create_branch("feature", tip).await.unwrap();

        let policy = MergePolicy {
            default: Box::new(AutoMerge),
            file_overrides: vec![],
        };
        let outcome = repo
            .merge("main", "feature", &author, "merge", &policy)
            .await
            .unwrap();
        assert!(matches!(outcome, MergeOutcome::UpToDate));
    }

    // --- Property tests ---

    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// merge_elements(x, x, x) == Some(x) for any element.
        #[test]
        fn prop_merge_elements_reflexive(
            xml in crate::store::prop_strategies::arb_xml_document()
        ) {
            let rt = crate::store::prop_strategies::runtime();
            rt.block_on(async {
                let store = MemoryStore::new();
                let h = import::import_xml(&store, &xml).await;
                prop_assume!(h.is_ok(), "skip unparseable XML");
                let doc_hash = h.unwrap();
                let doc_obj = store.get(&doc_hash).await.unwrap().unwrap();
                let Object::Document(doc) = doc_obj else {
                    return Ok(());
                };
                let r = merge_elements(&store, doc.root, doc.root, doc.root)
                    .await
                    .unwrap();
                prop_assert_eq!(r, Some(doc.root));
                Ok(())
            })?;
        }

        /// merge_elements(anc, ours, anc) == Some(ours).
        #[test]
        fn prop_merge_elements_one_side_unchanged(
            xml_a in crate::store::prop_strategies::arb_xml_document(),
            xml_b in crate::store::prop_strategies::arb_xml_document(),
        ) {
            let rt = crate::store::prop_strategies::runtime();
            rt.block_on(async {
                let store = MemoryStore::new();
                let ha = import::import_xml(&store, &xml_a).await;
                let hb = import::import_xml(&store, &xml_b).await;
                prop_assume!(ha.is_ok() && hb.is_ok(), "skip unparseable");
                let doc_a = ha.unwrap();
                let doc_b = hb.unwrap();

                let r = merge_documents(&store, doc_a, doc_b, doc_a).await.unwrap();
                prop_assert_eq!(r, Some(doc_b), "only ours changed, should take ours");

                let r2 = merge_documents(&store, doc_a, doc_a, doc_b).await.unwrap();
                prop_assert_eq!(r2, Some(doc_b), "only theirs changed, should take theirs");
                Ok(())
            })?;
        }

        /// key_children is deterministic.
        #[test]
        fn prop_key_children_deterministic(
            xml in crate::store::prop_strategies::arb_xml_document()
        ) {
            let rt = crate::store::prop_strategies::runtime();
            rt.block_on(async {
                let store = MemoryStore::new();
                let h = import::import_xml(&store, &xml).await;
                prop_assume!(h.is_ok(), "skip unparseable");
                let doc_hash = h.unwrap();
                let doc_obj = store.get(&doc_hash).await.unwrap().unwrap();
                let Object::Document(doc) = doc_obj else {
                    return Ok(());
                };
                let root_obj = store.get(&doc.root).await.unwrap().unwrap();
                let Object::Element(el) = root_obj else {
                    return Ok(());
                };
                let k1 = key_children(&store, &el.children).await.unwrap();
                let k2 = key_children(&store, &el.children).await.unwrap();
                prop_assert_eq!(k1, k2, "key_children should be deterministic");
                Ok(())
            })?;
        }

        /// merge_attributes is commutative when both succeed.
        #[test]
        fn prop_merge_attributes_commutative(
            attrs_a in prop::collection::vec(crate::store::prop_strategies::arb_attribute(), 0..=3),
            attrs_b in prop::collection::vec(crate::store::prop_strategies::arb_attribute(), 0..=3),
        ) {
            let empty: Vec<Attribute> = vec![];
            let r1 = merge_attributes(&empty, &attrs_a, &attrs_b);
            let r2 = merge_attributes(&empty, &attrs_b, &attrs_a);
            match (r1, r2) {
                (Some(m1), Some(m2)) => {
                    // Same attribute set (may differ in order, compare as sets).
                    let mut s1: Vec<(String, String)> = m1
                        .iter()
                        .map(|a| (a.local_name.clone(), a.value.clone()))
                        .collect();
                    let mut s2: Vec<(String, String)> = m2
                        .iter()
                        .map(|a| (a.local_name.clone(), a.value.clone()))
                        .collect();
                    s1.sort();
                    s2.sort();
                    prop_assert_eq!(s1, s2, "commutative: same attributes in both orders");
                }
                (None, None) => {} // both conflict: OK
                _ => {
                    // One conflicts and the other doesn't: should not happen
                    // with empty ancestor (both adding).
                    // Actually it CAN happen if attrs_a and attrs_b both add
                    // the same key with different values. Then (a,b) = conflict,
                    // (b,a) = conflict. If only one conflicts, something is wrong.
                    prop_assert!(false, "asymmetric conflict result");
                }
            }
        }

        /// merge_documents roundtrip: if merge succeeds, result is importable.
        #[test]
        fn prop_merge_result_is_exportable(
            xml in crate::store::prop_strategies::arb_xml_document()
        ) {
            let rt = crate::store::prop_strategies::runtime();
            rt.block_on(async {
                let store = MemoryStore::new();
                let h = import::import_xml(&store, &xml).await;
                prop_assume!(h.is_ok(), "skip unparseable");
                let doc = h.unwrap();
                // merge(doc, doc, doc) should return doc, and it should be exportable.
                let r = merge_documents(&store, doc, doc, doc).await.unwrap();
                prop_assert!(r.is_some());
                let exported = export::export_xml(&store, r.unwrap()).await;
                prop_assert!(exported.is_ok(), "merged result should be exportable");
                Ok(())
            })?;
        }
    }

    // --- Sidecar divergence tests ---

    #[tokio::test]
    async fn divergence_stored_as_sidecar() {
        let store = MemoryStore::new();
        let a = import::import_xml(&store, "<root>base</root>").await.unwrap();
        let o = import::import_xml(&store, "<root>ours</root>").await.unwrap();
        let t = import::import_xml(&store, "<root>theirs</root>").await.unwrap();

        let ancestor_tree = TreeObject::new(vec![TreeEntry {
            path: "doc.xml".into(),
            document: a,
        }]);
        let ours_tree = TreeObject::new(vec![TreeEntry {
            path: "doc.xml".into(),
            document: o,
        }]);
        let theirs_tree = TreeObject::new(vec![TreeEntry {
            path: "doc.xml".into(),
            document: t,
        }]);

        let policy = MergePolicy {
            default: Box::new(Manual),
            file_overrides: vec![],
        };
        let ch = ContentHash::from_canonical(b"c");

        let result = merge_trees(
            &store,
            &ancestor_tree,
            &ours_tree,
            &theirs_tree,
            &policy,
            "main",
            "feature",
            ch,
            ch,
            ch,
        )
        .await
        .unwrap();

        // Should have one conflict.
        assert_eq!(result.conflicts.len(), 1);

        // Load the merged tree and check its entries.
        let tree_obj = store.get(&result.tree).await.unwrap().unwrap();
        let Object::Tree(tree) = tree_obj else {
            panic!("expected tree");
        };

        // Original document kept at its path with ours' content.
        let doc_entry = tree.get("doc.xml").unwrap();
        assert_eq!(doc_entry.document, o, "original path should keep ours");

        // Divergence stored under sidecar prefix.
        assert!(tree_has_divergences(&tree));
        let divs = list_divergence_entries(&tree);
        assert_eq!(divs.len(), 1);
        assert!(
            divs[0].path.starts_with(DIVERGENCE_PREFIX),
            "divergence path should start with prefix"
        );
        assert!(
            divs[0].path.starts_with(&format!("{DIVERGENCE_PREFIX}doc.xml.")),
            "divergence path should be {DIVERGENCE_PREFIX}doc.xml.{{hash}}, got: {}",
            divs[0].path
        );

        // Structural validation: divergence document must be detectable
        // by the conflict detection infrastructure.
        let has = crate::conflict::has_conflicts(&store, divs[0].document)
            .await
            .unwrap();
        assert!(has, "divergence document should be detected by has_conflicts");

        let conflicts = crate::conflict::list_conflicts(&store, divs[0].document)
            .await
            .unwrap();
        assert_eq!(conflicts.len(), 1, "should find one conflict in divergence doc");
        assert_eq!(
            conflicts[0].path, "doc.xml",
            "conflict should reference original path"
        );

        // Structurally verify divergence document content via DAG traversal.
        let div_root = doc_root_element(&store, divs[0].document).await;
        assert_eq!(div_root.local_name, "divergence");
        assert_eq!(
            div_root.namespace_uri.as_deref(),
            Some(crate::object::REPO_NS)
        );
        assert_eq!(attr_value(&div_root, "path"), Some("doc.xml"));

        // Ancestor element should contain the base content.
        let (anc_hash, _anc_el) = find_child_element(&store, &div_root, "ancestor")
            .await
            .expect("divergence should have ancestor element");
        let anc_text = collect_text(&store, anc_hash).await;
        assert!(
            anc_text.contains("base"),
            "ancestor should embed base content: {anc_text}"
        );

        // Two side elements, one with ours' content, one with theirs'.
        let sides = find_child_elements(&store, &div_root, "side").await;
        assert_eq!(sides.len(), 2, "divergence should have two side elements");
        let side_texts: Vec<String> = {
            let mut texts = Vec::new();
            for (hash, _) in &sides {
                texts.push(collect_text(&store, *hash).await);
            }
            texts
        };
        assert!(
            side_texts.iter().any(|t| t.contains("ours")),
            "one side should contain ours: {side_texts:?}"
        );
        assert!(
            side_texts.iter().any(|t| t.contains("theirs")),
            "one side should contain theirs: {side_texts:?}"
        );
    }

    #[tokio::test]
    async fn delete_vs_modify_divergence_is_detectable() {
        // Full merge_trees path: ours deletes doc.xml, theirs modifies it.
        // Verify the divergence sidecar is structurally valid.
        let store = MemoryStore::new();
        let base = import::import_xml(&store, "<root>base</root>").await.unwrap();
        let modified = import::import_xml(&store, "<root>modified</root>")
            .await
            .unwrap();

        let ancestor_tree = TreeObject::new(vec![TreeEntry {
            path: "doc.xml".into(),
            document: base,
        }]);
        let ours_tree = TreeObject::new(vec![]);
        let theirs_tree = TreeObject::new(vec![TreeEntry {
            path: "doc.xml".into(),
            document: modified,
        }]);

        let policy = MergePolicy {
            default: Box::new(AutoMerge),
            file_overrides: vec![],
        };
        let ch = ContentHash::from_canonical(b"c");

        let result = merge_trees(
            &store,
            &ancestor_tree,
            &ours_tree,
            &theirs_tree,
            &policy,
            "main",
            "feature",
            ch,
            ch,
            ch,
        )
        .await
        .unwrap();

        assert_eq!(result.conflicts.len(), 1);

        let tree_obj = store.get(&result.tree).await.unwrap().unwrap();
        let Object::Tree(tree) = tree_obj else {
            panic!("expected tree");
        };

        // Tree-level detection.
        assert!(tree_has_divergences(&tree));

        // Document-level detection via conflict infrastructure.
        let divs = list_divergence_entries(&tree);
        assert_eq!(divs.len(), 1);

        let has = crate::conflict::has_conflicts(&store, divs[0].document)
            .await
            .unwrap();
        assert!(has, "delete-vs-modify divergence should be detected by has_conflicts");

        let conflicts = crate::conflict::list_conflicts(&store, divs[0].document)
            .await
            .unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].path, "doc.xml");
    }

    #[test]
    fn divergence_path_is_deterministic() {
        let a = ContentHash::from_canonical(b"ancestor");
        let o = ContentHash::from_canonical(b"ours");
        let t = ContentHash::from_canonical(b"theirs");
        let p1 = divergence_entry_path("doc.xml", Some(a), o, t);
        let p2 = divergence_entry_path("doc.xml", Some(a), o, t);
        assert_eq!(p1, p2);
    }

    #[test]
    fn divergence_path_differs_for_different_sides() {
        let a = ContentHash::from_canonical(b"ancestor");
        let o1 = ContentHash::from_canonical(b"ours-v1");
        let o2 = ContentHash::from_canonical(b"ours-v2");
        let t = ContentHash::from_canonical(b"theirs");
        let p1 = divergence_entry_path("doc.xml", Some(a), o1, t);
        let p2 = divergence_entry_path("doc.xml", Some(a), o2, t);
        assert_ne!(p1, p2, "different sides should produce different paths");
    }

    #[test]
    fn tree_has_divergences_empty() {
        let tree = TreeObject::new(vec![]);
        assert!(!tree_has_divergences(&tree));
    }

    #[test]
    fn tree_has_divergences_with_normal_files() {
        let h = ContentHash::from_canonical(b"doc");
        let tree = TreeObject::new(vec![TreeEntry {
            path: "overview.xml".into(),
            document: h,
        }]);
        assert!(!tree_has_divergences(&tree));
    }

    #[test]
    fn tree_has_divergences_detects_sidecar() {
        let h = ContentHash::from_canonical(b"doc");
        let tree = TreeObject::new(vec![
            TreeEntry {
                path: "overview.xml".into(),
                document: h,
            },
            TreeEntry {
                path: format!("{DIVERGENCE_PREFIX}overview.xml.abc123"),
                document: h,
            },
        ]);
        assert!(tree_has_divergences(&tree));
        assert_eq!(list_divergence_entries(&tree).len(), 1);
    }
}
