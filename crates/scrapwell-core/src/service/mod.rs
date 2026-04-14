use chrono::Utc;

use crate::{
    error::{Result, ScrapwellError},
    index::SearchIndex,
    model::{EntityMeta, MemoryEntry, MemoryId, Scope, SearchHit, SearchQuery, TreeNode},
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
        self.index.search(&SearchQuery { query, entity, limit })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        error::ScrapwellError,
        index::noop::NoopSearchIndex,
        store::fs::FsMemoryStore,
    };
    use tempfile::TempDir;

    // ---------- ヘルパー ----------

    fn make_service() -> (MemoryService<FsMemoryStore, NoopSearchIndex>, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = FsMemoryStore::new(dir.path().to_path_buf()).unwrap();
        let service = MemoryService::new(store, NoopSearchIndex);
        (service, dir)
    }

    // ---------- ハッピーパス ----------

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

    // ---------- エラーケース ----------

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

    // ---------- vault 全体の一意性 ----------

    #[test]
    fn vault_wide_name_uniqueness_across_entities() {
        let (svc, _dir) = make_service();

        svc.create_entity("alpha".to_string(), Scope::Knowledge, None, vec![])
            .unwrap();
        svc.create_entity("beta".to_string(), Scope::Knowledge, None, vec![])
            .unwrap();

        // alpha に "shared-name" を保存
        svc.save_memory(
            "alpha".to_string(),
            "shared-name".to_string(),
            "Title".to_string(),
            "Content".to_string(),
            None,
            vec![],
        )
        .unwrap();

        // beta に同じ "shared-name" → vault 全体で一意なので弾かれる
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

    // ---------- list_memories ----------

    #[test]
    fn list_memories_reflects_structure() {
        let (svc, _dir) = make_service();

        svc.create_entity("rust".to_string(), Scope::Knowledge, None, vec![])
            .unwrap();
        svc.create_entity("elasticsearch".to_string(), Scope::Knowledge, None, vec![])
            .unwrap();

        svc.save_memory("rust".to_string(), "anyhow".to_string(), "Anyhow".to_string(), "C".to_string(), None, vec![]).unwrap();
        svc.save_memory("rust".to_string(), "thiserror".to_string(), "Thiserror".to_string(), "C".to_string(), None, vec![]).unwrap();
        svc.save_memory("elasticsearch".to_string(), "nested-vector".to_string(), "Nested".to_string(), "C".to_string(), Some("mapping".to_string()), vec![]).unwrap();

        let tree = svc.list_memories(None, 2).unwrap();

        assert_eq!(tree.children.len(), 2);

        let rust = tree.children.iter().find(|n| n.name == "rust").unwrap();
        let es = tree.children.iter().find(|n| n.name == "elasticsearch").unwrap();

        assert_eq!(rust.document_count, 2);
        assert_eq!(es.document_count, 1);
        assert_eq!(es.children.len(), 1);
        assert_eq!(es.children[0].name, "mapping");
    }
}
