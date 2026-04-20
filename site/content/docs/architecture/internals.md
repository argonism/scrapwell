+++
title = "Internals"
description = "Layer architecture, Cargo workspace layout, trait definitions, and data flow."
date = 2026-04-16T00:00:00+00:00
updated = 2026-04-16T00:00:00+00:00
draft = false
weight = 10
sort_by = "weight"
template = "docs/page.html"

[extra]
lead = "scrapwell is a small Rust codebase. This page covers how the layers fit together."
toc = true
top = false
+++

## Layer architecture

```
┌─────────────────────────────┐
│  Transport (main.rs)        │  MCP stdio, dependency injection
├─────────────────────────────┤
│  Application (handler.rs)   │  MCP tool dispatch only
├─────────────────────────────┤
│  Service (service/)         │  Business logic
├─────────────────────────────┤
│  Domain (model, path)       │  Data structures, validation
├─────────────────────────────┤
│  Infrastructure             │  trait MemoryStore → FsMemoryStore
│                             │  trait SearchIndex → LanceDbSearchIndex
└─────────────────────────────┘
```

- **handler** receives MCP parameters and delegates to the service. No business logic.
- **service** owns business logic: store/index coordination, consistency guarantees. No MCP concepts.
- handler → service: concrete type dependency (no abstraction needed here)
- service → infrastructure: trait dependency only

## Cargo workspace

```
scrapwell/
  Cargo.toml                       # workspace root
  crates/
    scrapwell-core/                # library crate
      src/
        lib.rs
        model.rs                   # MemoryEntry, SearchHit, TreeNode, etc.
        path.rs                    # MemoryPath parsing and validation
        service/
          mod.rs                   # MemoryService
        store/
          mod.rs                   # trait MemoryStore
          fs.rs                    # FsMemoryStore (Markdown + SQLite)
        index/
          mod.rs                   # trait SearchIndex
          lancedb.rs               # LanceDbSearchIndex (default)
          tantivy.rs               # TantivySearchIndex (feature-gated)
        handler.rs                 # MemoryHandler (MCP tool dispatch)
    scrapwell/                     # binary crate
      src/
        main.rs                    # MCP stdio transport, config, DI
```

## Traits

### MemoryStore

```rust
pub trait MemoryStore: Send + Sync {
    fn save_entity(&self, entity: &EntityMeta) -> Result<()>;
    fn get_entity_by_name(&self, name: &str) -> Result<Option<EntityMeta>>;
    fn save(&self, entry: &MemoryEntry) -> Result<()>;
    fn get(&self, id: &MemoryId) -> Result<Option<MemoryEntry>>;
    fn list_tree(&self, entity: Option<&str>, depth: u32) -> Result<TreeNode>;
    fn check_name_unique(&self, name: &str) -> Result<bool>;
}
```

### SearchIndex

```rust
pub trait SearchIndex: Send + Sync {
    fn upsert(&self, entry: &MemoryEntry) -> Result<()>;
    fn search(&self, query: &SearchQuery) -> Result<Vec<SearchHit>>;
    fn remove(&self, id: &MemoryId) -> Result<()>;
    fn rebuild(&self, entries: &mut dyn Iterator<Item = MemoryEntry>) -> Result<()>;
}
```

## SQLite schema

`FsMemoryStore` maintains `~/.memory/metadata.db`:

```sql
CREATE TABLE entities (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    scope TEXT NOT NULL CHECK(scope IN ('knowledge', 'project')),
    description TEXT,
    tags TEXT,              -- JSON array
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE documents (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    topic TEXT,
    title TEXT NOT NULL,
    tags TEXT,              -- JSON array
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_documents_entity_id ON documents(entity_id);
CREATE INDEX idx_documents_name ON documents(name);
CREATE INDEX idx_entities_name ON entities(name);
```

SQLite is derived data — delete it and run `scrapwell rebuild` to regenerate from Markdown.

## Data flow

| Operation | Markdown | SQLite | Search index |
|---|---|---|---|
| `save` | write `.md` | INSERT (uniqueness check) | upsert |
| `update` | update `.md` | UPDATE | upsert |
| `delete` | remove `.md` | DELETE | remove |
| `search` | — | — | query → snippets |
| `list` | — | SELECT by entity | — |
| `get` | read `.md` | SELECT by id (path resolution) | — |

## Feature flags

```toml
[features]
default = ["lancedb-backend"]
lancedb-backend = ["dep:lancedb"]
tantivy-backend = ["dep:tantivy"]
```

Build with Tantivy instead:

```bash
cargo build --release --no-default-features --features tantivy-backend
```

## Concurrency

scrapwell starts one process per Claude Code session. Multiple concurrent sessions share the same memory root. Concurrency is delegated to SQLite's WAL mode:

- **Reads**: multiple processes can read concurrently
- **Writes**: SQLite serializes writes automatically; short retry on conflict
- **Markdown files**: each document is a separate file, so simultaneous writes to different documents do not conflict

## Error types

```rust
pub enum ScrapwellError {
    InvalidPath(String),
    NotFound(MemoryId),
    DuplicateName(String),
    Io(#[from] std::io::Error),
    Index(String),
}
```

## Contributing

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# Set up pre-push hook (runs fmt + clippy before push)
git config core.hooksPath .githooks
```
