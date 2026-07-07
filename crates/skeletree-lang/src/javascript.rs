use skeletree_core::Lang;

use crate::Language;

/// JavaScript parser — covers React (`.jsx`) and plain JS. The grammar parses
/// JSX natively. Queries in `queries/javascript*.scm`.
pub struct JavaScript;

impl Language for JavaScript {
    fn lang(&self) -> Lang {
        Lang::JavaScript
    }

    fn ts_language(&self) -> tree_sitter::Language {
        // NOTE: tree-sitter 0.22-era grammar API, matching tree-sitter-python.
        tree_sitter_javascript::language()
    }

    fn query_source(&self) -> &'static str {
        include_str!("../queries/javascript.scm")
    }

    fn edge_query_source(&self) -> &'static str {
        include_str!("../queries/javascript-edges.scm")
    }

    fn def_node_kinds(&self) -> &'static [&'static str] {
        &[
            "function_declaration",
            "class_declaration",
            "method_definition",
            // const-assigned arrow/function components are captured here.
            "variable_declarator",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Extractor;
    use skeletree_core::{EdgeKind, SymbolKind};

    // A React function component rendering a child component, plus a hook.
    const SRC: &str = r#"
import React from 'react';

const MAX = 10;

function Header() {
  return <h1>Hi</h1>;
}

const useCounter = () => {
  return 0;
};

export default function App() {
  const n = useCounter();
  return <Header />;
}

class Legacy extends React.Component {
  render() {
    return <Header />;
  }
}
"#;

    #[test]
    fn extracts_jsx_symbols() {
        let mut ex = Extractor::new(&JavaScript).unwrap();
        let syms = ex.extract(SRC.as_bytes()).unwrap().symbols;
        let got: Vec<(&str, SymbolKind)> = syms.iter().map(|s| (s.name.as_str(), s.kind)).collect();

        // Both declaration and const-arrow components come through as functions.
        assert!(got.contains(&("Header", SymbolKind::Function)));
        assert!(got.contains(&("App", SymbolKind::Function)));
        assert!(got.contains(&("useCounter", SymbolKind::Function)));
        assert!(got.contains(&("Legacy", SymbolKind::Class)));
        assert!(got.contains(&("MAX", SymbolKind::Constant)));
    }

    #[test]
    fn extracts_jsx_edges() {
        let mut ex = Extractor::new(&JavaScript).unwrap();
        let refs = ex.extract(SRC.as_bytes()).unwrap().refs;

        // App renders <Header/> -> a component-graph reference.
        assert!(refs
            .iter()
            .any(|r| r.kind == EdgeKind::References && r.to_name == "Header"));
        // App calls the useCounter hook.
        assert!(refs
            .iter()
            .any(|r| r.kind == EdgeKind::Calls && r.to_name == "useCounter"));
        // Legacy extends React.Component -> capture the member name.
        assert!(refs
            .iter()
            .any(|r| r.kind == EdgeKind::Extends && r.to_name == "Component"));
    }
}
