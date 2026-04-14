use std::{path::PathBuf, sync::Arc};

use clap::Parser;
use rmcp::{transport::stdio, ServiceExt};
use scrapwell_core::{
    index::tantivy_index::TantivySearchIndex, service::MemoryService, store::fs::FsMemoryStore,
    ScrapwellHandler,
};
use serde::Deserialize;

mod cli;

// ---------- CLI ----------

#[derive(Parser)]
#[command(
    name = "scrapwell",
    about = "MCP memory server for LLM agents",
    arg_required_else_help = true
)]
struct Cli {
    /// メモリのルートディレクトリ（デフォルト: ~/.memory）
    #[arg(long, env = "SCRAPWELL_ROOT")]
    root: Option<PathBuf>,

    #[command(subcommand)]
    command: cli::Commands,
}

// ---------- Config ----------

/// ~/.config/scrapwell/config.toml から読み込む設定
#[derive(Deserialize, Default)]
struct Config {
    root: Option<PathBuf>,
}

fn load_config() -> Config {
    let Some(path) = dirs::config_dir().map(|p| p.join("scrapwell").join("config.toml")) else {
        return Config::default();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Config::default();
    };
    toml::from_str(&content).unwrap_or_default()
}

/// root の決定順位: CLI --root > 環境変数 SCRAPWELL_ROOT > config.toml > ~/.memory
fn resolve_root(cli_root: Option<PathBuf>) -> PathBuf {
    if let Some(root) = cli_root {
        return root;
    }
    let config = load_config();
    if let Some(root) = config.root {
        return root;
    }
    dirs::home_dir()
        .expect("cannot determine home directory")
        .join(".memory")
}

// ---------- main ----------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli_args = Cli::parse();
    let root = resolve_root(cli_args.root);

    let store = FsMemoryStore::new(root.clone())?;
    let index = TantivySearchIndex::new(root.join("index"))?;
    let service = Arc::new(MemoryService::new(store, index));

    match cli_args.command {
        cli::Commands::Serve => {
            let handler = ScrapwellHandler::new(Arc::clone(&service));
            handler.serve(stdio()).await?.waiting().await?;
        }
        cli::Commands::Rebuild { target: _ } => {
            eprintln!("Rebuilding index from {:?} ...", root);
            let count = service.rebuild_index()?;
            eprintln!("Done: {} document(s) indexed.", count);
        }
        cli::Commands::Entity { cmd } => {
            cli::run_entity(cmd, service)?;
        }
        cli::Commands::Memory { cmd } => {
            cli::run_memory(cmd, service)?;
        }
    }

    Ok(())
}
