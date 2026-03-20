use std::sync::{Arc, OnceLock};

use clayers_repo::{MemoryStore, Repo};

#[cfg(feature = "sqlite")]
use clayers_repo::SqliteStore;

pub fn get_runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to create tokio runtime")
    })
}

pub enum RepoInner {
    Memory(Repo<MemoryStore>),
    #[cfg(feature = "sqlite")]
    Sqlite(Repo<SqliteStore>),
}

/// Dispatch a method call to the inner repo regardless of store type.
macro_rules! dispatch {
    ($self:expr, $repo:ident, $body:expr) => {
        match $self {
            crate::repo::inner::RepoInner::Memory($repo) => $body,
            #[cfg(feature = "sqlite")]
            crate::repo::inner::RepoInner::Sqlite($repo) => $body,
        }
    };
}

pub(crate) use dispatch;

pub type SharedRepo = Arc<RepoInner>;
