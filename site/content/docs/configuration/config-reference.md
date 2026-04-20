+++
title = "Configuration Reference"
description = "All configuration options for scrapwell."
date = 2026-04-16T00:00:00+00:00
updated = 2026-04-16T00:00:00+00:00
draft = false
weight = 10
sort_by = "weight"
template = "docs/page.html"

[extra]
lead = "scrapwell works out of the box with no configuration. All options are optional."
toc = true
top = false
+++

## Priority order

Configuration is resolved from highest to lowest priority:

```
CLI flag / environment variable
  > .scrapwell.toml   (project config — searched upward from cwd, like .git)
  > ~/.config/scrapwell/config.toml   (user config)
  > built-in defaults
```

## Options

### `root`

The directory where memories are stored.

| | |
|---|---|
| Type | string (path) |
| Default | `~/.memory/` |
| CLI | `--root <path>` |
| Env | `SCRAPWELL_ROOT` |

### `search_backend`

The search backend to use for full-text search.

| | |
|---|---|
| Type | string |
| Default | `"lancedb"` |
| Options | `"lancedb"`, `"tantivy"` |

Changing this requires a `rebuild_index` (or `scrapwell rebuild`) to re-index existing documents into the new backend.

## User config

`~/.config/scrapwell/config.toml` — applies to all projects:

```toml
root = "~/.memory/"
search_backend = "lancedb"
```

## Project config

`.scrapwell.toml` at the repository root — applies only to that project. Overrides user config for any field that is set:

```toml
# Store memories alongside this project instead of the global root
root = "./memory"
```

## CLI flags

```bash
scrapwell serve --root /path/to/memory
scrapwell rebuild --root /path/to/memory --target all
```

## Environment variables

```bash
SCRAPWELL_ROOT=~/my-memory scrapwell serve
```

## Memory directory structure

When scrapwell first runs, it creates the following layout under `root`:

```
~/.memory/
  metadata.db    # SQLite (auto-created, safe to delete and rebuild)
  index/         # search index (auto-created, safe to delete and rebuild)
  entities/      # your knowledge lives here
```
