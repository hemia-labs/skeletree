//! Parsing layer: the `Language` trait, per-language tree-sitter grammars and
//! the extension→language registry — the one extension point for new languages.
//!
//! Adding a language = implement [`Language`] + drop a `.scm` whose capture
//! names match [`skeletree_core::SymbolKind`] strings, then register it.

use std::path::Path;
use std::sync::Arc;

use skeletree_core::{EdgeKind, Lang, Span, SymbolKind};
use thiserror::Error;
use tree_sitter::{Node, Parser, Query, QueryCursor, Tree};

pub use skeletree_core as core;

mod python;
pub use python::Python;

mod typescript;
pub use typescript::TypeScript;

mod javascript;
pub use javascript::JavaScript;

mod rust;
pub use rust::Rust;

/// A symbol as produced by parsing, before the store assigns it an id.
/// (`skeletree_core::Symbol` is the stored form with ids.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub span: Span,
    pub signature: Option<String>,
}

/// An unresolved relationship: the enclosing definition's span (the source),
/// the name being referenced (the target, resolved to an id later), and its
/// kind. The engine turns these into `skeletree_core::Edge`s once ids exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRef {
    pub from: Span,
    pub to_name: String,
    pub kind: EdgeKind,
}

/// Everything one file yields in a single parse: its symbols and its raw refs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileParse {
    pub symbols: Vec<ParsedSymbol>,
    pub refs: Vec<RawRef>,
}

/// Everything language-specific: the grammar and its queries. Cheap and
/// shareable; the heavy per-thread state lives in [`Extractor`].
pub trait Language: Send + Sync {
    fn lang(&self) -> Lang;
    fn ts_language(&self) -> tree_sitter::Language;
    /// Query whose capture names are `SymbolKind` strings, plus a `@name`
    /// capture for the identifier.
    fn query_source(&self) -> &'static str;
    /// Query whose capture names are `EdgeKind` strings; each capture is the
    /// referenced identifier (callee, base class, …).
    fn edge_query_source(&self) -> &'static str;
    /// tree-sitter node kinds that own the symbols nested in them — the
    /// definitions `enclosing_def` walks up to find a ref's source symbol.
    /// Must be the same nodes the symbol query captures (spans have to match).
    fn def_node_kinds(&self) -> &'static [&'static str];
}

#[derive(Debug, Error)]
pub enum LangError {
    #[error("failed to set tree-sitter language: {0}")]
    SetLanguage(#[from] tree_sitter::LanguageError),
    #[error("failed to compile query: {0}")]
    Query(#[from] tree_sitter::QueryError),
    #[error("tree-sitter failed to parse the source")]
    Parse,
    #[error("symbol text was not valid UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
}

/// A compiled parser + query for one language. Not `Send`/`Sync` friendly to
/// share, so build one per thread (rayon gives each worker its own).
pub struct Extractor {
    parser: Parser,
    symbol_query: Query,
    edge_query: Query,
    def_kinds: &'static [&'static str],
}

impl Extractor {
    pub fn new(language: &dyn Language) -> Result<Self, LangError> {
        let ts = language.ts_language();
        let mut parser = Parser::new();
        parser.set_language(&ts)?;
        let symbol_query = Query::new(&ts, language.query_source())?;
        let edge_query = Query::new(&ts, language.edge_query_source())?;
        Ok(Self {
            parser,
            symbol_query,
            edge_query,
            def_kinds: language.def_node_kinds(),
        })
    }

    /// Parse one file's source once, extracting both its symbols and its refs.
    pub fn extract(&mut self, source: &[u8]) -> Result<FileParse, LangError> {
        let tree = self.parser.parse(source, None).ok_or(LangError::Parse)?;
        let symbols = self.symbols(&tree, source)?;
        let refs = self.refs(&tree, source)?;
        Ok(FileParse { symbols, refs })
    }

    fn symbols(&self, tree: &Tree, source: &[u8]) -> Result<Vec<ParsedSymbol>, LangError> {
        // NOTE: capture_names() returns &[&str] in tree-sitter 0.22. If a future
        // version returns &[String], read as `&names[i]` and call `.as_str()`.
        let names = self.symbol_query.capture_names();

        let mut out = Vec::new();
        let mut cursor = QueryCursor::new();
        // NOTE: `matches` is a std Iterator in tree-sitter 0.22. In 0.23+ it
        // became a StreamingIterator — switch to `while let Some(m) = it.next()`
        // with `use streaming_iterator::StreamingIterator` if you bump the grammar.
        for m in cursor.matches(&self.symbol_query, tree.root_node(), source) {
            let mut name_text: Option<String> = None;
            let mut def: Option<(SymbolKind, Node)> = None;

            for cap in m.captures {
                let cname = names[cap.index as usize];
                if cname == "name" {
                    name_text = Some(cap.node.utf8_text(source)?.to_owned());
                } else if let Ok(kind) = cname.parse::<SymbolKind>() {
                    def = Some((kind, cap.node));
                }
            }

            let (Some(name), Some((kind, node))) = (name_text, def) else {
                continue;
            };

            // Module-level assignments are captured broadly; keep only the ones
            // that read as constants by Python convention (UPPER_SNAKE_CASE).
            if kind == SymbolKind::Constant && !is_upper_snake(&name) {
                continue;
            }

            let signature = node
                .utf8_text(source)?
                .lines()
                .next()
                .map(|l| l.trim_end().to_owned());

            out.push(ParsedSymbol {
                name,
                kind,
                span: span_of(&node),
                signature,
            });
        }
        Ok(out)
    }

    fn refs(&self, tree: &Tree, source: &[u8]) -> Result<Vec<RawRef>, LangError> {
        let names = self.edge_query.capture_names();

        let mut out = Vec::new();
        let mut cursor = QueryCursor::new();
        for m in cursor.matches(&self.edge_query, tree.root_node(), source) {
            for cap in m.captures {
                let Ok(kind) = names[cap.index as usize].parse::<EdgeKind>() else {
                    continue;
                };
                // The "from" symbol is the nearest enclosing definition. A
                // module-level call has none, so it is skipped.
                let Some(def) = enclosing_def(cap.node, self.def_kinds) else {
                    continue;
                };
                out.push(RawRef {
                    from: span_of(&def),
                    to_name: cap.node.utf8_text(source)?.to_owned(),
                    kind,
                });
            }
        }
        Ok(out)
    }
}

/// Walk up from `node` to the nearest definition in `def_kinds`, which owns it.
fn enclosing_def<'a>(node: Node<'a>, def_kinds: &[&str]) -> Option<Node<'a>> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if def_kinds.contains(&parent.kind()) {
            return Some(parent);
        }
        current = parent;
    }
    None
}

fn span_of(node: &tree_sitter::Node) -> Span {
    Span {
        start_byte: node.start_byte() as u32,
        end_byte: node.end_byte() as u32,
        start_line: node.start_position().row as u32 + 1,
        end_line: node.end_position().row as u32 + 1,
    }
}

fn is_upper_snake(name: &str) -> bool {
    name.chars().any(|c| c.is_ascii_uppercase())
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// The set of languages Skeletree knows how to parse.
pub struct Registry {
    langs: Vec<Arc<dyn Language>>,
}

impl Registry {
    /// The MVP language set: Python, TypeScript/TSX, JavaScript/JSX, Rust.
    pub fn with_defaults() -> Self {
        Self {
            langs: vec![
                Arc::new(Python),
                Arc::new(TypeScript),
                Arc::new(JavaScript),
                Arc::new(Rust),
            ],
        }
    }

    /// The language for a path, by extension, if supported.
    pub fn for_path(&self, path: &Path) -> Option<&Arc<dyn Language>> {
        let ext = path.extension()?.to_str()?;
        let lang = Lang::from_extension(ext)?;
        self.langs.iter().find(|l| l.lang() == lang)
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::with_defaults()
    }
}
