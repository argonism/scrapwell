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
    /// Root directory for memory storage (default: ~/.memory)
    #[arg(long, env = "SCRAPWELL_ROOT")]
    root: Option<PathBuf>,

    #[command(subcommand)]
    command: cli::Commands,
}

// ---------- Config ----------

#[derive(Deserialize, Default, Clone, serde::Serialize)]
struct Config {
    root: Option<PathBuf>,
}

/// user scope path: ~/.config/scrapwell/config.toml
/// Use ~/.config (XDG-style) on all platforms so the documented path works on macOS too,
/// where dirs::config_dir() returns ~/Library/Application Support.
fn user_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|p| p.join(".config").join("scrapwell").join("config.toml"))
}

fn load_user_config() -> Config {
    let Some(path) = user_config_path() else {
        return Config::default();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Config::default();
    };
    toml::from_str(&content).unwrap_or_default()
}

/// Project scope: search for .scrapwell.toml by walking up from the current directory (git-style).
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

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "lowercase")]
enum RootSource {
    Cli,
    Project,
    User,
    Default,
}

impl RootSource {
    fn as_str(self) -> &'static str {
        match self {
            RootSource::Cli => "cli (--root / SCRAPWELL_ROOT)",
            RootSource::Project => "project config (.scrapwell.toml)",
            RootSource::User => "user config (~/.config/scrapwell/config.toml)",
            RootSource::Default => "default (~/.memory)",
        }
    }
}

/// Resolution order for root:
///   1. CLI --root / SCRAPWELL_ROOT env var  (handled by clap)
///   2. project config (.scrapwell.toml searched upward from cwd)
///   3. user config    (~/.config/scrapwell/config.toml)
///   4. default        (~/.memory)
fn resolve_root_with_source(cli_root: Option<PathBuf>) -> (PathBuf, RootSource) {
    if let Some(root) = cli_root {
        return (root, RootSource::Cli);
    }
    if let Some(root) = load_project_config().root {
        return (root, RootSource::Project);
    }
    if let Some(root) = load_user_config().root {
        return (root, RootSource::User);
    }
    let default = dirs::home_dir()
        .expect("cannot determine home directory")
        .join(".memory");
    (default, RootSource::Default)
}

fn resolve_root(cli_root: Option<PathBuf>) -> PathBuf {
    resolve_root_with_source(cli_root).0
}

// ---------- config command ----------

fn run_config_command(
    cli_root: Option<PathBuf>,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let project_path = find_project_config_path();
    let user_path = user_config_path();

    let project_content = project_path
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok());
    let user_content = user_path
        .as_ref()
        .filter(|p| p.is_file())
        .and_then(|p| std::fs::read_to_string(p).ok());

    let project_config: Config = project_content
        .as_deref()
        .and_then(|s| toml::from_str(s).ok())
        .unwrap_or_default();
    let user_config: Config = user_content
        .as_deref()
        .and_then(|s| toml::from_str(s).ok())
        .unwrap_or_default();

    let (resolved_root, source) = resolve_root_with_source(cli_root.clone());

    if json {
        let out = serde_json::json!({
            "sources": {
                "cli": {
                    "root": cli_root,
                },
                "project": {
                    "path": project_path,
                    "exists": project_path.is_some(),
                    "config": project_config,
                },
                "user": {
                    "path": user_path,
                    "exists": user_content.is_some(),
                    "config": user_config,
                },
            },
            "resolved": {
                "root": resolved_root,
                "source": source,
            },
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    println!("Config sources (highest priority first):");
    println!();

    println!("[1] CLI / env (--root, SCRAPWELL_ROOT)");
    match &cli_root {
        Some(p) => println!("    root = {}", p.display()),
        None => println!("    (not set)"),
    }
    println!();

    println!("[2] Project config (.scrapwell.toml, walked up from cwd)");
    match &project_path {
        Some(p) => {
            println!("    path: {}", p.display());
            print_config_content(project_content.as_deref());
        }
        None => println!("    (not found)"),
    }
    println!();

    println!("[3] User config (~/.config/scrapwell/config.toml)");
    match &user_path {
        Some(p) => {
            println!("    path: {}", p.display());
            if user_content.is_some() {
                print_config_content(user_content.as_deref());
            } else {
                println!("    (not found)");
            }
        }
        None => println!("    (cannot determine home directory)"),
    }
    println!();

    println!("[4] Default");
    println!("    root = ~/.memory");
    println!();

    println!("Resolved:");
    println!("    root   = {}", resolved_root.display());
    println!("    source = {}", source.as_str());

    Ok(())
}

fn print_config_content(content: Option<&str>) {
    match content {
        Some(c) if !c.trim().is_empty() => {
            for line in c.lines() {
                println!("    | {}", line);
            }
        }
        Some(_) => println!("    (empty file)"),
        None => println!("    (unreadable)"),
    }
}

// ---------- main ----------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli_args = Cli::parse();

    if let cli::Commands::Config { json } = cli_args.command {
        return run_config_command(cli_args.root, json);
    }

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
        cli::Commands::Search {
            query,
            entity,
            limit,
            json,
        } => {
            cli::run_memory(
                cli::MemoryCmd::Search {
                    query,
                    entity,
                    limit,
                    json,
                },
                service,
            )?;
        }
        cli::Commands::Config { .. } => unreachable!("handled before service init"),
    }

    Ok(())
}
