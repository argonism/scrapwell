+++
title = "Installation"
description = "Install scrapwell on macOS or Linux."
date = 2026-04-16T00:00:00+00:00
updated = 2026-04-16T00:00:00+00:00
draft = false
weight = 10
sort_by = "weight"
template = "docs/page.html"

[extra]
lead = "Install scrapwell on macOS or Linux in one command."
toc = true
top = false
+++

## macOS (Homebrew)

The easiest way to install on macOS:

```bash
brew install argonism/tap/scrapwell
```

## macOS / Linux (shell installer)

For any platform, use the shell installer:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/argonism/scrapwell/releases/latest/download/scrapwell-installer.sh | sh
```

The installer places the `scrapwell` binary in `~/.cargo/bin/` (or `~/.local/bin/` on Linux). Make sure that directory is in your `PATH`.

## Build from source

Requires Rust 1.75+.

```bash
git clone https://github.com/argonism/scrapwell.git
cd scrapwell
cargo build --release
# binary is at target/release/scrapwell
```

## Verify installation

```bash
scrapwell --version
```

## Next step

Once installed, [connect scrapwell to Claude Code →](../claude-code-setup/)
