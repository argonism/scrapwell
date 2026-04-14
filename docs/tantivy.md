# Tantivy 調査メモ（Phase 3 実装参考）

Tantivy は Apache Lucene にインスパイアされた Rust 製全文検索ライブラリ。
Phase 3 では `TantivySearchIndex` を実装し、`trait SearchIndex` を差し替える。

- **最新版**: 0.25.0（2025年現在）
- **リポジトリ**: https://github.com/quickwit-oss/tantivy

---

## スキーマ定義

### フィールドオプション

| オプション | 意味 |
|---|---|
| `TEXT` | トークン化してフルテキスト検索可能にする |
| `STRING` | トークン化しない（ID・カテゴリなど完全一致用） |
| `STORED` | ドキュメント取得時に値を復元できるようにする |
| `FAST` | ファセット・ソート用の高速フィールド（数値に多い） |

オプションは `|` で組み合わせる（例: `TEXT | STORED`）。

### Scrapwell 用スキーマ

```rust
use tantivy::schema::{Schema, STORED, STRING, TEXT};

fn build_schema() -> Schema {
    let mut b = Schema::builder();
    b.add_text_field("id",      STRING | STORED); // 完全一致・削除キー・取得用
    b.add_text_field("entity",  STRING | STORED); // フィルタリング用
    b.add_text_field("topic",   STRING | STORED); // フィルタリング用
    b.add_text_field("title",   TEXT   | STORED); // 全文検索 + 取得
    b.add_text_field("content", TEXT);             // 全文検索のみ（スニペット元）
    b.add_text_field("tags",    TEXT   | STORED); // 全文検索 + 取得
    b.build()
}
```

- `id` は `STRING`（トークン化なし）にしないと `delete_term` が機能しない
- `content` は `STORED` 不要（スニペット生成には元テキストが別途必要 → 後述）

---

## インデックスの初期化（永続化）

```rust
use tantivy::{Index, directory::MmapDirectory};

// ~/.memory/index/ に永続化
let dir = MmapDirectory::open(&index_path)?;
let index = Index::open_or_create(dir, schema.clone())?;
```

- `open_or_create` は既存インデックスがあれば開き、なければ作成する
- スキーマ変更時は古いインデックスを削除してから再作成が必要

---

## IndexWriter / IndexReader のライフサイクル

```rust
// writer: プロセスにつき1つだけ（ファイルロックで排他）
// 50 MB メモリバッファ
let writer = index.writer(50_000_000)?;

// reader: ReloadPolicy::OnCommitWithDelay でコミット後自動リロード
let reader = index
    .reader_builder()
    .reload_policy(tantivy::ReloadPolicy::OnCommitWithDelay)
    .try_into()?;
```

- `IndexWriter` は `Arc<Mutex<IndexWriter>>` でスレッド間共有する
- `reader.searcher()` を呼ぶたびに最新スナップショットの `Searcher` が得られる

---

## ドキュメントの追加・更新・削除

### 追加（upsert = 削除 + 追加）

Tantivy にネイティブな upsert はない。`delete_term` → `add_document` の順で実装する。

```rust
use tantivy::{Term, doc};

fn upsert(writer: &mut IndexWriter, schema: &Schema, entry: &MemoryEntry) -> tantivy::Result<()> {
    let id_field = schema.get_field("id").unwrap();

    // 既存ドキュメントを削除（存在しなければ no-op）
    writer.delete_term(Term::from_field_text(id_field, &entry.id.0));

    // 新規追加
    writer.add_document(doc!(
        id_field                                  => entry.id.0.as_str(),
        schema.get_field("entity").unwrap()       => entry.entity.as_str(),
        schema.get_field("topic").unwrap()        => entry.topic.as_deref().unwrap_or(""),
        schema.get_field("title").unwrap()        => entry.title.as_str(),
        schema.get_field("content").unwrap()      => entry.content.as_str(),
        schema.get_field("tags").unwrap()         => entry.tags.join(" ").as_str(),
    ))?;

    Ok(())
}
```

### コミット

```rust
writer.commit()?; // この時点でディスク永続化・検索可能化
```

### 削除

```rust
let id_field = schema.get_field("id").unwrap();
writer.delete_term(Term::from_field_text(id_field, &id.0));
writer.commit()?;
```

---

## 検索

### キーワード検索（全フィールド横断）

```rust
use tantivy::query::QueryParser;
use tantivy::collector::TopDocs;

let title   = schema.get_field("title").unwrap();
let content = schema.get_field("content").unwrap();
let tags    = schema.get_field("tags").unwrap();

let parser = QueryParser::for_index(&index, vec![title, content, tags]);
let query  = parser.parse_query(&search_query.query)?;

let searcher = reader.searcher();
let top_docs = searcher.search(&query, &TopDocs::with_limit(search_query.limit))?;
```

### entity でのフィルタリング

`entity` フィールドを `Must` 条件として追加する。

```rust
use tantivy::query::{BooleanQuery, Occur, TermQuery};
use tantivy::schema::IndexRecordOption;

fn build_query(
    index: &Index,
    schema: &Schema,
    q: &SearchQuery,
) -> tantivy::Result<Box<dyn tantivy::query::Query>> {
    let title   = schema.get_field("title").unwrap();
    let content = schema.get_field("content").unwrap();
    let tags    = schema.get_field("tags").unwrap();

    let parser   = QueryParser::for_index(index, vec![title, content, tags]);
    let kw_query = parser.parse_query(&q.query)?;

    if let Some(entity) = &q.entity {
        let entity_field = schema.get_field("entity").unwrap();
        let entity_term  = Term::from_field_text(entity_field, entity);
        let entity_query = TermQuery::new(entity_term, IndexRecordOption::Basic);

        Ok(Box::new(BooleanQuery::new(vec![
            (Occur::Must, kw_query),
            (Occur::Must, Box::new(entity_query)),
        ])))
    } else {
        Ok(kw_query)
    }
}
```

---

## スニペット生成（`<<ハイライト>>` 形式）

Tantivy の `SnippetGenerator` はデフォルトで HTML（`<b>` タグ）を出力するが、
`snippet.highlighted()` でハイライト範囲を取得して独自フォーマットに変換できる。

**重要**: `content` フィールドは `STORED` なしだとドキュメントから値を取り出せない。
スニペット生成には `SnippetGenerator::snippet_from_doc` を使うので、
`content` を `STORED` にするか、別途元テキストを渡す必要がある。

→ **実装判断**: `content` を `TEXT | STORED` にする（検索ヒット時に値が必要なため）。

```rust
use tantivy::snippet::SnippetGenerator;

fn generate_snippets(
    searcher: &tantivy::Searcher,
    query: &dyn tantivy::query::Query,
    schema: &Schema,
    doc_address: tantivy::DocAddress,
) -> tantivy::Result<Vec<String>> {
    let content_field = schema.get_field("content").unwrap();

    let snippet_gen = SnippetGenerator::create(searcher, query, content_field)?;
    let doc         = searcher.doc(doc_address)?;
    let snippet     = snippet_gen.snippet_from_doc(&doc);

    // <<ハイライト>> 形式に変換
    let fragment = snippet.fragment();
    let mut result = String::new();
    let mut prev   = 0usize;

    for (start, end) in snippet.highlighted() {
        result.push_str(&fragment[prev..start]);
        result.push_str("<<");
        result.push_str(&fragment[start..end]);
        result.push_str(">>");
        prev = end;
    }
    result.push_str(&fragment[prev..]);

    Ok(vec![result])
}
```

---

## エラー統合

`TantivyError` を `ScrapwellError` に追加する。

```rust
// error.rs
#[error("search index error: {0}")]
SearchIndex(#[from] tantivy::TantivyError),
```

`QueryParserError` は `TantivyError` に変換できないため、個別に対応：

```rust
// tantivy.rs 内
use tantivy::query::QueryParserError;

fn parse(q: &str) -> Result<Box<dyn Query>> {
    parser.parse_query(q).map_err(|e| {
        ScrapwellError::SearchIndex(tantivy::TantivyError::InvalidArgument(e.to_string()))
    })
}
```

---

## `TantivySearchIndex` 構造体の設計方針

```rust
pub struct TantivySearchIndex {
    schema:  Schema,
    index:   Index,
    writer:  Arc<Mutex<IndexWriter>>,
    reader:  IndexReader,
}

impl TantivySearchIndex {
    pub fn new(index_dir: PathBuf) -> Result<Self> {
        let schema = build_schema();
        let dir    = MmapDirectory::open(&index_dir)
            .map_err(|e| ScrapwellError::SearchIndex(e))?;
        let index  = Index::open_or_create(dir, schema.clone())
            .map_err(|e| ScrapwellError::SearchIndex(e))?;
        let writer = index.writer(50_000_000)
            .map_err(|e| ScrapwellError::SearchIndex(e))?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| ScrapwellError::SearchIndex(e))?;

        Ok(Self {
            schema,
            index,
            writer: Arc::new(Mutex::new(writer)),
            reader,
        })
    }
}
```

- `writer` は `Mutex` で包んで `&self` のままロックを取れるようにする
  （`trait SearchIndex` のシグネチャが `&self` のため）
- `reader` は `OnCommitWithDelay` で自動リロード。`reader.searcher()` で都度取得する

---

## rebuild の実装

```rust
fn rebuild(&self, entries: &mut dyn Iterator<Item = MemoryEntry>) -> Result<()> {
    let mut writer = self.writer.lock().unwrap();

    // 全ドキュメント削除
    writer.delete_all_documents()?;

    // 再追加
    for entry in entries {
        upsert(&mut writer, &self.schema, &entry)?;
    }

    writer.commit()?;
    Ok(())
}
```

---

## Cargo.toml への追加

```toml
# scrapwell-core/Cargo.toml

[features]
default = ["tantivy-backend"]
tantivy-backend = ["dep:tantivy"]

[dependencies]
tantivy = { version = "0.25", optional = true }
```

条件コンパイル:

```rust
// index/mod.rs
#[cfg(feature = "tantivy-backend")]
pub mod tantivy_index;
```

---

## 注意事項・落とし穴

1. **スキーマ変更時はインデックス再作成が必要**  
   既存インデックスとスキーマが合わない場合 `IncompatibleIndex` エラー。
   `rebuild_index` ツール（Phase 4）でカバーする。

2. **`content` は `TEXT | STORED` にする**  
   スニペット生成に `snippet_from_doc` を使うため、`content` の値が必要。

3. **`tags` はスペース区切りで結合して投入**  
   `Vec<String>` を `entry.tags.join(" ")` で結合してから `TEXT` フィールドに入れる。
   タグ完全一致検索が必要な場合は別途 `tags_exact` フィールド（`STRING | STORED`）の追加を検討。

4. **IndexWriter はプロセスにつき1つ**  
   `MmapDirectory::open` + `index.writer()` は1プロセス1つしか持てない。
   `Arc<Mutex<IndexWriter>>` で共有し、`writer.lock()` で排他制御する。

5. **`QueryParserError` は `TantivyError` ではない**  
   `map_err` で `TantivyError::InvalidArgument` に変換するのが最もシンプル。
