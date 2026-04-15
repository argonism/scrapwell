# scrapwell

A minimal MCP memory server for LLM agents. No hidden LLM calls. No server to run. Just human-readable Markdown files on disk — fully searchable, with usage prompts bundled in the tools themselves.

## Quick Start

```bash
# Install
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/argonism/scrapwell/releases/latest/download/scrapwell-installer.sh | sh

# Register with Claude Code
claude mcp add scrapwell --scope user scrapwell serve
```

That's it. scrapwell starts alongside Claude Code and persists knowledge automatically.

## Installation

**macOS (Homebrew)**

```bash
brew install argonism/tap/scrapwell
```

**macOS / Linux (shell installer)**

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/argonism/scrapwell/releases/latest/download/scrapwell-installer.sh | sh
```

## Why scrapwell

Most memory tools call an LLM to extract and classify facts — adding latency and cost on every save. scrapwell doesn't. The calling LLM decides what to save; scrapwell just stores it.

The storage format is plain Markdown — readable without any tooling. Browse, edit, or grep your memories in any text editor. No proprietary database, no migration scripts. Open the memory directory in Obsidian and it works.


## How It Works

Knowledge is organized in a flat **Entity > Topic > Document** hierarchy.

```
~/.memory/
  entities/
    rust/
      anyhow-vs-thiserror.md     # a document
    elasticsearch/
      mapping/                   # a topic (optional grouping)
        nested-dense-vector.md
      reindex-strategy.md
```

Each document is a plain Markdown file with a small YAML frontmatter. SQLite and the search index are derived data — delete them and run `scrapwell rebuild` to regenerate from Markdown.

## Obsidian

The memory directory is a valid Obsidian vault out of the box. Just open `~/.memory/` (or your configured root) as a vault.

- Documents use `[[wikilink]]` syntax for cross-references — the graph view shows knowledge connections automatically
- All filenames are unique across the vault, so wikilinks resolve without ambiguity
- Edit or annotate documents directly in Obsidian; scrapwell reads the Markdown as-is

## MCP Tools

10 tools over stdio. Claude Code calls these directly.

| Tool | |
|---|---|
| `save_memory` | Save a new document |
| `search_memory` | Full-text search with highlighted snippets |
| `list_memories` | Browse the Entity/Topic tree |
| `get_memory` | Fetch a single document |
| `update_memory` | Partial update |
| `delete_memory` | Delete a document |
| `create_entity` | Create an Entity (with similar-name check) |
| `update_entity` | Update Entity metadata |
| `delete_entity` | Delete Entity and all its documents |
| `rebuild_index` | Rebuild SQLite and search index from Markdown |

## Configuration

Optional. Defaults work out of the box.

Config is resolved in priority order:

```
CLI --root / SCRAPWELL_ROOT env var
  > .scrapwell.toml   (project config — searched upward from cwd, git-style)
  > ~/.config/scrapwell/config.toml   (user config)
```

**User config** (`~/.config/scrapwell/config.toml`):

```toml
root = "~/.memory/"
search_backend = "tantivy"  # or "lancedb"
```

**Project config** (`.scrapwell.toml` at repo root):

```toml
root = "./memory"   # store memories alongside the project
```

Any field in the project config overrides the user config for that project.

## CLI

```bash
scrapwell serve              # start MCP server (normal usage)
scrapwell rebuild            # rebuild index from Markdown
scrapwell rebuild --target metadata   # SQLite only
scrapwell rebuild --target search     # search index only
```

---

## For Developers

```bash
cargo build --release
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Set up the pre-push hook (runs fmt + clippy before each push):

```bash
git config core.hooksPath .githooks
```

More details: [architecture](docs/architecture.md) · [data model](docs/directory-structure.md) · [tool interfaces](docs/mcp-tools.md) · [roadmap](docs/roadmap.md)
