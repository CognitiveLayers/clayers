pub mod artifact;
pub mod assembly;
pub mod chunker;
pub mod connectivity;
pub mod coverage;
pub mod discovery;
pub mod drift;
pub mod fix;
pub mod hash;
pub mod namespace;
pub mod neighbors;
pub mod query;
pub mod rnc;
pub mod schema;
pub mod validate;
pub mod xsd_validation;

/// Errors from spec processing operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("XML error: {0}")]
    Xml(#[from] xot::Error),

    #[error("discovery error: {0}")]
    Discovery(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("query error: {0}")]
    Query(String),

    #[error("XML processing error: {0}")]
    XmlProcessing(#[from] clayers_xml::Error),
}
