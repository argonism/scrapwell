use super::SearchIndex;
use crate::{
    error::Result,
    model::{MemoryEntry, MemoryId, SearchHit, SearchQuery},
};

pub struct NoopSearchIndex;

impl SearchIndex for NoopSearchIndex {
    fn upsert(&self, _entry: &MemoryEntry) -> Result<()> {
        Ok(())
    }

    fn search(&self, _query: &SearchQuery) -> Result<Vec<SearchHit>> {
        Ok(vec![])
    }

    fn remove(&self, _id: &MemoryId) -> Result<()> {
        Ok(())
    }

    fn rebuild(&self, _entries: &mut dyn Iterator<Item = MemoryEntry>) -> Result<()> {
        Ok(())
    }
}
