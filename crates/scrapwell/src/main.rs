use std::sync::Arc;

use rmcp::{ServiceExt, transport::stdio};
use scrapwell_core::{
    ScrapwellHandler,
    index::tantivy_index::TantivySearchIndex,
    service::MemoryService,
    store::fs::FsMemoryStore,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = dirs::home_dir()
        .expect("cannot determine home directory")
        .join(".memory");

    let store = FsMemoryStore::new(root.clone())?;
    let index = TantivySearchIndex::new(root.join("index"))?;
    let service = Arc::new(MemoryService::new(store, index));
    let handler = ScrapwellHandler::new(service);

    handler.serve(stdio()).await?.waiting().await?;
    Ok(())
}
