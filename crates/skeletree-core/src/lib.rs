//! Skeletree domain vocabulary: `Symbol`, `Edge`, `File`, ids and kinds.
//!
//! This crate has no I/O dependencies on purpose — every other crate depends
//! on it, so keeping it pure keeps the whole workspace testable. Types mirror
//! the SQLite schema 1:1 so the store layer is a straight mapping.

mod entities;
mod error;
mod ids;
mod kinds;
mod lang;

pub use entities::{Edge, File, Span, Symbol};
pub use error::ParseError;
pub use ids::{FileId, SymbolId};
pub use kinds::{EdgeKind, SymbolKind};
pub use lang::Lang;

#[cfg(test)]
mod tests {
    use super::*;

    /// Every enum's `Display` and `FromStr` must round-trip through the exact
    /// string the SQLite columns store — a mismatch silently corrupts reads.
    #[test]
    fn enum_strings_round_trip() {
        for lang in [Lang::TypeScript, Lang::JavaScript, Lang::Python, Lang::Rust] {
            assert_eq!(lang.to_string().parse::<Lang>().unwrap(), lang);
        }
        for kind in [
            SymbolKind::Function,
            SymbolKind::Class,
            SymbolKind::Method,
            SymbolKind::Interface,
            SymbolKind::TypeAlias,
            SymbolKind::Constant,
        ] {
            assert_eq!(kind.to_string().parse::<SymbolKind>().unwrap(), kind);
        }
        for kind in [
            EdgeKind::Imports,
            EdgeKind::Calls,
            EdgeKind::Defines,
            EdgeKind::References,
            EdgeKind::Extends,
        ] {
            assert_eq!(kind.to_string().parse::<EdgeKind>().unwrap(), kind);
        }
    }

    #[test]
    fn unknown_strings_error() {
        assert!("cobol".parse::<Lang>().is_err());
        assert!("gadget".parse::<SymbolKind>().is_err());
        assert!("teleports".parse::<EdgeKind>().is_err());
    }

    #[test]
    fn extensions_map_to_langs() {
        assert_eq!(Lang::from_extension("tsx"), Some(Lang::TypeScript));
        assert_eq!(Lang::from_extension("mjs"), Some(Lang::JavaScript));
        assert_eq!(Lang::from_extension("pyi"), Some(Lang::Python));
        assert_eq!(Lang::from_extension("rs"), Some(Lang::Rust));
        assert_eq!(Lang::from_extension("cobol"), None);
    }
}
