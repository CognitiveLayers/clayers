//! Transport abstraction and Store supertrait.

use async_trait::async_trait;

use super::{ClientMessage, ServerMessage};
use crate::error::Result;
use crate::query::QueryStore;
use crate::store::{ObjectStore, RefStore};

/// Bidirectional message transport.
///
/// Not request-response: the client correlates responses by [`MessageId`](super::MessageId).
/// Server-initiated messages arrive through `recv()` with no matching request.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Send a client message to the server.
    async fn send(&self, msg: ClientMessage) -> Result<()>;
    /// Receive a server message.
    async fn recv(&self) -> Result<ServerMessage>;
    /// Close the transport and release any background IO resources.
    fn close(&self);
}

/// Combined store trait for use as a trait object.
///
/// Rust doesn't support `dyn ObjectStore + RefStore + QueryStore`, so this
/// supertrait combines all three. All existing stores (`MemoryStore`, `SqliteStore`)
/// auto-implement it via the blanket impl.
pub trait Store: ObjectStore + RefStore + QueryStore {}
impl<T: ObjectStore + RefStore + QueryStore> Store for T {}
