use std::fmt;
use std::str::FromStr;

use crate::error::ParseError;

/// What a symbol is. Limited to what the MVP extractors emit (roadmap week 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Function,
    Class,
    Method,
    Interface,
    TypeAlias,
    Constant,
}

impl SymbolKind {
    /// Canonical string stored in `symbols.kind`.
    pub fn as_str(&self) -> &'static str {
        match self {
            SymbolKind::Function => "function",
            SymbolKind::Class => "class",
            SymbolKind::Method => "method",
            SymbolKind::Interface => "interface",
            SymbolKind::TypeAlias => "type_alias",
            SymbolKind::Constant => "constant",
        }
    }
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SymbolKind {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "function" => Ok(SymbolKind::Function),
            "class" => Ok(SymbolKind::Class),
            "method" => Ok(SymbolKind::Method),
            "interface" => Ok(SymbolKind::Interface),
            "type_alias" => Ok(SymbolKind::TypeAlias),
            "constant" => Ok(SymbolKind::Constant),
            other => Err(ParseError::UnknownSymbolKind(other.to_owned())),
        }
    }
}

/// The relationship an edge encodes. Matches the `edges.kind` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    Imports,
    Calls,
    Defines,
    References,
    Extends,
}

impl EdgeKind {
    /// Canonical string stored in `edges.kind`.
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeKind::Imports => "imports",
            EdgeKind::Calls => "calls",
            EdgeKind::Defines => "defines",
            EdgeKind::References => "references",
            EdgeKind::Extends => "extends",
        }
    }
}

impl fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EdgeKind {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "imports" => Ok(EdgeKind::Imports),
            "calls" => Ok(EdgeKind::Calls),
            "defines" => Ok(EdgeKind::Defines),
            "references" => Ok(EdgeKind::References),
            "extends" => Ok(EdgeKind::Extends),
            other => Err(ParseError::UnknownEdgeKind(other.to_owned())),
        }
    }
}
