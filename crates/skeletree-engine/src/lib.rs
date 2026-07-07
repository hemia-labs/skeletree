//! The pipeline that ties parsing and storage together: walk → parse → persist.
//! Parsing is CPU-bound and runs in parallel (rayon); the SQLite write is a
//! single serial transaction, which is both correct (one writer) and fast.
//!
//! Ranking, incremental reindex and the watcher layer on top of this later.

use std::collections::hash_map::{Entry, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use rayon::prelude::*;
use skeletree_core::{Edge, EdgeKind, FileId, Lang, Span, SymbolId, SymbolKind};
use skeletree_lang::{Extractor, LangError, ParsedSymbol, RawRef, Registry};
use skeletree_store::{hash_contents, NewFile, NewSymbol, Store, StoreError};
use thiserror::Error;

mod rank;

pub use skeletree_core as core;
pub use skeletree_lang as lang;
pub use skeletree_store as store;

/// PageRank damping factor (standard 0.85) and iteration count.
const RANK_DAMPING: f64 = 0.85;
const RANK_ITERS: usize = 20;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("reading {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error(transparent)]
    Lang(#[from] LangError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("no language registered for {0}")]
    NoLanguage(PathBuf),
}

/// Result of an index run.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct IndexStats {
    pub files: usize,
    pub symbols: usize,
    /// Files skipped because they could not be read or parsed.
    pub skipped: usize,
}

/// Default index location for a repo root: `<root>/.skeletree/index.db`.
pub fn default_db_path(root: &Path) -> PathBuf {
    root.join(".skeletree").join("index.db")
}

/// A parsed file held in memory between the parallel parse and the serial write.
struct ParsedFile {
    path: PathBuf,
    lang: Lang,
    hash: String,
    symbols: Vec<ParsedSymbol>,
    refs: Vec<RawRef>,
}

/// Index `root` into `store`. Walks the repo (respecting `.gitignore`), parses
/// supported files in parallel, then writes everything in one transaction.
pub fn index(root: &Path, store: &mut Store) -> Result<IndexStats, EngineError> {
    let registry = Registry::with_defaults();
    let paths = walk(root, &registry);

    // Parse in parallel. Each rayon worker keeps its own per-language extractors
    // (compiling a query per file would be wasteful), so `map_init` seeds one
    // cache per thread and reuses it across files.
    let results: Vec<Result<ParsedFile, EngineError>> = paths
        .par_iter()
        .map_init(HashMap::<Lang, Extractor>::new, |extractors, path| {
            parse_one(&registry, extractors, path)
        })
        .collect();

    // A single unreadable/unparseable file must not abort the whole index.
    let mut parsed = Vec::with_capacity(results.len());
    let mut skipped = 0;
    for result in results {
        match result {
            Ok(file) => parsed.push(file),
            Err(_) => skipped += 1,
        }
    }

    let stats = IndexStats {
        files: parsed.len(),
        symbols: parsed.iter().map(|f| f.symbols.len()).sum(),
        skipped,
    };

    store.index_files(parsed.iter().map(|file| {
        NewFile {
            path: &file.path,
            lang: file.lang,
            hash: &file.hash,
            symbols: file
                .symbols
                .iter()
                .map(|s| NewSymbol {
                    name: &s.name,
                    kind: s.kind,
                    span: s.span,
                    signature: s.signature.as_deref(),
                })
                .collect(),
        }
    }))?;

    // Second pass: symbols now have ids, so refs can be resolved into edges.
    resolve_edges(store, &parsed)?;

    // Third pass: rank symbols by centrality over the resolved edges.
    rank_symbols(store)?;

    Ok(stats)
}

/// Compute PageRank over the current graph and persist each score. Standalone
/// so an incremental reindex can re-rank without re-parsing.
pub fn rank_symbols(store: &mut Store) -> Result<(), EngineError> {
    let nodes = store.list_symbol_ids()?;
    let edges = store.list_edges()?;
    let ranks = rank::page_rank(&nodes, &edges, RANK_DAMPING, RANK_ITERS);
    store.update_ranks(&ranks)?;
    Ok(())
}

/// Resolve the parsed refs into concrete edges and persist them. Runs after
/// symbols are stored, since edges are symbol-id → symbol-id.
// ponytail: name-based resolution — a call to `foo` links to every symbol named
// `foo` (preferring the same file). False positives are accepted; PageRank
// buries them. Precise resolution needs an LSP; that's the documented upgrade.
fn resolve_edges(store: &mut Store, parsed: &[ParsedFile]) -> Result<(), EngineError> {
    let symbols = store.list_symbols()?;
    let path_to_file: HashMap<String, FileId> = store
        .list_files()?
        .into_iter()
        .map(|(id, path)| (path, id))
        .collect();

    // Lookups built once over the whole symbol table.
    let mut by_name: HashMap<&str, Vec<(SymbolId, FileId)>> = HashMap::new();
    let mut by_span: HashMap<(i64, u32), SymbolId> = HashMap::new();
    let mut classes: HashMap<i64, Vec<(SymbolId, Span)>> = HashMap::new();
    for symbol in &symbols {
        by_name
            .entry(symbol.name.as_str())
            .or_default()
            .push((symbol.id, symbol.file_id));
        by_span.insert((symbol.file_id.0, symbol.span.start_byte), symbol.id);
        if symbol.kind == SymbolKind::Class {
            classes
                .entry(symbol.file_id.0)
                .or_default()
                .push((symbol.id, symbol.span));
        }
    }

    let mut edges = Vec::new();

    // calls + extends, resolved by name.
    for file in parsed {
        let key = file.path.to_string_lossy().into_owned();
        let Some(&file_id) = path_to_file.get(&key) else {
            continue;
        };
        for reference in &file.refs {
            let Some(&from_id) = by_span.get(&(file_id.0, reference.from.start_byte)) else {
                continue;
            };
            let Some(candidates) = by_name.get(reference.to_name.as_str()) else {
                continue;
            };
            // Prefer targets in the same file; only fall back to all if none.
            let same_file: Vec<SymbolId> = candidates
                .iter()
                .filter(|(_, f)| *f == file_id)
                .map(|(id, _)| *id)
                .collect();
            let targets = if same_file.is_empty() {
                candidates.iter().map(|(id, _)| *id).collect()
            } else {
                same_file
            };
            for to in targets {
                if to != from_id {
                    edges.push(Edge {
                        src: from_id,
                        dst: to,
                        kind: reference.kind,
                    });
                }
            }
        }
    }

    // defines: a class defines each method whose span it contains.
    for symbol in &symbols {
        if symbol.kind != SymbolKind::Method {
            continue;
        }
        let Some(file_classes) = classes.get(&symbol.file_id.0) else {
            continue;
        };
        if let Some((class_id, _)) = file_classes.iter().find(|(_, cspan)| {
            cspan.start_byte <= symbol.span.start_byte && symbol.span.end_byte <= cspan.end_byte
        }) {
            edges.push(Edge {
                src: *class_id,
                dst: symbol.id,
                kind: EdgeKind::Defines,
            });
        }
    }

    store.insert_edges(&edges)?;
    Ok(())
}

/// List the supported source files under `root`, honoring ignore rules.
fn walk(root: &Path, registry: &Registry) -> Vec<PathBuf> {
    WalkBuilder::new(root)
        .build()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_some_and(|t| t.is_file()))
        .map(ignore::DirEntry::into_path)
        .filter(|path| registry.for_path(path).is_some())
        .collect()
}

fn parse_one(
    registry: &Registry,
    extractors: &mut HashMap<Lang, Extractor>,
    path: &Path,
) -> Result<ParsedFile, EngineError> {
    let language = registry
        .for_path(path)
        .ok_or_else(|| EngineError::NoLanguage(path.to_owned()))?;
    let lang = language.lang();

    let bytes = fs::read(path).map_err(|source| EngineError::Io {
        path: path.to_owned(),
        source,
    })?;

    let extractor = match extractors.entry(lang) {
        Entry::Occupied(e) => e.into_mut(),
        Entry::Vacant(e) => e.insert(Extractor::new(language.as_ref())?),
    };

    let parse = extractor.extract(&bytes)?;
    Ok(ParsedFile {
        path: path.to_owned(),
        lang,
        hash: hash_contents(&bytes),
        symbols: parse.symbols,
        refs: parse.refs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_a_small_repo() {
        // Unique temp dir; a real repo on disk exercises walk + ignore + parse.
        let uniq = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("skeletree-test-{uniq}"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("mod.py"),
            "def foo():\n    pass\n\nclass Bar:\n    def m(self):\n        pass\n",
        )
        .unwrap();
        // Unsupported extension must be ignored by the walk.
        fs::write(dir.join("README.md"), "hi").unwrap();

        let mut store = Store::open_in_memory().unwrap();
        let stats = index(&dir, &mut store).unwrap();

        assert_eq!(stats.files, 1); // only mod.py
        assert_eq!(stats.symbols, 3); // foo, Bar, m
        assert_eq!(stats.skipped, 0);

        // Re-indexing the same repo must replace, not collide on the UNIQUE
        // path constraint nor double-count symbols.
        let again = index(&dir, &mut store).unwrap();
        assert_eq!(again.symbols, 3);
        assert_eq!(store.list_symbol_ids().unwrap().len(), 3);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolves_call_edges_end_to_end() {
        let uniq = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("skeletree-edges-{uniq}"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("app.py"),
            "def helper():\n    pass\n\ndef main():\n    helper()\n",
        )
        .unwrap();

        let mut store = Store::open_in_memory().unwrap();
        index(&dir, &mut store).unwrap();

        let symbols = store.list_symbols().unwrap();
        let main = symbols.iter().find(|s| s.name == "main").unwrap();
        let helper = symbols.iter().find(|s| s.name == "helper").unwrap();
        // The call edge main -> helper must make helper a neighbor of main.
        let neighbors = store.neighbors(main.id, 1).unwrap();
        assert!(neighbors.iter().any(|s| s.name == "helper"));
        // helper is called (pointed to), so it must outrank its caller.
        assert!(
            helper.rank > main.rank,
            "helper {} vs main {}",
            helper.rank,
            main.rank
        );

        fs::remove_dir_all(&dir).ok();
    }
}
