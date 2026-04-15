# scrapwell

A lightweight MCP server for persisting and searching knowledge acquired by LLM agents (primarily Claude Code) during task execution — stored locally.

## Motivation

- Insights gained while working on projects with Claude Code are lost between sessions
- Existing memory tools (e.g., Mem0) extract facts by calling an LLM under the hood, which incurs additional LLM costs we want to avoid
- Fact extraction and classification should be handled by the calling LLM itself; the MCP server should act purely as a storage + index layer
- Saved knowledge should remain as Markdown files that can be opened directly as an Obsidian vault

## Features

- **No additional LLM cost** — fact extraction and classification decisions are handled by the calling LLM
- **Markdown as source of truth** — SQLite and SearchIndex are derived data; they can be rebuilt if corrupted
- **Obsidian-compatible** — `[[wikilink]]` syntax, vault-wide unique filenames
- **Swappable search backend** — tantivy/lancedb loosely coupled behind a trait boundary
- **Entity-Document model** — knowledge organized in a 3-tier structure: Entity > Topic > Document
- **Self-contained guidelines** — usage instructions embedded in MCP tool descriptions, minimizing CLAUDE.md entries

## Installation

**macOS / Linux (shell installer)**

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/argonism/scrapwell/releases/latest/download/scrapwell-installer.sh | sh
```

**macOS (Homebrew)**

```bash
brew install argonism/tap/scrapwell
```

## Integration with Claude Code

After installation, register with Claude Code using the following command:

```bash
claude mcp add scrapwell --scope user scrapwell serve
```

To use a custom storage location, specify it via an environment variable:

```bash
claude mcp add scrapwell --scope user \
  --env SCRAPWELL_ROOT=/path/to/vault \
  scrapwell serve
```

## Configuration

The configuration file is optional. If absent, all defaults apply.

**`~/.memory/config.toml`**

```toml
# Memory root path (default: ~/.memory/)
root = "~/.memory/"

# Search backend (default: "tantivy")
# "tantivy" | "lancedb"
search_backend = "tantivy"
```

## Data Structure

Knowledge is managed using a 3-tier **Entity > Topic > Document** model.

| Layer | Description |
|---|---|
| **Entity** | The subject of knowledge (technology, project, library, concept, etc.) |
| **Topic** | Sub-theme classification within an Entity (optional; created when ~7+ documents exist and clear boundaries are present) |
| **Document** | An individual piece of knowledge. One Markdown file = one Document |

**Physical structure on disk**

```
~/.memory/
  config.toml
  metadata.db          # SQLite (metadata management)
  index/               # Derived data managed by SearchIndex

  entities/
    elasticsearch/
      _entity.md                   # Entity metadata
      mapping/                     # topic
        nested-dense-vector.md
        dynamic-templates.md
      performance/                 # topic
        shard-sizing.md
      reindex-strategy.md          # document directly under entity (no topic)
    rust/
      _entity.md
      anyhow-vs-thiserror.md
```

**Entity metadata (`_entity.md`)**

```markdown
---
id: "01JF3X..."
scope: knowledge
tags: ["search-engine", "database"]
created_at: "2026-04-06T12:00:00Z"
updated_at: "2026-04-06T12:00:00Z"
---

Knowledge about Elasticsearch.
```

**Document**

```markdown
---
id: "01JF3X..."
title: "nested + dense_vector mapping"
tags: ["performance"]
created_at: "2026-04-06T12:00:00Z"
updated_at: "2026-04-06T12:00:00Z"
---

When placing a dense_vector inside a nested field in Elasticsearch...

Related: [[anyhow-vs-thiserror]]
```

## MCP Tools

10 tools in total (Write 6 / Read 3 / Admin 1), transport: **stdio**.

### Write Tools

| Tool | Description |
|---|---|
| `create_entity` | Create a new Entity. Includes similar-name check (edit distance) |
| `update_entity` | Partial update of an existing Entity |
| `delete_entity` | Cascade delete an Entity and its documents |
| `save_memory` | Save a new Document |
| `update_memory` | Partial update of an existing Document |
| `delete_memory` | Delete a Document |

### Read Tools

| Tool | Description |
|---|---|
| `search_memory` | Full-text search. Returns highlighted snippets |
| `list_memories` | Returns the Entity/Topic tree structure |
| `get_memory` | Retrieve the full content of a single Document |

### Admin Tools

| Tool | Description |
|---|---|
| `rebuild_index` | Rebuild SQLite and search index from Markdown |

## Architecture

```
┌─────────────────────────────┐
│  Transport (main.rs)        │  MCP stdio, DI
├─────────────────────────────┤
│  Application (handler.rs)   │  MCP tool dispatch only
├─────────────────────────────┤
│  Service (service/)         │  Business logic
├─────────────────────────────┤
│  Domain (model, path)       │  Data structures, validation
├─────────────────────────────┤
│  Infrastructure             │  trait MemoryStore → FsMemoryStore
│                             │  trait SearchIndex → TantivySearchIndex
└─────────────────────────────┘
```

**Cargo workspace**

```
scrapwell/
  Cargo.toml                  # workspace root
  crates/
    scrapwell-core/           # library crate (business logic)
    scrapwell/                # binary crate (MCP stdio transport, config, DI)
```

For details, see [`docs/architecture.md`](docs/architecture.md).

## Documentation

- [`docs/directory-structure.md`](docs/directory-structure.md) — Entity-Document model, path conventions, tags
- [`docs/mcp-tools.md`](docs/mcp-tools.md) — Interface definitions for all 10 tools
- [`docs/architecture.md`](docs/architecture.md) — Cargo workspace structure, trait definitions, data flow
- [`docs/dependencies.md`](docs/dependencies.md) — Key external dependencies
- [`docs/roadmap.md`](docs/roadmap.md) — Unimplemented features and future extensions

## Roadmap

- Tag-based cross-search tool (dedicated MCP tool)
- LanceDB backend (for embedding-based search)
- HTTP/SSE transport (support for non-Claude Code clients)
- Export/import

## CLI Commands

```bash
# Start as MCP server (normal usage invoked from Claude Code)
scrapwell serve

# Rebuild index (recovery from corruption)
scrapwell rebuild
scrapwell rebuild --target metadata  # SQLite only
scrapwell rebuild --target search    # Search index only
```

---

## For Developers

### Build

```bash
cargo build --release
# Binary: target/release/scrapwell
```

### Test

```bash
cargo test --workspace
```

### Lint

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

### Setting up pre-push hook

Run once after cloning. This will automatically run fmt and clippy before each push.

```bash
git config core.hooksPath .githooks
```
