use std::path::PathBuf;

use crate::ids::{FileId, SymbolId};
use crate::kinds::{EdgeKind, SymbolKind};
use crate::lang::Lang;

/// Byte and line range of a symbol within its file. Byte offsets drive snippet
/// extraction; line numbers drive display. `u32` caps at 4 GiB per file.
// ponytail: u32 not usize — no source file hits 4 GiB; widen if one ever does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start_byte: u32,
    pub end_byte: u32,
    pub start_line: u32,
    pub end_line: u32,
}

/// A source file in the index. Mirrors the `files` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    pub id: FileId,
    pub path: PathBuf,
    pub lang: Lang,
    /// blake3 of the file contents — the basis for incremental reindexing.
    pub hash: String,
}

/// A named code entity — the read model, `symbols` joined with its file's path.
/// (The write model is `skeletree_store::NewSymbol`.)
// No `Eq`: `rank` is an f64. `PartialEq` is enough for tests and lookups.
#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    pub id: SymbolId,
    pub file_id: FileId,
    pub name: String,
    pub kind: SymbolKind,
    pub span: Span,
    /// Rendered signature, when the kind has one (constants may not).
    pub signature: Option<String>,
    /// PageRank centrality; 0 until the ranking pass runs.
    pub rank: f64,
    /// Path of the file this symbol lives in (from the join).
    pub file_path: String,
}

/// A directed relationship between two symbols. Mirrors the `edges` table.
// ponytail: file-level relations (e.g. imports) route through each file's
// module symbol rather than a separate file-edge type; add one if a query needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Edge {
    pub src: SymbolId,
    pub dst: SymbolId,
    pub kind: EdgeKind,
}
