//! MCP server over stdio and the small set of token-budgeted tools. Adding a
//! tool is a self-contained module here; the other tools stay untouched.

pub mod tools;

mod server;
pub use server::{serve_blocking, SkeletreeServer};

pub use skeletree_core as core;
pub use skeletree_engine as engine;
pub use skeletree_store as store;
