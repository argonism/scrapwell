+++
title = "Claude Code Setup"
description = "Register scrapwell as an MCP server in Claude Code."
date = 2026-04-16T00:00:00+00:00
updated = 2026-04-16T00:00:00+00:00
draft = false
weight = 20
sort_by = "weight"
template = "docs/page.html"

[extra]
lead = "Register scrapwell as an MCP server so Claude Code can persist knowledge between sessions."
toc = true
top = false
+++

## Register with Claude Code

Run this once to register scrapwell as a user-scoped MCP server:

```bash
claude mcp add scrapwell --scope user scrapwell serve
```

That's it. Claude Code will start scrapwell automatically when a session begins.

## Verify the connection

Open Claude Code and ask:

```
List my memories.
```

If scrapwell is connected, Claude Code will call `list_memories` and show the current knowledge tree (empty on first run).

## How it works

scrapwell runs as a child process of Claude Code over **stdio**. No network port, no daemon process — the server starts and stops with each Claude Code session.

Claude Code discovers the server via the MCP configuration you added above. During a session, it can call any of the [10 MCP tools](../../tools/overview/) directly.

## Configuration (optional)

By default, memories are stored in `~/.memory/`. To change the location or search backend, create a config file:

**User config** (`~/.config/scrapwell/config.toml`):

```toml
root = "~/.memory/"
search_backend = "lancedb"  # or "tantivy"
```

**Project config** (`.scrapwell.toml` at the repo root):

```toml
root = "./memory"  # store memories alongside the project
```

Project config takes precedence over user config for any field that is set. See the full [configuration reference →](../../configuration/config-reference/)

## Environment variable

You can also override the memory root without a config file:

```bash
SCRAPWELL_ROOT=~/my-memory scrapwell serve
```

## Unregister

To remove scrapwell from Claude Code:

```bash
claude mcp remove scrapwell --scope user
```
