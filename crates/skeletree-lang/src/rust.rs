use skeletree_core::Lang;

use crate::Language;

/// Rust parser. Grammar from `tree-sitter-rust`; queries in `queries/rust*.scm`.
pub struct Rust;

impl Language for Rust {
    fn lang(&self) -> Lang {
        Lang::Rust
    }

    fn ts_language(&self) -> tree_sitter::Language {
        // NOTE: tree-sitter 0.22-era grammar API, matching the other grammars.
        tree_sitter_rust::language()
    }

    fn query_source(&self) -> &'static str {
        include_str!("../queries/rust.scm")
    }

    fn edge_query_source(&self) -> &'static str {
        include_str!("../queries/rust-edges.scm")
    }

    fn def_node_kinds(&self) -> &'static [&'static str] {
        // Free fns and methods are both `function_item`; supertrait bounds
        // enclose in `trait_item`.
        &["function_item", "trait_item"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Extractor;
    use skeletree_core::{EdgeKind, SymbolKind};

    const SRC: &str = r#"
pub const MAX_SIZE: usize = 100;
static COUNTER: u32 = 0;

pub struct Point {
    x: i32,
    y: i32,
}

pub enum Shape {
    Circle,
    Square,
}

pub type Coord = (i32, i32);

pub trait Drawable: Shaped {
    fn draw(&self);
}

trait Shaped {}

pub fn origin() -> Point {
    Point { x: 0, y: 0 }
}

impl Point {
    fn norm(&self) -> i32 {
        helper(self.x)
    }
}

fn helper(v: i32) -> i32 {
    v
}
"#;

    #[test]
    fn extracts_rust_symbols() {
        let mut ex = Extractor::new(&Rust).unwrap();
        let syms = ex.extract(SRC.as_bytes()).unwrap().symbols;
        let got: Vec<(&str, SymbolKind)> = syms.iter().map(|s| (s.name.as_str(), s.kind)).collect();

        assert!(got.contains(&("origin", SymbolKind::Function)));
        assert!(got.contains(&("helper", SymbolKind::Function)));
        assert!(got.contains(&("norm", SymbolKind::Method)));
        assert!(got.contains(&("draw", SymbolKind::Method)));
        assert!(got.contains(&("Point", SymbolKind::Class)));
        assert!(got.contains(&("Shape", SymbolKind::Class)));
        assert!(got.contains(&("Drawable", SymbolKind::Interface)));
        assert!(got.contains(&("Coord", SymbolKind::TypeAlias)));
        assert!(got.contains(&("MAX_SIZE", SymbolKind::Constant)));
        assert!(got.contains(&("COUNTER", SymbolKind::Constant)));

        // A method is a function_item under impl, not a free function.
        let norm = syms.iter().find(|s| s.name == "norm").unwrap();
        assert_eq!(norm.kind, SymbolKind::Method);
    }

    #[test]
    fn extracts_rust_edges() {
        let mut ex = Extractor::new(&Rust).unwrap();
        let refs = ex.extract(SRC.as_bytes()).unwrap().refs;

        // norm() calls helper().
        assert!(refs
            .iter()
            .any(|r| r.kind == EdgeKind::Calls && r.to_name == "helper"));
        // Drawable: Shaped -> supertrait extends edge.
        assert!(refs
            .iter()
            .any(|r| r.kind == EdgeKind::Extends && r.to_name == "Shaped"));
    }
}
