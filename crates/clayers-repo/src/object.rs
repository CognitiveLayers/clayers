//! Object model for the content-addressed Merkle DAG.
//!
//! All objects are content-addressed: identity = `SHA-256(ExclusiveC14N(xml_representation))`.
//! Content objects represent XML Infoset nodes. Versioning objects (commits,
//! tags, documents) are XML elements in `urn:clayers:repository`.

use chrono::{DateTime, Utc};
use clayers_xml::ContentHash;

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
    pub fn to_xml(&self) -> String {
        use std::fmt::Write;
        let mut xml = format!("<repo:tree xmlns:repo=\"{REPO_NS}\">");
        for entry in &self.entries {
            let _ = write!(
                xml,
                "<repo:entry path=\"{}\">{}</repo:entry>",
                xml_escape(&entry.path),
                entry.document
            );
        }
        xml.push_str("</repo:tree>");
        xml
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
    pub fn to_xml(&self) -> String {
        use std::fmt::Write;
        let mut xml = format!(
            "<repo:document xmlns:repo=\"{REPO_NS}\" version=\"1.0\" encoding=\"UTF-8\">\
             <repo:root>{}</repo:root>",
            self.root
        );
        for h in &self.prologue {
            let _ = write!(xml, "<repo:prologue>{h}</repo:prologue>");
        }
        xml.push_str("</repo:document>");
        xml
    }
}

impl CommitObject {
    /// Serialize to XML in `urn:clayers:repository` namespace.
    #[must_use]
    pub fn to_xml(&self) -> String {
        use std::fmt::Write;
        let mut xml = format!("<repo:commit xmlns:repo=\"{REPO_NS}\">");
        let _ = write!(xml, "<repo:tree>{}</repo:tree>", self.tree);
        for parent in &self.parents {
            let _ = write!(xml, "<repo:parent>{parent}</repo:parent>");
        }
        let _ = write!(
            xml,
            "<repo:author name=\"{}\" email=\"{}\"/>",
            xml_escape(&self.author.name),
            xml_escape(&self.author.email)
        );
        let _ = write!(
            xml,
            "<repo:timestamp>{}</repo:timestamp>",
            self.timestamp.format("%Y-%m-%dT%H:%M:%SZ")
        );
        let _ = write!(
            xml,
            "<repo:message>{}</repo:message>",
            xml_escape(&self.message)
        );
        xml.push_str("</repo:commit>");
        xml
    }
}

impl TagObject {
    /// Serialize to XML in `urn:clayers:repository` namespace.
    #[must_use]
    pub fn to_xml(&self) -> String {
        use std::fmt::Write;
        let mut xml = format!("<repo:tag xmlns:repo=\"{REPO_NS}\">");
        let _ = write!(xml, "<repo:target>{}</repo:target>", self.target);
        let _ = write!(
            xml,
            "<repo:name>{}</repo:name>",
            xml_escape(&self.name)
        );
        let _ = write!(
            xml,
            "<repo:tagger name=\"{}\" email=\"{}\"/>",
            xml_escape(&self.tagger.name),
            xml_escape(&self.tagger.email)
        );
        let _ = write!(
            xml,
            "<repo:timestamp>{}</repo:timestamp>",
            self.timestamp.format("%Y-%m-%dT%H:%M:%SZ")
        );
        let _ = write!(
            xml,
            "<repo:message>{}</repo:message>",
            xml_escape(&self.message)
        );
        xml.push_str("</repo:tag>");
        xml
    }
}

/// Escape special XML characters in text content and attribute values.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
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
    fn xml_escape_handles_special_chars() {
        assert_eq!(xml_escape("<>&\"'"), "&lt;&gt;&amp;&quot;&apos;");
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
