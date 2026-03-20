use pyo3::prelude::*;

use super::inner::{SharedRepo, dispatch};
use super::objects::{Author, CommitObject, FileChange};
use super::store::MemoryStore;
use crate::errors::RepoError;
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

/// Async variant of Repo, exposed as `Repo` in `clayers.repo.aio`.
#[pyclass(name = "Repo")]
pub struct AsyncRepo {
    inner: SharedRepo,
}

#[pymethods]
impl AsyncRepo {
    #[new]
    fn new(store: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: make_inner(store)?,
        })
    }

    fn import_xml<'py>(&self, py: Python<'py>, xml: String) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let hash = dispatch!(&*inner, repo, repo.import_xml(&xml).await)
                .map_err(|e| RepoError::new_err(e.to_string()))?;
            Ok(ContentHash::from_inner(hash))
        })
    }

    fn export_xml<'py>(&self, py: Python<'py>, hash: ContentHash) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        let h = hash.inner();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let xml = dispatch!(&*inner, repo, repo.export_xml(h).await)
                .map_err(|e| RepoError::new_err(e.to_string()))?;
            Ok(xml)
        })
    }

    fn build_tree<'py>(
        &self,
        py: Python<'py>,
        entries: Vec<(String, ContentHash)>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        let rust_entries: Vec<(String, clayers_xml::ContentHash)> = entries
            .into_iter()
            .map(|(path, hash)| (path, hash.inner()))
            .collect();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let hash = dispatch!(&*inner, repo, repo.build_tree(rust_entries).await)
                .map_err(|e| RepoError::new_err(e.to_string()))?;
            Ok(ContentHash::from_inner(hash))
        })
    }

    fn commit<'py>(
        &self,
        py: Python<'py>,
        branch: String,
        tree: ContentHash,
        author: Author,
        message: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        let tree_hash = tree.inner();
        let rust_author = author.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let hash = dispatch!(
                &*inner,
                repo,
                repo.commit(&branch, tree_hash, &rust_author, &message)
                    .await
            )
            .map_err(|e| RepoError::new_err(e.to_string()))?;
            Ok(ContentHash::from_inner(hash))
        })
    }

    fn create_branch<'py>(
        &self,
        py: Python<'py>,
        name: String,
        target: ContentHash,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        let target = target.inner();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            dispatch!(&*inner, repo, repo.create_branch(&name, target).await)
                .map_err(|e| RepoError::new_err(e.to_string()))?;
            Ok(())
        })
    }

    fn delete_branch<'py>(&self, py: Python<'py>, name: String) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            dispatch!(&*inner, repo, repo.delete_branch(&name).await)
                .map_err(|e| RepoError::new_err(e.to_string()))?;
            Ok(())
        })
    }

    fn list_branches<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let branches = dispatch!(&*inner, repo, repo.list_branches().await)
                .map_err(|e| RepoError::new_err(e.to_string()))?;
            let result: Vec<(String, ContentHash)> = branches
                .into_iter()
                .map(|(name, hash)| (name, ContentHash::from_inner(hash)))
                .collect();
            Ok(result)
        })
    }

    fn create_tag<'py>(
        &self,
        py: Python<'py>,
        name: String,
        target: ContentHash,
        tagger: Author,
        message: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        let target = target.inner();
        let tagger = tagger.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            dispatch!(
                &*inner,
                repo,
                repo.create_tag(&name, target, &tagger, &message).await
            )
            .map_err(|e| RepoError::new_err(e.to_string()))?;
            Ok(())
        })
    }

    fn list_tags<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let tags = dispatch!(&*inner, repo, repo.list_tags().await)
                .map_err(|e| RepoError::new_err(e.to_string()))?;
            let result: Vec<(String, ContentHash)> = tags
                .into_iter()
                .map(|(name, hash)| (name, ContentHash::from_inner(hash)))
                .collect();
            Ok(result)
        })
    }

    #[pyo3(signature = (from_hash, limit=None))]
    fn log<'py>(
        &self,
        py: Python<'py>,
        from_hash: ContentHash,
        limit: Option<usize>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        let from = from_hash.inner();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let commits = dispatch!(&*inner, repo, repo.log(from, limit).await)
                .map_err(|e| RepoError::new_err(e.to_string()))?;
            let result: Vec<CommitObject> = commits.into_iter().map(Into::into).collect();
            Ok(result)
        })
    }

    fn diff_trees<'py>(
        &self,
        py: Python<'py>,
        a: ContentHash,
        b: ContentHash,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        let ha = a.inner();
        let hb = b.inner();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let changes = dispatch!(&*inner, repo, repo.diff_trees(ha, hb).await)
                .map_err(|e| RepoError::new_err(e.to_string()))?;
            let result: Vec<FileChange> = changes.into_iter().map(Into::into).collect();
            Ok(result)
        })
    }

    #[pyo3(signature = (xpath, *, mode="xml"))]
    fn query<'py>(
        &self,
        py: Python<'py>,
        xpath: String,
        mode: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        let qm = parse_query_mode_repo(mode)?;
        let namespaces = clayers_repo::NamespaceMap::new();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result = dispatch!(
                &*inner,
                repo,
                repo.query("HEAD", &xpath, qm, &namespaces).await
            )
            .map_err(|e| RepoError::new_err(e.to_string()))?;
            let qr: QueryResult = result.into();
            Ok(qr)
        })
    }

    fn __repr__(&self) -> String {
        "AsyncRepo(...)".to_string()
    }
}
