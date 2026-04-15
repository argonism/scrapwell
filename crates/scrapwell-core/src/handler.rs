use std::sync::Arc;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities},
    schemars, tool, tool_handler, tool_router, ServerHandler,
};
use serde::Deserialize;

use crate::{
    index::SearchIndex,
    model::{MemoryId, Scope},
    service::MemoryService,
    store::MemoryStore,
};

// ---------- Tool parameter types ----------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CreateEntityParams {
    /// Entity name. kebab-case (e.g. "elasticsearch", "my-project"). Lowercase letters, digits, and hyphens only.
    name: String,
    /// "knowledge" (general reusable knowledge) or "project" (project-specific context)
    scope: String,
    /// Description of what this Entity represents (optional)
    description: Option<String>,
    /// Entity-level tags (optional). Do not duplicate path information (e.g. the entity name itself).
    tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SaveMemoryParams {
    /// Target Entity name. Must already exist (create with create_entity first).
    entity: String,
    /// Document filename. Must be unique across the entire vault; kebab-case (e.g. "nested-dense-vector").
    /// On conflict, append a suffix and retry.
    name: String,
    /// Document title
    title: String,
    /// Markdown body. Use [[wikilink]] syntax to link to other documents.
    content: String,
    /// Topic name (optional). Use only when the Entity has ~7+ documents and a clear sub-theme boundary exists.
    topic: Option<String>,
    /// Cross-cutting tags (optional). Do not duplicate path information.
    tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetMemoryParams {
    /// Document ID (ULID)
    id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ListMemoriesParams {
    /// Filter by Entity name. Omit to return all Entities.
    entity: Option<String>,
    /// Tree expansion depth. 1 = Entities only, 2 = Entities + Topics (default: 2)
    depth: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct RebuildIndexParams {}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchMemoryParams {
    /// Search query. Searches across title, content, and tags.
    query: String,
    /// Filter by Entity name (optional)
    entity: Option<String>,
    /// Maximum number of results (default: 10)
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateEntityParams {
    /// Target Entity ID (ULID)
    id: String,
    /// New scope (optional)
    scope: Option<String>,
    /// New description (optional)
    description: Option<String>,
    /// New tags (optional, full replacement)
    tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DeleteEntityParams {
    /// Target Entity ID (ULID)
    id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateMemoryParams {
    /// Target document ID (ULID)
    id: String,
    /// New title (optional)
    title: Option<String>,
    /// New body (optional)
    content: Option<String>,
    /// New tags (optional, full replacement)
    tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DeleteMemoryParams {
    /// Target document ID (ULID)
    id: String,
}

// ---------- ツリー整形 ----------

fn format_tree(node: &crate::model::TreeNode, is_root: bool) -> String {
    if is_root {
        node.children
            .iter()
            .map(|e| {
                let mut lines = vec![format!("{}/  ({} documents)", e.name, e.document_count)];
                for t in &e.children {
                    lines.push(format!("  {}/  ({} documents)", t.name, t.document_count));
                }
                lines.join("\n")
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        let mut lines = vec![format!(
            "{}/  ({} documents)",
            node.name, node.document_count
        )];
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

    #[tool(description = "Create a new Entity.\n\
        An Entity represents the subject of knowledge (technology, library, project, etc.).\n\
        Before creating, call list_memories to check for existing Entities and avoid duplicates.\n\
        name must be kebab-case (lowercase letters, digits, and hyphens only).\n\
        If a similar name exists, returns error='similar_entity_exists' with suggestions.\n\
        You may ignore the warning if you intentionally want a separate Entity.")]
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
        match self
            .service
            .create_entity(p.name, scope, p.description, p.tags.unwrap_or_default())
        {
            Ok(id) => Ok(serde_json::json!({ "id": id.0 }).to_string()),
            Err(crate::error::ScrapwellError::SimilarEntityExists { name, suggestions }) => {
                Err(serde_json::json!({
                    "error": "similar_entity_exists",
                    "message": format!("Entity '{}' is similar to existing entities", name),
                    "suggestions": suggestions,
                })
                .to_string())
            }
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Partially update an existing Entity. Only the specified fields are changed.\n\
        Updates scope, description, and/or tags — only the fields you provide."
    )]
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

    #[tool(
        description = "Delete an Entity. All documents and Topic directories under it are cascade-deleted.\n\
        This operation cannot be undone."
    )]
    fn delete_entity(
        &self,
        Parameters(p): Parameters<DeleteEntityParams>,
    ) -> Result<String, String> {
        self.service
            .delete_entity(p.id)
            .map(|_| serde_json::json!({ "ok": true }).to_string())
            .map_err(|e| e.to_string())
    }

    #[tool(description = "Save a document to memory.\n\
        Write the title and content in the same language the user is using.\n\
        [Before saving]\n\
        1. Call list_memories to check the existing Entity/Topic structure.\n\
        2. If the target Entity does not exist, create it first with create_entity.\n\
        [Topics]\n\
        Only use a topic when the Entity has ~7+ documents and a clear sub-theme boundary exists.\n\
        If only 1-2 documents would go under a topic, place them directly under the Entity instead.\n\
        [name uniqueness]\n\
        name must be unique across the entire vault. On conflict, append a suffix and retry.\n\
        [tags]\n\
        Do not duplicate path information (entity name, topic name) in tags. Only add cross-cutting concerns.")]
    fn save_memory(&self, Parameters(p): Parameters<SaveMemoryParams>) -> Result<String, String> {
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

    #[tool(
        description = "Partially update an existing document. Only the specified fields are changed.\n\
        Updates title, content, and/or tags — only the fields you provide."
    )]
    fn update_memory(
        &self,
        Parameters(p): Parameters<UpdateMemoryParams>,
    ) -> Result<String, String> {
        self.service
            .update_memory(p.id, p.title, p.content, p.tags)
            .map(|_| serde_json::json!({ "ok": true }).to_string())
            .map_err(|e| e.to_string())
    }

    #[tool(description = "Rebuild the search index from scratch.\n\
        Run this when the Tantivy index is corrupted or after manually editing Markdown files.\n\
        Re-reads all documents recorded in SQLite and rebuilds the index.\n\
        Returns the number of documents reindexed.")]
    fn rebuild_index(
        &self,
        Parameters(_): Parameters<RebuildIndexParams>,
    ) -> Result<String, String> {
        self.service
            .rebuild_index()
            .map(|count| serde_json::json!({ "ok": true, "rebuilt": count }).to_string())
            .map_err(|e| e.to_string())
    }

    #[tool(description = "Full-text search by keyword.\n\
        Searches across title, content, and tags; returns results with highlighted snippets (<<highlight>> format).\n\
        Optionally filter to a specific Entity with the entity parameter.\n\
        Use the id from results with get_memory to fetch the full document.\n\
        Returns an empty array when no results are found.")]
    fn search_memory(
        &self,
        Parameters(p): Parameters<SearchMemoryParams>,
    ) -> Result<String, String> {
        self.service
            .search_memory(p.query, p.entity, p.limit.unwrap_or(10))
            .map(|hits| serde_json::to_string(&hits).unwrap_or_default())
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Delete a document. Removes the Markdown file and its index entry.\n\
        This operation cannot be undone."
    )]
    fn delete_memory(
        &self,
        Parameters(p): Parameters<DeleteMemoryParams>,
    ) -> Result<String, String> {
        self.service
            .delete_memory(p.id)
            .map(|_| serde_json::json!({ "ok": true }).to_string())
            .map_err(|e| e.to_string())
    }

    #[tool(description = "Fetch the full content of a document by ID.\n\
        The ID is the ULID returned by save_memory or obtained from search_memory results.")]
    fn get_memory(&self, Parameters(p): Parameters<GetMemoryParams>) -> Result<String, String> {
        let id = MemoryId(p.id);
        match self.service.get_memory(&id) {
            Ok(Some(entry)) => serde_json::to_string(&entry).map_err(|e| e.to_string()),
            Ok(None) => Err(format!("memory '{}' not found", id)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "List the Entity/Topic tree structure.\n\
        Always call this before saving a document to check the existing structure.\n\
        Omit entity to return all Entities.\n\
        depth=1 returns Entities only; depth=2 (default) also expands Topics.")]
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
        model::{CallToolRequestParams, CallToolResult, ClientInfo},
        ClientHandler, ServiceExt,
    };
    use tempfile::TempDir;

    use super::ScrapwellHandler;
    use crate::{index::noop::NoopSearchIndex, service::MemoryService, store::fs::FsMemoryStore};

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

        tool!(
            client,
            "create_entity",
            serde_json::json!({"name": "rust", "scope": "knowledge"})
        );

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

        tool!(
            client,
            "create_entity",
            serde_json::json!({"name": "rust", "scope": "knowledge"})
        );

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
        assert!(entry["content"]
            .as_str()
            .unwrap()
            .contains("非同期ランタイム"));

        client.cancel().await?;
        Ok(())
    }

    #[tokio::test]
    async fn get_memory_unknown_id_returns_error() -> Result<()> {
        let dir = TempDir::new()?;
        let client = start!(dir);

        let result = tool!(
            client,
            "get_memory",
            serde_json::json!({"id": "DOESNOTEXIST"})
        );

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
        assert_eq!(
            text_of(&result),
            "",
            "empty vault should return empty string"
        );

        client.cancel().await?;
        Ok(())
    }

    #[tokio::test]
    async fn list_memories_shows_tree() -> Result<()> {
        let dir = TempDir::new()?;
        let client = start!(dir);

        tool!(
            client,
            "create_entity",
            serde_json::json!({"name": "elasticsearch", "scope": "knowledge"})
        );
        tool!(
            client,
            "save_memory",
            serde_json::json!({
                "entity": "elasticsearch",
                "name": "nested-vector",
                "title": "Nested Vector",
                "content": "...",
                "topic": "mapping"
            })
        );
        tool!(
            client,
            "save_memory",
            serde_json::json!({
                "entity": "elasticsearch",
                "name": "shard-sizing",
                "title": "Shard Sizing",
                "content": "..."
            })
        );

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
            client,
            "create_entity",
            serde_json::json!({"name": "rust", "scope": "knowledge", "description": "old"})
        );
        let entity: serde_json::Value = serde_json::from_str(&text_of(&create_result))?;
        let id = entity["id"].as_str().unwrap();

        let update_result = tool!(
            client,
            "update_entity",
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
            client,
            "create_entity",
            serde_json::json!({"name": "rust", "scope": "knowledge"})
        );
        let entity: serde_json::Value = serde_json::from_str(&text_of(&create_result))?;
        let id = entity["id"].as_str().unwrap();

        let delete_result = tool!(client, "delete_entity", serde_json::json!({"id": id}));
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

        tool!(
            client,
            "create_entity",
            serde_json::json!({"name": "rust", "scope": "knowledge"})
        );

        let save_result = tool!(
            client,
            "save_memory",
            serde_json::json!({
                "entity": "rust", "name": "anyhow", "title": "Old Title", "content": "Old content"
            })
        );
        let saved: serde_json::Value = serde_json::from_str(&text_of(&save_result))?;
        let id = saved["id"].as_str().unwrap().to_string();

        let update_result = tool!(
            client,
            "update_memory",
            serde_json::json!({
                "id": id, "title": "New Title", "content": "New content"
            })
        );
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

        tool!(
            client,
            "create_entity",
            serde_json::json!({"name": "rust", "scope": "knowledge"})
        );

        let save_result = tool!(
            client,
            "save_memory",
            serde_json::json!({
                "entity": "rust", "name": "anyhow", "title": "Anyhow", "content": "Content"
            })
        );
        let saved: serde_json::Value = serde_json::from_str(&text_of(&save_result))?;
        let id = saved["id"].as_str().unwrap().to_string();

        let delete_result = tool!(client, "delete_memory", serde_json::json!({"id": id}));
        assert_ne!(delete_result.is_error, Some(true));

        let get_result = tool!(client, "get_memory", serde_json::json!({"id": id}));
        assert_eq!(
            get_result.is_error,
            Some(true),
            "should return error after deletion"
        );

        client.cancel().await?;
        Ok(())
    }

    // ---------- Phase 2: 類似名チェック ----------

    #[tokio::test]
    async fn similar_entity_name_returns_structured_error() -> Result<()> {
        let dir = TempDir::new()?;
        let client = start!(dir);

        tool!(
            client,
            "create_entity",
            serde_json::json!({"name": "elasticsearch", "scope": "knowledge"})
        );

        let result = tool!(
            client,
            "create_entity",
            serde_json::json!({"name": "elastic-search", "scope": "knowledge"})
        );

        assert_eq!(result.is_error, Some(true));
        let json: serde_json::Value = serde_json::from_str(&text_of(&result))?;
        assert_eq!(json["error"], "similar_entity_exists");
        assert!(json["suggestions"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("elasticsearch")));

        client.cancel().await?;
        Ok(())
    }
}
