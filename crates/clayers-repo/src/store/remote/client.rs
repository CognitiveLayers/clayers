//! Client-side remote store that delegates over a [`Transport`].

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use clayers_xml::ContentHash;
use futures_core::stream::BoxStream;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::AbortHandle;

use super::transport::Transport;
use super::{ClientMessage, MessageId, ServerMessage, TxId};
use crate::error::{Error, Result};
use crate::object::Object;
use crate::query::{QueryMode, QueryResult, QueryStore, NamespaceMap, default_query_document};
use crate::store::{ObjectStore, RefStore, Transaction};

/// A store that delegates to a remote server via a [`Transport`].
///
/// Implements `ObjectStore`, `RefStore`, and `QueryStore`. Spawns a background
/// reader task to dispatch incoming messages.
pub struct RemoteStore<T: Transport> {
    transport: Arc<T>,
    repo: String,
    next_id: Arc<AtomicU64>,
    pending: Arc<Mutex<HashMap<MessageId, oneshot::Sender<ServerMessage>>>>,
    streams: Arc<Mutex<HashMap<MessageId, mpsc::UnboundedSender<ServerMessage>>>>,
    notifications: Mutex<mpsc::UnboundedReceiver<ServerMessage>>,
    reader_abort: AbortHandle,
}

impl<T: Transport + 'static> RemoteStore<T> {
    /// Create a new remote store connected to the given repo.
    ///
    /// Spawns a background reader task that dispatches messages from the transport.
    pub fn new(transport: T, repo: &str) -> Self {
        let transport = Arc::new(transport);
        let pending: Arc<Mutex<HashMap<MessageId, oneshot::Sender<ServerMessage>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let streams: Arc<Mutex<HashMap<MessageId, mpsc::UnboundedSender<ServerMessage>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (notify_tx, notify_rx) = mpsc::unbounded_channel();

        let reader = {
            let transport = Arc::clone(&transport);
            let pending = Arc::clone(&pending);
            let streams = Arc::clone(&streams);
            let notify_tx = notify_tx.clone();
            tokio::spawn(async move {
                loop {
                    let Ok(msg) = transport.recv().await else {
                        break;
                    };

                    // Server-initiated notifications (RefUpdated, TransactionTerminated)
                    let Some(id) = msg.id() else {
                        // Receiver dropped = nobody listening; ok to discard.
                        drop(notify_tx.send(msg));
                        continue;
                    };

                    // Check if this message belongs to an active subtree stream
                    {
                        let mut streams_guard = streams.lock().await;
                        if streams_guard.contains_key(&id) {
                            match &msg {
                                ServerMessage::SubtreeEnd { .. } => {
                                    // Stream done: close the channel
                                    streams_guard.remove(&id);
                                }
                                ServerMessage::SubtreeItem { .. } => {
                                    if let Some(tx) = streams_guard.get(&id) {
                                        // Receiver dropped = stream consumer gone; ok to discard.
                                        drop(tx.send(msg));
                                    }
                                }
                                ServerMessage::Error { .. } => {
                                    // Forward error to stream, then close it
                                    if let Some(tx) = streams_guard.remove(&id) {
                                        drop(tx.send(msg));
                                    }
                                }
                                _ => {}
                            }
                            continue;
                        }
                    }

                    // Regular reply: dispatch to pending waiter.
                    // Receiver dropped = caller timed out; ok to discard.
                    let mut pending_guard = pending.lock().await;
                    if let Some(tx) = pending_guard.remove(&id) {
                        drop(tx.send(msg));
                    }
                }
            })
        };

        Self {
            transport,
            repo: repo.to_string(),
            next_id: Arc::new(AtomicU64::new(1)),
            pending,
            streams,
            notifications: Mutex::new(notify_rx),
            reader_abort: reader.abort_handle(),
        }
    }

    fn alloc_id(&self) -> MessageId {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    async fn request(&self, msg: ClientMessage, id: MessageId) -> Result<ServerMessage> {
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }
        self.transport.send(msg).await?;
        rx.await.map_err(|_| Error::Storage("connection closed".into()))
    }

    /// Receive the next server-initiated notification (e.g., `RefUpdated`).
    ///
    /// Returns `None` if the connection is closed.
    pub async fn recv_notification(&self) -> Option<ServerMessage> {
        self.notifications.lock().await.recv().await
    }

    /// List repositories available on the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the connection is closed.
    pub async fn list_repositories(&self) -> Result<Vec<String>> {
        let id = self.alloc_id();
        let resp = self.request(ClientMessage::ListRepositories { id }, id).await?;
        match resp {
            ServerMessage::RepositoryList { repos, .. } => Ok(repos),
            ServerMessage::Error { message, .. } => Err(Error::Storage(message)),
            _ => Err(Error::Storage("unexpected response".into())),
        }
    }
}

impl<T: Transport> Drop for RemoteStore<T> {
    fn drop(&mut self) {
        self.reader_abort.abort();
    }
}

#[async_trait]
impl<T: Transport + 'static> ObjectStore for RemoteStore<T> {
    async fn get(&self, hash: &ContentHash) -> Result<Option<Object>> {
        let id = self.alloc_id();
        let resp = self
            .request(
                ClientMessage::Get {
                    id,
                    repo: self.repo.clone(),
                    hash: *hash,
                },
                id,
            )
            .await?;
        match resp {
            ServerMessage::Object { object, .. } => Ok(object),
            ServerMessage::Error { message, .. } => Err(Error::Storage(message)),
            _ => Err(Error::Storage("unexpected response".into())),
        }
    }

    async fn contains(&self, hash: &ContentHash) -> Result<bool> {
        let id = self.alloc_id();
        let resp = self
            .request(
                ClientMessage::Contains {
                    id,
                    repo: self.repo.clone(),
                    hash: *hash,
                },
                id,
            )
            .await?;
        match resp {
            ServerMessage::Contains { exists, .. } => Ok(exists),
            ServerMessage::Error { message, .. } => Err(Error::Storage(message)),
            _ => Err(Error::Storage("unexpected response".into())),
        }
    }

    async fn transaction(&self) -> Result<Box<dyn Transaction>> {
        let id = self.alloc_id();
        let resp = self
            .request(
                ClientMessage::BeginTransaction {
                    id,
                    repo: self.repo.clone(),
                },
                id,
            )
            .await?;
        match resp {
            ServerMessage::TransactionCreated { tx_id, .. } => {
                Ok(Box::new(RemoteTransaction {
                    tx_id,
                    transport: Arc::clone(&self.transport),
                    pending: Arc::clone(&self.pending),
                    next_id: Arc::clone(&self.next_id),
                    finished: false,
                }))
            }
            ServerMessage::Error { message, .. } => Err(Error::Storage(message)),
            _ => Err(Error::Storage("unexpected response".into())),
        }
    }

    async fn get_by_inclusive_hash(
        &self,
        inclusive_hash: &ContentHash,
    ) -> Result<Option<(ContentHash, Object)>> {
        let id = self.alloc_id();
        let resp = self
            .request(
                ClientMessage::GetByInclusiveHash {
                    id,
                    repo: self.repo.clone(),
                    inclusive_hash: *inclusive_hash,
                },
                id,
            )
            .await?;
        match resp {
            ServerMessage::ObjectWithHash { result, .. } => Ok(result),
            ServerMessage::Error { message, .. } => Err(Error::Storage(message)),
            _ => Err(Error::Storage("unexpected response".into())),
        }
    }

    fn subtree<'a>(
        &'a self,
        root: &ContentHash,
    ) -> BoxStream<'a, Result<(ContentHash, Object)>> {
        let root = *root;
        let id = self.alloc_id();
        let transport = Arc::clone(&self.transport);
        let streams = Arc::clone(&self.streams);
        let repo = self.repo.clone();

        Box::pin(async_stream::try_stream! {
            let (tx, mut rx) = mpsc::unbounded_channel();
            {
                let mut streams_guard = streams.lock().await;
                streams_guard.insert(id, tx);
            }

            transport
                .send(ClientMessage::Subtree { id, repo, root })
                .await?;

            while let Some(msg) = rx.recv().await {
                match msg {
                    ServerMessage::SubtreeItem { hash, object, .. } => {
                        yield (hash, object);
                    }
                    ServerMessage::Error { message, .. } => {
                        Err(Error::Storage(message))?;
                    }
                    _ => {}
                }
            }
        })
    }
}

#[async_trait]
impl<T: Transport + 'static> RefStore for RemoteStore<T> {
    async fn get_ref(&self, name: &str) -> Result<Option<ContentHash>> {
        let id = self.alloc_id();
        let resp = self
            .request(
                ClientMessage::GetRef {
                    id,
                    repo: self.repo.clone(),
                    name: name.to_string(),
                },
                id,
            )
            .await?;
        match resp {
            ServerMessage::Ref { hash, .. } => Ok(hash),
            ServerMessage::Error { message, .. } => Err(Error::Storage(message)),
            _ => Err(Error::Storage("unexpected response".into())),
        }
    }

    async fn set_ref(&self, name: &str, hash: ContentHash) -> Result<()> {
        let id = self.alloc_id();
        let resp = self
            .request(
                ClientMessage::SetRef {
                    id,
                    repo: self.repo.clone(),
                    name: name.to_string(),
                    hash,
                },
                id,
            )
            .await?;
        match resp {
            ServerMessage::RefSet { .. } => Ok(()),
            ServerMessage::Error { message, .. } => Err(Error::Storage(message)),
            _ => Err(Error::Storage("unexpected response".into())),
        }
    }

    async fn delete_ref(&self, name: &str) -> Result<()> {
        let id = self.alloc_id();
        let resp = self
            .request(
                ClientMessage::DeleteRef {
                    id,
                    repo: self.repo.clone(),
                    name: name.to_string(),
                },
                id,
            )
            .await?;
        match resp {
            ServerMessage::RefDeleted { .. } => Ok(()),
            ServerMessage::Error { message, .. } => Err(Error::Storage(message)),
            _ => Err(Error::Storage("unexpected response".into())),
        }
    }

    async fn list_refs(&self, prefix: &str) -> Result<Vec<(String, ContentHash)>> {
        let id = self.alloc_id();
        let resp = self
            .request(
                ClientMessage::ListRefs {
                    id,
                    repo: self.repo.clone(),
                    prefix: prefix.to_string(),
                },
                id,
            )
            .await?;
        match resp {
            ServerMessage::RefList { refs, .. } => Ok(refs),
            ServerMessage::Error { message, .. } => Err(Error::Storage(message)),
            _ => Err(Error::Storage("unexpected response".into())),
        }
    }

    async fn cas_ref(
        &self,
        name: &str,
        expected: Option<ContentHash>,
        new: ContentHash,
    ) -> Result<bool> {
        let id = self.alloc_id();
        let resp = self
            .request(
                ClientMessage::CasRef {
                    id,
                    repo: self.repo.clone(),
                    name: name.to_string(),
                    expected,
                    new,
                },
                id,
            )
            .await?;
        match resp {
            ServerMessage::CasResult { swapped, .. } => Ok(swapped),
            ServerMessage::Error { message, .. } => Err(Error::Storage(message)),
            _ => Err(Error::Storage("unexpected response".into())),
        }
    }
}

#[async_trait]
impl<T: Transport + 'static> QueryStore for RemoteStore<T> {
    async fn query_document(
        &self,
        doc_hash: ContentHash,
        xpath: &str,
        mode: QueryMode,
        namespaces: &NamespaceMap,
    ) -> Result<QueryResult> {
        default_query_document(self, doc_hash, xpath, mode, namespaces).await
    }
}

/// A transaction operating over the remote transport.
pub struct RemoteTransaction<T: Transport + 'static> {
    tx_id: TxId,
    transport: Arc<T>,
    pending: Arc<Mutex<HashMap<MessageId, oneshot::Sender<ServerMessage>>>>,
    next_id: Arc<AtomicU64>,
    finished: bool,
}

impl<T: Transport> RemoteTransaction<T> {
    fn alloc_id(&self) -> MessageId {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    async fn request(&self, msg: ClientMessage, id: MessageId) -> Result<ServerMessage> {
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }
        self.transport.send(msg).await?;
        rx.await
            .map_err(|_| Error::Storage("connection closed".into()))
    }
}

#[async_trait]
impl<T: Transport + 'static> Transaction for RemoteTransaction<T> {
    async fn put(&mut self, hash: ContentHash, object: Object) -> Result<()> {
        if self.finished {
            return Err(Error::Storage("transaction already consumed".into()));
        }
        let id = self.alloc_id();
        let resp = self
            .request(
                ClientMessage::TxPut {
                    id,
                    tx_id: self.tx_id,
                    hash,
                    object,
                },
                id,
            )
            .await?;
        match resp {
            ServerMessage::Ok { .. } => Ok(()),
            ServerMessage::Error { message, .. } => Err(Error::Storage(message)),
            _ => Err(Error::Storage("unexpected response".into())),
        }
    }

    async fn commit(&mut self) -> Result<()> {
        if self.finished {
            return Err(Error::Storage("transaction already consumed".into()));
        }
        let id = self.alloc_id();
        let resp = self
            .request(
                ClientMessage::TxCommit {
                    id,
                    tx_id: self.tx_id,
                },
                id,
            )
            .await?;
        // Per trait contract, only successful commit consumes the tx.
        // On Err, caller may retry or rollback.
        match resp {
            ServerMessage::Ok { .. } => {
                self.finished = true;
                Ok(())
            }
            ServerMessage::Error { message, .. } => Err(Error::Storage(message)),
            _ => Err(Error::Storage("unexpected response".into())),
        }
    }

    async fn rollback(&mut self) -> Result<()> {
        if self.finished {
            return Err(Error::Storage("transaction already consumed".into()));
        }
        let id = self.alloc_id();
        let resp = self
            .request(
                ClientMessage::TxRollback {
                    id,
                    tx_id: self.tx_id,
                },
                id,
            )
            .await?;
        // Rollback always consumes, even if it returned an error.
        self.finished = true;
        match resp {
            ServerMessage::Ok { .. } => Ok(()),
            ServerMessage::Error { message, .. } => Err(Error::Storage(message)),
            _ => Err(Error::Storage("unexpected response".into())),
        }
    }
}

impl<T: Transport + 'static> Drop for RemoteTransaction<T> {
    fn drop(&mut self) {
        if !self.finished {
            let transport = Arc::clone(&self.transport);
            let tx_id = self.tx_id;
            let id = self.alloc_id();
            // Fire-and-forget rollback: if the transport is closed, the server
            // will clean up the transaction on disconnect anyway.
            tokio::spawn(async move {
                drop(
                    transport
                        .send(ClientMessage::TxRollback { id, tx_id })
                        .await,
                );
            });
        }
    }
}

/// List repositories without constructing a full `RemoteStore`.
///
/// Only safe on a freshly connected transport with no other in-flight requests.
///
/// # Errors
///
/// Returns an error if the request fails or the connection is closed.
pub async fn list_repositories<T: Transport + 'static>(transport: &T) -> Result<Vec<String>> {
    let id = 1;
    transport
        .send(ClientMessage::ListRepositories { id })
        .await?;
    let msg = transport.recv().await?;
    match msg {
        ServerMessage::RepositoryList { repos, .. } => Ok(repos),
        ServerMessage::Error { message, .. } => Err(Error::Storage(message)),
        _ => Err(Error::Storage("unexpected response".into())),
    }
}
