+++
title = "Obsidian Compatibility"
description = "Using the scrapwell memory directory as an Obsidian vault."
date = 2026-04-16T00:00:00+00:00
updated = 2026-04-16T00:00:00+00:00
draft = false
weight = 20
sort_by = "weight"
template = "docs/page.html"

[extra]
lead = "The memory directory is a valid Obsidian vault out of the box. Open it and your knowledge graph is ready."
toc = true
top = false
+++

## Open as a vault

In Obsidian, choose **Open folder as vault** and select your memory root (default: `~/.memory/`). No plugins required, no configuration needed.

## Wikilinks

Documents use `[[wikilink]]` syntax for cross-references:

```markdown
When choosing between error handling approaches, see [[anyhow-vs-thiserror]].
```

All filenames in the vault are unique, so wikilinks resolve without ambiguity. Obsidian's graph view shows the connection automatically.

## Edit in Obsidian

You can read and edit documents directly in Obsidian. scrapwell reads the Markdown files as-is, so changes made in Obsidian are reflected the next time Claude Code reads those documents.

> **Note:** If you edit the frontmatter (`id`, `created_at`, etc.), scrapwell may not be able to locate the document by ID until you run `rebuild_index`. Body and title edits are safe without a rebuild.

## Graph view

The graph view shows which documents reference each other via wikilinks. This is useful for discovering related knowledge that spans different Entities or Topics.

## Daily notes and other Obsidian features

scrapwell only manages the `entities/` directory. You can use the rest of the vault (daily notes, templates, canvases) freely — scrapwell won't touch files outside `entities/`.
