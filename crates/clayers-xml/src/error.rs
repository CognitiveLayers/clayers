/// Errors from XML processing operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("XML parse error: {0}")]
    XmlParse(String),

    #[error("XML serialization error: {0}")]
    XmlSerialize(String),

    #[error("C14N error: {0}")]
    Canonicalization(String),

    #[error("invalid hash format: {0}")]
    InvalidHashFormat(String),

    #[error("XPath query error: {0}")]
    Query(String),

    #[error("XSLT error: {0}")]
    Xslt(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<xot::Error> for Error {
    fn from(e: xot::Error) -> Self {
        Self::XmlParse(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_xml_parse() {
        let e = Error::XmlParse("unexpected EOF".into());
        assert!(e.to_string().contains("XML parse error"));
        assert!(e.to_string().contains("unexpected EOF"));
    }

    #[test]
    fn error_display_c14n() {
        let e = Error::Canonicalization("bad input".into());
        assert!(e.to_string().contains("C14N error"));
    }

    #[test]
    fn error_display_hash_format() {
        let e = Error::InvalidHashFormat("not hex".into());
        assert!(e.to_string().contains("invalid hash format"));
    }
}
