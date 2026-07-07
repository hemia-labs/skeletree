//! The rmcp adapter: exposes the [`crate::tools`] functions as MCP tools over
//! stdio. Deliberately thin — all logic lives in `tools`; this only wires
//! request schemas, locking, and transport.

use std::sync::{Arc, Mutex};

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
    ServerHandler, ServiceExt,
};
use serde::Deserialize;
use skeletree_store::Store;

use crate::tools;

const DEFAULT_BUDGET: usize = 1500;
fn default_budget() -> usize {
    DEFAULT_BUDGET
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct OverviewRequest {
    #[schemars(description = "Maximum tokens in the response")]
    #[serde(default = "default_budget")]
    token_budget: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct FindRequest {
    #[schemars(description = "Name or substring to search for")]
    query: String,
    #[schemars(
        description = "Optional kind filter: function, class, method, interface, type_alias, constant"
    )]
    #[serde(default)]
    kind: Option<String>,
    #[schemars(description = "Maximum tokens in the response")]
    #[serde(default = "default_budget")]
    token_budget: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct NeighborsRequest {
    #[schemars(description = "Symbol name to center on")]
    symbol: String,
    #[schemars(description = "How many hops out to include (1–3)")]
    #[serde(default = "default_depth")]
    depth: u32,
    #[schemars(description = "Maximum tokens in the response")]
    #[serde(default = "default_budget")]
    token_budget: usize,
}
fn default_depth() -> u32 {
    1
}

/// MCP server holding the (single-connection) index.
// ponytail: Arc<Mutex<Store>> serializes access through one SQLite connection —
// fine for one agent. Swap for a connection pool if concurrency ever matters.
#[derive(Clone)]
pub struct SkeletreeServer {
    store: Arc<Mutex<Store>>,
    // Read by the code `#[tool_handler]` generates, which rustc can't see.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl SkeletreeServer {
    pub fn new(store: Store) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl SkeletreeServer {
    #[tool(description = "Ranked map of the repository — the most central symbols first.")]
    fn overview(&self, Parameters(req): Parameters<OverviewRequest>) -> String {
        let store = self.store.lock().unwrap();
        tools::overview(&store, req.token_budget).unwrap_or_else(|e| format!("error: {e}\n"))
    }

    #[tool(description = "Find symbols by name (substring), optionally filtered by kind.")]
    fn find(&self, Parameters(req): Parameters<FindRequest>) -> String {
        let kind = req.kind.as_deref().and_then(|k| k.parse().ok());
        let store = self.store.lock().unwrap();
        tools::find(&store, &req.query, kind, req.token_budget)
            .unwrap_or_else(|e| format!("error: {e}\n"))
    }

    #[tool(
        description = "Symbols that call, use, or are used by the named symbol (graph neighbors)."
    )]
    fn neighbors(&self, Parameters(req): Parameters<NeighborsRequest>) -> String {
        let store = self.store.lock().unwrap();
        tools::neighbors(&store, &req.symbol, req.depth, req.token_budget)
            .unwrap_or_else(|e| format!("error: {e}\n"))
    }
}

#[tool_handler]
impl ServerHandler for SkeletreeServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Query your repository's symbol graph instead of reading files. \
             `overview` for a ranked map, `find` to locate a symbol, \
             `neighbors` to see what calls or uses it.",
        )
    }
}

/// Serve the MCP protocol over stdio until the client disconnects. Blocking:
/// owns its tokio runtime so the CLI stays sync.
pub fn serve_blocking(store: Store) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        let running = SkeletreeServer::new(store).serve(stdio()).await?;
        running.waiting().await?;
        Ok(())
    })
}
