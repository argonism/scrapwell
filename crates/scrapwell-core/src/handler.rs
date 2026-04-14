use std::sync::Arc;

use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities},
    schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;

use crate::{
    index::SearchIndex,
    model::{MemoryId, Scope},
    service::MemoryService,
    store::MemoryStore,
};

// ---------- ツールパラメータ型 ----------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CreateEntityParams {
    /// Entity 名。kebab-case（例: "elasticsearch", "my-project"）。小文字・数字・ハイフンのみ。
    name: String,
    /// "knowledge"（汎用的な知識）または "project"（プロジェクト固有の文脈）
    scope: String,
    /// この Entity が何を表すかの説明（任意）
    description: Option<String>,
    /// Entity レベルのタグ（任意）。パス情報（entity 名など）は重複して入れないこと。
    tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SaveMemoryParams {
    /// 保存先 Entity 名。事前に create_entity で作成済みであること。
    entity: String,
    /// ドキュメントのファイル名。vault 全体で一意な kebab-case 文字列（例: "nested-dense-vector"）。
    /// 重複時はエラーになるので、suffix を付けてリトライすること。
    name: String,
    /// ドキュメントのタイトル
    title: String,
    /// Markdown 本文。[[wikilink]] 記法で他ドキュメントへのリンクを張れる。
    content: String,
    /// Topic 名（任意）。同一 Entity 内のドキュメントが ~7 件を超えかつ明確なサブテーマ境界がある場合のみ使用。
    topic: Option<String>,
    /// 横断的タグ（任意）。パス情報は重複させないこと。
    tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetMemoryParams {
    /// ドキュメント ID（ULID 形式）
    id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ListMemoriesParams {
    /// 特定 Entity に絞り込む場合は Entity 名を指定。省略で全 Entity を返す。
    entity: Option<String>,
    /// ツリーの展開深さ。1 = Entity のみ、2 = Entity + Topic（デフォルト: 2）
    depth: Option<u32>,
}

// ---------- Phase 3/4 パラメータ型 ----------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct RebuildIndexParams {}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchMemoryParams {
    /// 検索キーワード。title・content・tags を横断して検索する。
    query: String,
    /// 特定 Entity に絞り込む場合は Entity 名を指定（任意）
    entity: Option<String>,
    /// 最大取得件数（デフォルト: 10）
    limit: Option<usize>,
}

// ---------- Phase 2 パラメータ型 ----------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateEntityParams {
    /// 対象 Entity の ID（ULID 形式）
    id: String,
    /// 新しいスコープ（任意）
    scope: Option<String>,
    /// 新しい説明（任意）
    description: Option<String>,
    /// 新しいタグ（任意、全置換）
    tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DeleteEntityParams {
    /// 対象 Entity の ID（ULID 形式）
    id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateMemoryParams {
    /// 対象ドキュメントの ID（ULID 形式）
    id: String,
    /// 新しいタイトル（任意）
    title: Option<String>,
    /// 新しい本文（任意）
    content: Option<String>,
    /// 新しいタグ（任意、全置換）
    tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DeleteMemoryParams {
    /// 対象ドキュメントの ID（ULID 形式）
    id: String,
}

// ---------- ツリー整形 ----------

fn format_tree(node: &crate::model::TreeNode, is_root: bool) -> String {
    if is_root {
        node.children
            .iter()
            .map(|e| {
                let mut lines =
                    vec![format!("{}/  ({} documents)", e.name, e.document_count)];
                for t in &e.children {
                    lines.push(format!("  {}/  ({} documents)", t.name, t.document_count));
                }
                lines.join("\n")
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        let mut lines =
            vec![format!("{}/  ({} documents)", node.name, node.document_count)];
        for t in &node.children {
            lines.push(format!("  {}/  ({} documents)", t.name, t.document_count));
        }
        lines.join("\n")
    }
}

// ---------- ハンドラー ----------

pub struct ScrapwellHandler<S, I>
where
    S: MemoryStore + 'static,
    I: SearchIndex + 'static,
{
    service: Arc<MemoryService<S, I>>,
    tool_router: ToolRouter<Self>,
}

// Arc<T>: Clone は T: Clone を必要としない。
// ToolRouter<T>: Clone も T: Clone を必要としない（手動実装のため）。
// FsMemoryStore は Clone でないため #[derive(Clone)] は使えない。
impl<S, I> Clone for ScrapwellHandler<S, I>
where
    S: MemoryStore + 'static,
    I: SearchIndex + 'static,
{
    fn clone(&self) -> Self {
        Self {
            service: Arc::clone(&self.service),
            tool_router: self.tool_router.clone(),
        }
    }
}

#[tool_router]
impl<S, I> ScrapwellHandler<S, I>
where
    S: MemoryStore + 'static,
    I: SearchIndex + 'static,
{
    pub fn new(service: Arc<MemoryService<S, I>>) -> Self {
        Self {
            service,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "新しい Entity を作成する。\n\
        Entity は知識の対象（技術・ライブラリ・プロジェクト等）を表す。\n\
        保存前にまず list_memories で既存の Entity 一覧を確認し、\n\
        類似した Entity が既に存在しないか確認すること。\n\
        name は kebab-case（小文字・数字・ハイフンのみ）で指定。\n\
        類似名が存在する場合は error='similar_entity_exists' で候補を返す。\n\
        意図的に別 Entity として作成したい場合は無視してよい。")]
    fn create_entity(
        &self,
        Parameters(p): Parameters<CreateEntityParams>,
    ) -> Result<String, String> {
        let scope = match p.scope.as_str() {
            "knowledge" => Scope::Knowledge,
            "project" => Scope::Project,
            other => {
                return Err(format!(
                    "invalid scope '{}': must be 'knowledge' or 'project'",
                    other
                ))
            }
        };
        match self.service.create_entity(p.name, scope, p.description, p.tags.unwrap_or_default()) {
            Ok(id) => Ok(serde_json::json!({ "id": id.0 }).to_string()),
            Err(crate::error::ScrapwellError::SimilarEntityExists { name, suggestions }) => {
                Err(serde_json::json!({
                    "error": "similar_entity_exists",
                    "message": format!("Entity '{}' is similar to existing entities", name),
                    "suggestions": suggestions,
                }).to_string())
            }
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "既存 Entity の部分更新。指定したフィールドのみ変更される。\n\
        scope・description・tags のうち指定したものだけ更新する。")]
    fn update_entity(
        &self,
        Parameters(p): Parameters<UpdateEntityParams>,
    ) -> Result<String, String> {
        let scope = p
            .scope
            .as_deref()
            .map(|s| match s {
                "knowledge" => Ok(Scope::Knowledge),
                "project" => Ok(Scope::Project),
                other => Err(format!("invalid scope '{}'", other)),
            })
            .transpose()?;

        self.service
            .update_entity(p.id, scope, p.description, p.tags)
            .map(|_| serde_json::json!({ "ok": true }).to_string())
            .map_err(|e| e.to_string())
    }

    #[tool(description = "Entity を削除する。配下のドキュメント・Topic ディレクトリもカスケード削除される。\n\
        この操作は取り消せない。")]
    fn delete_entity(
        &self,
        Parameters(p): Parameters<DeleteEntityParams>,
    ) -> Result<String, String> {
        self.service
            .delete_entity(p.id)
            .map(|_| serde_json::json!({ "ok": true }).to_string())
            .map_err(|e| e.to_string())
    }

    #[tool(description = "ドキュメントをメモリに保存する。\n\
        【保存前に必ず行うこと】\n\
        1. list_memories で既存の Entity・Topic 構造を確認する\n\
        2. 保存先 Entity が存在しない場合は create_entity で先に作成する\n\
        【Topic の使い方】\n\
        同一 Entity 内のドキュメントが ~7 件を超え、かつ明確なサブテーマ境界がある場合のみ topic を指定する。\n\
        1〜2 件しか入らない Topic は作らず Entity 直下に置く。\n\
        【name の重複】\n\
        name は vault 全体で一意でなければならない。重複時はエラーになるので suffix を付けてリトライすること。\n\
        【tags】\n\
        パス情報（entity 名・topic 名）は tags に重複させないこと。横断的な関心事のみ記載する。")]
    fn save_memory(
        &self,
        Parameters(p): Parameters<SaveMemoryParams>,
    ) -> Result<String, String> {
        self.service
            .save_memory(
                p.entity,
                p.name,
                p.title,
                p.content,
                p.topic,
                p.tags.unwrap_or_default(),
            )
            .map(|id| serde_json::json!({ "id": id.0 }).to_string())
            .map_err(|e| e.to_string())
    }

    #[tool(description = "既存ドキュメントの部分更新。指定したフィールドのみ変更される。\n\
        title・content・tags のうち指定したものだけ更新する。")]
    fn update_memory(
        &self,
        Parameters(p): Parameters<UpdateMemoryParams>,
    ) -> Result<String, String> {
        self.service
            .update_memory(p.id, p.title, p.content, p.tags)
            .map(|_| serde_json::json!({ "ok": true }).to_string())
            .map_err(|e| e.to_string())
    }

    #[tool(description = "検索インデックスを全件再構築する。\n\
        Tantivy インデックスが破損した場合や手動でファイルを編集した後に実行する。\n\
        SQLite に記録された全ドキュメントを読み直してインデックスを作り直す。\n\
        完了後に rebuilt（再インデックス件数）を返す。")]
    fn rebuild_index(
        &self,
        Parameters(_): Parameters<RebuildIndexParams>,
    ) -> Result<String, String> {
        self.service
            .rebuild_index()
            .map(|count| serde_json::json!({ "ok": true, "rebuilt": count }).to_string())
            .map_err(|e| e.to_string())
    }

    #[tool(description = "キーワードで全文検索する。\n\
        title・content・tags を横断して検索し、スニペット（<<ハイライト>> 形式）付きで結果を返す。\n\
        entity を指定すると特定の Entity 内に絞り込める。\n\
        検索結果の id を使って get_memory で全内容を取得できる。\n\
        検索結果が 0 件の場合は空配列を返す。")]
    fn search_memory(
        &self,
        Parameters(p): Parameters<SearchMemoryParams>,
    ) -> Result<String, String> {
        self.service
            .search_memory(p.query, p.entity, p.limit.unwrap_or(10))
            .map(|hits| serde_json::to_string(&hits).unwrap_or_default())
            .map_err(|e| e.to_string())
    }

    #[tool(description = "ドキュメントを削除する。Markdown ファイルとインデックスエントリを除去する。\n\
        この操作は取り消せない。")]
    fn delete_memory(
        &self,
        Parameters(p): Parameters<DeleteMemoryParams>,
    ) -> Result<String, String> {
        self.service
            .delete_memory(p.id)
            .map(|_| serde_json::json!({ "ok": true }).to_string())
            .map_err(|e| e.to_string())
    }

    #[tool(description = "ドキュメントの全内容を ID で取得する。\n\
        ID は save_memory の返却値、または list_memories（未実装）で得た ULID 文字列。")]
    fn get_memory(
        &self,
        Parameters(p): Parameters<GetMemoryParams>,
    ) -> Result<String, String> {
        let id = MemoryId(p.id);
        match self.service.get_memory(&id) {
            Ok(Some(entry)) => serde_json::to_string(&entry).map_err(|e| e.to_string()),
            Ok(None) => Err(format!("memory '{}' not found", id)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Entity・Topic のツリー構造を一覧表示する。\n\
        ドキュメントを保存する前に必ずこのツールで既存構造を確認すること。\n\
        entity を省略すると全 Entity の一覧を返す。\n\
        depth=1 で Entity のみ、depth=2（デフォルト）で Topic まで展開する。")]
    fn list_memories(
        &self,
        Parameters(p): Parameters<ListMemoriesParams>,
    ) -> Result<String, String> {
        let depth = p.depth.unwrap_or(2);
        let entity_ref = p.entity.as_deref();
        let is_root = entity_ref.is_none();
        self.service
            .list_memories(entity_ref, depth)
            .map(|tree| format_tree(&tree, is_root))
            .map_err(|e| e.to_string())
    }
}

#[tool_handler]
impl<S, I> ServerHandler for ScrapwellHandler<S, I>
where
    S: MemoryStore + 'static,
    I: SearchIndex + 'static,
{
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("scrapwell", env!("CARGO_PKG_VERSION")))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::Result;
    use rmcp::{
        ClientHandler, ServiceExt,
        model::{CallToolRequestParams, CallToolResult, ClientInfo},
    };
    use tempfile::TempDir;

    use crate::{
        index::noop::NoopSearchIndex,
        service::MemoryService,
        store::fs::FsMemoryStore,
    };
    use super::ScrapwellHandler;

    // ---------- テスト用クライアント ----------

    #[derive(Debug, Clone, Default)]
    struct DummyClient;

    impl ClientHandler for DummyClient {
        fn get_info(&self) -> ClientInfo {
            ClientInfo::default()
        }
    }

    // ---------- ヘルパー ----------

    /// レスポンスの最初のテキストコンテンツを取り出す
    fn text_of(result: &CallToolResult) -> String {
        result
            .content
            .first()
            .and_then(|c| c.raw.as_text())
            .map(|t| t.text.clone())
            .expect("expected text content in result")
    }

    /// インメモリトランスポートでサーバーを起動し、テスト用クライアントを返す。
    /// TempDir は呼び出し元が保持して生存期間を管理する。
    macro_rules! start {
        ($dir:expr) => {{
            let store = FsMemoryStore::new($dir.path().to_path_buf())?;
            let service = Arc::new(MemoryService::new(store, NoopSearchIndex));
            let handler = ScrapwellHandler::new(service);
            let (server_tx, client_tx) = tokio::io::duplex(4096);
            tokio::spawn(async move {
                let _ = handler.serve(server_tx).await.unwrap().waiting().await;
            });
            DummyClient.serve(client_tx).await?
        }};
    }

    /// RunningService は Deref<Target = Peer<RoleClient>> を実装しており、
    /// call_tool は Peer<RoleClient> のメソッド。
    /// このマクロで client.call_tool() を簡潔に呼ぶ。
    macro_rules! tool {
        ($client:expr, $name:expr, $args:expr) => {
            $client
                .call_tool(
                    CallToolRequestParams::new($name)
                        .with_arguments($args.as_object().unwrap().clone()),
                )
                .await?
        };
    }

    // ---------- create_entity ----------

    #[tokio::test]
    async fn create_entity_returns_id() -> Result<()> {
        let dir = TempDir::new()?;
        let client = start!(dir);

        let result = tool!(
            client,
            "create_entity",
            serde_json::json!({"name": "elasticsearch", "scope": "knowledge"})
        );

        assert_ne!(result.is_error, Some(true), "should succeed");
        let json: serde_json::Value = serde_json::from_str(&text_of(&result))?;
        assert!(
            json.get("id").and_then(|v| v.as_str()).is_some(),
            "response should contain id"
        );

        client.cancel().await?;
        Ok(())
    }

    #[tokio::test]
    async fn create_entity_invalid_scope_returns_error() -> Result<()> {
        let dir = TempDir::new()?;
        let client = start!(dir);

        let result = tool!(
            client,
            "create_entity",
            serde_json::json!({"name": "elasticsearch", "scope": "invalid"})
        );

        assert_eq!(result.is_error, Some(true), "should fail on bad scope");
        assert!(text_of(&result).contains("invalid scope"));

        client.cancel().await?;
        Ok(())
    }

    // ---------- save_memory ----------

    #[tokio::test]
    async fn save_memory_returns_id() -> Result<()> {
        let dir = TempDir::new()?;
        let client = start!(dir);

        tool!(client, "create_entity",
            serde_json::json!({"name": "rust", "scope": "knowledge"}));

        let result = tool!(
            client,
            "save_memory",
            serde_json::json!({
                "entity": "rust",
                "name": "anyhow-guide",
                "title": "Anyhow Guide",
                "content": "anyhow の使い方"
            })
        );

        assert_ne!(result.is_error, Some(true), "should succeed");
        let json: serde_json::Value = serde_json::from_str(&text_of(&result))?;
        assert!(json.get("id").and_then(|v| v.as_str()).is_some());

        client.cancel().await?;
        Ok(())
    }

    #[tokio::test]
    async fn save_memory_unknown_entity_returns_error() -> Result<()> {
        let dir = TempDir::new()?;
        let client = start!(dir);

        let result = tool!(
            client,
            "save_memory",
            serde_json::json!({
                "entity": "nonexistent",
                "name": "doc",
                "title": "Title",
                "content": "Content"
            })
        );

        assert_eq!(result.is_error, Some(true));
        assert!(text_of(&result).contains("not found"));

        client.cancel().await?;
        Ok(())
    }

    // ---------- get_memory ----------

    #[tokio::test]
    async fn get_memory_returns_full_entry() -> Result<()> {
        let dir = TempDir::new()?;
        let client = start!(dir);

        tool!(client, "create_entity",
            serde_json::json!({"name": "rust", "scope": "knowledge"}));

        let save_result = tool!(
            client,
            "save_memory",
            serde_json::json!({
                "entity": "rust",
                "name": "tokio-guide",
                "title": "Tokio Guide",
                "content": "tokio の非同期ランタイムについて",
                "topic": "async"
            })
        );
        let saved: serde_json::Value = serde_json::from_str(&text_of(&save_result))?;
        let id = saved["id"].as_str().unwrap().to_string();

        let result = tool!(client, "get_memory", serde_json::json!({"id": id}));

        assert_ne!(result.is_error, Some(true), "should succeed");
        let entry: serde_json::Value = serde_json::from_str(&text_of(&result))?;
        assert_eq!(entry["name"], "tokio-guide");
        assert_eq!(entry["title"], "Tokio Guide");
        assert_eq!(entry["topic"], "async");
        assert!(entry["content"].as_str().unwrap().contains("非同期ランタイム"));

        client.cancel().await?;
        Ok(())
    }

    #[tokio::test]
    async fn get_memory_unknown_id_returns_error() -> Result<()> {
        let dir = TempDir::new()?;
        let client = start!(dir);

        let result = tool!(client, "get_memory", serde_json::json!({"id": "DOESNOTEXIST"}));

        assert_eq!(result.is_error, Some(true));
        assert!(text_of(&result).contains("not found"));

        client.cancel().await?;
        Ok(())
    }

    // ---------- list_memories ----------

    #[tokio::test]
    async fn list_memories_empty_state() -> Result<()> {
        let dir = TempDir::new()?;
        let client = start!(dir);

        let result = tool!(client, "list_memories", serde_json::json!({}));

        assert_ne!(result.is_error, Some(true));
        assert_eq!(text_of(&result), "", "empty vault should return empty string");

        client.cancel().await?;
        Ok(())
    }

    #[tokio::test]
    async fn list_memories_shows_tree() -> Result<()> {
        let dir = TempDir::new()?;
        let client = start!(dir);

        tool!(client, "create_entity",
            serde_json::json!({"name": "elasticsearch", "scope": "knowledge"}));
        tool!(client, "save_memory", serde_json::json!({
            "entity": "elasticsearch",
            "name": "nested-vector",
            "title": "Nested Vector",
            "content": "...",
            "topic": "mapping"
        }));
        tool!(client, "save_memory", serde_json::json!({
            "entity": "elasticsearch",
            "name": "shard-sizing",
            "title": "Shard Sizing",
            "content": "..."
        }));

        let result = tool!(client, "list_memories", serde_json::json!({}));

        assert_ne!(result.is_error, Some(true));
        let text = text_of(&result);
        assert!(text.contains("elasticsearch/"), "entity should appear");
        assert!(text.contains("(2 documents)"), "doc count should be 2");
        assert!(text.contains("mapping/"), "topic should appear");

        client.cancel().await?;
        Ok(())
    }

    // ---------- Phase 2: update_entity ----------

    #[tokio::test]
    async fn update_entity_tool_persists_changes() -> Result<()> {
        let dir = TempDir::new()?;
        let client = start!(dir);

        let create_result = tool!(
            client, "create_entity",
            serde_json::json!({"name": "rust", "scope": "knowledge", "description": "old"})
        );
        let entity: serde_json::Value = serde_json::from_str(&text_of(&create_result))?;
        let id = entity["id"].as_str().unwrap();

        let update_result = tool!(
            client, "update_entity",
            serde_json::json!({"id": id, "scope": "project", "description": "new description"})
        );
        assert_ne!(update_result.is_error, Some(true));

        // list_memories で確認（scope はツリーに出ないが、エラーなしであれば OK）
        let list_result = tool!(client, "list_memories", serde_json::json!({}));
        assert_ne!(list_result.is_error, Some(true));
        assert!(text_of(&list_result).contains("rust/"));

        client.cancel().await?;
        Ok(())
    }

    // ---------- Phase 2: delete_entity ----------

    #[tokio::test]
    async fn delete_entity_tool_removes_entity() -> Result<()> {
        let dir = TempDir::new()?;
        let client = start!(dir);

        let create_result = tool!(
            client, "create_entity",
            serde_json::json!({"name": "rust", "scope": "knowledge"})
        );
        let entity: serde_json::Value = serde_json::from_str(&text_of(&create_result))?;
        let id = entity["id"].as_str().unwrap();

        let delete_result = tool!(
            client, "delete_entity",
            serde_json::json!({"id": id})
        );
        assert_ne!(delete_result.is_error, Some(true));

        // list_memories で消えていることを確認
        let list_result = tool!(client, "list_memories", serde_json::json!({}));
        assert_eq!(text_of(&list_result), "", "entity should be gone");

        client.cancel().await?;
        Ok(())
    }

    // ---------- Phase 2: update_memory ----------

    #[tokio::test]
    async fn update_memory_tool_persists_changes() -> Result<()> {
        let dir = TempDir::new()?;
        let client = start!(dir);

        tool!(client, "create_entity",
            serde_json::json!({"name": "rust", "scope": "knowledge"}));

        let save_result = tool!(client, "save_memory", serde_json::json!({
            "entity": "rust", "name": "anyhow", "title": "Old Title", "content": "Old content"
        }));
        let saved: serde_json::Value = serde_json::from_str(&text_of(&save_result))?;
        let id = saved["id"].as_str().unwrap().to_string();

        let update_result = tool!(client, "update_memory", serde_json::json!({
            "id": id, "title": "New Title", "content": "New content"
        }));
        assert_ne!(update_result.is_error, Some(true));

        // get_memory で更新内容を確認
        let get_result = tool!(client, "get_memory", serde_json::json!({"id": id}));
        let entry: serde_json::Value = serde_json::from_str(&text_of(&get_result))?;
        assert_eq!(entry["title"], "New Title");
        assert_eq!(entry["content"], "New content");

        client.cancel().await?;
        Ok(())
    }

    // ---------- Phase 2: delete_memory ----------

    #[tokio::test]
    async fn delete_memory_tool_removes_document() -> Result<()> {
        let dir = TempDir::new()?;
        let client = start!(dir);

        tool!(client, "create_entity",
            serde_json::json!({"name": "rust", "scope": "knowledge"}));

        let save_result = tool!(client, "save_memory", serde_json::json!({
            "entity": "rust", "name": "anyhow", "title": "Anyhow", "content": "Content"
        }));
        let saved: serde_json::Value = serde_json::from_str(&text_of(&save_result))?;
        let id = saved["id"].as_str().unwrap().to_string();

        let delete_result = tool!(client, "delete_memory", serde_json::json!({"id": id}));
        assert_ne!(delete_result.is_error, Some(true));

        let get_result = tool!(client, "get_memory", serde_json::json!({"id": id}));
        assert_eq!(get_result.is_error, Some(true), "should return error after deletion");

        client.cancel().await?;
        Ok(())
    }

    // ---------- Phase 2: 類似名チェック ----------

    #[tokio::test]
    async fn similar_entity_name_returns_structured_error() -> Result<()> {
        let dir = TempDir::new()?;
        let client = start!(dir);

        tool!(client, "create_entity",
            serde_json::json!({"name": "elasticsearch", "scope": "knowledge"}));

        let result = tool!(client, "create_entity",
            serde_json::json!({"name": "elastic-search", "scope": "knowledge"}));

        assert_eq!(result.is_error, Some(true));
        let json: serde_json::Value = serde_json::from_str(&text_of(&result))?;
        assert_eq!(json["error"], "similar_entity_exists");
        assert!(json["suggestions"].as_array().unwrap().contains(&serde_json::json!("elasticsearch")));

        client.cancel().await?;
        Ok(())
    }
}
