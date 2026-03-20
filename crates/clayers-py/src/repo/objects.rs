use pyo3::prelude::*;

use crate::xml::ContentHash;

#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct Author {
    pub inner: clayers_repo::Author,
}

#[pymethods]
impl Author {
    #[new]
    fn new(name: String, email: String) -> Self {
        Self {
            inner: clayers_repo::Author { name, email },
        }
    }

    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    #[getter]
    fn email(&self) -> &str {
        &self.inner.email
    }

    fn __repr__(&self) -> String {
        format!("Author('{}', '{}')", self.inner.name, self.inner.email)
    }
}

#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct CommitObject {
    #[pyo3(get)]
    pub tree: ContentHash,
    #[pyo3(get)]
    pub parents: Vec<ContentHash>,
    #[pyo3(get)]
    pub author: Author,
    #[pyo3(get)]
    pub timestamp: String,
    #[pyo3(get)]
    pub message: String,
}

impl From<clayers_repo::CommitObject> for CommitObject {
    fn from(c: clayers_repo::CommitObject) -> Self {
        Self {
            tree: ContentHash::from_inner(c.tree),
            parents: c.parents.into_iter().map(ContentHash::from_inner).collect(),
            author: Author { inner: c.author },
            timestamp: c.timestamp.to_rfc3339(),
            message: c.message,
        }
    }
}

#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct TreeEntry {
    #[pyo3(get)]
    pub path: String,
    #[pyo3(get)]
    pub document: ContentHash,
}

#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct FileChange {
    #[pyo3(get)]
    pub kind: String,
    #[pyo3(get)]
    pub path: String,
    #[pyo3(get)]
    pub old_hash: Option<ContentHash>,
    #[pyo3(get)]
    pub new_hash: Option<ContentHash>,
}

impl From<clayers_repo::FileChange> for FileChange {
    fn from(fc: clayers_repo::FileChange) -> Self {
        match fc {
            clayers_repo::FileChange::Added { path, document } => Self {
                kind: "added".into(),
                path,
                old_hash: None,
                new_hash: Some(ContentHash::from_inner(document)),
            },
            clayers_repo::FileChange::Removed { path, document } => Self {
                kind: "removed".into(),
                path,
                old_hash: Some(ContentHash::from_inner(document)),
                new_hash: None,
            },
            clayers_repo::FileChange::Modified {
                path,
                old_doc,
                new_doc,
            } => Self {
                kind: "modified".into(),
                path,
                old_hash: Some(ContentHash::from_inner(old_doc)),
                new_hash: Some(ContentHash::from_inner(new_doc)),
            },
        }
    }
}
