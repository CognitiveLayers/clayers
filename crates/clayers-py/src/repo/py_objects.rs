//! PyO3 wrappers for the Rust Object model types from `clayers-repo`.
//!
//! These `Store*` types wrap the content-addressed Merkle DAG object types
//! so that Python store implementations can receive and return typed objects.
//! The `Store` prefix disambiguates from the existing porcelain types in
//! `objects.rs` (e.g. `CommitObject`, `TreeEntry`).

use pyo3::prelude::*;

use crate::xml::ContentHash;

// ---------------------------------------------------------------------------
// StoreAttribute
// ---------------------------------------------------------------------------

/// An XML attribute with canonical namespace URI.
#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct StoreAttribute {
    #[pyo3(get)]
    pub local_name: String,
    #[pyo3(get)]
    pub namespace_uri: Option<String>,
    #[pyo3(get)]
    pub namespace_prefix: Option<String>,
    #[pyo3(get)]
    pub value: String,
}

#[pymethods]
impl StoreAttribute {
    #[new]
    fn new(
        local_name: String,
        value: String,
        namespace_uri: Option<String>,
        namespace_prefix: Option<String>,
    ) -> Self {
        Self {
            local_name,
            namespace_uri,
            namespace_prefix,
            value,
        }
    }

    fn __repr__(&self) -> String {
        match (&self.namespace_prefix, &self.namespace_uri) {
            (Some(pfx), _) => {
                format!("StoreAttribute('{}:{}', '{}')", pfx, self.local_name, self.value)
            }
            _ => format!("StoreAttribute('{}', '{}')", self.local_name, self.value),
        }
    }
}

impl From<clayers_repo::object::Attribute> for StoreAttribute {
    fn from(a: clayers_repo::object::Attribute) -> Self {
        Self {
            local_name: a.local_name,
            namespace_uri: a.namespace_uri,
            namespace_prefix: a.namespace_prefix,
            value: a.value,
        }
    }
}

impl StoreAttribute {
    pub fn to_rust(&self) -> clayers_repo::object::Attribute {
        clayers_repo::object::Attribute {
            local_name: self.local_name.clone(),
            namespace_uri: self.namespace_uri.clone(),
            namespace_prefix: self.namespace_prefix.clone(),
            value: self.value.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// StoreAuthor
// ---------------------------------------------------------------------------

/// A person (commit author or tag tagger) for the store object layer.
#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct StoreAuthor {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub email: String,
}

#[pymethods]
impl StoreAuthor {
    #[new]
    fn new(name: String, email: String) -> Self {
        Self { name, email }
    }

    fn __repr__(&self) -> String {
        format!("StoreAuthor('{}', '{}')", self.name, self.email)
    }
}

impl From<clayers_repo::object::Author> for StoreAuthor {
    fn from(a: clayers_repo::object::Author) -> Self {
        Self {
            name: a.name,
            email: a.email,
        }
    }
}

impl StoreAuthor {
    pub fn to_rust(&self) -> clayers_repo::object::Author {
        clayers_repo::object::Author {
            name: self.name.clone(),
            email: self.email.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// StoreTextObject
// ---------------------------------------------------------------------------

/// A text node (character data) in the Merkle DAG.
#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct StoreTextObject {
    #[pyo3(get)]
    pub content: String,
}

#[pymethods]
impl StoreTextObject {
    #[new]
    fn new(content: String) -> Self {
        Self { content }
    }

    fn __repr__(&self) -> String {
        let truncated = if self.content.len() > 40 {
            format!("{}...", &self.content[..40])
        } else {
            self.content.clone()
        };
        format!("StoreTextObject('{truncated}')")
    }
}

impl From<clayers_repo::object::TextObject> for StoreTextObject {
    fn from(obj: clayers_repo::object::TextObject) -> Self {
        Self {
            content: obj.content,
        }
    }
}

impl StoreTextObject {
    pub fn to_rust(&self) -> clayers_repo::object::TextObject {
        clayers_repo::object::TextObject {
            content: self.content.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// StoreCommentObject
// ---------------------------------------------------------------------------

/// An XML comment node in the Merkle DAG.
#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct StoreCommentObject {
    #[pyo3(get)]
    pub content: String,
}

#[pymethods]
impl StoreCommentObject {
    #[new]
    fn new(content: String) -> Self {
        Self { content }
    }

    fn __repr__(&self) -> String {
        format!("StoreCommentObject('{}')", self.content)
    }
}

impl From<clayers_repo::object::CommentObject> for StoreCommentObject {
    fn from(obj: clayers_repo::object::CommentObject) -> Self {
        Self {
            content: obj.content,
        }
    }
}

impl StoreCommentObject {
    pub fn to_rust(&self) -> clayers_repo::object::CommentObject {
        clayers_repo::object::CommentObject {
            content: self.content.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// StorePIObject
// ---------------------------------------------------------------------------

/// A processing instruction node in the Merkle DAG.
#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct StorePIObject {
    #[pyo3(get)]
    pub target: String,
    #[pyo3(get)]
    pub data: Option<String>,
}

#[pymethods]
impl StorePIObject {
    #[new]
    fn new(target: String, data: Option<String>) -> Self {
        Self { target, data }
    }

    fn __repr__(&self) -> String {
        match &self.data {
            Some(d) => format!("StorePIObject('{}', '{}')", self.target, d),
            None => format!("StorePIObject('{}')", self.target),
        }
    }
}

impl From<clayers_repo::object::PIObject> for StorePIObject {
    fn from(obj: clayers_repo::object::PIObject) -> Self {
        Self {
            target: obj.target,
            data: obj.data,
        }
    }
}

impl StorePIObject {
    pub fn to_rust(&self) -> clayers_repo::object::PIObject {
        clayers_repo::object::PIObject {
            target: self.target.clone(),
            data: self.data.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// StoreElementObject
// ---------------------------------------------------------------------------

/// Namespace declaration pair (prefix, URI).
#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct NamespaceDecl {
    #[pyo3(get)]
    pub prefix: String,
    #[pyo3(get)]
    pub uri: String,
}

#[pymethods]
impl NamespaceDecl {
    #[new]
    fn new(prefix: String, uri: String) -> Self {
        Self { prefix, uri }
    }

    fn __repr__(&self) -> String {
        format!("NamespaceDecl('{}', '{}')", self.prefix, self.uri)
    }
}

/// An element node in the Merkle DAG.
#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct StoreElementObject {
    #[pyo3(get)]
    pub local_name: String,
    #[pyo3(get)]
    pub namespace_uri: Option<String>,
    #[pyo3(get)]
    pub namespace_prefix: Option<String>,
    #[pyo3(get)]
    pub extra_namespaces: Vec<NamespaceDecl>,
    #[pyo3(get)]
    pub attributes: Vec<StoreAttribute>,
    #[pyo3(get)]
    pub children: Vec<ContentHash>,
    #[pyo3(get)]
    pub inclusive_hash: ContentHash,
}

#[pymethods]
impl StoreElementObject {
    #[new]
    #[pyo3(signature = (local_name, inclusive_hash, namespace_uri=None, namespace_prefix=None, extra_namespaces=vec![], attributes=vec![], children=vec![]))]
    fn new(
        local_name: String,
        inclusive_hash: ContentHash,
        namespace_uri: Option<String>,
        namespace_prefix: Option<String>,
        extra_namespaces: Vec<NamespaceDecl>,
        attributes: Vec<StoreAttribute>,
        children: Vec<ContentHash>,
    ) -> Self {
        Self {
            local_name,
            namespace_uri,
            namespace_prefix,
            extra_namespaces,
            attributes,
            children,
            inclusive_hash,
        }
    }

    fn __repr__(&self) -> String {
        match (&self.namespace_prefix, &self.namespace_uri) {
            (Some(pfx), _) => {
                format!(
                    "StoreElementObject('{}:{}', children={})",
                    pfx,
                    self.local_name,
                    self.children.len()
                )
            }
            _ => format!(
                "StoreElementObject('{}', children={})",
                self.local_name,
                self.children.len()
            ),
        }
    }
}

impl From<clayers_repo::object::ElementObject> for StoreElementObject {
    fn from(obj: clayers_repo::object::ElementObject) -> Self {
        Self {
            local_name: obj.local_name,
            namespace_uri: obj.namespace_uri,
            namespace_prefix: obj.namespace_prefix,
            extra_namespaces: obj
                .extra_namespaces
                .into_iter()
                .map(|(pfx, uri)| NamespaceDecl {
                    prefix: pfx,
                    uri,
                })
                .collect(),
            attributes: obj.attributes.into_iter().map(StoreAttribute::from).collect(),
            children: obj
                .children
                .into_iter()
                .map(ContentHash::from_inner)
                .collect(),
            inclusive_hash: ContentHash::from_inner(obj.inclusive_hash),
        }
    }
}

impl StoreElementObject {
    pub fn to_rust(&self) -> clayers_repo::object::ElementObject {
        clayers_repo::object::ElementObject {
            local_name: self.local_name.clone(),
            namespace_uri: self.namespace_uri.clone(),
            namespace_prefix: self.namespace_prefix.clone(),
            extra_namespaces: self
                .extra_namespaces
                .iter()
                .map(|ns| (ns.prefix.clone(), ns.uri.clone()))
                .collect(),
            attributes: self.attributes.iter().map(|a| a.to_rust()).collect(),
            children: self.children.iter().map(|h| h.inner()).collect(),
            inclusive_hash: self.inclusive_hash.inner(),
        }
    }
}

// ---------------------------------------------------------------------------
// StoreDocumentObject
// ---------------------------------------------------------------------------

/// A document object pointing to a root element.
#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct StoreDocumentObject {
    #[pyo3(get)]
    pub root: ContentHash,
    #[pyo3(get)]
    pub prologue: Vec<ContentHash>,
}

#[pymethods]
impl StoreDocumentObject {
    #[new]
    #[pyo3(signature = (root, prologue=vec![]))]
    fn new(root: ContentHash, prologue: Vec<ContentHash>) -> Self {
        Self { root, prologue }
    }

    fn __repr__(&self) -> String {
        format!(
            "StoreDocumentObject(root={}, prologue={})",
            self.root.inner(),
            self.prologue.len()
        )
    }
}

impl From<clayers_repo::object::DocumentObject> for StoreDocumentObject {
    fn from(obj: clayers_repo::object::DocumentObject) -> Self {
        Self {
            root: ContentHash::from_inner(obj.root),
            prologue: obj
                .prologue
                .into_iter()
                .map(ContentHash::from_inner)
                .collect(),
        }
    }
}

impl StoreDocumentObject {
    pub fn to_rust(&self) -> clayers_repo::object::DocumentObject {
        clayers_repo::object::DocumentObject {
            root: self.root.inner(),
            prologue: self.prologue.iter().map(|h| h.inner()).collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// StoreTreeEntry
// ---------------------------------------------------------------------------

/// An entry in a tree object, mapping a file path to a document hash.
#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct StoreTreeEntry {
    #[pyo3(get)]
    pub path: String,
    #[pyo3(get)]
    pub document: ContentHash,
}

#[pymethods]
impl StoreTreeEntry {
    #[new]
    fn new(path: String, document: ContentHash) -> Self {
        Self { path, document }
    }

    fn __repr__(&self) -> String {
        format!("StoreTreeEntry('{}', {})", self.path, self.document.inner())
    }
}

impl From<clayers_repo::object::TreeEntry> for StoreTreeEntry {
    fn from(entry: clayers_repo::object::TreeEntry) -> Self {
        Self {
            path: entry.path,
            document: ContentHash::from_inner(entry.document),
        }
    }
}

impl StoreTreeEntry {
    pub fn to_rust(&self) -> clayers_repo::object::TreeEntry {
        clayers_repo::object::TreeEntry {
            path: self.path.clone(),
            document: self.document.inner(),
        }
    }
}

// ---------------------------------------------------------------------------
// StoreTreeObject
// ---------------------------------------------------------------------------

/// A tree object mapping file paths to document hashes.
#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct StoreTreeObject {
    #[pyo3(get)]
    pub entries: Vec<StoreTreeEntry>,
}

#[pymethods]
impl StoreTreeObject {
    #[new]
    fn new(entries: Vec<StoreTreeEntry>) -> Self {
        Self { entries }
    }

    fn __repr__(&self) -> String {
        format!("StoreTreeObject(entries={})", self.entries.len())
    }
}

impl From<clayers_repo::object::TreeObject> for StoreTreeObject {
    fn from(obj: clayers_repo::object::TreeObject) -> Self {
        Self {
            entries: obj.entries.into_iter().map(StoreTreeEntry::from).collect(),
        }
    }
}

impl StoreTreeObject {
    pub fn to_rust(&self) -> clayers_repo::object::TreeObject {
        clayers_repo::object::TreeObject {
            entries: self.entries.iter().map(|e| e.to_rust()).collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// StoreCommitObject
// ---------------------------------------------------------------------------

/// A commit object with parent references.
///
/// Timestamps are represented as ISO 8601 strings (RFC 3339) for Python
/// interoperability.
#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct StoreCommitObject {
    #[pyo3(get)]
    pub tree: ContentHash,
    #[pyo3(get)]
    pub parents: Vec<ContentHash>,
    #[pyo3(get)]
    pub author: StoreAuthor,
    /// ISO 8601 timestamp string.
    #[pyo3(get)]
    pub timestamp: String,
    #[pyo3(get)]
    pub message: String,
}

#[pymethods]
impl StoreCommitObject {
    #[new]
    fn new(
        tree: ContentHash,
        parents: Vec<ContentHash>,
        author: StoreAuthor,
        timestamp: String,
        message: String,
    ) -> Self {
        Self {
            tree,
            parents,
            author,
            timestamp,
            message,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "StoreCommitObject(tree={}, parents={}, message='{}')",
            self.tree.inner(),
            self.parents.len(),
            self.message
        )
    }
}

impl From<clayers_repo::object::CommitObject> for StoreCommitObject {
    fn from(obj: clayers_repo::object::CommitObject) -> Self {
        Self {
            tree: ContentHash::from_inner(obj.tree),
            parents: obj
                .parents
                .into_iter()
                .map(ContentHash::from_inner)
                .collect(),
            author: StoreAuthor::from(obj.author),
            timestamp: obj.timestamp.to_rfc3339(),
            message: obj.message,
        }
    }
}

impl StoreCommitObject {
    pub fn to_rust(&self) -> Result<clayers_repo::object::CommitObject, String> {
        use chrono::DateTime;
        let timestamp = DateTime::parse_from_rfc3339(&self.timestamp)
            .map_err(|e| format!("invalid timestamp '{}': {}", self.timestamp, e))?
            .to_utc();
        Ok(clayers_repo::object::CommitObject {
            tree: self.tree.inner(),
            parents: self.parents.iter().map(|h| h.inner()).collect(),
            author: self.author.to_rust(),
            timestamp,
            message: self.message.clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// StoreTagObject
// ---------------------------------------------------------------------------

/// An annotated tag object.
///
/// Timestamps are represented as ISO 8601 strings (RFC 3339) for Python
/// interoperability.
#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct StoreTagObject {
    #[pyo3(get)]
    pub target: ContentHash,
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub tagger: StoreAuthor,
    /// ISO 8601 timestamp string.
    #[pyo3(get)]
    pub timestamp: String,
    #[pyo3(get)]
    pub message: String,
}

#[pymethods]
impl StoreTagObject {
    #[new]
    fn new(
        target: ContentHash,
        name: String,
        tagger: StoreAuthor,
        timestamp: String,
        message: String,
    ) -> Self {
        Self {
            target,
            name,
            tagger,
            timestamp,
            message,
        }
    }

    fn __repr__(&self) -> String {
        format!("StoreTagObject('{}', target={})", self.name, self.target.inner())
    }
}

impl From<clayers_repo::object::TagObject> for StoreTagObject {
    fn from(obj: clayers_repo::object::TagObject) -> Self {
        Self {
            target: ContentHash::from_inner(obj.target),
            name: obj.name,
            tagger: StoreAuthor::from(obj.tagger),
            timestamp: obj.timestamp.to_rfc3339(),
            message: obj.message,
        }
    }
}

impl StoreTagObject {
    pub fn to_rust(&self) -> Result<clayers_repo::object::TagObject, String> {
        use chrono::DateTime;
        let timestamp = DateTime::parse_from_rfc3339(&self.timestamp)
            .map_err(|e| format!("invalid timestamp '{}': {}", self.timestamp, e))?
            .to_utc();
        Ok(clayers_repo::object::TagObject {
            target: self.target.inner(),
            name: self.name.clone(),
            tagger: self.tagger.to_rust(),
            timestamp,
            message: self.message.clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// StoreObject (enum wrapper)
// ---------------------------------------------------------------------------

/// Discriminated wrapper for all object variants in the Merkle DAG.
///
/// The `kind` field is a string discriminator: "element", "text", "comment",
/// "pi", "document", "tree", "commit", or "tag". Use the corresponding
/// accessor method to retrieve the typed inner object.
#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct StoreObject {
    #[pyo3(get)]
    pub kind: String,
    inner: StoreObjectInner,
}

#[derive(Clone)]
enum StoreObjectInner {
    Element(StoreElementObject),
    Text(StoreTextObject),
    Comment(StoreCommentObject),
    PI(StorePIObject),
    Document(StoreDocumentObject),
    Tree(StoreTreeObject),
    Commit(StoreCommitObject),
    Tag(StoreTagObject),
}

#[pymethods]
impl StoreObject {
    /// Return the inner `StoreElementObject`, or `None` if this is not an element.
    fn as_element(&self) -> Option<StoreElementObject> {
        match &self.inner {
            StoreObjectInner::Element(e) => Some(e.clone()),
            _ => None,
        }
    }

    /// Return the inner `StoreTextObject`, or `None` if this is not a text node.
    fn as_text(&self) -> Option<StoreTextObject> {
        match &self.inner {
            StoreObjectInner::Text(t) => Some(t.clone()),
            _ => None,
        }
    }

    /// Return the inner `StoreCommentObject`, or `None` if this is not a comment.
    fn as_comment(&self) -> Option<StoreCommentObject> {
        match &self.inner {
            StoreObjectInner::Comment(c) => Some(c.clone()),
            _ => None,
        }
    }

    /// Return the inner `StorePIObject`, or `None` if this is not a PI.
    fn as_pi(&self) -> Option<StorePIObject> {
        match &self.inner {
            StoreObjectInner::PI(p) => Some(p.clone()),
            _ => None,
        }
    }

    /// Return the inner `StoreDocumentObject`, or `None` if this is not a document.
    fn as_document(&self) -> Option<StoreDocumentObject> {
        match &self.inner {
            StoreObjectInner::Document(d) => Some(d.clone()),
            _ => None,
        }
    }

    /// Return the inner `StoreTreeObject`, or `None` if this is not a tree.
    fn as_tree(&self) -> Option<StoreTreeObject> {
        match &self.inner {
            StoreObjectInner::Tree(t) => Some(t.clone()),
            _ => None,
        }
    }

    /// Return the inner `StoreCommitObject`, or `None` if this is not a commit.
    fn as_commit(&self) -> Option<StoreCommitObject> {
        match &self.inner {
            StoreObjectInner::Commit(c) => Some(c.clone()),
            _ => None,
        }
    }

    /// Return the inner `StoreTagObject`, or `None` if this is not a tag.
    fn as_tag(&self) -> Option<StoreTagObject> {
        match &self.inner {
            StoreObjectInner::Tag(t) => Some(t.clone()),
            _ => None,
        }
    }

    fn __repr__(&self) -> String {
        format!("StoreObject(kind='{}')", self.kind)
    }
}

impl From<clayers_repo::object::Object> for StoreObject {
    fn from(obj: clayers_repo::object::Object) -> Self {
        match obj {
            clayers_repo::object::Object::Element(e) => Self {
                kind: "element".to_string(),
                inner: StoreObjectInner::Element(e.into()),
            },
            clayers_repo::object::Object::Text(t) => Self {
                kind: "text".to_string(),
                inner: StoreObjectInner::Text(t.into()),
            },
            clayers_repo::object::Object::Comment(c) => Self {
                kind: "comment".to_string(),
                inner: StoreObjectInner::Comment(c.into()),
            },
            clayers_repo::object::Object::PI(p) => Self {
                kind: "pi".to_string(),
                inner: StoreObjectInner::PI(p.into()),
            },
            clayers_repo::object::Object::Document(d) => Self {
                kind: "document".to_string(),
                inner: StoreObjectInner::Document(d.into()),
            },
            clayers_repo::object::Object::Tree(t) => Self {
                kind: "tree".to_string(),
                inner: StoreObjectInner::Tree(t.into()),
            },
            clayers_repo::object::Object::Commit(c) => Self {
                kind: "commit".to_string(),
                inner: StoreObjectInner::Commit(c.into()),
            },
            clayers_repo::object::Object::Tag(t) => Self {
                kind: "tag".to_string(),
                inner: StoreObjectInner::Tag(t.into()),
            },
        }
    }
}

impl StoreObject {
    pub fn to_rust(&self) -> Result<clayers_repo::object::Object, String> {
        match &self.inner {
            StoreObjectInner::Element(e) => {
                Ok(clayers_repo::object::Object::Element(e.to_rust()))
            }
            StoreObjectInner::Text(t) => {
                Ok(clayers_repo::object::Object::Text(t.to_rust()))
            }
            StoreObjectInner::Comment(c) => {
                Ok(clayers_repo::object::Object::Comment(c.to_rust()))
            }
            StoreObjectInner::PI(p) => {
                Ok(clayers_repo::object::Object::PI(p.to_rust()))
            }
            StoreObjectInner::Document(d) => {
                Ok(clayers_repo::object::Object::Document(d.to_rust()))
            }
            StoreObjectInner::Tree(t) => {
                Ok(clayers_repo::object::Object::Tree(t.to_rust()))
            }
            StoreObjectInner::Commit(c) => {
                Ok(clayers_repo::object::Object::Commit(c.to_rust()?))
            }
            StoreObjectInner::Tag(t) => {
                Ok(clayers_repo::object::Object::Tag(t.to_rust()?))
            }
        }
    }

    /// Construct a `StoreObject` from one of the typed variant wrappers.
    pub fn from_element(e: StoreElementObject) -> Self {
        Self {
            kind: "element".to_string(),
            inner: StoreObjectInner::Element(e),
        }
    }

    pub fn from_text(t: StoreTextObject) -> Self {
        Self {
            kind: "text".to_string(),
            inner: StoreObjectInner::Text(t),
        }
    }

    pub fn from_comment(c: StoreCommentObject) -> Self {
        Self {
            kind: "comment".to_string(),
            inner: StoreObjectInner::Comment(c),
        }
    }

    pub fn from_pi(p: StorePIObject) -> Self {
        Self {
            kind: "pi".to_string(),
            inner: StoreObjectInner::PI(p),
        }
    }

    pub fn from_document(d: StoreDocumentObject) -> Self {
        Self {
            kind: "document".to_string(),
            inner: StoreObjectInner::Document(d),
        }
    }

    pub fn from_tree(t: StoreTreeObject) -> Self {
        Self {
            kind: "tree".to_string(),
            inner: StoreObjectInner::Tree(t),
        }
    }

    pub fn from_commit(c: StoreCommitObject) -> Self {
        Self {
            kind: "commit".to_string(),
            inner: StoreObjectInner::Commit(c),
        }
    }

    pub fn from_tag(t: StoreTagObject) -> Self {
        Self {
            kind: "tag".to_string(),
            inner: StoreObjectInner::Tag(t),
        }
    }
}
