<p align="center">
  <img src="https://raw.githubusercontent.com/hemia-labs/skeletree/main/assets/logo.webp" alt="Skeletree" width="480">
</p>

<p align="center">
  <a href="https://crates.io/crates/skeletree"><img src="https://img.shields.io/crates/v/skeletree.svg" alt="crates.io"></a>
  <a href="https://github.com/hemia-labs/skeletree/actions/workflows/ci.yml"><img src="https://github.com/hemia-labs/skeletree/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/crates/l/skeletree.svg" alt="license"></a>
  <img src="https://img.shields.io/badge/rust-stable-orange.svg" alt="rust">
  <a href="https://hemia.dev"><img src="https://img.shields.io/badge/build%20by-hemia-000000.svg?labelColor=000000" alt="build by hemia"></a>
</p>

> Your agent stops reading your codebase and starts querying it.

A local indexer that turns any repo into a queryable graph of symbols, calls,
imports and dependencies, and exposes it over [MCP](https://modelcontextprotocol.io)
— so AI agents navigate code structure with hundreds of tokens instead of tens
of thousands.

**Status:** early development. Python, TypeScript/TSX, JavaScript/JSX and Rust —
NestJS, React and Next.js all index out of the box.

## What it does

Skeletree parses a repository with tree-sitter, extracts every symbol
(functions, classes, methods, interfaces, type aliases, constants) and the
relationships between them (calls, imports, defines, references, extends),
stores the result as a graph in a local SQLite file, and ranks symbols by
PageRank centrality. An agent then queries that graph over MCP instead of
grepping and reading files.

Each MCP response is **token-budgeted**: you ask for a map, a search, or a
symbol's neighbors and get back compact text trimmed to a token ceiling, so a
query costs hundreds of tokens where reading the files would cost tens of
thousands.

## How it works

```
walk (respect .gitignore) → parse (tree-sitter, in parallel)
  → persist symbols (SQLite, one transaction)
  → resolve edges (name-based)  → rank (PageRank)
```

1. **Walk** — `ignore` crate honors `.gitignore`; files are matched to a
   language by extension.
2. **Parse** — each file goes through a tree-sitter grammar + `.scm` queries;
   parsing is CPU-bound and runs across cores with `rayon`.
3. **Persist** — symbols are written in a single SQLite transaction. A full
   reindex replaces the previous contents atomically.
4. **Resolve edges** — refs are linked to symbols by name (preferring the same
   file). This is a heuristic, not an LSP: false positives are accepted and
   PageRank buries them.
5. **Rank** — PageRank over the edge graph scores each symbol's centrality, so
   the most-connected code surfaces first.

The index lives at `<repo>/.skeletree/index.db` — a portable SQLite file.

## Language support

| Language | Extensions | Symbols | Edges | Status |
|----------|-----------|---------|-------|--------|
| Python | `.py` `.pyi` | functions, classes, methods, UPPER_SNAKE constants | calls, extends, defines | ✅ supported |
| TypeScript / TSX | `.ts` `.tsx` `.mts` `.cts` | functions, classes, methods, interfaces, type aliases, const arrow/fn components, constants | calls, extends, defines, JSX references | ✅ supported |
| JavaScript / JSX | `.js` `.jsx` `.mjs` `.cjs` | functions, classes, methods, const arrow/fn components, constants | calls, extends, defines, JSX references | ✅ supported |
| Rust | `.rs` | fns, methods, structs/enums/unions (→ class), traits (→ interface), type aliases, consts/statics | calls, supertrait extends | ✅ supported |

Frameworks that need no special handling — they're just the languages above:

- **NestJS** — decorated classes/methods (`@Controller`, `@Injectable`, `@Get`) parse as plain nodes.
- **React** — function, arrow, and class components; hooks. `<Child/>` usage becomes a `references` edge, so the component graph feeds ranking.
- **Next.js** — `.tsx`/`.jsx` pages and components index the same way.

### Scope roadmap

Ordered by intended arrival. Nothing below blocks indexing today — it sharpens
precision and widens coverage.

| Item | What it adds | Status |
|------|--------------|--------|
| Python / TS / JS / Rust | The four languages above | ✅ done |
| Rust `impl` defines edges | Link a struct/enum to methods in its `impl` blocks (span containment doesn't reach across `impl`, so this needs impl-aware resolution) | ⬜ planned |
| Import extraction | `imports` edges (the enum variant exists but is unused) | ⬜ planned |
| Precise cross-file resolution | Resolve `import { X } from '...'` (relative paths, then tsconfig `paths`/`baseUrl` aliases, barrels) so a call to `X` links to the exact `X`, not every same-named symbol. Needs LSP-grade module resolution — until then edges resolve by name and PageRank buries false positives | ⬜ planned |
| `.tsx` type-cast fidelity | The `tsx` grammar parses all TS, losing `<Type>value` casts (JSX-ambiguous). Split by extension if it ever matters for extraction | ⬜ if needed |
| Go | `.go` — funcs, types, methods, interfaces | ⬜ post-MVP |
| Java | `.java` — classes, methods, interfaces | ⬜ post-MVP |

Adding a language is a self-contained change behind the `Language` trait — see
[Adding a language](#adding-a-language).

## Workspace layout

| Crate | Responsibility |
|-------|----------------|
| `skeletree-core` | Domain types (symbols, edges, ids). No I/O. |
| `skeletree-lang` | tree-sitter parsing + the `Language` trait/registry. |
| `skeletree-store` | SQLite persistence + recursive graph queries. |
| `skeletree-engine` | Pipeline: walk → parse → persist → rank. |
| `skeletree-mcp` | MCP server + token-budgeted tools. |
| `skeletree` | The `skeletree` binary (CLI + MCP entry point). |
| `skeletree-bench` | Reproducible token-savings benchmark harness. |

## Commands

| Command | What it does |
|---------|--------------|
| `skeletree index [PATH]` | Index a repo into `.skeletree/index.db` (default `.`). |
| `skeletree serve [--watch]` | Run the MCP server over stdio against the current repo's index. |
| `skeletree stats [PATH] [--limit N]` | Print the top-N symbols by rank. |
| `skeletree export [PATH] [--format md\|json\|mermaid] [--budget N]` | Export a ranked map. |
| `skeletree init` | One-command setup: index + MCP config + git hook. |

`serve`, `stats`, and `export` read the existing index; they fail with a clear
message if you haven't indexed yet. `init` and `serve --watch` are still on
the roadmap and currently stubs.

## MCP tools

Point any MCP client at `skeletree serve`. It exposes three tools, each
accepting a `token_budget` (default 1500):

| Tool | Arguments | Returns |
|------|-----------|---------|
| `overview` | `token_budget` | Ranked map of the repo, most central symbols first. |
| `find` | `query`, `kind?`, `token_budget` | Symbols matching a name substring, optionally filtered by kind. |
| `neighbors` | `symbol`, `depth` (1–3), `token_budget` | Symbols that call, use, or are used by the named symbol. |

Once the server is registered (see setup below), just ask your agent in plain
language — e.g. "give me an overview of this repo" or "find the `Registry`
symbol and show its neighbors" — the agent picks the right tool and args on
its own; no special syntax needed. Example, asking for `Registry`'s neighbors
in this repo:

```
> find the Registry symbol and show its neighbors

struct    Registry  (crates/skeletree-lang/src/lib.rs:220)
fn        with_defaults  crates/skeletree-lang/src/lib.rs:226  pub fn with_defaults() -> Self
fn        for_path       crates/skeletree-lang/src/lib.rs:238  pub fn for_path(&self, path: &Path) -> Option<&Arc<dyn Language>>
```

One line per symbol: `kind  name  file:line  signature`, ranked, trimmed to
the token budget — that's all three tools return.

`skeletree serve` is a **local, stdio-based** MCP server, so it works in any
client that can spawn a local process: Claude Code (all permission modes —
default, auto-accept, plan, bypass — MCP tools aren't affected by the mode),
Claude Desktop, Codex, and any other MCP-compatible agent (setup differs
slightly per client — see below). It does **not** work in Claude.ai (web) or
the Claude mobile app, since those only support remote MCP servers, not local
stdio ones.

## Usage

Install the binary (once published):

```sh
cargo install skeletree
```

Update to the latest release the same way — `--force` reinstalls over the
existing binary:

```sh
cargo install skeletree --force
```

Index your repo — this writes `.skeletree/index.db`:

```sh
cd your-repo
skeletree index .
skeletree stats --limit 10       # sanity-check the ranking
```

Then wire `skeletree serve` into your agent as an MCP server. Run it from the
repo root — it reads `.skeletree/index.db` relative to the current directory.

### Claude Code

```sh
cd your-repo
claude mcp add skeletree -- skeletree serve
```

That writes the server into `.mcp.json` (project scope). Add `-s user` to make
it available in every project instead. Verify with `claude mcp list`.

### Claude Desktop

Claude Desktop doesn't launch the server from your repo, so `claude mcp add`
and a bare `serve` won't find the index. Edit the config file directly and pass
the **absolute path** to the indexed repo as an argument to `serve`. You can
open it faster from the app: **Settings → Developer → Edit Config**.

- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "skeletree": {
      "command": "skeletree",
      "args": ["serve", "/absolute/path/to/your-repo"]
    }
  }
}
```

If `skeletree` isn't on the launcher's `PATH`, use its absolute path as
`command` too (`which skeletree` to find it). Restart Claude Desktop after
editing.

### Codex

Codex reads MCP servers from `~/.codex/config.toml`. Add:

```toml
[mcp_servers.skeletree]
command = "skeletree"
args = ["serve", "/absolute/path/to/your-repo"]
```

The path argument is optional — omit it to serve the current directory — but
passing the absolute repo path avoids depending on where Codex is launched.

### Any other MCP client

Point it at the `skeletree` binary with the `serve` argument:

```json
{
  "mcpServers": {
    "skeletree": {
      "command": "skeletree",
      "args": ["serve"]
    }
  }
}
```

## Development

Requires a recent stable Rust toolchain (pinned in `rust-toolchain.toml`).

```sh
cargo build --workspace           # build everything
cargo test  --workspace           # run all tests
cargo run -p skeletree -- --help
```

Try it end to end against this repo or any Python project:

```sh
cargo run -p skeletree -- index /path/to/python/repo
cargo run -p skeletree -- stats /path/to/python/repo --limit 20
```

CI runs build + test + clippy + fmt (`.github/workflows/ci.yml`).

### Adding a language

The one extension point is the `Language` trait in `skeletree-lang`:

1. Implement `Language` for the new grammar (see `python.rs`).
2. Drop `.scm` queries whose capture names match the `SymbolKind` /
   `EdgeKind` strings.
3. Register it in `Registry::with_defaults`.
4. Map its file extensions in `Lang::from_extension` (`skeletree-core`).

## License

Apache-2.0
