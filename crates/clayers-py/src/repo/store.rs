use pyo3::prelude::*;

use clayers_repo::MemoryStore as RustMemoryStore;
#[cfg(feature = "sqlite")]
use clayers_repo::SqliteStore as RustSqliteStore;

use super::inner::{RepoInner, SharedRepo};
use crate::errors::RepoError;

#[pyclass]
pub struct MemoryStore {}

#[pymethods]
impl MemoryStore {
    #[new]
    fn new() -> Self {
        Self {}
    }
}

impl MemoryStore {
    pub fn create_repo_inner() -> SharedRepo {
        let store = RustMemoryStore::new();
        let repo = clayers_repo::Repo::init(store);
        std::sync::Arc::new(RepoInner::Memory(repo))
    }
}

#[cfg(feature = "sqlite")]
#[pyclass]
pub struct SqliteStore {
    path: Option<String>,
}

#[cfg(feature = "sqlite")]
#[pymethods]
impl SqliteStore {
    #[staticmethod]
    fn open(path: &str) -> PyResult<Self> {
        // Validate the path works
        RustSqliteStore::open(path).map_err(|e| RepoError::new_err(e.to_string()))?;
        Ok(Self {
            path: Some(path.to_string()),
        })
    }

    #[staticmethod]
    fn open_in_memory() -> PyResult<Self> {
        RustSqliteStore::open_in_memory().map_err(|e| RepoError::new_err(e.to_string()))?;
        Ok(Self { path: None })
    }
}

#[cfg(feature = "sqlite")]
impl SqliteStore {
    pub fn create_repo_inner(&self) -> PyResult<SharedRepo> {
        let store = if let Some(ref path) = self.path {
            RustSqliteStore::open(path).map_err(|e| RepoError::new_err(e.to_string()))?
        } else {
            RustSqliteStore::open_in_memory().map_err(|e| RepoError::new_err(e.to_string()))?
        };
        let repo = clayers_repo::Repo::init(store);
        Ok(std::sync::Arc::new(RepoInner::Sqlite(repo)))
    }
}
