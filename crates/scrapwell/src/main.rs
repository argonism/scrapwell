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

#[derive(Deserialize, Default)]
struct Config {
    root: Option<PathBuf>,
}

impl Config {
    /// self が高優先度、base が低優先度。各 Option フィールドで self の Some を優先する。
    fn merge_over(self, base: Config) -> Config {
        Config {
            root: self.root.or(base.root),
        }
    }
}

/// user スコープ: ~/.config/scrapwell/config.toml
fn load_user_config() -> Config {
    let Some(path) = dirs::config_dir().map(|p| p.join("scrapwell").join("config.toml")) else {
        return Config::default();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Config::default();
    };
    toml::from_str(&content).unwrap_or_default()
}

/// project スコープ: カレントディレクトリから祖先方向に .scrapwell.toml を探す
fn find_project_config_path() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let mut dir = cwd.as_path();
    loop {
        let candidate = dir.join(".scrapwell.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = dir.parent()?;
    }
}

fn load_project_config() -> Config {
    let Some(path) = find_project_config_path() else {
        return Config::default();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Config::default();
    };
    toml::from_str(&content).unwrap_or_default()
}

/// root の決定順位:
///   1. CLI --root / SCRAPWELL_ROOT 環境変数  (clap が処理)
///   2. project config (.scrapwell.toml を祖先方向に検索)
///   3. user config    (~/.config/scrapwell/config.toml)
///   4. デフォルト     (~/.memory)
fn resolve_root(cli_root: Option<PathBuf>) -> PathBuf {
    if let Some(root) = cli_root {
        return root;
    }
    let merged = load_project_config().merge_over(load_user_config());
    if let Some(root) = merged.root {
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
