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

#[cfg(test)]
#[allow(clippy::doc_markdown, clippy::missing_panics_doc)]
mod tests {
    //! Mock-transport tests for client-side error handling, list
    //! cardinality, and transaction drop semantics.
    //!
    //! These tests bypass the WS layer entirely: they instantiate a
    //! `RemoteStore` (or call `list_repositories`) against a hand-rolled
    //! `MockTransport` that lets the test script server responses by
    //! request type. This makes error-path coverage tractable without
    //! injecting failures into a real backend.

    use std::collections::VecDeque;
    use std::sync::Arc;

    use tokio::sync::{Mutex, Notify};

    use super::{
        ClientMessage, MessageId, RemoteStore, ServerMessage, Transport, TxId,
        list_repositories,
    };
    use crate::error::{Error, Result};
    use crate::object::{Object, TextObject};
    use crate::store::{ObjectStore, RefStore};
    use clayers_xml::ContentHash;

    /// Extract the `id` field from any `ClientMessage` variant.
    fn client_msg_id(m: &ClientMessage) -> MessageId {
        match m {
            ClientMessage::ListRepositories { id }
            | ClientMessage::Get { id, .. }
            | ClientMessage::Contains { id, .. }
            | ClientMessage::GetByInclusiveHash { id, .. }
            | ClientMessage::Subtree { id, .. }
            | ClientMessage::GetRef { id, .. }
            | ClientMessage::SetRef { id, .. }
            | ClientMessage::DeleteRef { id, .. }
            | ClientMessage::ListRefs { id, .. }
            | ClientMessage::CasRef { id, .. }
            | ClientMessage::BeginTransaction { id, .. }
            | ClientMessage::TxPut { id, .. }
            | ClientMessage::TxCommit { id, .. }
            | ClientMessage::TxRollback { id, .. } => *id,
        }
    }

    /// Set the `id` on a reply-type `ServerMessage`. Notifications are
    /// returned unchanged.
    fn with_id(mut m: ServerMessage, new_id: MessageId) -> ServerMessage {
        match &mut m {
            ServerMessage::RepositoryList { id, .. }
            | ServerMessage::Object { id, .. }
            | ServerMessage::Contains { id, .. }
            | ServerMessage::ObjectWithHash { id, .. }
            | ServerMessage::SubtreeItem { id, .. }
            | ServerMessage::SubtreeEnd { id, .. }
            | ServerMessage::Ref { id, .. }
            | ServerMessage::RefSet { id, .. }
            | ServerMessage::RefDeleted { id, .. }
            | ServerMessage::RefList { id, .. }
            | ServerMessage::CasResult { id, .. }
            | ServerMessage::Ok { id, .. }
            | ServerMessage::TransactionCreated { id, .. }
            | ServerMessage::Error { id, .. } => *id = new_id,
            ServerMessage::RefUpdated { .. } | ServerMessage::TransactionTerminated { .. } => {}
        }
        m
    }

    /// A scripted response: produces a sequence of messages from the
    /// observed request.
    type Responder = Box<dyn Fn(&ClientMessage) -> Vec<ServerMessage> + Send + Sync>;

    /// In-memory transport that auto-responds based on the request type.
    /// Captures sent messages for assertion.
    pub(super) struct MockTransport {
        sent: Mutex<Vec<ClientMessage>>,
        incoming: Mutex<VecDeque<ServerMessage>>,
        notify: Notify,
        responder: Responder,
    }

    impl MockTransport {
        fn new(responder: Responder) -> Self {
            Self {
                sent: Mutex::new(Vec::new()),
                incoming: Mutex::new(VecDeque::new()),
                notify: Notify::new(),
                responder,
            }
        }

        /// Drain the captured sent messages. Empties the internal buffer.
        async fn drain_sent(&self) -> Vec<ClientMessage> {
            std::mem::take(&mut *self.sent.lock().await)
        }
    }

    #[async_trait::async_trait]
    impl Transport for MockTransport {
        async fn send(&self, msg: ClientMessage) -> Result<()> {
            let id = client_msg_id(&msg);
            let responses = (self.responder)(&msg);
            self.sent.lock().await.push(msg);
            {
                let mut incoming = self.incoming.lock().await;
                for resp in responses {
                    incoming.push_back(with_id(resp, id));
                }
            }
            self.notify.notify_waiters();
            Ok(())
        }

        async fn recv(&self) -> Result<ServerMessage> {
            loop {
                {
                    let mut incoming = self.incoming.lock().await;
                    if let Some(m) = incoming.pop_front() {
                        return Ok(m);
                    }
                }
                self.notify.notified().await;
            }
        }
    }

    fn err_responder(message: &'static str) -> Responder {
        let m = message.to_string();
        Box::new(move |_msg| {
            vec![ServerMessage::Error { id: 0, message: m.clone() }]
        })
    }

    /// Wrapper around `Arc<MockTransport>` so a test can hold a handle
    /// to inspect captured messages while the `RemoteStore` owns its
    /// own transport (delegating via this wrapper).
    struct DelegatingTransport(Arc<MockTransport>);

    #[async_trait::async_trait]
    impl Transport for DelegatingTransport {
        async fn send(&self, msg: ClientMessage) -> Result<()> {
            self.0.send(msg).await
        }
        async fn recv(&self) -> Result<ServerMessage> {
            self.0.recv().await
        }
    }

    fn assert_storage_err_contains(result: Result<impl std::fmt::Debug>, needle: &str) {
        match result {
            Err(Error::Storage(msg)) => assert!(
                msg.contains(needle),
                "expected error containing {needle:?}, got {msg:?}",
            ),
            Err(e) => panic!("expected Error::Storage, got {e:?}"),
            Ok(v) => panic!("expected error, got Ok({v:?})"),
        }
    }

    // ── list_repositories: cardinality and contents ────────────────────

    #[tokio::test]
    async fn list_repositories_returns_exact_repos_from_server() {
        let transport = MockTransport::new(Box::new(|_msg| {
            vec![ServerMessage::RepositoryList {
                id: 0,
                repos: vec!["alpha".into(), "beta".into(), "gamma".into()],
            }]
        }));
        let store = RemoteStore::new(transport, "ignored");
        let listed = store.list_repositories().await.unwrap();
        assert_eq!(listed, vec!["alpha".to_string(), "beta".into(), "gamma".into()]);
    }

    #[tokio::test]
    async fn list_repositories_empty_list_returned_as_empty() {
        let transport = MockTransport::new(Box::new(|_msg| {
            vec![ServerMessage::RepositoryList { id: 0, repos: vec![] }]
        }));
        let store = RemoteStore::new(transport, "ignored");
        let listed = store.list_repositories().await.unwrap();
        assert!(listed.is_empty());
    }

    #[tokio::test]
    async fn list_repositories_does_not_invent_names() {
        let transport = MockTransport::new(Box::new(|_msg| {
            vec![ServerMessage::RepositoryList { id: 0, repos: vec!["only".into()] }]
        }));
        let store = RemoteStore::new(transport, "ignored");
        let listed = store.list_repositories().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert!(!listed.contains(&"xyzzy".to_string()));
        assert!(!listed.contains(&String::new()));
    }

    #[tokio::test]
    async fn standalone_list_repositories_returns_exact_repos() {
        let transport = MockTransport::new(Box::new(|_msg| {
            vec![ServerMessage::RepositoryList {
                id: 0,
                repos: vec!["one".into(), "two".into()],
            }]
        }));
        let listed = list_repositories(&transport).await.unwrap();
        assert_eq!(listed, vec!["one".to_string(), "two".into()]);
    }

    #[tokio::test]
    async fn standalone_list_repositories_propagates_server_error() {
        let transport = MockTransport::new(err_responder("server says no"));
        let result = list_repositories(&transport).await;
        assert_storage_err_contains(result, "server says no");
    }

    // ── Server Error responses surface to caller verbatim ─────────────

    #[tokio::test]
    async fn list_repositories_propagates_server_error() {
        let transport = MockTransport::new(err_responder("auth required"));
        let store = RemoteStore::new(transport, "ignored");
        let result = store.list_repositories().await;
        assert_storage_err_contains(result, "auth required");
    }

    #[tokio::test]
    async fn get_propagates_server_error() {
        let transport = MockTransport::new(err_responder("get failed"));
        let store = RemoteStore::new(transport, "myrepo");
        let result = store.get(&ContentHash::from_canonical(b"x")).await;
        assert_storage_err_contains(result, "get failed");
    }

    #[tokio::test]
    async fn contains_propagates_server_error() {
        let transport = MockTransport::new(err_responder("contains failed"));
        let store = RemoteStore::new(transport, "myrepo");
        let result = store.contains(&ContentHash::from_canonical(b"x")).await;
        assert_storage_err_contains(result, "contains failed");
    }

    #[tokio::test]
    async fn transaction_begin_propagates_server_error() {
        let transport = MockTransport::new(err_responder("repo not found"));
        let store = RemoteStore::new(transport, "myrepo");
        // Box<dyn Transaction> isn't Debug; check by hand.
        match store.transaction().await {
            Err(Error::Storage(msg)) => {
                assert!(msg.contains("repo not found"), "got: {msg:?}");
            }
            Err(e) => panic!("expected Storage error, got {e:?}"),
            Ok(_) => panic!("expected error, got Ok(transaction)"),
        }
    }

    #[tokio::test]
    async fn get_by_inclusive_hash_propagates_server_error() {
        let transport = MockTransport::new(err_responder("inclusive lookup failed"));
        let store = RemoteStore::new(transport, "myrepo");
        let result = store
            .get_by_inclusive_hash(&ContentHash::from_canonical(b"x"))
            .await;
        assert_storage_err_contains(result, "inclusive lookup failed");
    }

    #[tokio::test]
    async fn get_ref_propagates_server_error() {
        let transport = MockTransport::new(err_responder("get_ref failed"));
        let store = RemoteStore::new(transport, "myrepo");
        let result = store.get_ref("refs/heads/main").await;
        assert_storage_err_contains(result, "get_ref failed");
    }

    #[tokio::test]
    async fn set_ref_propagates_server_error() {
        let transport = MockTransport::new(err_responder("set_ref denied"));
        let store = RemoteStore::new(transport, "myrepo");
        let result = store
            .set_ref("refs/heads/main", ContentHash::from_canonical(b"v1"))
            .await;
        assert_storage_err_contains(result, "set_ref denied");
    }

    #[tokio::test]
    async fn tx_rollback_propagates_server_error() {
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter_for_responder = Arc::clone(&counter);
        let transport = MockTransport::new(Box::new(move |msg| {
            let n = counter_for_responder.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            match (n, msg) {
                (0, ClientMessage::BeginTransaction { .. }) => {
                    vec![ServerMessage::TransactionCreated { id: 0, tx_id: TxId(101) }]
                }
                _ => vec![ServerMessage::Error {
                    id: 0,
                    message: "rollback rejected".into(),
                }],
            }
        }));
        let store = RemoteStore::new(transport, "myrepo");
        let mut tx = store.transaction().await.unwrap();
        let result = tx.rollback().await;
        assert_storage_err_contains(result, "rollback rejected");
    }

    #[tokio::test]
    async fn delete_ref_propagates_server_error() {
        let transport = MockTransport::new(err_responder("delete denied"));
        let store = RemoteStore::new(transport, "myrepo");
        let result = store.delete_ref("refs/heads/main").await;
        assert_storage_err_contains(result, "delete denied");
    }

    #[tokio::test]
    async fn list_refs_propagates_server_error() {
        let transport = MockTransport::new(err_responder("list_refs failed"));
        let store = RemoteStore::new(transport, "myrepo");
        let result = store.list_refs("refs/heads/").await;
        assert_storage_err_contains(result, "list_refs failed");
    }

    #[tokio::test]
    async fn cas_ref_propagates_server_error() {
        let transport = MockTransport::new(err_responder("cas failed"));
        let store = RemoteStore::new(transport, "myrepo");
        let result = store
            .cas_ref(
                "refs/heads/main",
                None,
                ContentHash::from_canonical(b"v1"),
            )
            .await;
        assert_storage_err_contains(result, "cas failed");
    }

    #[tokio::test]
    async fn tx_put_propagates_server_error() {
        // First request begins the tx (returns Ok); subsequent put
        // returns Error. We need a stateful responder.
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter_for_responder = Arc::clone(&counter);
        let transport = MockTransport::new(Box::new(move |msg| {
            let n = counter_for_responder.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            match (n, msg) {
                (0, ClientMessage::BeginTransaction { .. }) => {
                    vec![ServerMessage::TransactionCreated { id: 0, tx_id: TxId(99) }]
                }
                _ => vec![ServerMessage::Error { id: 0, message: "put failed".into() }],
            }
        }));
        let store = RemoteStore::new(transport, "myrepo");
        let mut tx = store.transaction().await.unwrap();
        let result = tx
            .put(
                ContentHash::from_canonical(b"x"),
                Object::Text(TextObject { content: "hi".into() }),
            )
            .await;
        assert_storage_err_contains(result, "put failed");
    }

    // ── RemoteTransaction::Drop sends fire-and-forget rollback ────────

    #[tokio::test]
    async fn tx_drop_without_finish_sends_rollback() {
        let transport_arc = Arc::new(MockTransport::new(Box::new(|msg| match msg {
            ClientMessage::BeginTransaction { .. } => {
                vec![ServerMessage::TransactionCreated { id: 0, tx_id: TxId(7) }]
            }
            // For any subsequent request (e.g., rollback from drop),
            // respond with Ok so the spawned task doesn't hang.
            _ => vec![ServerMessage::Ok { id: 0 }],
        })));

        let delegating = DelegatingTransport(Arc::clone(&transport_arc));
        let store = RemoteStore::new(delegating, "myrepo");
        let tx = store.transaction().await.unwrap();
        drop(tx);

        // Give the spawned drop task a moment to run.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let sent = transport_arc.drain_sent().await;
        let saw_rollback = sent.iter().any(|m| {
            matches!(m, ClientMessage::TxRollback { tx_id, .. } if tx_id.0 == 7)
        });
        assert!(
            saw_rollback,
            "drop on unfinished tx must send TxRollback; saw: {sent:?}",
        );
    }

    #[tokio::test]
    async fn tx_drop_after_commit_does_not_send_rollback() {
        let transport_arc = Arc::new(MockTransport::new(Box::new(|msg| match msg {
            ClientMessage::BeginTransaction { .. } => {
                vec![ServerMessage::TransactionCreated { id: 0, tx_id: TxId(8) }]
            }
            _ => vec![ServerMessage::Ok { id: 0 }],
        })));

        let delegating = DelegatingTransport(Arc::clone(&transport_arc));
        let store = RemoteStore::new(delegating, "myrepo");
        let mut tx = store.transaction().await.unwrap();
        tx.commit().await.unwrap();
        drop(tx);

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let sent = transport_arc.drain_sent().await;
        let rollback_count = sent
            .iter()
            .filter(|m| matches!(m, ClientMessage::TxRollback { .. }))
            .count();
        assert_eq!(
            rollback_count, 0,
            "drop after commit must not send TxRollback; saw: {sent:?}",
        );
    }

    #[tokio::test]
    async fn tx_drop_after_rollback_does_not_send_rollback() {
        let transport_arc = Arc::new(MockTransport::new(Box::new(|msg| match msg {
            ClientMessage::BeginTransaction { .. } => {
                vec![ServerMessage::TransactionCreated { id: 0, tx_id: TxId(9) }]
            }
            _ => vec![ServerMessage::Ok { id: 0 }],
        })));

        let delegating = DelegatingTransport(Arc::clone(&transport_arc));
        let store = RemoteStore::new(delegating, "myrepo");
        let mut tx = store.transaction().await.unwrap();
        tx.rollback().await.unwrap();
        drop(tx);

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let sent = transport_arc.drain_sent().await;
        // Exactly one rollback (the explicit one); no second one from drop.
        let rollback_count = sent
            .iter()
            .filter(|m| matches!(m, ClientMessage::TxRollback { .. }))
            .count();
        assert_eq!(
            rollback_count, 1,
            "expected exactly one TxRollback (from explicit call); saw: {sent:?}",
        );
    }
}
