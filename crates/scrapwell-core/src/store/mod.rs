pub mod fs;

use crate::{
    error::Result,
    model::{EntityMeta, MemoryEntry, MemoryId, TreeNode},
};

pub trait MemoryStore: Send + Sync {
    fn save_entity(&self, entity: &EntityMeta) -> Result<()>;
    fn get_entity_by_name(&self, name: &str) -> Result<Option<EntityMeta>>;
    fn save(&self, entry: &MemoryEntry) -> Result<()>;
    fn get(&self, id: &MemoryId) -> Result<Option<MemoryEntry>>;
    fn list_tree(&self, entity: Option<&str>, depth: u32) -> Result<TreeNode>;
    fn check_name_unique(&self, name: &str) -> Result<bool>;
}
