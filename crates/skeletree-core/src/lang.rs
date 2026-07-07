use std::fmt;
use std::str::FromStr;

use crate::error::ParseError;

/// A language Skeletree can parse. TS/JS/Python/Rust for the MVP; Go/Java later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    TypeScript,
    JavaScript,
    Python,
    Rust,
}

impl Lang {
    /// Map a file extension (without the dot) to a language, if supported.
    pub fn from_extension(ext: &str) -> Option<Self> {
        Some(match ext {
            "ts" | "tsx" | "mts" | "cts" => Lang::TypeScript,
            "js" | "jsx" | "mjs" | "cjs" => Lang::JavaScript,
            "py" | "pyi" => Lang::Python,
            "rs" => Lang::Rust,
            _ => return None,
        })
    }

    /// Canonical lowercase name — the exact string stored in `files.lang`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Lang::TypeScript => "typescript",
            Lang::JavaScript => "javascript",
            Lang::Python => "python",
            Lang::Rust => "rust",
        }
    }
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Lang {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "typescript" => Ok(Lang::TypeScript),
            "javascript" => Ok(Lang::JavaScript),
            "python" => Ok(Lang::Python),
            "rust" => Ok(Lang::Rust),
            other => Err(ParseError::UnknownLang(other.to_owned())),
        }
    }
}
