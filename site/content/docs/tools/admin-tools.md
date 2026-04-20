+++
title = "Admin Tools"
description = "Reference for the admin tool: rebuild_index."
date = 2026-04-16T00:00:00+00:00
updated = 2026-04-16T00:00:00+00:00
draft = false
weight = 40
sort_by = "weight"
template = "docs/page.html"

[extra]
lead = "One admin tool for rebuilding derived data from the Markdown source of truth."
toc = true
top = false
+++

## rebuild_index

Rebuild the SQLite metadata database and/or the search index from the Markdown files in `entities/`. Use this when you suspect data inconsistency, after manually editing Markdown files, or after a crash.

**Parameters**

| Name | Type | Required | Description |
|---|---|---|---|
| `target` | string | no | `"metadata"`, `"search"`, or `"all"` (default: `"all"`) |

**Returns:** `{ entities: number, documents: number }` — count of records rebuilt

**Target values:**

| Value | Effect |
|---|---|
| `"all"` | Rebuild both SQLite metadata and the search index |
| `"metadata"` | Rebuild only the SQLite tables (`entities`, `documents`) |
| `"search"` | Rebuild only the search index |

## CLI equivalent

The same rebuild logic is available as a CLI subcommand:

```bash
scrapwell rebuild                    # rebuild all (metadata + search)
scrapwell rebuild --target metadata  # SQLite only
scrapwell rebuild --target search    # search index only
```

## When to use

- After manually editing or deleting Markdown files outside of scrapwell
- After a crash or unexpected shutdown left the database in an inconsistent state
- After changing the `search_backend` config option (e.g. switching from `lancedb` to `tantivy`)
- If search results seem stale or missing

## How it works

1. Scans `entities/` for `_entity.md` files → rebuilds the `entities` SQLite table
2. Scans all other `.md` files in `entities/` → rebuilds the `documents` SQLite table
3. Re-indexes all documents into the search backend

Because Markdown is the source of truth, a full rebuild is always safe and idempotent.
