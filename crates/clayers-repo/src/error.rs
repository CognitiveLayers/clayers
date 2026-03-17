//! Error types for repository operations.

use clayers_xml::ContentHash;

/// Errors from repository operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An XML processing error from clayers-xml.
    #[error("XML error: {0}")]
    Xml(#[from] clayers_xml::Error),

    /// An XML parsing error from xot.
    #[error("XML parse error: {0}")]
    XmlParse(String),

    /// A storage backend error.
    #[error("storage error: {0}")]
    Storage(String),

    /// A requested object was not found in the store.
    #[error("object not found: {0}")]
    NotFound(ContentHash),

    /// An object could not be interpreted or is structurally invalid.
    #[error("invalid object: {0}")]
    InvalidObject(String),

    /// A ref operation error (branch, tag, HEAD).
    #[error("ref error: {0}")]
    Ref(String),

    /// A document with no root element was encountered.
    #[error("empty document: no root element")]
    EmptyDocument,
}

impl From<xot::Error> for Error {
    fn from(e: xot::Error) -> Self {
        Self::XmlParse(e.to_string())
    }
}

/// Convenience alias for `Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;
