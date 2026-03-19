//! Object model for the content-addressed Merkle DAG.
//!
//! All objects are content-addressed: identity = `SHA-256(ExclusiveC14N(xml_representation))`.
//! Content objects represent XML Infoset nodes. Versioning objects (commits,
//! tags, documents) are XML elements in `urn:clayers:repository`.

use chrono::{DateTime, Utc};
use clayers_xml::ContentHash;
use xot::Xot;

/// The `urn:clayers:repository` namespace URI.
pub const REPO_NS: &str = "urn:clayers:repository";

/// An XML attribute with canonical namespace URI (not prefix).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Attribute {
    /// The attribute's local name.
    pub local_name: String,
    /// The attribute's namespace URI, if any.
    pub namespace_uri: Option<String>,
    /// The namespace prefix used in the original XML (e.g. "app" for `app:id`).
    #[cfg_attr(feature = "serde", serde(default))]
    pub namespace_prefix: Option<String>,
    /// The attribute value.
    pub value: String,
}

/// A person (commit author or tag tagger).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Author {
    /// Display name.
    pub name: String,
    /// Email address.
    pub email: String,
}

/// An element node in the Merkle DAG.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ElementObject {
    /// The element's local name.
    pub local_name: String,
    /// The element's namespace URI, if any.
    pub namespace_uri: Option<String>,
    /// The namespace prefix used in the original XML (e.g. "app" for `<app:item>`).
    #[cfg_attr(feature = "serde", serde(default))]
    pub namespace_prefix: Option<String>,
    /// Extra namespace declarations on this element for descendant convenience
    /// (prefix, URI pairs not used by this element itself).
    #[cfg_attr(feature = "serde", serde(default))]
    pub extra_namespaces: Vec<(String, String)>,
    /// Attributes in canonical order.
    pub attributes: Vec<Attribute>,
    /// Ordered child object hashes for graph traversal.
    pub children: Vec<ContentHash>,
    /// Inclusive C14N hash, indexed for drift detection compatibility.
    pub inclusive_hash: ContentHash,
}

/// A text node (character data).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextObject {
    /// The character data content.
    pub content: String,
}

/// A comment node.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CommentObject {
    /// The comment text.
    pub content: String,
}

/// A processing instruction node.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PIObject {
    /// The PI target.
    pub target: String,
    /// The PI data (optional).
    pub data: Option<String>,
}

/// A document object pointing to a root element.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DocumentObject {
    /// Hash of the root element object.
    pub root: ContentHash,
    /// Hashes of document-level children before the root element
    /// (comments, processing instructions). Preserves prologues.
    #[cfg_attr(feature = "serde", serde(default))]
    pub prologue: Vec<ContentHash>,
}

/// An entry in a tree object, mapping a file path to a document hash.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TreeEntry {
    /// File path (e.g., "overview.xml").
    pub path: String,
    /// Hash of the `DocumentObject` for this file.
    pub document: ContentHash,
}

/// A tree object mapping file paths to document hashes (like git's tree).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TreeObject {
    /// Entries sorted by path for deterministic hashing.
    pub entries: Vec<TreeEntry>,
}

impl TreeObject {
    /// Create a new tree with entries sorted by path.
    #[must_use]
    pub fn new(mut entries: Vec<TreeEntry>) -> Self {
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Self { entries }
    }

    /// Look up a document hash by path.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&TreeEntry> {
        self.entries.iter().find(|e| e.path == path)
    }

    /// Return sorted list of all paths in the tree.
    #[must_use]
    pub fn paths(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.path.as_str()).collect()
    }

    /// Serialize to XML in `urn:clayers:repository` namespace.
    #[must_use]
    #[allow(clippy::missing_panics_doc)]
    pub fn to_xml(&self) -> String {
        let mut xot = Xot::new();
        let ns = xot.add_namespace(REPO_NS);
        let prefix = xot.add_prefix("repo");
        let tree_name = xot.add_name_ns("tree", ns);
        let entry_name = xot.add_name_ns("entry", ns);
        let path_attr = xot.add_name("path");

        let tree_el = xot.new_element(tree_name);
        xot.namespaces_mut(tree_el).insert(prefix, ns);

        for entry in &self.entries {
            let entry_el = xot.new_element(entry_name);
            xot.attributes_mut(entry_el)
                .insert(path_attr, entry.path.clone());
            let text = xot.new_text(&entry.document.to_string());
            xot.append(entry_el, text).expect("append text");
            xot.append(tree_el, entry_el).expect("append entry");
        }

        xot.to_string(tree_el).expect("serialize tree")
    }
}

/// A commit object with parent references.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CommitObject {
    /// Hash of the tree object this commit snapshots.
    pub tree: ContentHash,
    /// Parent commit hashes (empty for initial commit, 2+ for multi-parent commits).
    pub parents: Vec<ContentHash>,
    /// The commit author.
    pub author: Author,
    /// Commit timestamp.
    pub timestamp: DateTime<Utc>,
    /// Commit message.
    pub message: String,
}

/// An annotated tag object.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TagObject {
    /// Hash of the tagged object (usually a commit).
    pub target: ContentHash,
    /// Tag name.
    pub name: String,
    /// The tagger.
    pub tagger: Author,
    /// Tag timestamp.
    pub timestamp: DateTime<Utc>,
    /// Tag message.
    pub message: String,
}

/// A content-addressed object in the Merkle DAG.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Object {
    /// An XML element with its subtree.
    Element(ElementObject),
    /// Character data.
    Text(TextObject),
    /// An XML comment.
    Comment(CommentObject),
    /// A processing instruction.
    PI(PIObject),
    /// A document root pointer.
    Document(DocumentObject),
    /// A tree mapping file paths to documents.
    Tree(TreeObject),
    /// A commit (versioning).
    Commit(CommitObject),
    /// An annotated tag (versioning).
    Tag(TagObject),
}

impl DocumentObject {
    /// Serialize to XML in `urn:clayers:repository` namespace.
    #[must_use]
    #[allow(clippy::missing_panics_doc)]
    pub fn to_xml(&self) -> String {
        let mut xot = Xot::new();
        let ns = xot.add_namespace(REPO_NS);
        let prefix = xot.add_prefix("repo");
        let doc_name = xot.add_name_ns("document", ns);
        let root_name = xot.add_name_ns("root", ns);
        let prologue_name = xot.add_name_ns("prologue", ns);
        let version_attr = xot.add_name("version");
        let encoding_attr = xot.add_name("encoding");

        let doc_el = xot.new_element(doc_name);
        xot.namespaces_mut(doc_el).insert(prefix, ns);
        xot.attributes_mut(doc_el)
            .insert(encoding_attr, "UTF-8".into());
        xot.attributes_mut(doc_el)
            .insert(version_attr, "1.0".into());

        let root_el = xot.new_element(root_name);
        let root_text = xot.new_text(&self.root.to_string());
        xot.append(root_el, root_text).expect("append text");
        xot.append(doc_el, root_el).expect("append root");

        for h in &self.prologue {
            let prologue_el = xot.new_element(prologue_name);
            let text = xot.new_text(&h.to_string());
            xot.append(prologue_el, text).expect("append text");
            xot.append(doc_el, prologue_el).expect("append prologue");
        }

        xot.to_string(doc_el).expect("serialize document")
    }
}

impl CommitObject {
    /// Serialize to XML in `urn:clayers:repository` namespace.
    #[must_use]
    #[allow(clippy::missing_panics_doc)]
    pub fn to_xml(&self) -> String {
        let mut xot = Xot::new();
        let ns = xot.add_namespace(REPO_NS);
        let prefix = xot.add_prefix("repo");
        let commit_name = xot.add_name_ns("commit", ns);
        let tree_name = xot.add_name_ns("tree", ns);
        let parent_name = xot.add_name_ns("parent", ns);
        let author_name = xot.add_name_ns("author", ns);
        let timestamp_name = xot.add_name_ns("timestamp", ns);
        let message_name = xot.add_name_ns("message", ns);
        let name_attr = xot.add_name("name");
        let email_attr = xot.add_name("email");

        let commit_el = xot.new_element(commit_name);
        xot.namespaces_mut(commit_el).insert(prefix, ns);

        // <repo:tree>
        let tree_el = xot.new_element(tree_name);
        let text = xot.new_text(&self.tree.to_string());
        xot.append(tree_el, text).expect("append text");
        xot.append(commit_el, tree_el).expect("append tree");

        // <repo:parent>
        for p in &self.parents {
            let parent_el = xot.new_element(parent_name);
            let text = xot.new_text(&p.to_string());
            xot.append(parent_el, text).expect("append text");
            xot.append(commit_el, parent_el).expect("append parent");
        }

        // <repo:author name="..." email="..."/>
        let author_el = xot.new_element(author_name);
        xot.attributes_mut(author_el)
            .insert(email_attr, self.author.email.clone());
        xot.attributes_mut(author_el)
            .insert(name_attr, self.author.name.clone());
        xot.append(commit_el, author_el).expect("append author");

        // <repo:timestamp>
        let ts_el = xot.new_element(timestamp_name);
        let ts_text = xot.new_text(&self.timestamp.format("%Y-%m-%dT%H:%M:%SZ").to_string());
        xot.append(ts_el, ts_text).expect("append text");
        xot.append(commit_el, ts_el).expect("append timestamp");

        // <repo:message>
        let msg_el = xot.new_element(message_name);
        let msg_text = xot.new_text(&self.message);
        xot.append(msg_el, msg_text).expect("append text");
        xot.append(commit_el, msg_el).expect("append message");

        xot.to_string(commit_el).expect("serialize commit")
    }
}

impl TagObject {
    /// Serialize to XML in `urn:clayers:repository` namespace.
    #[must_use]
    #[allow(clippy::missing_panics_doc)]
    pub fn to_xml(&self) -> String {
        let mut xot = Xot::new();
        let ns = xot.add_namespace(REPO_NS);
        let prefix = xot.add_prefix("repo");
        let tag_name = xot.add_name_ns("tag", ns);
        let target_name = xot.add_name_ns("target", ns);
        let name_name = xot.add_name_ns("name", ns);
        let tagger_name = xot.add_name_ns("tagger", ns);
        let timestamp_name = xot.add_name_ns("timestamp", ns);
        let message_name = xot.add_name_ns("message", ns);
        let name_attr = xot.add_name("name");
        let email_attr = xot.add_name("email");

        let tag_el = xot.new_element(tag_name);
        xot.namespaces_mut(tag_el).insert(prefix, ns);

        // <repo:target>
        let target_el = xot.new_element(target_name);
        let text = xot.new_text(&self.target.to_string());
        xot.append(target_el, text).expect("append text");
        xot.append(tag_el, target_el).expect("append target");

        // <repo:name>
        let name_el = xot.new_element(name_name);
        let name_text = xot.new_text(&self.name);
        xot.append(name_el, name_text).expect("append text");
        xot.append(tag_el, name_el).expect("append name");

        // <repo:tagger name="..." email="..."/>
        let tagger_el = xot.new_element(tagger_name);
        xot.attributes_mut(tagger_el)
            .insert(email_attr, self.tagger.email.clone());
        xot.attributes_mut(tagger_el)
            .insert(name_attr, self.tagger.name.clone());
        xot.append(tag_el, tagger_el).expect("append tagger");

        // <repo:timestamp>
        let ts_el = xot.new_element(timestamp_name);
        let ts_text = xot.new_text(&self.timestamp.format("%Y-%m-%dT%H:%M:%SZ").to_string());
        xot.append(ts_el, ts_text).expect("append text");
        xot.append(tag_el, ts_el).expect("append timestamp");

        // <repo:message>
        let msg_el = xot.new_element(message_name);
        let msg_text = xot.new_text(&self.message);
        xot.append(msg_el, msg_text).expect("append text");
        xot.append(tag_el, msg_el).expect("append message");

        xot.to_string(tag_el).expect("serialize tag")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_to_xml_contains_root_hash() {
        let hash = ContentHash::from_canonical(b"test");
        let doc = DocumentObject { root: hash, prologue: vec![] };
        let xml = doc.to_xml();
        assert!(xml.contains(&hash.to_string()));
        assert!(xml.contains(REPO_NS));
    }

    #[test]
    fn commit_to_xml_contains_all_fields() {
        let hash = ContentHash::from_canonical(b"test");
        let commit = CommitObject {
            tree: hash,
            parents: vec![hash],
            author: Author {
                name: "Alice".into(),
                email: "alice@example.com".into(),
            },
            timestamp: DateTime::parse_from_rfc3339("2026-03-17T10:30:00Z")
                .expect("valid timestamp")
                .to_utc(),
            message: "Test commit".into(),
        };
        let xml = commit.to_xml();
        assert!(xml.contains("repo:commit"));
        assert!(xml.contains("repo:tree"));
        assert!(xml.contains("repo:parent"));
        assert!(xml.contains("Alice"));
        assert!(xml.contains("Test commit"));
    }

    #[test]
    fn tree_sorts_entries() {
        let h1 = ContentHash::from_canonical(b"doc1");
        let h2 = ContentHash::from_canonical(b"doc2");
        let tree = TreeObject::new(vec![
            TreeEntry { path: "z.xml".into(), document: h1 },
            TreeEntry { path: "a.xml".into(), document: h2 },
        ]);
        assert_eq!(tree.entries[0].path, "a.xml");
        assert_eq!(tree.entries[1].path, "z.xml");
    }

    #[test]
    fn tree_get_by_path() {
        let h1 = ContentHash::from_canonical(b"doc1");
        let tree = TreeObject::new(vec![
            TreeEntry { path: "file.xml".into(), document: h1 },
        ]);
        assert!(tree.get("file.xml").is_some());
        assert_eq!(tree.get("file.xml").unwrap().document, h1);
    }

    #[test]
    fn tree_get_missing() {
        let tree = TreeObject::new(vec![]);
        assert!(tree.get("nonexistent.xml").is_none());
    }

    #[test]
    fn tree_to_xml_deterministic() {
        let h1 = ContentHash::from_canonical(b"doc1");
        let h2 = ContentHash::from_canonical(b"doc2");
        let tree1 = TreeObject::new(vec![
            TreeEntry { path: "z.xml".into(), document: h1 },
            TreeEntry { path: "a.xml".into(), document: h2 },
        ]);
        let tree2 = TreeObject::new(vec![
            TreeEntry { path: "a.xml".into(), document: h2 },
            TreeEntry { path: "z.xml".into(), document: h1 },
        ]);
        assert_eq!(tree1.to_xml(), tree2.to_xml());
    }

    #[test]
    fn tree_to_xml_empty() {
        let tree = TreeObject::new(vec![]);
        let xml = tree.to_xml();
        assert!(xml.contains("repo:tree"));
        assert!(!xml.contains("repo:entry"));
    }

    #[test]
    fn tree_to_xml_contains_entries() {
        let h = ContentHash::from_canonical(b"doc1");
        let tree = TreeObject::new(vec![
            TreeEntry { path: "file.xml".into(), document: h },
        ]);
        let xml = tree.to_xml();
        assert!(xml.contains("repo:entry"));
        assert!(xml.contains("path=\"file.xml\""));
        assert!(xml.contains(&h.to_string()));
    }

    #[test]
    fn tree_paths() {
        let h = ContentHash::from_canonical(b"doc1");
        let tree = TreeObject::new(vec![
            TreeEntry { path: "c.xml".into(), document: h },
            TreeEntry { path: "a.xml".into(), document: h },
            TreeEntry { path: "b.xml".into(), document: h },
        ]);
        assert_eq!(tree.paths(), vec!["a.xml", "b.xml", "c.xml"]);
    }

    #[test]
    fn tag_to_xml_contains_all_fields() {
        let hash = ContentHash::from_canonical(b"test");
        let tag = TagObject {
            target: hash,
            name: "v1.0".into(),
            tagger: Author {
                name: "Bob".into(),
                email: "bob@example.com".into(),
            },
            timestamp: DateTime::parse_from_rfc3339("2026-03-17T10:30:00Z")
                .expect("valid timestamp")
                .to_utc(),
            message: "Release v1.0".into(),
        };
        let xml = tag.to_xml();
        assert!(xml.contains("repo:tag"));
        assert!(xml.contains("v1.0"));
        assert!(xml.contains("Bob"));
    }
}
