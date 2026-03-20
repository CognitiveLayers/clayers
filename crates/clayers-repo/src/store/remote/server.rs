//! Server-side handler that dispatches client messages to store backends.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use std::pin::pin;

use async_trait::async_trait;
use futures_core::Stream;
use tokio::sync::Mutex;

use super::transport::Store;
use super::{ClientMessage, ServerMessage, TxId};
use crate::error::Result;
use crate::store::Transaction;

/// Provides access to named repositories.
#[async_trait]
pub trait RepositoryProvider: Send + Sync {
    /// List available repository names.
    ///
    /// # Errors
    ///
    /// Returns an error if the provider cannot list repositories.
    async fn list(&self) -> Result<Vec<String>>;

    /// Get a store for the named repository.
    ///
    /// # Errors
    ///
    /// Returns an error if the repository is not found.
    async fn get(&self, name: &str) -> Result<Arc<dyn Store>>;
}

/// A static map of repository name to store.
pub struct StaticRepositories {
    repos: HashMap<String, Arc<dyn Store>>,
}

impl StaticRepositories {
    /// Create from a map of repo names to stores.
    #[must_use]
    pub fn new(repos: HashMap<String, Arc<dyn Store>>) -> Self {
        Self { repos }
    }
}

#[async_trait]
impl RepositoryProvider for StaticRepositories {
    async fn list(&self) -> Result<Vec<String>> {
        Ok(self.repos.keys().cloned().collect())
    }

    async fn get(&self, name: &str) -> Result<Arc<dyn Store>> {
        self.repos
            .get(name)
            .cloned()
            .ok_or_else(|| crate::error::Error::Storage(format!("repository not found: {name}")))
    }
}

/// Unique identifier for a connected client.
pub type ConnectionId = u64;

/// Sender for a connection (used for broadcasting notifications).
pub type ConnectionSender = tokio::sync::mpsc::UnboundedSender<ServerMessage>;

struct TxState {
    tx: Box<dyn Transaction>,
    connection_id: ConnectionId,
    #[allow(dead_code)]
    repo: String,
}

/// Server that dispatches client messages to stores via a [`RepositoryProvider`].
pub struct Server<P: RepositoryProvider> {
    provider: Arc<P>,
    transactions: Arc<Mutex<HashMap<TxId, TxState>>>,
    next_tx_id: AtomicU64,
    connections: Arc<Mutex<HashMap<ConnectionId, ConnectionSender>>>,
    next_conn_id: AtomicU64,
}

impl<P: RepositoryProvider + 'static> Server<P> {
    /// Create a new server backed by the given provider.
    #[must_use]
    pub fn new(provider: P) -> Self {
        Self {
            provider: Arc::new(provider),
            transactions: Arc::new(Mutex::new(HashMap::new())),
            next_tx_id: AtomicU64::new(1),
            connections: Arc::new(Mutex::new(HashMap::new())),
            next_conn_id: AtomicU64::new(1),
        }
    }

    /// Register a new connection and return its ID and a receiver for notifications.
    pub async fn register_connection(
        &self,
    ) -> (ConnectionId, tokio::sync::mpsc::UnboundedReceiver<ServerMessage>) {
        let id = self.next_conn_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.connections.lock().await.insert(id, tx);
        (id, rx)
    }

    /// Unregister a connection and roll back its open transactions.
    pub async fn disconnect(&self, conn_id: ConnectionId) {
        self.connections.lock().await.remove(&conn_id);

        let mut txns = self.transactions.lock().await;
        let to_rollback: Vec<TxId> = txns
            .iter()
            .filter(|(_, state)| state.connection_id == conn_id)
            .map(|(id, _)| *id)
            .collect();
        for tx_id in to_rollback {
            if let Some(mut state) = txns.remove(&tx_id) {
                let _ = state.tx.rollback().await;
            }
        }
    }

    /// Handle a single client message, returning the response(s) to send back.
    #[allow(clippy::too_many_lines)]
    pub async fn handle(&self, conn_id: ConnectionId, msg: ClientMessage) -> Vec<ServerMessage> {
        match msg {
            ClientMessage::ListRepositories { id } => match self.provider.list().await {
                Ok(repos) => vec![ServerMessage::RepositoryList { id, repos }],
                Err(e) => vec![err(id, &e)],
            },
            ClientMessage::Get { id, repo, hash } => {
                self.with_store(id, &repo, |store| async move {
                    store.get(&hash).await.map(|object| ServerMessage::Object { id, object })
                })
                .await
            }
            ClientMessage::Contains { id, repo, hash } => {
                self.with_store(id, &repo, |store| async move {
                    store
                        .contains(&hash)
                        .await
                        .map(|exists| ServerMessage::Contains { id, exists })
                })
                .await
            }
            ClientMessage::GetByInclusiveHash {
                id,
                repo,
                inclusive_hash,
            } => {
                self.with_store(id, &repo, |store| async move {
                    store
                        .get_by_inclusive_hash(&inclusive_hash)
                        .await
                        .map(|result| ServerMessage::ObjectWithHash { id, result })
                })
                .await
            }
            ClientMessage::Subtree { id, repo, root } => {
                self.handle_subtree(id, &repo, root).await
            }
            ClientMessage::GetRef { id, repo, name } => {
                self.with_store(id, &repo, |store| async move {
                    store
                        .get_ref(&name)
                        .await
                        .map(|hash| ServerMessage::Ref { id, hash })
                })
                .await
            }
            ClientMessage::SetRef {
                id,
                repo,
                name,
                hash,
            } => self.handle_set_ref(conn_id, id, &repo, &name, hash).await,
            ClientMessage::DeleteRef { id, repo, name } => {
                self.handle_delete_ref(conn_id, id, &repo, &name).await
            }
            ClientMessage::ListRefs { id, repo, prefix } => {
                self.with_store(id, &repo, |store| async move {
                    store
                        .list_refs(&prefix)
                        .await
                        .map(|refs| ServerMessage::RefList { id, refs })
                })
                .await
            }
            ClientMessage::CasRef {
                id,
                repo,
                name,
                expected,
                new,
            } => {
                self.handle_cas_ref(conn_id, id, &repo, &name, expected, new)
                    .await
            }
            ClientMessage::BeginTransaction { id, repo } => {
                self.handle_begin_tx(conn_id, id, &repo).await
            }
            ClientMessage::TxPut {
                id,
                tx_id,
                hash,
                object,
            } => {
                let mut txns = self.transactions.lock().await;
                match txns.get_mut(&tx_id) {
                    Some(state) if state.connection_id == conn_id => {
                        match state.tx.put(hash, object).await {
                            Ok(()) => vec![ServerMessage::Ok { id }],
                            Err(e) => vec![err(id, &e)],
                        }
                    }
                    _ => vec![ServerMessage::Error {
                        id,
                        message: format!("transaction not found: {}", tx_id.0),
                    }],
                }
            }
            ClientMessage::TxCommit { id, tx_id } => {
                let mut txns = self.transactions.lock().await;
                match txns.get_mut(&tx_id) {
                    Some(state) if state.connection_id == conn_id => {
                        match state.tx.commit().await {
                            Ok(()) => {
                                txns.remove(&tx_id);
                                vec![ServerMessage::Ok { id }]
                            }
                            Err(e) => vec![err(id, &e)],
                        }
                    }
                    _ => vec![ServerMessage::Error {
                        id,
                        message: format!("transaction not found: {}", tx_id.0),
                    }],
                }
            }
            ClientMessage::TxRollback { id, tx_id } => {
                let mut txns = self.transactions.lock().await;
                match txns.remove(&tx_id) {
                    Some(mut state) if state.connection_id == conn_id => {
                        match state.tx.rollback().await {
                            Ok(()) => vec![ServerMessage::Ok { id }],
                            Err(e) => vec![err(id, &e)],
                        }
                    }
                    _ => vec![ServerMessage::Ok { id }],
                }
            }
        }
    }

    // Helper: run a closure with a store, returning vec of one message
    async fn with_store<F, Fut>(
        &self,
        id: super::MessageId,
        repo: &str,
        f: F,
    ) -> Vec<ServerMessage>
    where
        F: FnOnce(Arc<dyn Store>) -> Fut,
        Fut: std::future::Future<Output = Result<ServerMessage>>,
    {
        match self.provider.get(repo).await {
            Ok(store) => match f(store).await {
                Ok(msg) => vec![msg],
                Err(e) => vec![err(id, &e)],
            },
            Err(e) => vec![err(id, &e)],
        }
    }

    async fn handle_subtree(
        &self,
        id: super::MessageId,
        repo: &str,
        root: clayers_xml::ContentHash,
    ) -> Vec<ServerMessage> {
        let store = match self.provider.get(repo).await {
            Ok(s) => s,
            Err(e) => return vec![err(id, &e)],
        };
        let mut msgs = Vec::new();
        let mut stream = pin!(store.subtree(&root));
        while let Some(item) = std::future::poll_fn(|cx| stream.as_mut().poll_next(cx)).await {
            match item {
                Ok((hash, object)) => {
                    msgs.push(ServerMessage::SubtreeItem { id, hash, object });
                }
                Err(e) => {
                    msgs.push(err(id, &e));
                    return msgs;
                }
            }
        }
        msgs.push(ServerMessage::SubtreeEnd { id });
        msgs
    }

    async fn handle_set_ref(
        &self,
        conn_id: ConnectionId,
        id: super::MessageId,
        repo: &str,
        name: &str,
        hash: clayers_xml::ContentHash,
    ) -> Vec<ServerMessage> {
        let store = match self.provider.get(repo).await {
            Ok(s) => s,
            Err(e) => return vec![err(id, &e)],
        };
        let old = store.get_ref(name).await.ok().flatten();
        match store.set_ref(name, hash).await {
            Ok(()) => {
                self.broadcast_ref_updated(conn_id, repo, name, old, Some(hash))
                    .await;
                vec![ServerMessage::RefSet { id }]
            }
            Err(e) => vec![err(id, &e)],
        }
    }

    async fn handle_delete_ref(
        &self,
        conn_id: ConnectionId,
        id: super::MessageId,
        repo: &str,
        name: &str,
    ) -> Vec<ServerMessage> {
        let store = match self.provider.get(repo).await {
            Ok(s) => s,
            Err(e) => return vec![err(id, &e)],
        };
        let old = store.get_ref(name).await.ok().flatten();
        match store.delete_ref(name).await {
            Ok(()) => {
                self.broadcast_ref_updated(conn_id, repo, name, old, None)
                    .await;
                vec![ServerMessage::RefDeleted { id }]
            }
            Err(e) => vec![err(id, &e)],
        }
    }

    async fn handle_cas_ref(
        &self,
        conn_id: ConnectionId,
        id: super::MessageId,
        repo: &str,
        name: &str,
        expected: Option<clayers_xml::ContentHash>,
        new: clayers_xml::ContentHash,
    ) -> Vec<ServerMessage> {
        let store = match self.provider.get(repo).await {
            Ok(s) => s,
            Err(e) => return vec![err(id, &e)],
        };
        let old = store.get_ref(name).await.ok().flatten();
        match store.cas_ref(name, expected, new).await {
            Ok(swapped) => {
                if swapped {
                    self.broadcast_ref_updated(conn_id, repo, name, old, Some(new))
                        .await;
                }
                vec![ServerMessage::CasResult { id, swapped }]
            }
            Err(e) => vec![err(id, &e)],
        }
    }

    async fn handle_begin_tx(
        &self,
        conn_id: ConnectionId,
        id: super::MessageId,
        repo: &str,
    ) -> Vec<ServerMessage> {
        let store = match self.provider.get(repo).await {
            Ok(s) => s,
            Err(e) => return vec![err(id, &e)],
        };
        match store.transaction().await {
            Ok(tx) => {
                let tx_id = TxId(self.next_tx_id.fetch_add(1, Ordering::Relaxed));
                self.transactions.lock().await.insert(
                    tx_id,
                    TxState {
                        tx,
                        connection_id: conn_id,
                        repo: repo.to_string(),
                    },
                );
                vec![ServerMessage::TransactionCreated { id, tx_id }]
            }
            Err(e) => vec![err(id, &e)],
        }
    }

    async fn broadcast_ref_updated(
        &self,
        origin: ConnectionId,
        repo: &str,
        name: &str,
        old: Option<clayers_xml::ContentHash>,
        new: Option<clayers_xml::ContentHash>,
    ) {
        let conns = self.connections.lock().await;
        for (&id, tx) in conns.iter() {
            if id != origin {
                let _ = tx.send(ServerMessage::RefUpdated {
                    repo: repo.to_string(),
                    name: name.to_string(),
                    old,
                    new,
                });
            }
        }
    }
}

fn err(id: super::MessageId, e: &crate::error::Error) -> ServerMessage {
    ServerMessage::Error {
        id,
        message: e.to_string(),
    }
}
