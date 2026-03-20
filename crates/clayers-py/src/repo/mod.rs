pub mod inner;
pub mod objects;
pub mod repo_async;
pub mod repo_sync;
pub mod store;

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
