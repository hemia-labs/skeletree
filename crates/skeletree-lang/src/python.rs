use skeletree_core::Lang;

use crate::Language;

/// Python parser. Grammar from `tree-sitter-python`; query in `queries/python.scm`.
pub struct Python;

impl Language for Python {
    fn lang(&self) -> Lang {
        Lang::Python
    }

    fn ts_language(&self) -> tree_sitter::Language {
        // NOTE: tree-sitter 0.22-era grammar API. If bumping to a version that
        // exports `LANGUAGE: LanguageFn`, use `tree_sitter_python::LANGUAGE.into()`.
        tree_sitter_python::language()
    }

    fn query_source(&self) -> &'static str {
        include_str!("../queries/python.scm")
    }

    fn edge_query_source(&self) -> &'static str {
        include_str!("../queries/python-edges.scm")
    }

    fn def_node_kinds(&self) -> &'static [&'static str] {
        &["function_definition", "class_definition"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Extractor;
    use skeletree_core::{EdgeKind, SymbolKind};

    const SRC: &str = r#"
MAX_SIZE = 100
lower_case = 2

def top_level():
    pass

@decorator
def decorated():
    pass

class Foo(Base):
    def method(self):
        pass
"#;

    #[test]
    fn extracts_python_symbols() {
        let mut ex = Extractor::new(&Python).unwrap();
        let syms = ex.extract(SRC.as_bytes()).unwrap().symbols;
        let got: Vec<(&str, SymbolKind)> = syms.iter().map(|s| (s.name.as_str(), s.kind)).collect();

        assert!(got.contains(&("top_level", SymbolKind::Function)));
        assert!(got.contains(&("decorated", SymbolKind::Function)));
        assert!(got.contains(&("Foo", SymbolKind::Class)));
        assert!(got.contains(&("method", SymbolKind::Method)));
        assert!(got.contains(&("MAX_SIZE", SymbolKind::Constant)));

        // lower_case is an assignment but not a constant by convention.
        assert!(!got.iter().any(|(n, _)| *n == "lower_case"));

        // Signature is the definition's first line, decorators excluded.
        let decorated = syms.iter().find(|s| s.name == "decorated").unwrap();
        assert_eq!(decorated.signature.as_deref(), Some("def decorated():"));
    }

    #[test]
    fn extracts_python_edges() {
        const S: &str = "\
class A:
    pass

class B(A):
    def go(self):
        helper()

def helper():
    pass
";
        let mut ex = Extractor::new(&Python).unwrap();
        let refs = ex.extract(S.as_bytes()).unwrap().refs;

        // B extends A.
        assert!(refs
            .iter()
            .any(|r| r.kind == EdgeKind::Extends && r.to_name == "A"));
        // go() calls helper(); the module-level nature of helper is irrelevant,
        // what matters is the call site sits inside `go`.
        assert!(refs
            .iter()
            .any(|r| r.kind == EdgeKind::Calls && r.to_name == "helper"));
    }
}
