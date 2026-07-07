//! The tool logic, kept free of any MCP/transport types so it stays unit-
//! testable. The rmcp layer is a thin adapter over these functions.
//!
//! Every function renders compact text and respects `token_budget`: it stops
//! emitting rows once the estimate would exceed the budget, so an agent never
//! gets more than it asked for.

use skeletree_core::{Symbol, SymbolKind};
use skeletree_store::{Store, StoreError};

/// How many candidates to pull before the budget trims them.
const MAX_ROWS: usize = 500;

/// Ranked map of the whole repo — the most central symbols first.
pub fn overview(store: &Store, token_budget: usize) -> Result<String, StoreError> {
    Ok(render(&store.top_symbols(MAX_ROWS)?, token_budget))
}

/// Symbols whose name contains `query`, optionally filtered by kind.
pub fn find(
    store: &Store,
    query: &str,
    kind: Option<SymbolKind>,
    token_budget: usize,
) -> Result<String, StoreError> {
    let pattern = format!("%{}%", escape_like(query));
    Ok(render(
        &store.search(&pattern, kind, MAX_ROWS)?,
        token_budget,
    ))
}

/// Symbols within `depth` hops of the best-ranked symbol named `symbol`.
pub fn neighbors(
    store: &Store,
    symbol: &str,
    depth: u32,
    token_budget: usize,
) -> Result<String, StoreError> {
    // Resolve to the highest-ranked exact-name match. Escape so `_`/`%` in a
    // name are literal, not LIKE wildcards (LIKE is ASCII case-insensitive).
    let Some(root) = store
        .search(&escape_like(symbol), None, 1)?
        .into_iter()
        .next()
    else {
        return Ok(format!("no symbol named `{symbol}`\n"));
    };
    let mut header = format!("{} {} ({})\n", root.kind, root.name, location(&root));
    header.push_str(&render(&store.neighbors(root.id, depth)?, token_budget));
    Ok(header)
}

/// One line per symbol: `kind name  location  signature`, ranked as given,
/// truncated to the token budget.
fn render(symbols: &[Symbol], token_budget: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    for symbol in symbols {
        let line = format!(
            "{:<9} {}  {}{}\n",
            symbol.kind,
            symbol.name,
            location(symbol),
            symbol
                .signature
                .as_deref()
                .map(|s| format!("  {s}"))
                .unwrap_or_default(),
        );
        let cost = estimate_tokens(&line);
        if used + cost > token_budget && !out.is_empty() {
            out.push_str("… (truncated to fit token budget)\n");
            break;
        }
        used += cost;
        out.push_str(&line);
    }
    if out.is_empty() {
        out.push_str("(nothing found)\n");
    }
    out
}

fn location(symbol: &Symbol) -> String {
    format!("{}:{}", symbol.file_path, symbol.span.start_line)
}

/// Rough token estimate. ponytail: ~4 chars/token is close enough for budgeting;
/// swap in `tiktoken-rs` if precise accounting becomes the selling point.
fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

/// Escape LIKE wildcards so a user query of `get_x` doesn't treat `_` as "any".
fn escape_like(query: &str) -> String {
    query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an indexed in-memory store from a temp Python repo.
    fn indexed(src: &str) -> Store {
        // ponytail: nanos alone raced under parallel test threads (clock
        // resolution < thread-spawn jitter) and two tests collided on one
        // dir. A per-process counter guarantees uniqueness regardless.
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let uniq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("skeletree-mcp-{}-{uniq}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("m.py"), src).unwrap();
        let mut store = Store::open_in_memory().unwrap();
        skeletree_engine::index(&dir, &mut store).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        store
    }

    const SRC: &str = "def helper():\n    pass\n\ndef caller():\n    helper()\n";

    #[test]
    fn find_matches_by_name() {
        let store = indexed(SRC);
        let out = find(&store, "help", None, 1500).unwrap();
        assert!(out.contains("helper"));
        assert!(!out.contains("caller"));
    }

    #[test]
    fn find_filters_by_kind() {
        let store = indexed("class Thing:\n    pass\n\ndef thing_fn():\n    pass\n");
        let out = find(&store, "thing", Some(SymbolKind::Class), 1500).unwrap();
        assert!(out.contains("Thing"));
        assert!(!out.contains("thing_fn"));
    }

    #[test]
    fn neighbors_follows_calls() {
        let store = indexed(SRC);
        let out = neighbors(&store, "caller", 1, 1500).unwrap();
        assert!(out.contains("helper"));
    }

    #[test]
    fn overview_is_rank_ordered_and_located() {
        let store = indexed(SRC);
        let out = overview(&store, 1500).unwrap();
        // helper is called, so it ranks above caller (appears earlier).
        let hp = out.find("helper").unwrap();
        let cp = out.find("caller").unwrap();
        assert!(hp < cp, "helper should rank above caller:\n{out}");
        assert!(out.contains("m.py:"));
    }

    #[test]
    fn budget_truncates() {
        let store = indexed(SRC);
        // A tiny budget yields at most one row plus the truncation marker.
        let out = overview(&store, 3).unwrap();
        assert!(out.contains("truncated"));
        assert!(out.lines().count() <= 2);
    }

    #[test]
    fn missing_symbol_is_graceful() {
        let store = indexed(SRC);
        let out = neighbors(&store, "does_not_exist", 1, 1500).unwrap();
        assert!(out.contains("no symbol named"));
    }
}
