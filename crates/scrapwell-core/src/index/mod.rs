pub mod noop;

use crate::{
    error::Result,
    model::{MemoryEntry, MemoryId, SearchHit, SearchQuery},
};

pub trait SearchIndex: Send + Sync {
    fn upsert(&self, entry: &MemoryEntry) -> Result<()>;
    fn search(&self, query: &SearchQuery) -> Result<Vec<SearchHit>>;
    fn remove(&self, id: &MemoryId) -> Result<()>;
    fn rebuild(&self, entries: &mut dyn Iterator<Item = MemoryEntry>) -> Result<()>;
}
