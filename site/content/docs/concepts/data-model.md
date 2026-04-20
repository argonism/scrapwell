+++
title = "Data Model"
description = "How knowledge is organized in scrapwell: Entity, Topic, and Document."
date = 2026-04-16T00:00:00+00:00
updated = 2026-04-16T00:00:00+00:00
draft = false
weight = 10
sort_by = "weight"
template = "docs/page.html"

[extra]
lead = "Knowledge in scrapwell is organized in a three-level hierarchy: Entity > Topic > Document."
toc = true
top = false
+++

## Three-level hierarchy

```
Entity          — the subject of knowledge (a technology, project, library, concept)
  Topic         — a sub-theme within an Entity (optional)
    Document    — a single piece of knowledge (one Markdown file)
```

A **Document** always belongs to an **Entity**. A **Topic** is an optional grouping layer — use it only when a single Entity has many documents that clearly cluster into sub-themes.

## On-disk layout

```
~/.memory/
  config.toml          # optional config
  metadata.db          # SQLite (derived — safe to delete and rebuild)
  index/               # search index (derived — safe to delete and rebuild)
  entities/
    elasticsearch/
      _entity.md                    # Entity metadata
      mapping/                      # Topic (optional)
        nested-dense-vector.md
        dynamic-templates.md
      performance/                  # Topic (optional)
        shard-sizing.md
      reindex-strategy.md           # Document directly under Entity
    rust/
      _entity.md
      anyhow-vs-thiserror.md
```

**Markdown is the source of truth.** SQLite and the search index are derived data. Delete them and run `scrapwell rebuild` to regenerate everything from the Markdown files.

## Entity metadata

Each Entity has a `_entity.md` file with a small YAML frontmatter:

```markdown
---
id: "01JF3X..."
scope: knowledge
tags: ["search-engine", "database"]
created_at: "2026-04-06T12:00:00Z"
updated_at: "2026-04-06T12:00:00Z"
---

Knowledge about Elasticsearch: mappings, query tuning, and production operations.
```

`scope` is either:
- `knowledge` — general, reusable knowledge (technologies, libraries, concepts)
- `project` — project-specific knowledge tied to a particular codebase

## Document format

Each Document is a standard Markdown file:

```markdown
---
id: "01JF3X..."
title: "nested + dense_vector mapping"
tags: ["performance"]
created_at: "2026-04-06T12:00:00Z"
updated_at: "2026-04-06T12:00:00Z"
---

When using a nested field containing a dense_vector in Elasticsearch...

Related: [[anyhow-vs-thiserror]]
```

- IDs are [ULIDs](https://github.com/ulid/spec) — time-sortable and unique
- Cross-references use `[[wikilink]]` syntax (Obsidian-compatible)
- No `links` frontmatter field — links live in the document body

## Path conventions

- Path format: `entities/<entity>/<topic>/<document>`
- Maximum depth: 4 levels (including `entities/`)
- Names use kebab-case, ASCII alphanumeric + hyphens only
- Topics are optional — if an Entity has fewer than ~7 documents, put them directly under the Entity

## Filename uniqueness

All filenames must be unique across the entire vault (required for Obsidian `[[wikilink]]` resolution). When you call `save_memory`, scrapwell checks for conflicts. If a conflict is detected, it returns an error and the caller (Claude Code) retries with a suffix.

## When to create a Topic

Create a Topic only when **both** conditions are met:
1. The Entity has **more than ~7 documents**
2. The documents clearly cluster into **distinct sub-themes**

Avoid creating Topics with only 1–2 documents — flat is fine.

## Tags vs path

The path captures *where* a document belongs. Tags capture *cross-cutting concerns* that span multiple Entities or Topics.

Example: tag a document with `performance` and searching for `tag:performance` returns matching documents from `elasticsearch/`, `rust/`, and `llm/` in a single query — regardless of their Entity or Topic.
