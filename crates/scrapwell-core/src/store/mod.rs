pub mod fs;

use crate::{
    error::Result,
    model::{EntityMeta, EntityPatch, MemoryEntry, MemoryId, MemoryPatch, TreeNode},
};

pub trait MemoryStore: Send + Sync {
    // Entity CRUD
    fn save_entity(&self, entity: &EntityMeta) -> Result<()>;
    fn get_entity_by_name(&self, name: &str) -> Result<Option<EntityMeta>>;
    fn update_entity(&self, id: &MemoryId, patch: &EntityPatch) -> Result<()>;
    fn delete_entity(&self, id: &MemoryId) -> Result<()>;
    fn list_entity_names(&self) -> Result<Vec<String>>;

    // Document CRUD
    fn save(&self, entry: &MemoryEntry) -> Result<()>;
    fn get(&self, id: &MemoryId) -> Result<Option<MemoryEntry>>;
    fn update(&self, id: &MemoryId, patch: &MemoryPatch) -> Result<()>;
    fn delete(&self, id: &MemoryId) -> Result<()>;
    fn list_tree(&self, entity: Option<&str>, depth: u32) -> Result<TreeNode>;
    fn check_name_unique(&self, name: &str) -> Result<bool>;

    /// 全ドキュメントを取得する（インデックス再構築用）
    fn iter_all(&self) -> Result<Vec<MemoryEntry>>;
}
