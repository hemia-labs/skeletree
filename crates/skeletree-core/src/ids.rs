//! Newtype ids so a file id and a symbol id can never be swapped by accident.
//! Both wrap SQLite's rowid (`i64`).

/// Row id of a file in the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(pub i64);

/// Row id of a symbol in the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymbolId(pub i64);
