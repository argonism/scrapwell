# ロードマップ

## 未実装・将来の拡張

- **タグベースの横断検索ツール** — 専用のMCPツールとして追加予定
- **LanceDB backend** — embedding検索が必要になった場合にfeature-gatedで追加
- **HTTP/SSEトランスポート** — Claude Code以外のクライアント対応時
- **エクスポート/インポート** — 他ツールとの連携
- **メモリの有効期限・鮮度管理** — outdatedタグの自動付与等

## 次のステップ

1. `FsMemoryStore` の具体実装（frontmatter ser/de、IDマップ管理）
2. `main.rs` のMCP stdioトランスポート層
3. `TantivySearchIndex` の実装
4. 統合テスト
