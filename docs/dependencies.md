# 主要な依存crate

| crate | 用途 |
|---|---|
| `rmcp` or 自前実装 | MCP stdio transport |
| `lancedb` | 全文検索エンジン（デフォルトbackend） |
| `tantivy` | 全文検索エンジン（feature-gated alternative） |
| `rusqlite` | メタデータ管理（ID逆引き、ファイル名一意性チェック等） |
| `serde` / `serde_yaml` | frontmatter ser/de |
| `ulid` | 時系列ソート可能なID生成 |
| `chrono` | タイムスタンプ |
| `thiserror` | エラー型定義 |
| `toml` | config.toml 読み込み |
