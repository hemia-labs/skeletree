//! `skeletree` — CLI entry point. Subcommands are stubs until their roadmap
//! step lands; the wiring and arg surface are real so the UX is stable early.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use skeletree_core::Symbol;
use skeletree_engine as engine;
use skeletree_store::Store;

/// Same cap as the MCP tools' `MAX_ROWS` — plenty of candidates for the
/// budget trim below to work with.
const MAX_ROWS: usize = 500;

#[derive(Parser)]
#[command(
    name = "skeletree",
    version,
    about = "Queryable graph of your repo for AI agents"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Index a repository into `.skeletree/index.db`.
    Index {
        /// Repo root to index.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Run the MCP server over stdio.
    Serve {
        /// Repo root whose index to serve. Defaults to the current directory —
        /// pass an absolute path when the launcher (e.g. Claude Desktop) can't
        /// set the working directory to your repo.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Reindex changed files in the background as they change.
        #[arg(long)]
        watch: bool,
    },
    /// Export a ranked map of the repo.
    Export {
        /// Repo root whose index to read.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value_t = ExportFormat::Md)]
        format: ExportFormat,
        /// Token budget for the exported map.
        #[arg(long, default_value_t = 2000)]
        budget: usize,
    },
    /// Show graph stats: the most central symbols by rank.
    Stats {
        /// Repo root whose index to read.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// How many top symbols to show.
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// One-command setup: index + generate MCP config + git hook.
    Init,
}

#[derive(Copy, Clone, ValueEnum)]
enum ExportFormat {
    Md,
    Json,
    Mermaid,
}

/// Startup banner. To stderr, never stdout — `serve` speaks MCP over stdout
/// and `stats`/`export` are pipeable; the banner must not pollute either.
const BANNER: &str = r"
   _____ _        _      _
  / ____| |      | |    | |
 | (___ | | _____| | ___| |_ _ __ ___  ___
  \___ \| |/ / _ \ |/ _ \ __| '__/ _ \/ _ \
  ____) |   <  __/ |  __/ |_| | |  __/  __/
 |_____/|_|\_\___|_|\___|\__|_|  \___|\___|
";

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    eprintln!("{BANNER}");

    let cli = Cli::parse();
    match cli.command {
        Command::Index { path } => {
            let db = engine::default_db_path(&path);
            if let Some(dir) = db.parent() {
                std::fs::create_dir_all(dir)?;
                // Self-ignoring dir: `*` keeps the binary index out of the
                // user's repo without touching their root .gitignore. Write
                // once; don't clobber if they've customized it.
                let ignore = dir.join(".gitignore");
                if !ignore.exists() {
                    std::fs::write(&ignore, "*\n")?;
                }
            }
            let mut store = Store::open(&db)?;
            let stats = engine::index(&path, &mut store)?;
            println!(
                "indexed {} files, {} symbols ({} skipped) → {}",
                stats.files,
                stats.symbols,
                stats.skipped,
                db.display()
            );
            Ok(())
        }
        Command::Serve { path, watch: _ } => {
            // ponytail: --watch (background reindex) lands with the watcher step;
            // for now serve reads the existing index.
            let db = engine::default_db_path(&path);
            let store = open_existing(&db)?;
            skeletree_mcp::serve_blocking(store)?;
            Ok(())
        }
        Command::Export {
            path,
            format,
            budget,
        } => {
            let store = open_existing(&engine::default_db_path(&path))?;
            match format {
                ExportFormat::Md => print!("{}", skeletree_mcp::tools::overview(&store, budget)?),
                ExportFormat::Json => print!("{}", export_json(&store, budget)?),
                ExportFormat::Mermaid => print!("{}", export_mermaid(&store, budget)?),
            }
            Ok(())
        }
        Command::Stats { path, limit } => {
            let store = open_existing(&engine::default_db_path(&path))?;
            for sym in store.top_symbols(limit)? {
                println!("{:>8.4}  {:<9} {}", sym.rank, sym.kind, sym.name);
            }
            Ok(())
        }
        Command::Init => todo!("init"),
    }
}

/// Open an index that must already exist. Read commands (`stats`, `serve`)
/// need a populated db; opening blindly would create an empty one and report
/// nothing, so bail with a pointer to `index` instead.
fn open_existing(db: &Path) -> Result<Store> {
    if !db.exists() {
        anyhow::bail!("no index at {} — run `skeletree index` first", db.display());
    }
    Ok(Store::open(db)?)
}

/// One exported symbol, kept flat and small — same fields the text tools show.
#[derive(Serialize)]
struct ExportSymbol {
    kind: String,
    name: String,
    location: String,
    rank: f64,
    signature: Option<String>,
}

impl From<&Symbol> for ExportSymbol {
    fn from(s: &Symbol) -> Self {
        ExportSymbol {
            kind: s.kind.to_string(),
            name: s.name.clone(),
            location: format!("{}:{}", s.file_path, s.span.start_line),
            rank: s.rank,
            signature: s.signature.clone(),
        }
    }
}

/// Top-ranked symbols serialized as a JSON array, trimmed to `token_budget`
/// (~4 chars/token, same estimate the MCP tools use).
fn export_json(store: &Store, token_budget: usize) -> Result<String> {
    let budget_chars = token_budget * 4;
    let mut items = Vec::new();
    let mut used_chars = 2; // "[]"
    for sym in store.top_symbols(MAX_ROWS)? {
        let item = ExportSymbol::from(&sym);
        let cost = serde_json::to_string(&item)?.len() + 1; // + comma/newline
        if used_chars + cost > budget_chars && !items.is_empty() {
            break;
        }
        used_chars += cost;
        items.push(item);
    }
    Ok(serde_json::to_string_pretty(&items)? + "\n")
}

/// Top-ranked symbols as a Mermaid `graph TD`, edges limited to pairs where
/// both ends made the cut.
fn export_mermaid(store: &Store, token_budget: usize) -> Result<String> {
    let budget_chars = token_budget * 4;
    let mut symbols = Vec::new();
    let mut used_chars = "graph TD\n".len();
    for sym in store.top_symbols(MAX_ROWS)? {
        let label = format!(
            "    s{}[\"{} {}\"]\n",
            sym.id.0,
            sym.kind,
            sym.name.replace('"', "'")
        );
        if used_chars + label.len() > budget_chars && !symbols.is_empty() {
            break;
        }
        used_chars += label.len();
        symbols.push((sym.id, label));
    }

    let mut out = String::from("graph TD\n");
    let ids: HashSet<_> = symbols.iter().map(|(id, _)| *id).collect();
    for (_, label) in &symbols {
        out.push_str(label);
    }
    for (src, dst) in store.list_edges()? {
        if ids.contains(&src) && ids.contains(&dst) {
            out.push_str(&format!("    s{} --> s{}\n", src.0, dst.0));
        }
    }
    Ok(out)
}
