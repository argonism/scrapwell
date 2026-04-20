+++
title = "scrapwell"
description = "A minimal MCP memory server for LLM agents."

[extra]
lead = '<b>scrapwell</b> is a minimal MCP memory server for LLM agents and people. No hidden LLM calls. No server to run. Just human-readable Markdown files on disk — fully searchable.'
url = "/docs/getting-started/installation/"
url_button = "Get started"
repo_version = "MIT License"
repo_license = "Open-source MIT License."
repo_url = "https://github.com/argonism/scrapwell"

[[extra.menu.main]]
name = "Docs"
section = "docs"
url = "/docs/getting-started/installation/"
weight = 10

[[extra.menu.main]]
name = "GitHub"
section = ""
url = "https://github.com/argonism/scrapwell"
weight = 20

[[extra.list]]
title = "No hidden LLM calls"
content = "The calling LLM decides what to save and how to classify it. scrapwell just stores and indexes — zero extra API cost per save."

[[extra.list]]
title = "Markdown as source of truth"
content = "All memories are plain Markdown files on disk. Browse, edit, or grep them in any text editor. No proprietary database, no lock-in."

[[extra.list]]
title = "Obsidian compatible"
content = 'Open the memory directory as an Obsidian vault. <code>[[wikilinks]]</code> resolve automatically and the graph view shows knowledge connections.'

[[extra.list]]
title = "10 MCP tools over stdio"
content = "Full CRUD for entities and documents, full-text search with highlighted snippets, tree browsing, and index rebuild — all over MCP stdio."

[[extra.list]]
title = "Pluggable search backend"
content = "Swap between LanceDB (default) and Tantivy via a single config field. Both backends are abstracted behind a trait."

[[extra.list]]
title = "Entity → Topic → Document"
content = "A simple three-level hierarchy keeps knowledge organized without enforcing rigid schemas. Topics are optional and created on demand."
+++
