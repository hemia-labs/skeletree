use skeletree_core::Lang;

use crate::Language;

/// TypeScript parser — covers NestJS (decorated classes) and React/Next.js
/// (`.tsx` + JSX). Grammar from `tree-sitter-typescript`; queries in
/// `queries/typescript*.scm`.
pub struct TypeScript;

impl Language for TypeScript {
    fn lang(&self) -> Lang {
        Lang::TypeScript
    }

    fn ts_language(&self) -> tree_sitter::Language {
        // The `tsx` grammar, used for all TypeScript: it's a superset that
        // also parses JSX, so `.tsx` React components work. The one thing it
        // loses is `<Type>value` casts (JSX-ambiguous), which don't affect the
        // definitions/edges we extract. ponytail: one grammar over splitting
        // Lang into Ts/Tsx for a cast form we never read.
        tree_sitter_typescript::language_tsx()
    }

    fn query_source(&self) -> &'static str {
        include_str!("../queries/typescript.scm")
    }

    fn edge_query_source(&self) -> &'static str {
        include_str!("../queries/typescript-edges.scm")
    }

    fn def_node_kinds(&self) -> &'static [&'static str] {
        &[
            "function_declaration",
            "class_declaration",
            "abstract_class_declaration",
            "method_definition",
            // const-assigned arrow/function components and hooks are captured
            // at their declarator, so it must count as an enclosing def too.
            "variable_declarator",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Extractor;
    use skeletree_core::{EdgeKind, SymbolKind};

    // A trimmed-down NestJS controller + service: decorators, DI, inheritance.
    const SRC: &str = r#"
import { Controller, Get } from '@nestjs/common';

export const MAX_ITEMS = 100;
const helper = 1;

export interface Cat {
    name: string;
}

export type CatId = string;

class Base {}

@Controller('cats')
export class CatsController extends Base {
    constructor(private readonly service: CatsService) {}

    @Get()
    findAll(): Cat[] {
        return this.service.findAll();
    }
}

@Injectable()
export class CatsService {
    findAll(): Cat[] {
        return [];
    }
}
"#;

    #[test]
    fn extracts_typescript_symbols() {
        let mut ex = Extractor::new(&TypeScript).unwrap();
        let syms = ex.extract(SRC.as_bytes()).unwrap().symbols;
        let got: Vec<(&str, SymbolKind)> = syms.iter().map(|s| (s.name.as_str(), s.kind)).collect();

        assert!(got.contains(&("CatsController", SymbolKind::Class)));
        assert!(got.contains(&("CatsService", SymbolKind::Class)));
        assert!(got.contains(&("findAll", SymbolKind::Method)));
        assert!(got.contains(&("Cat", SymbolKind::Interface)));
        assert!(got.contains(&("CatId", SymbolKind::TypeAlias)));
        assert!(got.contains(&("MAX_ITEMS", SymbolKind::Constant)));

        // lower-case declarator is not a constant by convention.
        assert!(!got.iter().any(|(n, _)| *n == "helper"));
    }

    #[test]
    fn extracts_typescript_edges() {
        let mut ex = Extractor::new(&TypeScript).unwrap();
        let refs = ex.extract(SRC.as_bytes()).unwrap().refs;

        // CatsController extends Base.
        assert!(refs
            .iter()
            .any(|r| r.kind == EdgeKind::Extends && r.to_name == "Base"));
        // findAll() calls the service's findAll (member call captures the name).
        assert!(refs
            .iter()
            .any(|r| r.kind == EdgeKind::Calls && r.to_name == "findAll"));
    }

    // A React/Next.js `.tsx` page: arrow component + JSX child, typed props.
    const TSX: &str = r#"
interface Props { title: string }

const Header = ({ title }: Props) => <h1>{title}</h1>;

export default function Page() {
    return <Header title="hi" />;
}
"#;

    #[test]
    fn extracts_tsx_component_and_graph() {
        let mut ex = Extractor::new(&TypeScript).unwrap();
        let parse = ex.extract(TSX.as_bytes()).unwrap();

        // The tsx grammar parses JSX: both components are symbols.
        let got: Vec<&str> = parse.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(got.contains(&"Header"), "symbols: {got:?}");
        assert!(got.contains(&"Page"), "symbols: {got:?}");

        // Page renders <Header/> -> a component-graph reference.
        assert!(parse
            .refs
            .iter()
            .any(|r| r.kind == EdgeKind::References && r.to_name == "Header"));
    }
}
