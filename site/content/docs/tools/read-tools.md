+++
title = "Read Tools"
description = "Reference for the 3 read tools: search_memory, list_memories, get_memory."
date = 2026-04-16T00:00:00+00:00
updated = 2026-04-16T00:00:00+00:00
draft = false
weight = 30
sort_by = "weight"
template = "docs/page.html"

[extra]
lead = "Three tools for retrieving knowledge: full-text search, tree browsing, and full-document fetch."
toc = true
top = false
+++

## search_memory

Full-text search across all documents. Returns matching documents with highlighted snippets showing where the query matched.

**Parameters**

| Name | Type | Required | Description |
|---|---|---|---|
| `query` | string | yes | Search query |
| `entity` | string | no | Restrict results to a specific Entity |
| `limit` | number | no | Maximum number of results (default: 10) |

**Returns:**

```json
[
  {
    "id": "01JF3X...",
    "entity": "elasticsearch",
    "topic": "mapping",
    "name": "nested-dense-vector",
    "title": "nested + dense_vector mapping",
    "tags": ["performance"],
    "snippets": ["...nested fieldで<<dense_vector>>を定義する際..."],
    "score": 0.85
  }
]
```

Query matches are highlighted with `<<...>>` in the snippets.

---

## list_memories

Browse the Entity/Topic tree. Use this before `save_memory` to understand what already exists.

**Parameters**

| Name | Type | Required | Description |
|---|---|---|---|
| `entity` | string | no | Restrict to a specific Entity (omit for full tree) |
| `depth` | number | no | Expansion depth (default: 2) |

**Returns:**

```
elasticsearch/ (5 documents)
  mapping/ (2 documents)
  performance/ (1 document)
rust/ (3 documents)
llm/ (4 documents)
misedoko/ (6 documents)
  api/ (2 documents)
work-legaltech/ (3 documents)
```

---

## get_memory

Fetch the full content of a single Document, including its frontmatter and body.

**Parameters**

| Name | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes | Document ID (from `search_memory` or `list_memories`) |

**Returns:** The complete Markdown file content:

```markdown
---
id: "01JF3X..."
title: "nested + dense_vector mapping"
tags: ["performance"]
created_at: "2026-04-06T12:00:00Z"
updated_at: "2026-04-06T12:00:00Z"
---

When using a nested field containing a dense_vector in Elasticsearch...
```
