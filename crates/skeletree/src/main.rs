//! `skeletree` — CLI entry point. Subcommands are stubs until their roadmap
//! step lands; the wiring and arg surface are real so the UX is stable early.

use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use skeletree_engine as engine;
use skeletree_store::Store;

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
        /// Reindex changed files in the background as they change.
        #[arg(long)]
        watch: bool,
    },
    /// Export a ranked map of the repo.
    Export {
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
        Command::Serve { watch: _ } => {
            // ponytail: --watch (background reindex) lands with the watcher step;
            // for now serve reads the existing index.
            let db = engine::default_db_path(&std::env::current_dir()?);
            let store = open_existing(&db)?;
            skeletree_mcp::serve_blocking(store)?;
            Ok(())
        }
        Command::Export { format: _, budget } => todo!("export (budget={budget})"),
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
