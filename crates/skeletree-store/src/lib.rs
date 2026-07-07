//! Persistence layer over SQLite. Owns the schema, inserts, and the recursive
//! graph queries the MCP tools rely on. Depends only on `skeletree-core`, so it
//! never sees parser types — the engine maps `ParsedSymbol` onto [`NewSymbol`].

use std::path::Path;

use rusqlite::{params, Connection};
use skeletree_core::{Edge, FileId, Lang, ParseError, Span, Symbol, SymbolId, SymbolKind};
use thiserror::Error;

mod schema;

pub use skeletree_core as core;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

type Result<T> = std::result::Result<T, StoreError>;

/// A symbol to insert, before it has an id. The engine builds these from the
/// parser's `ParsedSymbol`.
pub struct NewSymbol<'a> {
    pub name: &'a str,
    pub kind: SymbolKind,
    pub span: Span,
    pub signature: Option<&'a str>,
}

/// A file and its symbols to insert together. The engine builds these from
/// parser output; [`Store::index_files`] persists a batch in one transaction.
pub struct NewFile<'a> {
    pub path: &'a Path,
    pub lang: Lang,
    pub hash: &'a str,
    pub symbols: Vec<NewSymbol<'a>>,
}

/// blake3 of file contents in hex — the exact form stored in `files.hash`.
/// Lives here so the engine and the schema agree on one format.
pub fn hash_contents(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// The index database.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (creating if needed) an on-disk index and run migrations.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        // WAL lets the watcher reindex while the MCP server reads.
        conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")?;
        schema::migrate(&conn)?;
        Ok(Self { conn })
    }

    /// In-memory index, for tests.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        schema::migrate(&conn)?;
        Ok(Self { conn })
    }

    pub fn insert_file(&self, path: &Path, lang: Lang, hash: &str) -> Result<FileId> {
        insert_file_row(&self.conn, path, lang, hash)
    }

    pub fn insert_symbol(&self, file_id: FileId, sym: &NewSymbol) -> Result<SymbolId> {
        insert_symbol_row(&self.conn, file_id, sym)
    }

    pub fn insert_edge(&self, edge: Edge) -> Result<()> {
        insert_edge_row(&self.conn, edge)
    }

    /// Persist a batch of files and their symbols in a single transaction —
    /// the fast path for a full index (thousands of files).
    ///
    /// Replaces the entire index: `files.path` is UNIQUE, so a full reindex
    /// clears prior rows first (cascading to symbols and edges) rather than
    /// colliding. Atomic — a failed reindex leaves the previous contents.
    // ponytail: full-replace, not incremental. Per-file diffing lands with the
    // watcher step; until then re-running is a clean rebuild.
    pub fn index_files<'a>(&mut self, files: impl IntoIterator<Item = NewFile<'a>>) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM files", [])?;
        for file in files {
            let file_id = insert_file_row(&tx, file.path, file.lang, file.hash)?;
            for sym in &file.symbols {
                insert_symbol_row(&tx, file_id, sym)?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Persist a batch of edges in one transaction. Duplicates are ignored.
    pub fn insert_edges(&mut self, edges: &[Edge]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for edge in edges {
            insert_edge_row(&tx, *edge)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Every symbol in the index. Used to build name/span lookups for edge
    /// resolution.
    pub fn list_symbols(&self) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(SYMBOL_SELECT)?;
        let rows = stmt.query_map([], RawSymbol::from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?.into_symbol()?);
        }
        Ok(out)
    }

    /// Just the ids of every symbol — the node set for ranking, without paying
    /// to build full `Symbol` rows (join + kind parse + string allocs).
    pub fn list_symbol_ids(&self) -> Result<Vec<SymbolId>> {
        let mut stmt = self.conn.prepare("SELECT id FROM symbols")?;
        let rows = stmt.query_map([], |row| Ok(SymbolId(row.get(0)?)))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Every directed edge as `(src, dst)`, for building the rank graph.
    pub fn list_edges(&self) -> Result<Vec<(SymbolId, SymbolId)>> {
        let mut stmt = self.conn.prepare("SELECT src, dst FROM edges")?;
        let rows = stmt.query_map([], |row| Ok((SymbolId(row.get(0)?), SymbolId(row.get(1)?))))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Overwrite the `rank` of each listed symbol, in one transaction.
    pub fn update_ranks(&mut self, ranks: &[(SymbolId, f64)]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare("UPDATE symbols SET rank = ?2 WHERE id = ?1")?;
            for (id, rank) in ranks {
                stmt.execute(params![id.0, rank])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The `limit` highest-ranked symbols — the core of `overview`/`stats`.
    pub fn top_symbols(&self, limit: usize) -> Result<Vec<Symbol>> {
        let mut stmt = self
            .conn
            .prepare(&format!("{SYMBOL_SELECT} ORDER BY s.rank DESC LIMIT ?1"))?;
        let rows = stmt.query_map(params![limit as i64], RawSymbol::from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?.into_symbol()?);
        }
        Ok(out)
    }

    /// Symbols whose name matches a SQL `LIKE` pattern, optionally filtered by
    /// kind, best-ranked first. Powers the `find` tool.
    pub fn search(
        &self,
        name_like: &str,
        kind: Option<SymbolKind>,
        limit: usize,
    ) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(&format!(
            "{SYMBOL_SELECT} WHERE s.name LIKE ?1 ESCAPE '\\' AND (?2 IS NULL OR s.kind = ?2)
             ORDER BY s.rank DESC LIMIT ?3"
        ))?;
        let rows = stmt.query_map(
            params![name_like, kind.map(|k| k.as_str()), limit as i64],
            RawSymbol::from_row,
        )?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?.into_symbol()?);
        }
        Ok(out)
    }

    /// Every `(id, path)` in the index, so the engine can map a file back to its
    /// assigned id after a batch insert.
    pub fn list_files(&self) -> Result<Vec<(FileId, String)>> {
        let mut stmt = self.conn.prepare("SELECT id, path FROM files")?;
        let rows = stmt.query_map([], |row| {
            Ok((FileId(row.get(0)?), row.get::<_, String>(1)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Symbols within `depth` hops of `root`, traversing edges as undirected
    /// (who calls/imports/uses it, and what it calls/imports/uses).
    // ponytail: depth-bounded BFS via a recursive CTE — right for small depths
    // (1–3). If deep traversals get hot, load the graph into petgraph instead.
    pub fn neighbors(&self, root: SymbolId, depth: u32) -> Result<Vec<Symbol>> {
        let sql = format!(
            "{NEIGHBORS_CTE} {SYMBOL_SELECT}
             WHERE s.id IN (SELECT id FROM reachable) AND s.id != ?1"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![root.0, depth], RawSymbol::from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?.into_symbol()?);
        }
        Ok(out)
    }
}

// Row inserts as free functions over `&Connection` so both the single-insert
// methods and the transactional batch (where `&Transaction` derefs to
// `&Connection`) share one implementation.

fn insert_file_row(conn: &Connection, path: &Path, lang: Lang, hash: &str) -> Result<FileId> {
    // ponytail: to_string_lossy — non-UTF8 paths get lossily encoded; store as
    // BLOB if a repo ever has them.
    conn.execute(
        "INSERT INTO files (path, hash, lang) VALUES (?1, ?2, ?3)",
        params![path.to_string_lossy(), hash, lang.as_str()],
    )?;
    Ok(FileId(conn.last_insert_rowid()))
}

fn insert_symbol_row(conn: &Connection, file_id: FileId, sym: &NewSymbol) -> Result<SymbolId> {
    conn.execute(
        "INSERT INTO symbols
         (file_id, name, kind, start_byte, end_byte, start_line, end_line, signature)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            file_id.0,
            sym.name,
            sym.kind.as_str(),
            sym.span.start_byte,
            sym.span.end_byte,
            sym.span.start_line,
            sym.span.end_line,
            sym.signature,
        ],
    )?;
    Ok(SymbolId(conn.last_insert_rowid()))
}

fn insert_edge_row(conn: &Connection, edge: Edge) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO edges (src, dst, kind) VALUES (?1, ?2, ?3)",
        params![edge.src.0, edge.dst.0, edge.kind.as_str()],
    )?;
    Ok(())
}

/// The shared `Symbol` read: columns in `RawSymbol::from_row` order, joined to
/// the file for its path. Append `WHERE`/`ORDER BY`/`LIMIT` as needed.
const SYMBOL_SELECT: &str = "
SELECT s.id, s.file_id, s.name, s.kind, s.start_byte, s.end_byte,
       s.start_line, s.end_line, s.signature, s.rank, f.path
FROM symbols s JOIN files f ON f.id = s.file_id";

/// Recursive CTE naming `reachable(id, dist)`: symbols within `?2` undirected
/// hops of `?1`. Prefix a `SYMBOL_SELECT` that filters on it.
const NEIGHBORS_CTE: &str = "
WITH RECURSIVE
adj(a, b) AS (
    SELECT src, dst FROM edges
    UNION ALL
    SELECT dst, src FROM edges
),
reachable(id, dist) AS (
    SELECT ?1, 0
    UNION
    SELECT adj.b, reachable.dist + 1
    FROM reachable JOIN adj ON adj.a = reachable.id
    WHERE reachable.dist < ?2
)";

/// A symbols row before `kind` is parsed. Keeps the SQL layer free of the enum
/// parse (which can fail) so errors surface as `StoreError::Parse`.
struct RawSymbol {
    id: i64,
    file_id: i64,
    name: String,
    kind: String,
    start_byte: i64,
    end_byte: i64,
    start_line: i64,
    end_line: i64,
    signature: Option<String>,
    rank: f64,
    file_path: String,
}

impl RawSymbol {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            file_id: row.get(1)?,
            name: row.get(2)?,
            kind: row.get(3)?,
            start_byte: row.get(4)?,
            end_byte: row.get(5)?,
            start_line: row.get(6)?,
            end_line: row.get(7)?,
            signature: row.get(8)?,
            rank: row.get(9)?,
            file_path: row.get(10)?,
        })
    }

    fn into_symbol(self) -> std::result::Result<Symbol, ParseError> {
        Ok(Symbol {
            id: SymbolId(self.id),
            file_id: FileId(self.file_id),
            name: self.name,
            kind: self.kind.parse()?,
            span: Span {
                start_byte: self.start_byte as u32,
                end_byte: self.end_byte as u32,
                start_line: self.start_line as u32,
                end_line: self.end_line as u32,
            },
            signature: self.signature,
            rank: self.rank,
            file_path: self.file_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skeletree_core::EdgeKind;

    fn span() -> Span {
        Span {
            start_byte: 0,
            end_byte: 1,
            start_line: 1,
            end_line: 1,
        }
    }

    fn sym(store: &Store, file: FileId, name: &str) -> SymbolId {
        store
            .insert_symbol(
                file,
                &NewSymbol {
                    name,
                    kind: SymbolKind::Function,
                    span: span(),
                    signature: None,
                },
            )
            .unwrap()
    }

    fn names(mut syms: Vec<Symbol>) -> Vec<String> {
        syms.sort_by(|a, b| a.name.cmp(&b.name));
        syms.into_iter().map(|s| s.name).collect()
    }

    #[test]
    fn insert_and_traverse() {
        let store = Store::open_in_memory().unwrap();
        let f = store
            .insert_file(Path::new("a.py"), Lang::Python, "h")
            .unwrap();
        let a = sym(&store, f, "a");
        let b = sym(&store, f, "b");
        let c = sym(&store, f, "c");
        // a -> b -> c
        store
            .insert_edge(Edge {
                src: a,
                dst: b,
                kind: EdgeKind::Calls,
            })
            .unwrap();
        store
            .insert_edge(Edge {
                src: b,
                dst: c,
                kind: EdgeKind::Calls,
            })
            .unwrap();

        // Depth 1 from a reaches only b.
        assert_eq!(names(store.neighbors(a, 1).unwrap()), ["b"]);
        // Depth 2 from a reaches b and c.
        assert_eq!(names(store.neighbors(a, 2).unwrap()), ["b", "c"]);
        // Undirected: from b, depth 1 reaches both a and c.
        assert_eq!(names(store.neighbors(b, 1).unwrap()), ["a", "c"]);
    }

    #[test]
    fn migrations_are_idempotent() {
        // open_in_memory migrates once; re-running migrate must be a no-op.
        let store = Store::open_in_memory().unwrap();
        schema::migrate(&store.conn).unwrap();
    }
}
