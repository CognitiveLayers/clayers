//! Remote transport store: client/server communication over pluggable transports.
//!
//! The `remote` feature provides protocol types, transport abstraction, and
//! client/server implementations. The `websocket` feature adds a WebSocket
//! transport with configurable codec and auth.

mod client;
mod codec;
mod server;
mod transport;
#[cfg(feature = "websocket")]
mod websocket;

pub use client::{RemoteStore, RemoteTransaction, list_repositories};
pub use codec::Codec;
#[cfg(feature = "websocket")]
pub use codec::JsonCodec;
pub use server::{RepositoryProvider, Server, StaticRepositories};
pub use transport::{Store, Transport};
#[cfg(feature = "websocket")]
pub use websocket::{
    BearerToken, MultiTokenValidator, WsRequestTransformer, WsRequestValidator, WsTransport,
    serve_ws,
};

use clayers_xml::ContentHash;
use serde::{Deserialize, Serialize};

/// Correlation ID for matching requests to responses.
pub type MessageId = u64;

/// Server-assigned transaction identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TxId(pub u64);

/// Messages sent from client to server.
#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    // Repository discovery
    ListRepositories {
        id: MessageId,
    },

    // ObjectStore
    Get {
        id: MessageId,
        repo: String,
        hash: ContentHash,
    },
    Contains {
        id: MessageId,
        repo: String,
        hash: ContentHash,
    },
    GetByInclusiveHash {
        id: MessageId,
        repo: String,
        inclusive_hash: ContentHash,
    },
    Subtree {
        id: MessageId,
        repo: String,
        root: ContentHash,
    },

    // RefStore
    GetRef {
        id: MessageId,
        repo: String,
        name: String,
    },
    SetRef {
        id: MessageId,
        repo: String,
        name: String,
        hash: ContentHash,
    },
    DeleteRef {
        id: MessageId,
        repo: String,
        name: String,
    },
    ListRefs {
        id: MessageId,
        repo: String,
        prefix: String,
    },
    CasRef {
        id: MessageId,
        repo: String,
        name: String,
        expected: Option<ContentHash>,
        new: ContentHash,
    },

    // Transaction lifecycle
    BeginTransaction {
        id: MessageId,
        repo: String,
    },
    TxPut {
        id: MessageId,
        tx_id: TxId,
        hash: ContentHash,
        object: crate::object::Object,
    },
    TxCommit {
        id: MessageId,
        tx_id: TxId,
    },
    TxRollback {
        id: MessageId,
        tx_id: TxId,
    },
}

/// Messages sent from server to client.
#[derive(Debug, Serialize, Deserialize)]
pub enum ServerMessage {
    // Replies (carry client's MessageId)
    RepositoryList {
        id: MessageId,
        repos: Vec<String>,
    },
    Object {
        id: MessageId,
        object: Option<crate::object::Object>,
    },
    Contains {
        id: MessageId,
        exists: bool,
    },
    ObjectWithHash {
        id: MessageId,
        result: Option<(ContentHash, crate::object::Object)>,
    },
    SubtreeItem {
        id: MessageId,
        hash: ContentHash,
        object: crate::object::Object,
    },
    SubtreeEnd {
        id: MessageId,
    },
    Ref {
        id: MessageId,
        hash: Option<ContentHash>,
    },
    RefSet {
        id: MessageId,
    },
    RefDeleted {
        id: MessageId,
    },
    RefList {
        id: MessageId,
        refs: Vec<(String, ContentHash)>,
    },
    CasResult {
        id: MessageId,
        swapped: bool,
    },
    Ok {
        id: MessageId,
    },
    TransactionCreated {
        id: MessageId,
        tx_id: TxId,
    },
    Error {
        id: MessageId,
        message: String,
    },

    // Server-initiated notifications (no correlation ID)
    RefUpdated {
        repo: String,
        name: String,
        old: Option<ContentHash>,
        new: Option<ContentHash>,
    },
    TransactionTerminated {
        tx_id: TxId,
        reason: String,
    },
}

impl ServerMessage {
    /// Extract the message ID, if this is a reply (not a notification).
    #[must_use]
    pub fn id(&self) -> Option<MessageId> {
        match self {
            Self::RepositoryList { id, .. }
            | Self::Object { id, .. }
            | Self::Contains { id, .. }
            | Self::ObjectWithHash { id, .. }
            | Self::SubtreeItem { id, .. }
            | Self::SubtreeEnd { id, .. }
            | Self::Ref { id, .. }
            | Self::RefSet { id, .. }
            | Self::RefDeleted { id, .. }
            | Self::RefList { id, .. }
            | Self::CasResult { id, .. }
            | Self::Ok { id, .. }
            | Self::TransactionCreated { id, .. }
            | Self::Error { id, .. } => Some(*id),
            Self::RefUpdated { .. } | Self::TransactionTerminated { .. } => None,
        }
    }

    /// Returns true if this is a subtree stream message (item or end).
    #[must_use]
    pub fn is_subtree_stream(&self) -> bool {
        matches!(self, Self::SubtreeItem { .. } | Self::SubtreeEnd { .. })
    }
}
