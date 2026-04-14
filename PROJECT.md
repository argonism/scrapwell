# scrapwell — 軽量メモリMCPサーバー

## 概要

LLMエージェント（主にClaude Code）がタスク遂行中に獲得した知識を、ローカルに永続化・検索するための軽量MCPサーバー。

### 動機

- Claude Codeで仕事や個人プロジェクトをこなす中で得た知見が、セッション間で失われる
- 既存のメモリツール（Mem0等）は裏でLLMを叩いてファクト抽出するが、追加のLLMコストを避けたい
- Claude Code自身がファクト抽出・分類を行い、MCPサーバーは純粋にストレージ+インデックスとして機能すべき
- 保存された知識は人間が直接読めるMarkdownファイルとして残したい
- Obsidianのvaultとしても開けるMarkdown構造を採用

### 設計原則

1. **Markdownがsource of truth** — インデックスは派生データ。壊れても再構築可能
2. **追加のLLMコールなし** — ファクト抽出・分類の判断は呼び出し元のLLM（Claude Code）が担う
3. **検索バックエンドは差し替え可能** — trait境界で抽象化し、tantivy/lancedb等を疎結合に
4. **Entity-Documentモデル** — Entity > Topic > Document の3層構造で知識を整理
5. **Obsidian互換** — `[[wikilink]]` 記法によるリンク、vault全体でユニークなファイル名
6. **ガイドラインはMCPサーバー側に内包** — ツールのdescriptionに埋め込み、CLAUDE.mdへの記述を最小化

## ドキュメント

- [ディレクトリ構造](docs/directory-structure.md) — Entity-Documentモデル、パス規約、tags
- [MCPツール](docs/mcp-tools.md) — 全10ツールのインターフェース定義
- [アーキテクチャ](docs/architecture.md) — Cargo workspace構成、trait定義、データフロー
- [依存crate](docs/dependencies.md) — 主要な外部依存
- [ロードマップ](docs/roadmap.md) — 未実装・将来の拡張
