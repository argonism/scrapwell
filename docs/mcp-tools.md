# MCPツールインターフェース

トランスポート: **stdio**（Claude Codeとの接続に最適）

全10ツール: Write 6 + Read 3 + Admin 1

## Writeツール

### create_entity

新規Entityを作成。類似名チェック（編集距離）を行い、既存の類似Entityがあればエラーで候補を返す。

| パラメータ | 型 | 必須 | 説明 |
|---|---|---|---|
| name | string | yes | Entity名（例: `"elasticsearch"`） |
| scope | string | yes | `"knowledge"` または `"project"` |
| description | string | no | Entityの説明 |
| tags | string[] | no | Entityレベルのタグ |

返却: `{ id: string }`

類似名が存在する場合:

```json
{
  "error": "similar_entity_exists",
  "message": "Entity 'elastic-search' is similar to existing entities",
  "suggestions": ["elasticsearch"]
}
```

### update_entity

既存Entityの部分更新。指定されたフィールドのみ変更。

| パラメータ | 型 | 必須 | 説明 |
|---|---|---|---|
| id | string | yes | 対象EntityのID |
| scope | string | no | 新しいスコープ |
| description | string | no | 新しい説明 |
| tags | string[] | no | 新しいタグ（全置換） |

### delete_entity

Entityを削除。配下のドキュメント・Topicディレクトリもすべてカスケード削除する。SQLiteレコード、SearchIndexエントリ、Markdownファイルをすべて除去。

| パラメータ | 型 | 必須 | 説明 |
|---|---|---|---|
| id | string | yes | 対象EntityのID |

### save_memory

新規ドキュメントを保存。Topicディレクトリは自動作成される。存在しないEntityへの保存はエラー。

| パラメータ | 型 | 必須 | 説明 |
|---|---|---|---|
| entity | string | yes | Entity名（例: `"elasticsearch"`） |
| name | string | yes | ドキュメントのファイル名（例: `"nested-dense-vector"`） |
| title | string | yes | ドキュメントのタイトル |
| content | string | yes | Markdown本文（`[[wikilink]]` を含めてよい） |
| topic | string | no | Topic名（例: `"mapping"`） |
| tags | string[] | no | 横断的タグ |

返却: `{ id: string }`

ファイル名がvault内で重複する場合はエラーを返す。呼び出し元がsuffixを付けてリトライすること。

ツールdescriptionに以下のガイドラインを埋め込む:

- `create_entity` で事前にEntityを作成すること
- Topic分割の基準（~7件超 + 明確なサブテーマ境界）
- `list_memories` で既存構造を確認してから保存すること
- tagsにはパス情報を重複させないこと

### update_memory

既存ドキュメントの部分更新。指定されたフィールドのみ変更。

| パラメータ | 型 | 必須 | 説明 |
|---|---|---|---|
| id | string | yes | 対象ドキュメントのID |
| title | string | no | 新しいタイトル |
| content | string | no | 新しい本文 |
| tags | string[] | no | 新しいタグ（全置換） |

### delete_memory

ドキュメントを削除。Markdownファイルとインデックスエントリを除去。

| パラメータ | 型 | 必須 | 説明 |
|---|---|---|---|
| id | string | yes | 対象ドキュメントのID |

## Readツール

### search_memory

全文検索。クエリにマッチした箇所をハイライト付きスニペットで返す。

| パラメータ | 型 | 必須 | 説明 |
|---|---|---|---|
| query | string | yes | 検索クエリ |
| entity | string | no | Entity絞り込み（例: `"elasticsearch"`） |
| limit | number | no | 最大件数（デフォルト10） |

返却:

```json
[
  {
    "id": "01JF3X...",
    "entity": "elasticsearch",
    "topic": "mapping",
    "name": "nested-dense-vector",
    "title": "nested + dense_vector mapping",
    "tags": ["performance"],
    "snippets": ["...nested fieldで<<dense_vector>>を定義する際..."],
    "score": 0.85
  }
]
```

`<<...>>` でクエリマッチ箇所をハイライト。

### list_memories

Entity/Topicのツリー構造を返す。save前に既存構造を把握するために使う。

| パラメータ | 型 | 必須 | 説明 |
|---|---|---|---|
| entity | string | no | 特定Entityに絞り込み（Noneで全体） |
| depth | number | no | 展開深さ（デフォルト2） |

返却例:

```
elasticsearch/ (5 documents)
  mapping/ (2 documents)
  performance/ (1 document)
rust/ (3 documents)
llm/ (4 documents)
misedoko/ (6 documents)
  api/ (2 documents)
work-legaltech/ (3 documents)
```

### get_memory

単一ドキュメントの全内容を取得。

| パラメータ | 型 | 必須 | 説明 |
|---|---|---|---|
| id | string | yes | ドキュメントID |

返却: ドキュメント全体（frontmatter + 本文）

## Adminツール

### rebuild_index

Markdownファイルからメタデータ（SQLite）と検索インデックスを再構築する。データの不整合やインデックス破損時に使用。

| パラメータ | 型 | 必須 | 説明 |
|---|---|---|---|
| target | string | no | `"metadata"`, `"search"`, `"all"`（デフォルト: `"all"`） |

返却: `{ entities: number, documents: number }` — 再構築されたレコード数
