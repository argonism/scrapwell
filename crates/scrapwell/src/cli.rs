use std::{io::Read, path::PathBuf, sync::Arc};

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use scrapwell_core::{
    index::SearchIndex,
    model::{MemoryEntry, MemoryId, Scope, TreeNode},
    service::MemoryService,
    store::MemoryStore,
    ScrapwellError,
};

// ---------- コマンド定義 ----------

#[derive(Subcommand)]
pub enum Commands {
    /// MCPサーバーとして起動する (stdio transport)
    Serve,
    /// 全文検索インデックスを再構築する
    Rebuild {
        /// 再構築対象: all | metadata | search
        #[arg(long, default_value = "all")]
        target: String,
    },
    /// Entity の管理
    Entity {
        #[command(subcommand)]
        cmd: EntityCmd,
    },
    /// ドキュメント（メモリ）の管理
    Memory {
        #[command(subcommand)]
        cmd: MemoryCmd,
    },
}

#[derive(Subcommand)]
pub enum EntityCmd {
    /// Entity 一覧を表示する
    List {
        /// JSON 形式で出力する
        #[arg(long)]
        json: bool,
    },
    /// 新しい Entity を作成する
    Create {
        /// Entity 名 (kebab-case, ASCII 英数字・ハイフンのみ)
        name: String,
        /// スコープ: knowledge (汎用) | project (プロジェクト固有)
        #[arg(long, default_value = "knowledge")]
        scope: String,
        /// 説明
        #[arg(long)]
        desc: Option<String>,
        /// タグ (--tag foo --tag bar のように複数指定可)
        #[arg(long = "tag")]
        tags: Vec<String>,
    },
    /// Entity を削除する (配下のドキュメントもすべて削除される)
    Delete {
        /// Entity ID (ULID)
        id: String,
        /// 確認プロンプトをスキップする
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum MemoryCmd {
    /// ドキュメント一覧をツリー形式で表示する
    List {
        /// 特定 Entity に絞り込む
        #[arg(long)]
        entity: Option<String>,
        /// JSON 形式で出力する
        #[arg(long)]
        json: bool,
    },
    /// キーワードで全文検索する
    Search {
        /// 検索クエリ
        query: String,
        /// 特定 Entity に絞り込む
        #[arg(long)]
        entity: Option<String>,
        /// 最大取得件数
        #[arg(long, default_value = "10")]
        limit: usize,
        /// JSON 形式で出力する
        #[arg(long)]
        json: bool,
    },
    /// ドキュメントの全内容を取得する
    Get {
        /// ドキュメント ID (ULID)
        id: String,
        /// JSON 形式で出力する
        #[arg(long)]
        json: bool,
    },
    /// ドキュメントを保存する (本文は --content / --file / stdin のいずれかで指定)
    Save {
        /// 保存先 Entity 名
        #[arg(long)]
        entity: String,
        /// ファイル名 (vault 全体でユニーク, kebab-case)
        #[arg(long)]
        name: String,
        /// ドキュメントのタイトル
        #[arg(long)]
        title: String,
        /// 本文文字列 (--file / stdin と排他)
        #[arg(long, conflicts_with = "file")]
        content: Option<String>,
        /// 本文を読み込むファイルパス (--content / stdin と排他)
        #[arg(long, conflicts_with = "content")]
        file: Option<PathBuf>,
        /// Topic 名 (任意)
        #[arg(long)]
        topic: Option<String>,
        /// タグ (--tag foo --tag bar のように複数指定可)
        #[arg(long = "tag")]
        tags: Vec<String>,
    },
    /// ドキュメントを削除する
    Delete {
        /// ドキュメント ID (ULID)
        id: String,
        /// 確認プロンプトをスキップする
        #[arg(long)]
        yes: bool,
    },
}

// ---------- ディスパッチ ----------

pub fn run_entity<S: MemoryStore, I: SearchIndex>(
    cmd: EntityCmd,
    service: Arc<MemoryService<S, I>>,
) -> Result<()> {
    match cmd {
        EntityCmd::List { json } => {
            let entities = service.list_entities()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&entities)?);
            } else if entities.is_empty() {
                println!("No entities found.");
            } else {
                println!("{:<28} {:<24} {}", "ID", "NAME", "SCOPE");
                println!("{}", "-".repeat(60));
                for e in &entities {
                    println!("{:<28} {:<24} {}", e.id, e.name, scope_str(e.scope));
                }
            }
        }
        EntityCmd::Create { name, scope, desc, tags } => {
            let scope = parse_scope(&scope)?;
            match service.create_entity(name.clone(), scope, desc, tags) {
                Ok(id) => println!("Created entity '{}' (id: {})", name, id),
                Err(ScrapwellError::SimilarEntityExists { suggestions, .. }) => {
                    bail!(
                        "similar entity already exists: {}\n\
                         Use a distinct name, or check with `entity list`.",
                        suggestions.join(", ")
                    );
                }
                Err(e) => return Err(e).context(format!("failed to create entity '{}'", name)),
            }
        }
        EntityCmd::Delete { id, yes } => {
            if !yes {
                confirm(&format!(
                    "Delete entity '{}'? All documents under it will also be deleted.",
                    id
                ))?;
            }
            service
                .delete_entity(id.clone())
                .with_context(|| format!("failed to delete entity '{}'", id))?;
            println!("Deleted.");
        }
    }
    Ok(())
}

pub fn run_memory<S: MemoryStore, I: SearchIndex>(
    cmd: MemoryCmd,
    service: Arc<MemoryService<S, I>>,
) -> Result<()> {
    match cmd {
        MemoryCmd::List { entity, json } => {
            let tree = service.list_memories(entity.as_deref(), 2)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&tree)?);
            } else {
                let text = format_tree(&tree);
                if text.is_empty() {
                    println!("No documents found.");
                } else {
                    println!("{}", text);
                }
            }
        }
        MemoryCmd::Search { query, entity, limit, json } => {
            let hits = service.search_memory(query, entity, limit)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&hits)?);
            } else if hits.is_empty() {
                println!("No results found.");
            } else {
                for hit in &hits {
                    let path = match &hit.topic {
                        Some(t) => format!("{}/{}", hit.entity, t),
                        None => hit.entity.clone(),
                    };
                    println!("[{:.2}] {}  ({})", hit.score, hit.name, path);
                    for snippet in &hit.snippets {
                        println!("       {}", snippet);
                    }
                }
            }
        }
        MemoryCmd::Get { id, json } => {
            let memory_id = MemoryId(id.clone());
            match service.get_memory(&memory_id)? {
                Some(entry) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&entry)?);
                    } else {
                        print_entry(&entry);
                    }
                }
                None => bail!("document '{}' not found", id),
            }
        }
        MemoryCmd::Save { entity, name, title, content, file, topic, tags } => {
            let body = resolve_content(content, file)?;
            let id = service
                .save_memory(entity.clone(), name.clone(), title, body, topic, tags)
                .with_context(|| format!("failed to save '{}' in '{}'", name, entity))?;
            println!("Saved document '{}' (id: {})", name, id);
        }
        MemoryCmd::Delete { id, yes } => {
            if !yes {
                confirm(&format!("Delete document '{}'?", id))?;
            }
            service
                .delete_memory(id.clone())
                .with_context(|| format!("failed to delete document '{}'", id))?;
            println!("Deleted.");
        }
    }
    Ok(())
}

// ---------- ヘルパー ----------

fn parse_scope(s: &str) -> Result<Scope> {
    match s {
        "knowledge" => Ok(Scope::Knowledge),
        "project" => Ok(Scope::Project),
        other => bail!("invalid scope '{}': must be 'knowledge' or 'project'", other),
    }
}

fn scope_str(scope: Scope) -> &'static str {
    match scope {
        Scope::Knowledge => "knowledge",
        Scope::Project => "project",
    }
}

fn confirm(prompt: &str) -> Result<()> {
    eprint!("{} [y/N] ", prompt);
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    if !input.trim().eq_ignore_ascii_case("y") {
        bail!("Aborted.");
    }
    Ok(())
}

fn resolve_content(content: Option<String>, file: Option<PathBuf>) -> Result<String> {
    if let Some(c) = content {
        return Ok(c);
    }
    if let Some(path) = file {
        return std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read '{}'", path.display()));
    }
    use std::io::IsTerminal;
    if std::io::stdin().is_terminal() {
        eprintln!("Reading content from stdin (Ctrl+D to finish):");
    }
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}

fn format_tree(root: &TreeNode) -> String {
    root.children
        .iter()
        .map(|entity| {
            let mut lines =
                vec![format!("{}/  ({} documents)", entity.name, entity.document_count)];
            for topic in &entity.children {
                lines.push(format!(
                    "  {}/  ({} documents)",
                    topic.name, topic.document_count
                ));
            }
            lines.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn print_entry(entry: &MemoryEntry) {
    let path = match &entry.topic {
        Some(t) => format!("{}/{}/{}", entry.entity, t, entry.name),
        None => format!("{}/{}", entry.entity, entry.name),
    };
    println!("# {}", entry.title);
    println!("path:  {}", path);
    println!("id:    {}", entry.id);
    if !entry.tags.is_empty() {
        println!("tags:  {}", entry.tags.join(", "));
    }
    println!();
    println!("{}", entry.content);
}
