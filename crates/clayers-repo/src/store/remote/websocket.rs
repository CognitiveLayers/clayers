//! WebSocket transport implementation.
//!
//! Uses internal channels so the transport can be used from any runtime/thread.
//! All actual WS IO happens on background tasks spawned during `connect()`.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, mpsc};
use tokio::task::{AbortHandle, JoinHandle};
use tokio_tungstenite::tungstenite::Message;

use super::codec::Codec;
use super::server::{RepositoryProvider, Server};
use super::transport::Transport;
use super::{ClientMessage, ServerMessage};
use crate::error::{Error, Result};

/// WebSocket transport for the client side.
///
/// Internally uses channels to decouple from the runtime that created the socket.
/// Background tasks handle the actual WS reads and writes.
pub struct WsTransport<C: Codec = super::JsonCodec> {
    outgoing: mpsc::UnboundedSender<ClientMessage>,
    incoming: Mutex<mpsc::UnboundedReceiver<ServerMessage>>,
    #[allow(dead_code)]
    codec: C,
    _writer_abort: AbortHandle,
    _reader_abort: AbortHandle,
}

impl<C: Codec> WsTransport<C> {
    /// Connect to a WebSocket server.
    ///
    /// Spawns background reader and writer tasks on the current tokio runtime.
    /// The returned transport can be used from any thread or runtime.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection or handshake fails.
    pub async fn connect(
        url: &str,
        codec: C,
        auth: Option<&dyn WsRequestTransformer>,
    ) -> Result<Self> {
        let (ws_stream, _) = if let Some(transformer) = auth {
            // Build a request with auth headers applied
            let req = http::Request::builder()
                .uri(url)
                .header("Host", url_host(url))
                .header("Connection", "Upgrade")
                .header("Upgrade", "websocket")
                .header("Sec-WebSocket-Version", "13")
                .header(
                    "Sec-WebSocket-Key",
                    tokio_tungstenite::tungstenite::handshake::client::generate_key(),
                )
                .body(())
                .map_err(|e| Error::Storage(e.to_string()))?;
            let req = transformer.transform(req);
            tokio_tungstenite::connect_async(req)
                .await
                .map_err(|e| Error::Storage(e.to_string()))?
        } else {
            tokio_tungstenite::connect_async(url)
                .await
                .map_err(|e| Error::Storage(e.to_string()))?
        };

        let (mut ws_write, mut ws_read) = ws_stream.split();

        // Outgoing channel: client sends ClientMessage -> writer task encodes and sends over WS
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ClientMessage>();
        let codec_w = codec.clone();
        let writer = tokio::spawn(async move {
            while let Some(msg) = out_rx.recv().await {
                if let Ok(bytes) = codec_w.encode(&msg)
                    && ws_write.send(Message::Binary(bytes.into())).await.is_err()
                {
                    break;
                }
            }
        });

        // Incoming channel: reader task reads from WS, decodes, sends ServerMessage
        let (in_tx, in_rx) = mpsc::unbounded_channel::<ServerMessage>();
        let codec_r = codec.clone();
        let reader = tokio::spawn(async move {
            while let Some(Ok(msg)) = ws_read.next().await {
                let bytes = match &msg {
                    Message::Binary(b) => &b[..],
                    Message::Text(t) => t.as_bytes(),
                    Message::Close(_) => break,
                    _ => continue,
                };
                if let Ok(server_msg) = codec_r.decode::<ServerMessage>(bytes)
                    && in_tx.send(server_msg).is_err()
                {
                    break;
                }
            }
        });

        Ok(Self {
            outgoing: out_tx,
            incoming: Mutex::new(in_rx),
            codec,
            _writer_abort: writer.abort_handle(),
            _reader_abort: reader.abort_handle(),
        })
    }
}

#[async_trait]
impl<C: Codec> Transport for WsTransport<C> {
    async fn send(&self, msg: ClientMessage) -> Result<()> {
        self.outgoing
            .send(msg)
            .map_err(|_| Error::Storage("connection closed".into()))
    }

    async fn recv(&self) -> Result<ServerMessage> {
        let mut rx = self.incoming.lock().await;
        rx.recv()
            .await
            .ok_or_else(|| Error::Storage("connection closed".into()))
    }
}

/// Transforms an HTTP request before the WebSocket handshake (client-side).
pub trait WsRequestTransformer: Send + Sync {
    /// Transform the HTTP upgrade request (e.g., add auth headers).
    fn transform(&self, req: http::Request<()>) -> http::Request<()>;
}

/// Validates an HTTP request during the WebSocket handshake (server-side).
pub trait WsRequestValidator: Send + Sync {
    /// Validate the HTTP upgrade request. Return `Err(reason)` to reject.
    ///
    /// # Errors
    ///
    /// Returns an error string if the request is invalid.
    fn validate(&self, req: &http::Request<()>) -> std::result::Result<(), String>;
}

/// Bearer token authentication for WebSocket connections.
///
/// Implements both `WsRequestTransformer` (client) and `WsRequestValidator` (server).
pub struct BearerToken(pub String);

impl WsRequestTransformer for BearerToken {
    fn transform(&self, req: http::Request<()>) -> http::Request<()> {
        let (mut parts, body) = req.into_parts();
        parts.headers.insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_str(&format!("Bearer {}", self.0))
                .expect("valid header value"),
        );
        http::Request::from_parts(parts, body)
    }
}

impl WsRequestValidator for BearerToken {
    fn validate(&self, req: &http::Request<()>) -> std::result::Result<(), String> {
        let auth = req
            .headers()
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| "missing Authorization header".to_string())?;
        let expected = format!("Bearer {}", self.0);
        if auth == expected {
            Ok(())
        } else {
            Err("invalid bearer token".to_string())
        }
    }
}

/// Extract the host:port from a `ws://` or `wss://` URL.
fn url_host(url: &str) -> String {
    let without_scheme = url
        .strip_prefix("ws://")
        .or_else(|| url.strip_prefix("wss://"))
        .unwrap_or(url);
    without_scheme.split('/').next().unwrap_or(url).to_string()
}

/// Start a WebSocket server that dispatches to a [`RepositoryProvider`].
///
/// Returns a join handle for the server task and the bound address.
///
/// # Errors
///
/// Returns an error if binding the TCP listener fails.
pub async fn serve_ws<P: RepositoryProvider + 'static, C: Codec>(
    addr: &str,
    provider: P,
    validator: Option<Box<dyn WsRequestValidator>>,
    codec: C,
) -> Result<(JoinHandle<()>, SocketAddr)> {
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| Error::Storage(e.to_string()))?;
    let local_addr = listener
        .local_addr()
        .map_err(|e| Error::Storage(e.to_string()))?;

    let server = Arc::new(Server::new(provider));
    let validator: Option<Arc<dyn WsRequestValidator>> = validator.map(Arc::from);

    let handle = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };

            let server = Arc::clone(&server);
            let codec = codec.clone();
            let validator = validator.clone();

            tokio::spawn(async move {
                #[allow(clippy::result_large_err)]
                fn reject_connection(
                    req: &http::Request<()>,
                    resp: http::Response<()>,
                    validator: &dyn WsRequestValidator,
                ) -> std::result::Result<http::Response<()>, http::Response<Option<String>>> {
                    match validator.validate(req) {
                        Ok(()) => Ok(resp),
                        Err(reason) => Err(http::Response::builder()
                            .status(http::StatusCode::UNAUTHORIZED)
                            .body(Some(reason))
                            .expect("building reject response")),
                    }
                }

                let ws_stream = if let Some(ref v) = validator {
                    let v = Arc::clone(v);
                    #[allow(clippy::result_large_err)]
                    let cb = move |req: &http::Request<()>, resp: http::Response<()>| {
                        reject_connection(req, resp, v.as_ref())
                    };
                    match tokio_tungstenite::accept_hdr_async(stream, cb)
                    .await
                    {
                        Ok(ws) => ws,
                        Err(_) => return,
                    }
                } else {
                    let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
                        return;
                    };
                    ws
                };

                let (conn_id, mut notify_rx) = server.register_connection().await;
                let (mut write, mut read) = ws_stream.split();

                let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ServerMessage>();
                let codec_write = codec.clone();

                // Writer task: merge handler responses and notifications
                let write_handle = tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            Some(msg) = out_rx.recv() => {
                                if let Ok(bytes) = codec_write.encode(&msg)
                                    && write.send(Message::Binary(bytes.into())).await.is_err()
                                {
                                    break;
                                }
                            }
                            Some(notif) = notify_rx.recv() => {
                                if let Ok(bytes) = codec_write.encode(&notif)
                                    && write.send(Message::Binary(bytes.into())).await.is_err()
                                {
                                    break;
                                }
                            }
                            else => break,
                        }
                    }
                });

                // Reader loop: dispatch messages to the server handler
                while let Some(Ok(msg)) = read.next().await {
                    let bytes = match &msg {
                        Message::Binary(b) => &b[..],
                        Message::Text(t) => t.as_bytes(),
                        Message::Close(_) => break,
                        _ => continue,
                    };

                    let Ok(client_msg) = codec.decode::<ClientMessage>(bytes) else {
                        continue;
                    };

                    let responses = server.handle(conn_id, client_msg).await;
                    for resp in responses {
                        if out_tx.send(resp).is_err() {
                            break;
                        }
                    }
                }

                // Cleanup
                write_handle.abort();
                server.disconnect(conn_id).await;
            });
        }
    });

    Ok((handle, local_addr))
}

/// Validates bearer tokens against a mutable set (supports hot-reload).
///
/// Wraps `Arc<RwLock<HashSet<String>>>` so cloning shares the same token set.
/// Use `update_tokens` to swap the set during config hot-reload.
#[derive(Clone)]
pub struct MultiTokenValidator {
    /// Exposed for the server to clone into `serve_ws`.
    pub tokens: Arc<tokio::sync::RwLock<HashSet<String>>>,
}

impl MultiTokenValidator {
    /// Create a new validator from an initial set of tokens.
    #[must_use]
    pub fn new(tokens: HashSet<String>) -> Self {
        Self {
            tokens: Arc::new(tokio::sync::RwLock::new(tokens)),
        }
    }

    /// Replace the token set (called during hot-reload).
    pub async fn update_tokens(&self, tokens: HashSet<String>) {
        *self.tokens.write().await = tokens;
    }
}

impl WsRequestValidator for MultiTokenValidator {
    fn validate(&self, req: &http::Request<()>) -> std::result::Result<(), String> {
        let auth = req
            .headers()
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| "missing Authorization header".to_string())?;
        let token = auth
            .strip_prefix("Bearer ")
            .ok_or_else(|| "invalid Authorization format".to_string())?;
        // Use try_read to avoid async in a sync trait method.
        // If the lock is held by a writer (hot-reload), reject transiently.
        let guard = self
            .tokens
            .try_read()
            .map_err(|_| "server reloading, retry".to_string())?;
        if guard.contains(token) {
            Ok(())
        } else {
            Err("invalid token".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU64, Ordering};

    use async_trait::async_trait;
    use tokio::sync::Mutex;

    use super::super::codec::JsonCodec;
    use super::super::server::RepositoryProvider;
    use super::super::transport::Store;
    use super::*;
    use crate::store::memory::MemoryStore;

    /// Repository provider that creates a fresh `MemoryStore` per repo name.
    struct DynamicTestProvider {
        repos: Arc<Mutex<std::collections::HashMap<String, Arc<MemoryStore>>>>,
    }

    impl Default for DynamicTestProvider {
        fn default() -> Self {
            Self {
                repos: Arc::new(Mutex::new(std::collections::HashMap::new())),
            }
        }
    }

    #[async_trait]
    impl RepositoryProvider for DynamicTestProvider {
        async fn list(&self) -> crate::error::Result<Vec<String>> {
            Ok(self.repos.lock().await.keys().cloned().collect())
        }

        async fn get(&self, name: &str) -> crate::error::Result<Arc<dyn Store>> {
            let mut repos = self.repos.lock().await;
            Ok(repos
                .entry(name.to_string())
                .or_insert_with(|| Arc::new(MemoryStore::new()))
                .clone())
        }
    }

    // Shared multi-thread runtime for all WS IO (server + client connections).
    // Using a single runtime avoids spawning thousands of threads for proptests.
    static TEST_RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    static SERVER_ADDR: OnceLock<SocketAddr> = OnceLock::new();
    static REPO_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn test_runtime() -> &'static tokio::runtime::Runtime {
        TEST_RT.get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap()
        })
    }

    fn shared_server_addr() -> SocketAddr {
        *SERVER_ADDR.get_or_init(|| {
            let rt = test_runtime();
            let (addr_tx, addr_rx) = std::sync::mpsc::channel();
            rt.spawn(async move {
                let provider = DynamicTestProvider::default();
                let (_, addr) = serve_ws("127.0.0.1:0", provider, None, JsonCodec)
                    .await
                    .unwrap();
                let _ = addr_tx.send(addr);
            });
            addr_rx.recv().unwrap()
        })
    }

    fn create_remote_store() -> super::super::RemoteStore<WsTransport<JsonCodec>> {
        let addr = shared_server_addr();
        let repo = format!("test-{}", REPO_COUNTER.fetch_add(1, Ordering::Relaxed));

        // Create the WS connection on the shared runtime (where the IO driver lives).
        // The returned WsTransport uses channels internally, so it can be used from
        // any thread/runtime.
        let (store_tx, store_rx) = std::sync::mpsc::channel();
        test_runtime().spawn(async move {
            let transport = WsTransport::connect(&format!("ws://{addr}"), JsonCodec, None)
                .await
                .unwrap();
            // RemoteStore::new spawns a reader task on the current runtime (shared).
            let store = super::super::RemoteStore::new(transport, &repo);
            let _ = store_tx.send(store);
        });
        store_rx.recv().unwrap()
    }

    #[tokio::test]
    async fn ref_updated_broadcast_to_other_client() {
        use crate::store::RefStore;
        let addr = shared_server_addr();
        let repo_name = format!(
            "notify-{}",
            REPO_COUNTER.fetch_add(1, Ordering::Relaxed)
        );

        // Client A and Client B connect to the same repo.
        let (store_a_tx, store_a_rx) = std::sync::mpsc::channel();
        let (store_b_tx, store_b_rx) = std::sync::mpsc::channel();
        let repo_a = repo_name.clone();
        let repo_b = repo_name.clone();

        test_runtime().spawn(async move {
            let t = WsTransport::connect(&format!("ws://{addr}"), JsonCodec, None)
                .await
                .unwrap();
            let s = super::super::RemoteStore::new(t, &repo_a);
            let _ = store_a_tx.send(s);
        });
        test_runtime().spawn(async move {
            let t = WsTransport::connect(&format!("ws://{addr}"), JsonCodec, None)
                .await
                .unwrap();
            let s = super::super::RemoteStore::new(t, &repo_b);
            let _ = store_b_tx.send(s);
        });

        let client_a = store_a_rx.recv().unwrap();
        let client_b = store_b_rx.recv().unwrap();

        // Client A sets a ref.
        let hash = clayers_xml::ContentHash::from_canonical(b"test-ref-updated");
        client_a.set_ref("refs/heads/main", hash).await.unwrap();

        // Client B should receive a RefUpdated notification.
        let notification = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            client_b.recv_notification(),
        )
        .await
        .expect("timed out waiting for RefUpdated")
        .expect("connection closed");

        match notification {
            super::super::ServerMessage::RefUpdated {
                name, new, ..
            } => {
                assert_eq!(name, "refs/heads/main");
                assert_eq!(new, Some(hash));
            }
            other => panic!("expected RefUpdated, got {other:?}"),
        }
    }

    mod remote_tests {
        use super::create_remote_store;
        crate::store::tests::store_tests!(create_remote_store());
    }

    mod remote_prop_tests {
        use super::create_remote_store;
        crate::store::prop_tests::prop_store_tests!(create_remote_store());
    }

    mod remote_concurrency {
        use super::create_remote_store;
        crate::store::concurrency_tests::concurrency_tests!(create_remote_store());
    }

    mod remote_prop_concurrency {
        use super::create_remote_store;
        crate::store::concurrency_tests::prop_concurrency_tests!(create_remote_store());
    }
}
