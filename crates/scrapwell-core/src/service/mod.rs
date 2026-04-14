use chrono::Utc;

use crate::{
    error::{Result, ScrapwellError},
    index::SearchIndex,
    model::{
        EntityMeta, EntityPatch, MemoryEntry, MemoryId, MemoryPatch, Scope, SearchHit, SearchQuery,
        TreeNode,
    },
    store::MemoryStore,
};

pub struct MemoryService<S, I>
where
    S: MemoryStore,
    I: SearchIndex,
{
    store: S,
    index: I,
}

impl<S: MemoryStore, I: SearchIndex> MemoryService<S, I> {
    pub fn new(store: S, index: I) -> Self {
        Self { store, index }
    }

    pub fn create_entity(
        &self,
        name: String,
        scope: Scope,
        description: Option<String>,
        tags: Vec<String>,
    ) -> Result<MemoryId> {
        // 類似名チェック
        let existing = self.store.list_entity_names()?;
        let similar: Vec<String> = existing
            .iter()
            .filter(|n| strsim::jaro_winkler(n.as_str(), name.as_str()) > 0.85)
            .cloned()
            .collect();
        if !similar.is_empty() {
            return Err(ScrapwellError::SimilarEntityExists {
                name: name.clone(),
                suggestions: similar,
            });
        }

        let now = Utc::now();
        let entity = EntityMeta {
            id: MemoryId::new(),
            name,
            scope,
            description,
            tags,
            created_at: now,
            updated_at: now,
        };
        self.store.save_entity(&entity)?;
        Ok(entity.id)
    }

    pub fn update_entity(
        &self,
        id: String,
        scope: Option<Scope>,
        description: Option<String>,
        tags: Option<Vec<String>>,
    ) -> Result<()> {
        let patch = EntityPatch {
            scope,
            description,
            tags,
        };
        self.store.update_entity(&MemoryId(id), &patch)
    }

    pub fn delete_entity(&self, id: String) -> Result<()> {
        self.store.delete_entity(&MemoryId(id))
    }

    pub fn save_memory(
        &self,
        entity_name: String,
        name: String,
        title: String,
        content: String,
        topic: Option<String>,
        tags: Vec<String>,
    ) -> Result<MemoryId> {
        // Entity が存在するか確認
        self.store
            .get_entity_by_name(&entity_name)?
            .ok_or_else(|| ScrapwellError::NotFound(format!("entity '{}'", entity_name)))?;

        // ファイル名がvault全体で一意か確認
        if !self.store.check_name_unique(&name)? {
            return Err(ScrapwellError::DuplicateName(name));
        }

        let now = Utc::now();
        let entry = MemoryEntry {
            id: MemoryId::new(),
            entity: entity_name,
            topic,
            name,
            title,
            content,
            tags,
            created_at: now,
            updated_at: now,
        };

        self.store.save(&entry)?;
        self.index.upsert(&entry)?;

        Ok(entry.id)
    }

    pub fn update_memory(
        &self,
        id: String,
        title: Option<String>,
        content: Option<String>,
        tags: Option<Vec<String>>,
    ) -> Result<()> {
        let memory_id = MemoryId(id);
        let patch = MemoryPatch {
            title,
            content,
            tags,
        };
        self.store.update(&memory_id, &patch)?;
        // 更新後のエントリで検索インデックスを再構築（Phase 3 で Tantivy に差し替え）
        if let Some(entry) = self.store.get(&memory_id)? {
            self.index.upsert(&entry)?;
        }
        Ok(())
    }

    pub fn delete_memory(&self, id: String) -> Result<()> {
        let memory_id = MemoryId(id);
        self.index.remove(&memory_id)?;
        self.store.delete(&memory_id)?;
        Ok(())
    }

    pub fn get_memory(&self, id: &MemoryId) -> Result<Option<MemoryEntry>> {
        self.store.get(id)
    }

    pub fn list_memories(&self, entity: Option<&str>, depth: u32) -> Result<TreeNode> {
        self.store.list_tree(entity, depth)
    }

    pub fn search_memory(
        &self,
        query: String,
        entity: Option<String>,
        limit: usize,
    ) -> Result<Vec<SearchHit>> {
        self.index.search(&SearchQuery {
            query,
            entity,
            limit,
        })
    }

    pub fn rebuild_index(&self) -> Result<usize> {
        let entries = self.store.iter_all()?;
        let count = entries.len();
        self.index.rebuild(&mut entries.into_iter())?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{error::ScrapwellError, index::noop::NoopSearchIndex, store::fs::FsMemoryStore};
    use tempfile::TempDir;

    // ---------- ヘルパー ----------

    fn make_service() -> (MemoryService<FsMemoryStore, NoopSearchIndex>, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = FsMemoryStore::new(dir.path().to_path_buf()).unwrap();
        let service = MemoryService::new(store, NoopSearchIndex);
        (service, dir)
    }

    // ---------- Phase 1: ハッピーパス ----------

    #[test]
    fn create_entity_save_memory_and_get_roundtrip() {
        let (svc, _dir) = make_service();

        svc.create_entity(
            "elasticsearch".to_string(),
            Scope::Knowledge,
            Some("About Elasticsearch".to_string()),
            vec!["search-engine".to_string()],
        )
        .unwrap();

        let id = svc
            .save_memory(
                "elasticsearch".to_string(),
                "nested-dense-vector".to_string(),
                "Nested + Dense Vector".to_string(),
                "Content about nested dense vector".to_string(),
                Some("mapping".to_string()),
                vec!["performance".to_string()],
            )
            .unwrap();

        let entry = svc.get_memory(&id).unwrap().unwrap();
        assert_eq!(entry.entity, "elasticsearch");
        assert_eq!(entry.name, "nested-dense-vector");
        assert_eq!(entry.title, "Nested + Dense Vector");
        assert_eq!(entry.content, "Content about nested dense vector");
        assert_eq!(entry.topic, Some("mapping".to_string()));
        assert_eq!(entry.tags, vec!["performance".to_string()]);
    }

    #[test]
    fn get_memory_returns_none_for_unknown_id() {
        let (svc, _dir) = make_service();
        let result = svc.get_memory(&MemoryId("UNKNOWN".to_string())).unwrap();
        assert!(result.is_none());
    }

    // ---------- Phase 1: エラーケース ----------

    #[test]
    fn save_memory_without_entity_fails() {
        let (svc, _dir) = make_service();

        let err = svc
            .save_memory(
                "nonexistent".to_string(),
                "doc".to_string(),
                "Title".to_string(),
                "Content".to_string(),
                None,
                vec![],
            )
            .unwrap_err();

        assert!(matches!(err, ScrapwellError::NotFound(_)));
    }

    #[test]
    fn save_memory_duplicate_name_within_entity_fails() {
        let (svc, _dir) = make_service();

        svc.create_entity("rust".to_string(), Scope::Knowledge, None, vec![])
            .unwrap();
        svc.save_memory(
            "rust".to_string(),
            "anyhow".to_string(),
            "Anyhow".to_string(),
            "Content".to_string(),
            None,
            vec![],
        )
        .unwrap();

        let err = svc
            .save_memory(
                "rust".to_string(),
                "anyhow".to_string(),
                "Anyhow again".to_string(),
                "More content".to_string(),
                None,
                vec![],
            )
            .unwrap_err();

        assert!(matches!(err, ScrapwellError::DuplicateName(_)));
    }

    // ---------- Phase 1: vault 全体の一意性 ----------

    #[test]
    fn vault_wide_name_uniqueness_across_entities() {
        let (svc, _dir) = make_service();

        svc.create_entity("alpha".to_string(), Scope::Knowledge, None, vec![])
            .unwrap();
        svc.create_entity("beta".to_string(), Scope::Knowledge, None, vec![])
            .unwrap();

        svc.save_memory(
            "alpha".to_string(),
            "shared-name".to_string(),
            "Title".to_string(),
            "Content".to_string(),
            None,
            vec![],
        )
        .unwrap();

        let err = svc
            .save_memory(
                "beta".to_string(),
                "shared-name".to_string(),
                "Title".to_string(),
                "Content".to_string(),
                None,
                vec![],
            )
            .unwrap_err();

        assert!(
            matches!(err, ScrapwellError::DuplicateName(_)),
            "vault-wide uniqueness must be enforced across entities"
        );
    }

    // ---------- Phase 1: list_memories ----------

    #[test]
    fn list_memories_reflects_structure() {
        let (svc, _dir) = make_service();

        svc.create_entity("rust".to_string(), Scope::Knowledge, None, vec![])
            .unwrap();
        svc.create_entity("elasticsearch".to_string(), Scope::Knowledge, None, vec![])
            .unwrap();

        svc.save_memory(
            "rust".to_string(),
            "anyhow".to_string(),
            "Anyhow".to_string(),
            "C".to_string(),
            None,
            vec![],
        )
        .unwrap();
        svc.save_memory(
            "rust".to_string(),
            "thiserror".to_string(),
            "Thiserror".to_string(),
            "C".to_string(),
            None,
            vec![],
        )
        .unwrap();
        svc.save_memory(
            "elasticsearch".to_string(),
            "nested-vector".to_string(),
            "Nested".to_string(),
            "C".to_string(),
            Some("mapping".to_string()),
            vec![],
        )
        .unwrap();

        let tree = svc.list_memories(None, 2).unwrap();

        assert_eq!(tree.children.len(), 2);

        let rust = tree.children.iter().find(|n| n.name == "rust").unwrap();
        let es = tree
            .children
            .iter()
            .find(|n| n.name == "elasticsearch")
            .unwrap();

        assert_eq!(rust.document_count, 2);
        assert_eq!(es.document_count, 1);
        assert_eq!(es.children.len(), 1);
        assert_eq!(es.children[0].name, "mapping");
    }

    // ---------- Phase 2: 類似名チェック ----------

    #[test]
    fn similar_entity_name_is_rejected() {
        let (svc, _dir) = make_service();

        svc.create_entity("elasticsearch".to_string(), Scope::Knowledge, None, vec![])
            .unwrap();

        // "elastic-search" は "elasticsearch" と類似度 > 0.85 のためエラー
        let err = svc
            .create_entity("elastic-search".to_string(), Scope::Knowledge, None, vec![])
            .unwrap_err();

        match err {
            ScrapwellError::SimilarEntityExists { name, suggestions } => {
                assert_eq!(name, "elastic-search");
                assert!(suggestions.contains(&"elasticsearch".to_string()));
            }
            other => panic!("expected SimilarEntityExists, got {:?}", other),
        }
    }

    #[test]
    fn distinct_entity_names_are_accepted() {
        let (svc, _dir) = make_service();

        svc.create_entity("rust".to_string(), Scope::Knowledge, None, vec![])
            .unwrap();

        // "redis" は "rust" と類似度 < 0.85 のため通過
        svc.create_entity("redis".to_string(), Scope::Knowledge, None, vec![])
            .unwrap();
    }

    // ---------- Phase 2: update_entity ----------

    #[test]
    fn update_entity_persists_changes() {
        let (svc, _dir) = make_service();

        svc.create_entity(
            "elasticsearch".to_string(),
            Scope::Knowledge,
            Some("old description".to_string()),
            vec!["old-tag".to_string()],
        )
        .unwrap();

        // entity_id を取得
        let entity = svc
            .store
            .get_entity_by_name("elasticsearch")
            .unwrap()
            .unwrap();

        svc.update_entity(
            entity.id.0.clone(),
            Some(Scope::Project),
            Some("new description".to_string()),
            Some(vec!["new-tag".to_string()]),
        )
        .unwrap();

        let updated = svc
            .store
            .get_entity_by_name("elasticsearch")
            .unwrap()
            .unwrap();
        assert_eq!(updated.scope, Scope::Project);
        assert_eq!(updated.description, Some("new description".to_string()));
        assert_eq!(updated.tags, vec!["new-tag".to_string()]);
    }

    // ---------- Phase 2: delete_entity ----------

    #[test]
    fn delete_entity_cascades_to_documents() {
        let (svc, _dir) = make_service();

        svc.create_entity("rust".to_string(), Scope::Knowledge, None, vec![])
            .unwrap();
        let doc_id = svc
            .save_memory(
                "rust".to_string(),
                "anyhow".to_string(),
                "Anyhow".to_string(),
                "Content".to_string(),
                None,
                vec![],
            )
            .unwrap();

        let entity = svc.store.get_entity_by_name("rust").unwrap().unwrap();
        svc.delete_entity(entity.id.0.clone()).unwrap();

        // Entity も document も消えている
        assert!(svc.store.get_entity_by_name("rust").unwrap().is_none());
        assert!(svc.get_memory(&doc_id).unwrap().is_none());
    }

    // ---------- Phase 2: update_memory ----------

    #[test]
    fn update_memory_persists_changes() {
        let (svc, _dir) = make_service();

        svc.create_entity("rust".to_string(), Scope::Knowledge, None, vec![])
            .unwrap();
        let id = svc
            .save_memory(
                "rust".to_string(),
                "anyhow".to_string(),
                "Old Title".to_string(),
                "Old content".to_string(),
                None,
                vec![],
            )
            .unwrap();

        svc.update_memory(
            id.0.clone(),
            Some("New Title".to_string()),
            Some("New content".to_string()),
            Some(vec!["updated".to_string()]),
        )
        .unwrap();

        let updated = svc.get_memory(&id).unwrap().unwrap();
        assert_eq!(updated.title, "New Title");
        assert_eq!(updated.content, "New content");
        assert_eq!(updated.tags, vec!["updated".to_string()]);
    }

    // ---------- Phase 4: rebuild_index ----------

    #[test]
    fn rebuild_index_returns_document_count() {
        let (svc, _dir) = make_service();

        svc.create_entity("rust".to_string(), Scope::Knowledge, None, vec![])
            .unwrap();
        svc.save_memory(
            "rust".to_string(),
            "anyhow".to_string(),
            "Anyhow".to_string(),
            "Content A".to_string(),
            None,
            vec![],
        )
        .unwrap();
        svc.save_memory(
            "rust".to_string(),
            "thiserror".to_string(),
            "Thiserror".to_string(),
            "Content B".to_string(),
            None,
            vec![],
        )
        .unwrap();

        let count = svc.rebuild_index().unwrap();
        assert_eq!(
            count, 2,
            "rebuild should report number of indexed documents"
        );
    }

    #[test]
    fn rebuild_index_on_empty_vault_returns_zero() {
        let (svc, _dir) = make_service();
        let count = svc.rebuild_index().unwrap();
        assert_eq!(count, 0);
    }

    // ---------- Phase 2: delete_memory ----------

    #[test]
    fn delete_memory_removes_document() {
        let (svc, _dir) = make_service();

        svc.create_entity("rust".to_string(), Scope::Knowledge, None, vec![])
            .unwrap();
        let id = svc
            .save_memory(
                "rust".to_string(),
                "anyhow".to_string(),
                "Anyhow".to_string(),
                "Content".to_string(),
                None,
                vec![],
            )
            .unwrap();

        svc.delete_memory(id.0.clone()).unwrap();

        assert!(svc.get_memory(&id).unwrap().is_none());
    }
}
