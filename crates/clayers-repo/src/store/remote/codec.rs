//! Serialization codec for the WebSocket transport.

use serde::{Serialize, de::DeserializeOwned};

use crate::error::Result;
#[cfg(feature = "websocket")]
use crate::error::Error;

/// Codec for encoding/decoding messages to/from bytes.
///
/// Internal to the WebSocket transport. The abstract [`Transport`](super::Transport)
/// trait operates on typed messages, not bytes.
pub trait Codec: Send + Sync + Clone + 'static {
    /// Encode a value to bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    fn encode<T: Serialize>(&self, value: &T) -> Result<Vec<u8>>;

    /// Decode bytes to a value.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    fn decode<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T>;
}

/// JSON codec using `serde_json`.
#[cfg(feature = "websocket")]
#[derive(Clone, Debug)]
pub struct JsonCodec;

#[cfg(feature = "websocket")]
impl Codec for JsonCodec {
    fn encode<T: Serialize>(&self, value: &T) -> Result<Vec<u8>> {
        serde_json::to_vec(value).map_err(|e| Error::Storage(e.to_string()))
    }

    fn decode<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T> {
        serde_json::from_slice(bytes).map_err(|e| Error::Storage(e.to_string()))
    }
}
