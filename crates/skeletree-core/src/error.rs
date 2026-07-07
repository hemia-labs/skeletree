use thiserror::Error;

/// Failures parsing a domain enum back from its stored string form.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    #[error("unknown language: {0}")]
    UnknownLang(String),
    #[error("unknown symbol kind: {0}")]
    UnknownSymbolKind(String),
    #[error("unknown edge kind: {0}")]
    UnknownEdgeKind(String),
}
