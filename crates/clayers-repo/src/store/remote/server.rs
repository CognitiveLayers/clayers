//! Server-side handler that dispatches client messages to store backends.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::RwLock as StdRwLock;
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

struct ConnState {
    sender: ConnectionSender,
    subscribed_repos: StdMutex<HashSet<String>>,
}

/// Server that dispatches client messages to stores via a [`RepositoryProvider`].
pub struct Server<P: RepositoryProvider> {
    provider: Arc<P>,
    transactions: Arc<Mutex<HashMap<TxId, TxState>>>,
    next_tx_id: AtomicU64,
    /// Map of connections. Each entry is an Arc so we can snapshot the
    /// values for broadcasts without holding the top-level lock during
    /// per-connection work.
    connections: Arc<Mutex<HashMap<ConnectionId, Arc<ConnState>>>>,
    /// Repository subscriptions keyed by repository name. A connection only
    /// receives notifications (e.g., `RefUpdated`) for repos it has
    /// successfully resolved, preventing cross-tenant information
    /// leakage without scanning every connection on each broadcast.
    subscribers: Arc<StdRwLock<HashMap<String, HashSet<ConnectionId>>>>,
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
            subscribers: Arc::new(StdRwLock::new(HashMap::new())),
            next_conn_id: AtomicU64::new(1),
        }
    }

    /// Register a new connection and return its ID and a receiver for notifications.
    pub async fn register_connection(
        &self,
    ) -> (
        ConnectionId,
        tokio::sync::mpsc::UnboundedReceiver<ServerMessage>,
    ) {
        let id = self.next_conn_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let state = Arc::new(ConnState {
            sender: tx,
            subscribed_repos: StdMutex::new(HashSet::new()),
        });
        self.connections.lock().await.insert(id, state);
        (id, rx)
    }

    /// Unregister a connection and roll back its open transactions.
    pub async fn disconnect(&self, conn_id: ConnectionId) {
        let state = self.connections.lock().await.remove(&conn_id);
        if let Some(state) = state {
            let repos = match state.subscribed_repos.lock() {
                Ok(subscribed_repos) => subscribed_repos.clone(),
                Err(poisoned) => poisoned.into_inner().clone(),
            };
            {
                let mut subscribers = match self.subscribers.write() {
                    Ok(subscribers) => subscribers,
                    Err(poisoned) => poisoned.into_inner(),
                };
                for repo in repos {
                    let remove_repo = if let Some(ids) = subscribers.get_mut(&repo) {
                        ids.remove(&conn_id);
                        ids.is_empty()
                    } else {
                        false
                    };
                    if remove_repo {
                        subscribers.remove(&repo);
                    }
                }
            }
        }

        let mut txns = self.transactions.lock().await;
        let to_rollback: Vec<TxId> = txns
            .iter()
            .filter(|(_, state)| state.connection_id == conn_id)
            .map(|(id, _)| *id)
            .collect();
        for tx_id in to_rollback {
            if let Some(mut state) = txns.remove(&tx_id) {
                // Best-effort cleanup: rollback failure is non-fatal on disconnect.
                drop(state.tx.rollback().await);
            }
        }
    }

    /// Record that a connection has successfully resolved a repo. Idempotent.
    /// Subsequent `RefUpdated` notifications for that repo will be
    /// delivered to this connection.
    async fn subscribe(&self, conn_id: ConnectionId, repo: &str) {
        let state = self.connections.lock().await.get(&conn_id).cloned();
        let Some(state) = state else {
            return;
        };

        let inserted = {
            let mut subscribed_repos = match state.subscribed_repos.lock() {
                Ok(subscribed_repos) => subscribed_repos,
                Err(poisoned) => poisoned.into_inner(),
            };
            subscribed_repos.insert(repo.to_string())
        };
        if !inserted {
            return;
        }

        let mut subscribers = match self.subscribers.write() {
            Ok(subscribers) => subscribers,
            Err(poisoned) => poisoned.into_inner(),
        };
        subscribers
            .entry(repo.to_string())
            .or_default()
            .insert(conn_id);
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
                self.with_store(conn_id, id, &repo, |store| async move {
                    store
                        .get(&hash)
                        .await
                        .map(|object| ServerMessage::Object { id, object })
                })
                .await
            }
            ClientMessage::Contains { id, repo, hash } => {
                self.with_store(conn_id, id, &repo, |store| async move {
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
                self.with_store(conn_id, id, &repo, |store| async move {
                    store
                        .get_by_inclusive_hash(&inclusive_hash)
                        .await
                        .map(|result| ServerMessage::ObjectWithHash { id, result })
                })
                .await
            }
            ClientMessage::Subtree { id, repo, root } => {
                self.handle_subtree(conn_id, id, &repo, root).await
            }
            ClientMessage::GetRef { id, repo, name } => {
                self.with_store(conn_id, id, &repo, |store| async move {
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
                self.with_store(conn_id, id, &repo, |store| async move {
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
                // Check ownership BEFORE removing. Without this, any
                // connection could silently cancel another's tx.
                let owns = matches!(
                    txns.get(&tx_id),
                    Some(state) if state.connection_id == conn_id
                );
                if owns {
                    if let Some(mut state) = txns.remove(&tx_id) {
                        match state.tx.rollback().await {
                            Ok(()) => vec![ServerMessage::Ok { id }],
                            Err(e) => vec![err(id, &e)],
                        }
                    } else {
                        // Should be unreachable: we just confirmed
                        // the entry exists. Treat as missing for safety.
                        vec![ServerMessage::Error {
                            id,
                            message: format!("transaction not found: {}", tx_id.0),
                        }]
                    }
                } else {
                    vec![ServerMessage::Error {
                        id,
                        message: format!("transaction not found: {}", tx_id.0),
                    }]
                }
            }
        }
    }

    async fn resolve_store(
        &self,
        conn_id: ConnectionId,
        id: super::MessageId,
        repo: &str,
    ) -> std::result::Result<Arc<dyn Store>, ServerMessage> {
        match self.provider.get(repo).await {
            Ok(store) => {
                self.subscribe(conn_id, repo).await;
                Ok(store)
            }
            Err(e) => Err(err(id, &e)),
        }
    }

    // Helper: run a closure with a store, returning vec of one message
    async fn with_store<F, Fut>(
        &self,
        conn_id: ConnectionId,
        id: super::MessageId,
        repo: &str,
        f: F,
    ) -> Vec<ServerMessage>
    where
        F: FnOnce(Arc<dyn Store>) -> Fut,
        Fut: std::future::Future<Output = Result<ServerMessage>>,
    {
        match self.resolve_store(conn_id, id, repo).await {
            Ok(store) => match f(store).await {
                Ok(msg) => vec![msg],
                Err(e) => vec![err(id, &e)],
            },
            Err(msg) => vec![msg],
        }
    }

    async fn handle_subtree(
        &self,
        conn_id: ConnectionId,
        id: super::MessageId,
        repo: &str,
        root: clayers_xml::ContentHash,
    ) -> Vec<ServerMessage> {
        let store = match self.resolve_store(conn_id, id, repo).await {
            Ok(store) => store,
            Err(msg) => return vec![msg],
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
        let store = match self.resolve_store(conn_id, id, repo).await {
            Ok(store) => store,
            Err(msg) => return vec![msg],
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
        let store = match self.resolve_store(conn_id, id, repo).await {
            Ok(store) => store,
            Err(msg) => return vec![msg],
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
        let store = match self.resolve_store(conn_id, id, repo).await {
            Ok(store) => store,
            Err(msg) => return vec![msg],
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
        let store = match self.resolve_store(conn_id, id, repo).await {
            Ok(store) => store,
            Err(msg) => return vec![msg],
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
        let subscriber_ids: Vec<_> = {
            let subscribers = match self.subscribers.read() {
                Ok(subscribers) => subscribers,
                Err(poisoned) => poisoned.into_inner(),
            };
            let Some(ids) = subscribers.get(repo) else {
                return;
            };
            ids.iter().copied().filter(|id| *id != origin).collect()
        };
        if subscriber_ids.is_empty() {
            return;
        }

        let conns = self.connections.lock().await;
        for id in subscriber_ids {
            if let Some(state) = conns.get(&id) {
                drop(state.sender.send(ServerMessage::RefUpdated {
                    repo: repo.to_string(),
                    name: name.to_string(),
                    old,
                    new,
                }));
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

#[cfg(test)]
#[allow(clippy::doc_markdown, clippy::missing_panics_doc)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use super::super::transport::Store;
    use super::{RepositoryProvider, StaticRepositories};
    use crate::store::memory::MemoryStore;

    fn provider_with(names: &[&str]) -> StaticRepositories {
        let mut map: HashMap<String, Arc<dyn Store>> = HashMap::new();
        for name in names {
            map.insert(
                (*name).to_string(),
                Arc::new(MemoryStore::new()) as Arc<dyn Store>,
            );
        }
        StaticRepositories::new(map)
    }

    #[tokio::test]
    async fn empty_provider_lists_no_repos() {
        let provider = provider_with(&[]);
        let listed = provider.list().await.unwrap();
        assert_eq!(listed.len(), 0);
    }

    #[tokio::test]
    async fn list_returns_exactly_inserted_names() {
        let provider = provider_with(&["alpha", "beta", "gamma"]);
        let listed = provider.list().await.unwrap();
        assert_eq!(
            listed.len(),
            3,
            "list cardinality must match provider input"
        );
        let listed_set: HashSet<&str> = listed.iter().map(String::as_str).collect();
        let expected: HashSet<&str> = ["alpha", "beta", "gamma"].into_iter().collect();
        assert_eq!(listed_set, expected, "listed names must match exactly");
    }

    #[tokio::test]
    async fn list_does_not_contain_unrelated_names() {
        let provider = provider_with(&["alpha"]);
        let listed = provider.list().await.unwrap();
        assert!(!listed.contains(&"beta".to_string()));
        assert!(!listed.contains(&String::new()));
        assert!(!listed.contains(&"xyzzy".to_string()));
    }

    #[tokio::test]
    async fn list_excludes_empty_string_when_not_inserted() {
        let provider = provider_with(&["a", "b"]);
        let listed = provider.list().await.unwrap();
        assert!(!listed.iter().any(String::is_empty));
    }

    #[tokio::test]
    async fn get_unknown_returns_error() {
        let provider = provider_with(&["alpha"]);
        let result = provider.get("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_returns_inserted_store() {
        let provider = provider_with(&["alpha"]);
        let _store = provider.get("alpha").await.unwrap();
    }

    // ── Cross-connection transaction isolation ─────────────────────────

    use super::Server;
    use crate::store::remote::{ClientMessage, ServerMessage, TxId};
    use clayers_xml::ContentHash;

    /// Begin a transaction on `conn_id` for the given repo. Returns the
    /// server-assigned tx_id. Panics if the response isn't TransactionCreated.
    async fn begin_tx<P: RepositoryProvider + 'static>(
        server: &Server<P>,
        conn_id: super::ConnectionId,
        repo: &str,
    ) -> TxId {
        let resp = server
            .handle(
                conn_id,
                ClientMessage::BeginTransaction {
                    id: 1,
                    repo: repo.into(),
                },
            )
            .await;
        match resp.as_slice() {
            [ServerMessage::TransactionCreated { tx_id, .. }] => *tx_id,
            other => panic!("expected TransactionCreated, got {other:?}"),
        }
    }

    /// A non-owner connection sending TxPut for someone else's tx_id
    /// must receive an Error (not Ok).
    #[tokio::test]
    async fn tx_put_rejects_foreign_connection() {
        let provider = provider_with(&["myrepo"]);
        let server = Server::new(provider);
        let (conn_a, _rx_a) = server.register_connection().await;
        let (conn_b, _rx_b) = server.register_connection().await;

        let tx_id = begin_tx(&server, conn_a, "myrepo").await;

        let resp = server
            .handle(
                conn_b,
                ClientMessage::TxPut {
                    id: 42,
                    tx_id,
                    hash: ContentHash::from_canonical(b"foreign_put"),
                    object: crate::object::Object::Text(crate::object::TextObject {
                        content: "hi".into(),
                    }),
                },
            )
            .await;

        assert!(
            matches!(resp.as_slice(), [ServerMessage::Error { .. }]),
            "non-owner TxPut must error, got {resp:?}",
        );
    }

    /// A non-owner connection sending TxCommit for someone else's tx_id
    /// must receive an Error and must NOT consume the transaction.
    #[tokio::test]
    async fn tx_commit_rejects_foreign_connection_and_preserves_tx() {
        let provider = provider_with(&["myrepo"]);
        let server = Server::new(provider);
        let (conn_a, _rx_a) = server.register_connection().await;
        let (conn_b, _rx_b) = server.register_connection().await;

        let tx_id = begin_tx(&server, conn_a, "myrepo").await;

        // Conn B tries to commit A's tx.
        let resp_b = server
            .handle(conn_b, ClientMessage::TxCommit { id: 1, tx_id })
            .await;
        assert!(
            matches!(resp_b.as_slice(), [ServerMessage::Error { .. }]),
            "non-owner TxCommit must error, got {resp_b:?}",
        );

        // Conn A's tx must still be operable: legitimate commit succeeds.
        let resp_a = server
            .handle(conn_a, ClientMessage::TxCommit { id: 2, tx_id })
            .await;
        assert!(
            matches!(resp_a.as_slice(), [ServerMessage::Ok { .. }]),
            "owner TxCommit must succeed after foreign TxCommit was rejected, got {resp_a:?}",
        );
    }

    /// A non-owner connection sending TxRollback for someone else's
    /// tx_id must receive an Error and must NOT cancel the transaction.
    /// (Pre-fix, this returned Ok and silently dropped the state.)
    #[tokio::test]
    async fn tx_rollback_rejects_foreign_connection_and_preserves_tx() {
        let provider = provider_with(&["myrepo"]);
        let server = Server::new(provider);
        let (conn_a, _rx_a) = server.register_connection().await;
        let (conn_b, _rx_b) = server.register_connection().await;

        let tx_id = begin_tx(&server, conn_a, "myrepo").await;

        // Conn B tries to rollback A's tx.
        let resp_b = server
            .handle(conn_b, ClientMessage::TxRollback { id: 1, tx_id })
            .await;
        assert!(
            matches!(resp_b.as_slice(), [ServerMessage::Error { .. }]),
            "non-owner TxRollback must error, got {resp_b:?}",
        );

        // Owner's commit must still work — proves the tx was not silently
        // cancelled by the foreign rollback.
        let resp_a = server
            .handle(conn_a, ClientMessage::TxCommit { id: 2, tx_id })
            .await;
        assert!(
            matches!(resp_a.as_slice(), [ServerMessage::Ok { .. }]),
            "owner TxCommit must succeed after foreign TxRollback was rejected, got {resp_a:?}",
        );
    }

    /// Connections are subscribed to a repo only after touching it.
    /// A connection that has never named a repo must NOT receive
    /// `RefUpdated` notifications for that repo.
    #[tokio::test]
    async fn ref_updated_not_broadcast_to_unsubscribed_connection() {
        let provider = provider_with(&["alpha", "beta"]);
        let server = Server::new(provider);

        // Connection A subscribes to alpha; connection B subscribes to beta.
        let (conn_a, mut rx_a) = server.register_connection().await;
        let (conn_b, mut rx_b) = server.register_connection().await;

        // A touches alpha (e.g., reads a ref).
        let _ = server
            .handle(
                conn_a,
                ClientMessage::GetRef {
                    id: 1,
                    repo: "alpha".into(),
                    name: "refs/heads/main".into(),
                },
            )
            .await;
        // B touches beta.
        let _ = server
            .handle(
                conn_b,
                ClientMessage::GetRef {
                    id: 2,
                    repo: "beta".into(),
                    name: "refs/heads/main".into(),
                },
            )
            .await;

        // A sets a ref on alpha. B is on beta and should NOT see this.
        let _ = server
            .handle(
                conn_a,
                ClientMessage::SetRef {
                    id: 3,
                    repo: "alpha".into(),
                    name: "refs/heads/main".into(),
                    hash: ContentHash::from_canonical(b"alpha_v1"),
                },
            )
            .await;

        // Give the broadcast a moment to fan out (it shouldn't, but
        // we want to be sure if it did, we'd see it).
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        // B's channel must be empty.
        match rx_b.try_recv() {
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
            other => panic!("B should not have received any notification, got {other:?}"),
        }

        // A is the origin and never receives its own broadcast.
        match rx_a.try_recv() {
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
            other => panic!("A should not have received its own broadcast, got {other:?}"),
        }
    }

    /// A failed lookup must not subscribe the connection to the requested
    /// repo name. Otherwise a client could ask for a nonexistent repo and
    /// later receive notifications if that repo appears.
    #[tokio::test]
    async fn failed_repo_lookup_does_not_subscribe_connection() {
        let provider = provider_with(&["alpha"]);
        let server = Server::new(provider);

        let (conn, _rx) = server.register_connection().await;
        let resp = server
            .handle(
                conn,
                ClientMessage::GetRef {
                    id: 1,
                    repo: "missing".into(),
                    name: "refs/heads/main".into(),
                },
            )
            .await;

        assert!(
            matches!(resp.as_slice(), [ServerMessage::Error { .. }]),
            "missing repo lookup should fail, got {resp:?}",
        );

        let subscribers = match server.subscribers.read() {
            Ok(subscribers) => subscribers,
            Err(poisoned) => poisoned.into_inner(),
        };
        assert!(
            !subscribers
                .get("missing")
                .is_some_and(|ids| ids.contains(&conn)),
            "failed lookup must not subscribe the connection",
        );
    }

    /// A connection subscribed to a repo DOES receive notifications when
    /// another connection mutates that repo. This is the positive path
    /// for the subscription filter.
    #[tokio::test]
    async fn ref_updated_broadcast_to_subscribed_connection() {
        let provider = provider_with(&["shared"]);
        let server = Server::new(provider);

        let (conn_a, _rx_a) = server.register_connection().await;
        let (conn_b, mut rx_b) = server.register_connection().await;

        // Both connections touch the shared repo.
        let _ = server
            .handle(
                conn_a,
                ClientMessage::GetRef {
                    id: 1,
                    repo: "shared".into(),
                    name: "refs/heads/main".into(),
                },
            )
            .await;
        let _ = server
            .handle(
                conn_b,
                ClientMessage::GetRef {
                    id: 2,
                    repo: "shared".into(),
                    name: "refs/heads/main".into(),
                },
            )
            .await;

        // A mutates the repo.
        let hash = ContentHash::from_canonical(b"shared_v1");
        let _ = server
            .handle(
                conn_a,
                ClientMessage::SetRef {
                    id: 3,
                    repo: "shared".into(),
                    name: "refs/heads/main".into(),
                    hash,
                },
            )
            .await;

        // B receives a notification for the shared repo.
        let notif = tokio::time::timeout(std::time::Duration::from_millis(200), rx_b.recv())
            .await
            .expect("B should receive a notification within timeout")
            .expect("channel closed");

        match notif {
            ServerMessage::RefUpdated {
                repo, name, new, ..
            } => {
                assert_eq!(repo, "shared");
                assert_eq!(name, "refs/heads/main");
                assert_eq!(new, Some(hash));
            }
            other => panic!("expected RefUpdated, got {other:?}"),
        }
    }

    /// Disconnecting a connection clears its subscription set so that
    /// subsequent broadcasts cannot reference it.
    #[tokio::test]
    async fn disconnect_clears_subscriptions() {
        let provider = provider_with(&["alpha"]);
        let server = Server::new(provider);

        let (conn_a, _rx_a) = server.register_connection().await;
        let _ = server
            .handle(
                conn_a,
                ClientMessage::GetRef {
                    id: 1,
                    repo: "alpha".into(),
                    name: "refs/heads/main".into(),
                },
            )
            .await;

        // Disconnect A.
        server.disconnect(conn_a).await;

        // Connection state for A is gone.
        let conns = server.connections.lock().await;
        assert!(!conns.contains_key(&conn_a));
        drop(conns);

        // Subscriber index for A is gone too.
        let subscribers = match server.subscribers.read() {
            Ok(subscribers) => subscribers,
            Err(poisoned) => poisoned.into_inner(),
        };
        assert!(!subscribers.values().any(|ids| ids.contains(&conn_a)));
    }

    /// Owner's own TxPut, TxCommit, and TxRollback succeed (positive
    /// control: ensures the connection_id check doesn't reject legitimate
    /// requests).
    #[tokio::test]
    async fn owner_tx_operations_all_succeed() {
        let provider = provider_with(&["myrepo"]);
        let server = Server::new(provider);
        let (conn, _rx) = server.register_connection().await;

        // Tx 1: put + commit
        let tx_id = begin_tx(&server, conn, "myrepo").await;
        let put_resp = server
            .handle(
                conn,
                ClientMessage::TxPut {
                    id: 1,
                    tx_id,
                    hash: ContentHash::from_canonical(b"owner_put"),
                    object: crate::object::Object::Text(crate::object::TextObject {
                        content: "ok".into(),
                    }),
                },
            )
            .await;
        assert!(
            matches!(put_resp.as_slice(), [ServerMessage::Ok { .. }]),
            "owner TxPut must succeed, got {put_resp:?}",
        );

        let commit_resp = server
            .handle(conn, ClientMessage::TxCommit { id: 2, tx_id })
            .await;
        assert!(matches!(commit_resp.as_slice(), [ServerMessage::Ok { .. }]));

        // Tx 2: rollback
        let tx_id_2 = begin_tx(&server, conn, "myrepo").await;
        let rollback_resp = server
            .handle(
                conn,
                ClientMessage::TxRollback {
                    id: 3,
                    tx_id: tx_id_2,
                },
            )
            .await;
        assert!(
            matches!(rollback_resp.as_slice(), [ServerMessage::Ok { .. }]),
            "owner TxRollback must succeed, got {rollback_resp:?}",
        );
    }
}
