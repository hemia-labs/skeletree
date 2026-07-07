//! SQLite schema as an ordered list of migrations, applied by `user_version`.
//! Add a new `&str` to `MIGRATIONS` to evolve the schema; never edit an old one.

use rusqlite::Connection;

/// One entry per schema version. Index 0 = version 1, applied in order.
const MIGRATIONS: &[&str] = &[
    // v1 — files, symbols, edges + lookup indexes.
    "
    CREATE TABLE files (
        id   INTEGER PRIMARY KEY,
        path TEXT NOT NULL UNIQUE,
        hash TEXT NOT NULL,
        lang TEXT NOT NULL
    );

    CREATE TABLE symbols (
        id         INTEGER PRIMARY KEY,
        file_id    INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
        name       TEXT NOT NULL,
        kind       TEXT NOT NULL,
        start_byte INTEGER NOT NULL,
        end_byte   INTEGER NOT NULL,
        start_line INTEGER NOT NULL,
        end_line   INTEGER NOT NULL,
        signature  TEXT
    );

    CREATE TABLE edges (
        src  INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
        dst  INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
        kind TEXT NOT NULL,
        PRIMARY KEY (src, dst, kind)
    ) WITHOUT ROWID;

    CREATE INDEX idx_symbols_name ON symbols(name);
    CREATE INDEX idx_symbols_file ON symbols(file_id);
    CREATE INDEX idx_edges_dst    ON edges(dst);
    ",
    // v2 — PageRank score per symbol (0 until ranked).
    "ALTER TABLE symbols ADD COLUMN rank REAL NOT NULL DEFAULT 0;",
];

/// Bring `conn` up to the latest schema version. Idempotent.
pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        conn.execute_batch(sql)?;
        conn.pragma_update(None, "user_version", (i + 1) as i64)?;
    }
    Ok(())
}
