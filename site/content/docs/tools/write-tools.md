+++
title = "Write Tools"
description = "Reference for the 6 write tools: create_entity, update_entity, delete_entity, save_memory, update_memory, delete_memory."
date = 2026-04-16T00:00:00+00:00
updated = 2026-04-16T00:00:00+00:00
draft = false
weight = 20
sort_by = "weight"
template = "docs/page.html"

[extra]
lead = "Six tools for creating and modifying entities and documents."
toc = true
top = false
+++

## create_entity

Create a new Entity. Performs a similarity check against existing entity names (edit distance) and returns an error with candidate suggestions if a similar entity already exists.

**Parameters**

| Name | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | Entity name (e.g. `"elasticsearch"`) |
| `scope` | string | yes | `"knowledge"` or `"project"` |
| `description` | string | no | Human-readable description of the Entity |
| `tags` | string[] | no | Entity-level tags |

**Returns:** `{ id: string }`

**Similar name response:**

```json
{
  "error": "similar_entity_exists",
  "message": "Entity 'elastic-search' is similar to existing entities",
  "suggestions": ["elasticsearch"]
}
```

When you receive this, either reuse the suggested entity or choose a clearly distinct name.

---

## update_entity

Partial update of an existing Entity. Only the fields you specify are changed.

**Parameters**

| Name | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes | Target entity ID |
| `scope` | string | no | New scope |
| `description` | string | no | New description |
| `tags` | string[] | no | New tags (full replacement) |

---

## delete_entity

Delete an Entity and cascade-delete all its Documents and Topic directories. Removes the Markdown files, SQLite records, and search index entries.

**Parameters**

| Name | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes | Target entity ID |

> **Warning:** This is irreversible. All documents under the Entity are permanently deleted.

---

## save_memory

Save a new Document under an existing Entity. Topic directories are created automatically if they don't exist.

**Parameters**

| Name | Type | Required | Description |
|---|---|---|---|
| `entity` | string | yes | Entity name (e.g. `"elasticsearch"`) |
| `name` | string | yes | Document filename without extension (e.g. `"nested-dense-vector"`) |
| `title` | string | yes | Human-readable title |
| `content` | string | yes | Markdown body (may include `[[wikilinks]]`) |
| `topic` | string | no | Topic name for grouping (e.g. `"mapping"`) |
| `tags` | string[] | no | Cross-cutting tags |

**Returns:** `{ id: string }`

**Filename conflict:** If the filename already exists anywhere in the vault, scrapwell returns an error. Add a suffix (e.g. `-2`) and retry.

**Best practices (built into the tool description):**

- Call `create_entity` before the first `save_memory` for a new Entity
- Call `list_memories` to see the existing structure before saving
- Create a Topic only when the Entity has ~7+ documents and they cluster into clear sub-themes
- Don't put path information (entity or topic names) into tags

---

## update_memory

Partial update of an existing Document. Only the fields you specify are changed.

**Parameters**

| Name | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes | Target document ID |
| `title` | string | no | New title |
| `content` | string | no | New body (full replacement) |
| `tags` | string[] | no | New tags (full replacement) |

---

## delete_memory

Delete a Document. Removes the Markdown file and its search index entry.

**Parameters**

| Name | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes | Target document ID |
