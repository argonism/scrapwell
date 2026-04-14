# 実装プラン

## 設計方針

- **動くものを最速で手元に置く** — 検索バックエンド（Tantivy/LanceDB）は後回し。まず「保存・取得・一覧」が動くMCPサーバーを完成させ、そこに機能を積み上げる
- **interfaceによる厳密な抽象化** — `MemoryService` はtraitにのみ依存。具体実装（`FsMemoryStore`, `GithubMemoryStore` 等）の組み立ては `main.rs` だけが担う
- **ジェネリクスを採用** — `MemoryService<S, I>` と `ScrapwellHandler<S, I>` で型パラメータを使い、ゼロコスト抽象化を実現。具体型の選択は `main.rs` でのみ行い、それ以外の層は trait のみを参照する

---

## レイヤー構成（完成形）

```
main.rs  （DI: 全具体型を組み立て）
  └─ ScrapwellHandler<S, I>  （MCPツールのディスパッチのみ）
       └─ MemoryService<S, I> { store: S, index: I }
              ├─ S: trait MemoryStore  ←── FsMemoryStore      (Phase 1)
              │                       ←── GithubMemoryStore   (将来)
              └─ I: trait SearchIndex  ←── TantivySearchIndex (Phase 3)
                                      ←── LanceDbSearchIndex  (将来)
```

各層の責務:

| 層 | 責務 |
|---|---|
| `handler` | MCPパラメータの受け取りとサービスへの委譲のみ。ロジックを書かない |
| `service` | Store/Indexの協調、整合性の担保、ビジネスルールの実施。MCPの概念を持ち込まない |
| `store` | Markdownファイル読み書き、SQLiteメタデータ管理 |
| `index` | 全文検索インデックスの更新・クエリ |

---

## Phase 1 — 動くMCPサーバー（検索なし）

**ゴール**: `create_entity`, `save_memory`, `get_memory`, `list_memories` の4ツールが動く

### 1-1. `store/` レイヤー

```
crates/scrapwell-core/src/store/
  mod.rs    # trait MemoryStore
  fs.rs     # FsMemoryStore（Markdown + SQLite）
```

**`trait MemoryStore`**:
```rust
pub trait MemoryStore: Send + Sync {
    fn save_entity(&self, entity: &EntityMeta) -> Result<()>;
    fn get_entity(&self, id: &MemoryId) -> Result<Option<EntityMeta>>;
    fn update_entity(&self, id: &MemoryId, patch: &EntityPatch) -> Result<()>;
    fn delete_entity(&self, id: &MemoryId) -> Result<()>;
    fn list_entity_names(&self) -> Result<Vec<String>>;

    fn save(&self, entry: &MemoryEntry) -> Result<()>;
    fn get(&self, id: &MemoryId) -> Result<Option<MemoryEntry>>;
    fn update(&self, id: &MemoryId, patch: &MemoryPatch) -> Result<()>;
    fn delete(&self, id: &MemoryId) -> Result<()>;
    fn list_tree(&self, entity: Option<&str>, depth: u32) -> Result<TreeNode>;
    fn iter_all(&self) -> Result<Box<dyn Iterator<Item = Result<MemoryEntry>>>>;
    fn resolve_id(&self, id: &MemoryId) -> Result<Option<MemoryPath>>;
    fn check_name_unique(&self, name: &str) -> Result<bool>;
}
```

**`FsMemoryStore`** の実装内容:
- `new(root: PathBuf)` — SQLite初期化（WALモード）、テーブル作成
- `save` — frontmatter生成 → `.md`書き込み → SQLite INSERT（トランザクション）
- `get` — SQLiteでIDからパス解決 → `.md`読み込み → frontmatterパース
- `list_tree` — SQLite SELECTでEntity/Topicのツリーを組み立て
- `delete_entity` — SQLite CASCADE DELETE + ディレクトリ削除

依存追加: `rusqlite` (bundled feature)

### 1-2. `index/` レイヤー（ダミー実装）

検索機能は後回しだが、`MemoryService<S, I>` がPhase 1から `I: SearchIndex` を持つ設計を守る。Phase 1ではno-op実装を使う。

```
crates/scrapwell-core/src/index/
  mod.rs      # trait SearchIndex
  noop.rs     # NoopSearchIndex（何もしない）
```

```rust
pub trait SearchIndex: Send + Sync {
    fn upsert(&self, entry: &MemoryEntry) -> Result<()>;
    fn search(&self, query: &SearchQuery) -> Result<Vec<SearchHit>>;
    fn remove(&self, id: &MemoryId) -> Result<()>;
    fn rebuild(&self, entries: &mut dyn Iterator<Item = MemoryEntry>) -> Result<()>;
}

pub struct NoopSearchIndex;
impl SearchIndex for NoopSearchIndex { /* 全メソッドが Ok(()) / Ok(vec![]) */ }
```

### 1-3. `service/` レイヤー

```
crates/scrapwell-core/src/service/
  mod.rs    # MemoryService
```

```rust
pub struct MemoryService<S: MemoryStore, I: SearchIndex> {
    store: S,
    index: I,
}

impl<S: MemoryStore, I: SearchIndex> MemoryService<S, I> {
    pub fn new(store: S, index: I) -> Self { ... }
}
```

`save_memory` の実装例（Store + Index の協調がここに入る）:
```rust
pub fn save_memory(&self, ...) -> Result<MemoryId> {
    self.store.check_name_unique(&name)?;   // ビジネスルール
    let entry = MemoryEntry { ... };
    self.store.save(&entry)?;               // Markdown + SQLite
    self.index.upsert(&entry)?;             // 検索インデックス（Phase 1ではno-op）
    Ok(entry.id)
}
```

Phase 1で実装するメソッド: `create_entity`, `save_memory`, `get_memory`, `list_memories`

### 1-4. `handler.rs` — 4ツール実装

```rust
#[tool(description = "...")]
fn create_entity(&self, name: String, scope: String, ...) -> Result<CallToolResult>

#[tool(description = "...")]
fn save_memory(&self, entity: String, name: String, ...) -> Result<CallToolResult>

#[tool(description = "...")]
fn get_memory(&self, id: String) -> Result<CallToolResult>

#[tool(description = "...")]
fn list_memories(&self, entity: Option<String>, depth: Option<u32>) -> Result<CallToolResult>
```

- `description` にガイドラインを埋め込む（Topic分割基準、`list_memories` で確認してから保存 etc.）

### 1-5. `main.rs` — DI組み立て

```rust
// 具体型を直接渡す。Box不要。型は ScrapwellHandler<FsMemoryStore, NoopSearchIndex> に単態化される。
let root = dirs::home_dir().unwrap().join(".memory");
let store = FsMemoryStore::new(root)?;
let index = NoopSearchIndex;
let service = Arc::new(MemoryService::new(store, index));
let handler = ScrapwellHandler::new(service);
handler.serve(stdio()).await?.waiting().await?;
```

依存追加: `dirs`

**Phase 1 完了の確認**: `cargo build` が通り、Claude Codeから `save_memory` → `get_memory` が往復できる

---

## Phase 2 — Entity管理の完成

**ゴール**: 全6つのWriteツールが動く

- `update_entity` — description/scope/tags の部分更新
- `delete_entity` — カスケード削除（ドキュメント・Topicディレクトリごと削除）
- `update_memory` — content/title/tags の部分更新
- `delete_memory` — `.md`削除 + SQLite DELETE

### 類似名チェック

`create_entity` 時に編集距離で既存Entity名と比較:

```rust
// service::create_entity の内部
let existing = self.store.list_entity_names()?;
let similar: Vec<String> = existing
    .iter()
    .filter(|n| strsim::jaro_winkler(n, &req.name) > 0.85)
    .cloned()
    .collect();
if !similar.is_empty() {
    return Err(ScrapwellError::SimilarEntityExists(similar));
}
```

依存追加: `strsim`

**Phase 2 完了の確認**: 全6つのWriteツールが動く

---

## Phase 3 — 全文検索（Tantivy）

**ゴール**: `search_memory` が動く

```
crates/scrapwell-core/src/index/
  mod.rs       # trait SearchIndex（Phase 1から存在）
  noop.rs      # NoopSearchIndex（Phase 1から存在）
  tantivy.rs   # TantivySearchIndex（ここで追加）
```

**`TantivySearchIndex`** の実装内容:
- スキーマ: `id`, `entity`, `topic`, `title`, `content`, `tags`
- snippet生成: Tantivyのsnippet APIで `<<...>>` ハイライト
- インデックスは `~/.memory/index/` に永続化

`main.rs` の変更（`index` の差し替えのみ）:
```rust
// NoopSearchIndex → TantivySearchIndex に切り替え。MemoryService は変更なし。
let index = TantivySearchIndex::new(root.join("index"))?;
let service = Arc::new(MemoryService::new(store, index));
// 型は ScrapwellHandler<FsMemoryStore, TantivySearchIndex> に単態化される
```

`MemoryService` は変更なし。

`Cargo.toml` でfeature-gated:
```toml
[features]
default = ["tantivy-backend"]
tantivy-backend = ["dep:tantivy"]
lancedb-backend = ["dep:lancedb"]
```

依存追加: `tantivy` (feature-gated)

**Phase 3 完了の確認**: `search_memory` でキーワード検索→スニペット付きで返ってくる

---

## Phase 4 — Admin + 再構築

**ゴール**: `rebuild_index` が動く。本番運用できる品質

- `rebuild_index` MCPツール + `scrapwell rebuild` CLIサブコマンド
- `MemoryService::rebuild(target)` — `entities/` 走査 → SQLite再構築 → SearchIndex再構築
- `config.toml` 読み込み（rootパス、search_backendの切り替え）

依存追加: `toml`, `clap`（CLIサブコマンド用）

**Phase 4 完了の確認**: `rebuild_index` で壊れたインデックスが回復できる

---

## Phase 5 — 拡張（将来）

ロードマップに記載の拡張:

- `GithubMemoryStore` — `trait MemoryStore` の実装として追加。`MemoryService` は変更なし
- `LanceDbSearchIndex` — `trait SearchIndex` の実装として追加。`MemoryService` は変更なし
- タグベースの横断検索ツール
- HTTP/SSEトランスポート

---

## ファイル構成サマリー

```
crates/scrapwell-core/src/
  store/
    mod.rs        ← Phase 1（trait MemoryStore）
    fs.rs         ← Phase 1（FsMemoryStore）
  index/
    mod.rs        ← Phase 1（trait SearchIndex）
    noop.rs       ← Phase 1（NoopSearchIndex）
    tantivy.rs    ← Phase 3（TantivySearchIndex）
    lancedb.rs    ← Phase 5（LanceDbSearchIndex）
  service/
    mod.rs        ← Phase 1-4で段階的に拡充
  handler.rs      ← Phase 1-4で段階的に拡充
  model.rs        ← 現状維持（適宜拡充）
  path.rs         ← 現状維持
  error.rs        ← 適宜拡充
  lib.rs          ← pub useの更新
```

## 依存クレートサマリー

| Phase | 追加するクレート |
|---|---|
| 1 | `rusqlite` (bundled), `dirs` |
| 2 | `strsim` |
| 3 | `tantivy` (feature-gated) |
| 4 | `toml`, `clap` |
| 5 | `lancedb` (feature-gated) |
