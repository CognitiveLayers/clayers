use pyo3::prelude::*;

use super::inner::{SharedRepo, dispatch, get_runtime};
use super::objects::{Author, CommitObject, FileChange};
use super::store::MemoryStore;
use crate::errors::{RepoError, repo_err};
use crate::query::{QueryResult, parse_query_mode_repo};
use crate::xml::ContentHash;

#[cfg(feature = "sqlite")]
use super::store::SqliteStore;

fn make_inner(store: &Bound<'_, PyAny>) -> PyResult<SharedRepo> {
    if store.cast::<MemoryStore>().is_ok() {
        return Ok(MemoryStore::create_repo_inner());
    }
    #[cfg(feature = "sqlite")]
    if let Ok(cell) = store.cast::<SqliteStore>() {
        let ss = cell.borrow();
        return ss.create_repo_inner();
    }
    Err(RepoError::new_err("expected MemoryStore or SqliteStore"))
}

#[pyclass]
pub struct Repo {
    inner: SharedRepo,
}

#[pymethods]
impl Repo {
    #[new]
    fn new(store: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: make_inner(store)?,
        })
    }

    fn import_xml(&self, py: Python<'_>, xml: String) -> PyResult<ContentHash> {
        let inner = self.inner.clone();
        let hash = py.detach(|| {
            let rt = get_runtime();
            rt.block_on(async { dispatch!(&*inner, repo, repo.import_xml(&xml).await) })
                .map_err(repo_err)
        })?;
        Ok(ContentHash::from_inner(hash))
    }

    fn export_xml(&self, py: Python<'_>, hash: &ContentHash) -> PyResult<String> {
        let inner = self.inner.clone();
        let h = hash.inner();
        py.detach(|| {
            let rt = get_runtime();
            rt.block_on(async { dispatch!(&*inner, repo, repo.export_xml(h).await) })
                .map_err(repo_err)
        })
    }

    fn build_tree(
        &self,
        py: Python<'_>,
        entries: Vec<(String, ContentHash)>,
    ) -> PyResult<ContentHash> {
        let inner = self.inner.clone();
        let rust_entries: Vec<(String, clayers_xml::ContentHash)> = entries
            .into_iter()
            .map(|(path, hash)| (path, hash.inner()))
            .collect();
        let hash = py.detach(|| {
            let rt = get_runtime();
            rt.block_on(async {
                dispatch!(&*inner, repo, repo.build_tree(rust_entries).await)
            })
            .map_err(repo_err)
        })?;
        Ok(ContentHash::from_inner(hash))
    }

    fn commit(
        &self,
        py: Python<'_>,
        branch: &str,
        tree: &ContentHash,
        author: &Author,
        message: &str,
    ) -> PyResult<ContentHash> {
        let inner = self.inner.clone();
        let tree_hash = tree.inner();
        let rust_author = author.inner.clone();
        let branch = branch.to_string();
        let message = message.to_string();
        let hash = py.detach(|| {
            let rt = get_runtime();
            rt.block_on(async {
                dispatch!(
                    &*inner,
                    repo,
                    repo.commit(&branch, tree_hash, &rust_author, &message)
                        .await
                )
            })
            .map_err(repo_err)
        })?;
        Ok(ContentHash::from_inner(hash))
    }

    fn create_branch(
        &self,
        py: Python<'_>,
        name: &str,
        target: &ContentHash,
    ) -> PyResult<()> {
        let inner = self.inner.clone();
        let name = name.to_string();
        let target = target.inner();
        py.detach(|| {
            let rt = get_runtime();
            rt.block_on(async {
                dispatch!(&*inner, repo, repo.create_branch(&name, target).await)
            })
            .map_err(repo_err)
        })
    }

    fn delete_branch(&self, py: Python<'_>, name: &str) -> PyResult<()> {
        let inner = self.inner.clone();
        let name = name.to_string();
        py.detach(|| {
            let rt = get_runtime();
            rt.block_on(async {
                dispatch!(&*inner, repo, repo.delete_branch(&name).await)
            })
            .map_err(repo_err)
        })
    }

    fn list_branches(&self, py: Python<'_>) -> PyResult<Vec<(String, ContentHash)>> {
        let inner = self.inner.clone();
        let branches = py.detach(|| {
            let rt = get_runtime();
            rt.block_on(async {
                dispatch!(&*inner, repo, repo.list_branches().await)
            })
            .map_err(repo_err)
        })?;
        Ok(branches
            .into_iter()
            .map(|(name, hash)| (name, ContentHash::from_inner(hash)))
            .collect())
    }

    fn create_tag(
        &self,
        py: Python<'_>,
        name: &str,
        target: &ContentHash,
        tagger: &Author,
        message: &str,
    ) -> PyResult<()> {
        let inner = self.inner.clone();
        let name = name.to_string();
        let target = target.inner();
        let tagger = tagger.inner.clone();
        let message = message.to_string();
        py.detach(|| {
            let rt = get_runtime();
            rt.block_on(async {
                dispatch!(
                    &*inner,
                    repo,
                    repo.create_tag(&name, target, &tagger, &message).await
                )
            })
            .map_err(repo_err)
        })
    }

    fn list_tags(&self, py: Python<'_>) -> PyResult<Vec<(String, ContentHash)>> {
        let inner = self.inner.clone();
        let tags = py.detach(|| {
            let rt = get_runtime();
            rt.block_on(async { dispatch!(&*inner, repo, repo.list_tags().await) })
                .map_err(repo_err)
        })?;
        Ok(tags
            .into_iter()
            .map(|(name, hash)| (name, ContentHash::from_inner(hash)))
            .collect())
    }

    #[pyo3(signature = (from_hash, limit=None))]
    fn log(
        &self,
        py: Python<'_>,
        from_hash: &ContentHash,
        limit: Option<usize>,
    ) -> PyResult<Vec<CommitObject>> {
        let inner = self.inner.clone();
        let from = from_hash.inner();
        let commits = py.detach(|| {
            let rt = get_runtime();
            rt.block_on(async { dispatch!(&*inner, repo, repo.log(from, limit).await) })
                .map_err(repo_err)
        })?;
        Ok(commits.into_iter().map(Into::into).collect())
    }

    fn diff_trees(
        &self,
        py: Python<'_>,
        a: &ContentHash,
        b: &ContentHash,
    ) -> PyResult<Vec<FileChange>> {
        let inner = self.inner.clone();
        let ha = a.inner();
        let hb = b.inner();
        let changes = py.detach(|| {
            let rt = get_runtime();
            rt.block_on(async {
                dispatch!(&*inner, repo, repo.diff_trees(ha, hb).await)
            })
            .map_err(repo_err)
        })?;
        Ok(changes.into_iter().map(Into::into).collect())
    }

    #[pyo3(signature = (xpath, *, mode="xml"))]
    fn query(&self, py: Python<'_>, xpath: &str, mode: &str) -> PyResult<QueryResult> {
        let inner = self.inner.clone();
        let qm = parse_query_mode_repo(mode)?;
        let xpath = xpath.to_string();
        let namespaces = clayers_repo::NamespaceMap::new();
        let result = py.detach(|| {
            let rt = get_runtime();
            rt.block_on(async {
                dispatch!(
                    &*inner,
                    repo,
                    repo.query("HEAD", &xpath, qm, &namespaces).await
                )
            })
            .map_err(repo_err)
        })?;
        Ok(result.into())
    }

    fn __repr__(&self) -> String {
        "Repo(...)".to_string()
    }
}
