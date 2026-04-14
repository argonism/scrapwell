# scrapwell

LLMエージェント（主にClaude Code）がタスク遂行中に獲得した知識を、ローカルに永続化・検索するための軽量MCPサーバー。

## 動機

- Claude Codeで仕事や個人プロジェクトをこなす中で得た知見が、セッション間で失われる
- 既存のメモリツール（Mem0等）は裏でLLMを叩いてファクト抽出するが、追加のLLMコストを避けたい
- Claude Code自身がファクト抽出・分類を行い、MCPサーバーは純粋にストレージ＋インデックスとして機能すべき
- 保存された知識はそのままObsidian vaultとして開けるMarkdownファイルとして残したい

## 特徴

- **追加LLMコストなし** — ファクト抽出・分類の判断は呼び出し元のLLMが担う
- **Markdownがsource of truth** — SQLiteとSearchIndexは派生データ。壊れても再構築可能
- **Obsidian互換** — `[[wikilink]]` 記法、vault全体でユニークなファイル名
- **差し替え可能な検索バックエンド** — trait境界でtantivy/lancedbを疎結合に抽象化
- **Entity-Documentモデル** — Entity > Topic > Document の3層構造で知識を整理
- **ガイドライン内包** — MCPツールのdescriptionに使い方を埋め込み、CLAUDE.mdへの記述を最小化

## インストール

```bash
git clone https://github.com/your-org/scrapwell
cd scrapwell
cargo build --release
# バイナリは target/release/scrapwell に生成される
```

デフォルトは tantivy 検索バックエンドを有効にしてビルドされます。

```bash
# tantivy バックエンド（デフォルト）
cargo build --release

# 検索バックエンドなし（メタデータのみ）
cargo build --release --no-default-features
```

## Claude Codeとの統合

`~/.claude/settings.json`（またはプロジェクトの `.claude/settings.json`）に以下を追加します。

```json
{
  "mcpServers": {
    "scrapwell": {
      "command": "/path/to/scrapwell",
      "args": []
    }
  }
}
```

## 設定

設定ファイルは任意です。存在しない場合はすべてデフォルト値で動作します。

**`~/.memory/config.toml`**

```toml
# メモリルートパス（デフォルト: ~/.memory/）
root = "~/.memory/"

# 検索バックエンド（デフォルト: "tantivy"）
# "tantivy" | "lancedb"
search_backend = "tantivy"
```

## データ構造

知識は **Entity > Topic > Document** の3層モデルで管理されます。

| 層 | 説明 |
|---|---|
| **Entity** | 知識の対象（技術・プロジェクト・ライブラリ・概念など） |
| **Topic** | Entity内のサブテーマ分類（任意。ドキュメントが~7件超かつ明確な境界がある場合に作成） |
| **Document** | 個別の知見。Markdownファイル1つが1ドキュメント |

**ディスク上の物理構造**

```
~/.memory/
  config.toml
  metadata.db          # SQLite（メタデータ管理）
  index/               # SearchIndexが管理する派生データ

  entities/
    elasticsearch/
      _entity.md                   # Entityメタデータ
      mapping/                     # topic
        nested-dense-vector.md
        dynamic-templates.md
      performance/                 # topic
        shard-sizing.md
      reindex-strategy.md          # topicなし直下ドキュメント
    rust/
      _entity.md
      anyhow-vs-thiserror.md
```

**Entityメタデータ（`_entity.md`）**

```markdown
---
id: "01JF3X..."
scope: knowledge
tags: ["search-engine", "database"]
created_at: "2026-04-06T12:00:00Z"
updated_at: "2026-04-06T12:00:00Z"
---

Elasticsearch に関する知見。
```

**ドキュメント**

```markdown
---
id: "01JF3X..."
title: "nested + dense_vector mapping"
tags: ["performance"]
created_at: "2026-04-06T12:00:00Z"
updated_at: "2026-04-06T12:00:00Z"
---

Elasticsearchでnested fieldの中にdense_vectorを持たせる場合...

関連: [[anyhow-vs-thiserror]]
```

## MCPツール一覧

全10ツール（Write 6 / Read 3 / Admin 1）、トランスポートは **stdio**。

### Writeツール

| ツール | 説明 |
|---|---|
| `create_entity` | 新規Entityを作成。類似名チェック（編集距離）つき |
| `update_entity` | 既存Entityの部分更新 |
| `delete_entity` | Entityと配下ドキュメントをカスケード削除 |
| `save_memory` | 新規ドキュメントを保存 |
| `update_memory` | 既存ドキュメントの部分更新 |
| `delete_memory` | ドキュメントを削除 |

### Readツール

| ツール | 説明 |
|---|---|
| `search_memory` | 全文検索。ハイライト付きスニペットを返す |
| `list_memories` | Entity/Topicのツリー構造を返す |
| `get_memory` | 単一ドキュメントの全内容を取得 |

### Adminツール

| ツール | 説明 |
|---|---|
| `rebuild_index` | MarkdownからSQLiteと検索インデックスを再構築 |

## アーキテクチャ

```
┌─────────────────────────────┐
│  Transport (main.rs)        │  MCP stdio、DI
├─────────────────────────────┤
│  Application (handler.rs)   │  MCPツールディスパッチのみ
├─────────────────────────────┤
│  Service (service/)         │  ビジネスロジック
├─────────────────────────────┤
│  Domain (model, path)       │  データ構造、バリデーション
├─────────────────────────────┤
│  Infrastructure             │  trait MemoryStore → FsMemoryStore
│                             │  trait SearchIndex → TantivySearchIndex
└─────────────────────────────┘
```

**Cargo workspace**

```
scrapwell/
  Cargo.toml                  # workspace root
  crates/
    scrapwell-core/           # library crate（ビジネスロジック）
    scrapwell/                # binary crate（MCP stdio transport、設定、DI）
```

詳細は [`docs/architecture.md`](docs/architecture.md) を参照してください。

## ドキュメント

- [`docs/directory-structure.md`](docs/directory-structure.md) — Entity-Documentモデル、パス規約、tags
- [`docs/mcp-tools.md`](docs/mcp-tools.md) — 全10ツールのインターフェース定義
- [`docs/architecture.md`](docs/architecture.md) — Cargo workspace構成、trait定義、データフロー
- [`docs/dependencies.md`](docs/dependencies.md) — 主要な外部依存
- [`docs/roadmap.md`](docs/roadmap.md) — 未実装・将来の拡張

## ロードマップ

- タグベースの横断検索ツール（専用MCPツール）
- LanceDB backend（embedding検索が必要な場合）
- HTTP/SSEトランスポート（Claude Code以外のクライアント対応）
- エクスポート/インポート

## CLIコマンド

```bash
# MCPサーバーとして起動（Claude Codeから呼び出される通常の使い方）
scrapwell serve

# インデックスを再構築（破損時のリカバリ）
scrapwell rebuild
scrapwell rebuild --target metadata  # SQLiteのみ
scrapwell rebuild --target search    # 検索インデックスのみ
```
