# ディレクトリ構造

## データモデル

**Entity > Topic > Document** の3層モデル。

- **Entity** — 知識の対象（技術、プロジェクト、ライブラリ、概念など）
- **Topic** — Entity内のサブテーマによる分類（任意）
- **Document** — 個別の知見。Markdownファイル1つが1ドキュメント

## 物理構造

```
~/.memory/
  config.toml                 # 設定ファイル（任意。なくても動く）
  metadata.db                 # SQLite（メタデータ管理）
  index/                      # SearchIndex実装が管理する派生データ

  entities/
    elasticsearch/
      _entity.md              # Entityメタデータ
      mapping/                # topic
        nested-dense-vector.md
        dynamic-templates.md
      performance/            # topic
        shard-sizing.md
      reindex-strategy.md     # topicなしの直下ドキュメント
    rust/
      _entity.md
      anyhow-vs-thiserror.md
    llm/
      _entity.md
      llm-as-judge-bias.md
    misedoko/
      _entity.md
      api/                    # topic
        hotpepper-rate-limits.md
      curation-layer-design.md
    work-legaltech/
      _entity.md
      es-indexing-schema.md
```

## Entityメタデータ（`_entity.md`）

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

- `scope` は `knowledge`（汎用・再利用可能）または `project`（プロジェクト固有）
- 旧 `projects/` vs `knowledge/` のトップレベル分割はEntityの属性に移動

## ドキュメントのMarkdownフォーマット

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

- IDにはULIDを使用（時系列ソート可能）
- リンクはObsidian互換の `[[wikilink]]` 記法（本文中に記述）
- frontmatterの `links` フィールドは持たない

## パスの規約

- パス: `entities/<entity>/<topic>/<document>`
- 最大深さ: `entities/` 含めて4階層
- kebab-case、ASCII英数字+ハイフンのみ
- Topicは任意。ドキュメントが少なければEntityの直下に置く

## ファイル名の一意性

- ファイル名はvault全体でユニークである必要がある（Obsidian互換のため）
- `save_memory` 時にMCPサーバーがファイル名の重複をチェック
- 衝突した場合はエラーを返し、呼び出し元（Claude Code）がsuffixを付けてリトライする

## Topic分割の基準

- 同一Entity内のドキュメントが **~7件を超え**、かつ **明確なサブテーマ境界がある** 場合にTopicを作成
- 1–2件しか入らないTopicは作らず、Entityの直下に置く
- この基準はMCPツールのdescriptionに埋め込み、Claude Codeが自律的に従う

## config.toml

設定ファイルは任意。存在しない場合はすべてデフォルト値で動作する。

```toml
# メモリルートパス（デフォルト: ~/.memory/）
root = "~/.memory/"

# 検索バックエンド（デフォルト: "lancedb"）
# "lancedb" | "tantivy"
search_backend = "lancedb"
```

## tagsの役割

パスが「所属」を表すのに対し、tagsは **Entity/Topicを横断する関心事** に使う:

- 例: Entity `elasticsearch` + `tags: ["performance", "production-issue"]`
- "performance"タグで検索すれば、ES・Rust・LLM問わずパフォーマンス関連の知見が横断的にヒット
