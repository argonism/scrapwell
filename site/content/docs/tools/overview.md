+++
title = "Tools Overview"
description = "All 10 MCP tools provided by scrapwell at a glance."
date = 2026-04-16T00:00:00+00:00
updated = 2026-04-16T00:00:00+00:00
draft = false
weight = 10
sort_by = "weight"
template = "docs/page.html"

[extra]
lead = "scrapwell exposes 10 tools over MCP stdio. Claude Code calls these directly during a session."
toc = true
top = false
+++

## Transport

All tools operate over **stdio** — the standard MCP transport for local tools. No network port is opened.

## Tool categories

### Write tools (6)

| Tool | Description |
|---|---|
| [`create_entity`](../write-tools/#create_entity) | Create a new Entity with a similar-name check |
| [`update_entity`](../write-tools/#update_entity) | Partial update of an Entity's metadata |
| [`delete_entity`](../write-tools/#delete_entity) | Delete an Entity and all its documents (cascade) |
| [`save_memory`](../write-tools/#save_memory) | Save a new Document under an Entity |
| [`update_memory`](../write-tools/#update_memory) | Partial update of a Document |
| [`delete_memory`](../write-tools/#delete_memory) | Delete a Document |

### Read tools (3)

| Tool | Description |
|---|---|
| [`search_memory`](../read-tools/#search_memory) | Full-text search with highlighted snippets |
| [`list_memories`](../read-tools/#list_memories) | Browse the Entity/Topic tree |
| [`get_memory`](../read-tools/#get_memory) | Fetch the full content of a single Document |

### Admin tools (1)

| Tool | Description |
|---|---|
| [`rebuild_index`](../admin-tools/#rebuild_index) | Rebuild SQLite metadata and search index from Markdown |

## Typical session flow

```
1. list_memories          — understand what already exists
2. create_entity          — create an Entity if needed (with similar-name check)
3. save_memory            — save a new Document under the Entity
4. search_memory          — retrieve knowledge in later sessions
5. get_memory             — read the full content of a specific Document
6. update_memory          — append new findings or correct existing content
```

## Built-in guidance

Each tool's `description` field contains usage guidelines. Claude Code reads these automatically — you don't need to add instructions to `CLAUDE.md` for normal usage.
