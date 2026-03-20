pub mod inner;
pub mod objects;
pub mod py_objects;
pub mod py_store;
pub mod repo_async;
pub mod repo_sync;
pub mod store;

#[cfg(feature = "compliance")]
pub mod compliance;

use pyo3::prelude::*;

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "repo")?;
    m.add_class::<repo_sync::Repo>()?;
    m.add_class::<store::MemoryStore>()?;
    #[cfg(feature = "sqlite")]
    m.add_class::<store::SqliteStore>()?;
    m.add_class::<objects::Author>()?;
    m.add_class::<objects::CommitObject>()?;
    m.add_class::<objects::TreeEntry>()?;
    m.add_class::<objects::FileChange>()?;
    m.add_class::<py_objects::StoreAttribute>()?;
    m.add_class::<py_objects::StoreAuthor>()?;
    m.add_class::<py_objects::StoreTextObject>()?;
    m.add_class::<py_objects::StoreCommentObject>()?;
    m.add_class::<py_objects::StorePIObject>()?;
    m.add_class::<py_objects::NamespaceDecl>()?;
    m.add_class::<py_objects::StoreElementObject>()?;
    m.add_class::<py_objects::StoreDocumentObject>()?;
    m.add_class::<py_objects::StoreTreeEntry>()?;
    m.add_class::<py_objects::StoreTreeObject>()?;
    m.add_class::<py_objects::StoreCommitObject>()?;
    m.add_class::<py_objects::StoreTagObject>()?;
    m.add_class::<py_objects::StoreObject>()?;

    // Compliance test runner (optional feature)
    #[cfg(feature = "compliance")]
    {
        m.add_class::<compliance::ComplianceResult>()?;
        m.add_class::<compliance::ComplianceMemoryStore>()?;
        m.add_class::<compliance::ComplianceTransaction>()?;
        m.add_function(pyo3::wrap_pyfunction!(compliance::run_store_compliance, &m)?)?;
    }

    // Register aio submodule
    let aio = PyModule::new(parent.py(), "aio")?;
    aio.add_class::<repo_async::AsyncRepo>()?;
    m.add_submodule(&aio)?;

    parent.add_submodule(&m)?;

    // Make submodules importable
    let sys_modules = parent.py().import("sys")?.getattr("modules")?;
    sys_modules.set_item("clayers._clayers.repo", &m)?;
    sys_modules.set_item("clayers._clayers.repo.aio", &aio)?;

    Ok(())
}
