use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Mutex,
};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::de::DeserializeOwned;

use crate::{
    error::{Result, ScrapwellError},
    model::{
        DocumentFrontmatter, EntityFrontmatter, EntityMeta, EntityPatch, MemoryEntry, MemoryId,
        MemoryPatch, Scope, TreeNode,
    },
    path::MemoryPath,
};
use super::MemoryStore;

pub struct FsMemoryStore {
    root: PathBuf,
    conn: Mutex<Connection>,
}

impl FsMemoryStore {
    pub fn new(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&root)?;
        std::fs::create_dir_all(root.join("entities"))?;

        let db_path = root.join("metadata.db");
        let conn = Connection::open(&db_path)?;

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS entities (
                id          TEXT PRIMARY KEY,
                name        TEXT UNIQUE NOT NULL,
                scope       TEXT NOT NULL CHECK(scope IN ('knowledge', 'project')),
                description TEXT,
                tags        TEXT NOT NULL DEFAULT '[]',
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS documents (
                id          TEXT PRIMARY KEY,
                name        TEXT UNIQUE NOT NULL,
                entity_id   TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
                topic       TEXT,
                title       TEXT NOT NULL,
                tags        TEXT NOT NULL DEFAULT '[]',
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_documents_entity_id ON documents(entity_id);
            CREATE INDEX IF NOT EXISTS idx_documents_name      ON documents(name);
            CREATE INDEX IF NOT EXISTS idx_entities_name       ON entities(name);",
        )?;

        Ok(Self {
            root,
            conn: Mutex::new(conn),
        })
    }
}

// ---------- ヘルパー関数 ----------

fn scope_to_str(scope: Scope) -> &'static str {
    match scope {
        Scope::Knowledge => "knowledge",
        Scope::Project => "project",
    }
}

fn str_to_scope(s: &str) -> Result<Scope> {
    match s {
        "knowledge" => Ok(Scope::Knowledge),
        "project" => Ok(Scope::Project),
        other => Err(ScrapwellError::InvalidPath(format!("unknown scope: {}", other))),
    }
}

fn tags_to_json(tags: &[String]) -> Result<String> {
    Ok(serde_json::to_string(tags)?)
}

fn json_to_tags(s: &str) -> Result<Vec<String>> {
    Ok(serde_json::from_str(s)?)
}

fn parse_datetime(s: &str) -> Result<DateTime<Utc>> {
    s.parse::<DateTime<Utc>>()
        .map_err(|e| ScrapwellError::InvalidPath(format!("invalid datetime '{}': {}", s, e)))
}

/// Markdown ファイルに書き出す文字列を生成する。
/// serde_yaml は末尾に \n を付けるため、区切り `---` はそのまま続く。
fn render_md<T: serde::Serialize>(fm: &T, body: &str) -> Result<String> {
    let yaml = serde_yaml::to_string(fm)?;
    if body.is_empty() {
        Ok(format!("---\n{}---\n", yaml))
    } else {
        Ok(format!("---\n{}---\n\n{}", yaml, body))
    }
}

/// `---\n{yaml}\n---\n` 形式の frontmatter をパースして (frontmatter, body) を返す。
fn parse_frontmatter<T: DeserializeOwned>(src: &str) -> Result<(T, String)> {
    let src = src.trim_start_matches('\n');
    let rest = src
        .strip_prefix("---\n")
        .ok_or_else(|| ScrapwellError::InvalidPath("missing frontmatter '---'".into()))?;
    let end = rest
        .find("\n---\n")
        .ok_or_else(|| ScrapwellError::InvalidPath("unclosed frontmatter".into()))?;
    let yaml = &rest[..end];
    let body = rest[end + 5..].trim_start_matches('\n').to_string();
    let fm = serde_yaml::from_str(yaml)?;
    Ok((fm, body))
}

fn map_constraint_err(e: rusqlite::Error, name: &str) -> ScrapwellError {
    if let rusqlite::Error::SqliteFailure(ref err, _) = e {
        if err.code == rusqlite::ErrorCode::ConstraintViolation {
            return ScrapwellError::DuplicateName(name.to_string());
        }
    }
    ScrapwellError::Database(e)
}

// ---------- MemoryStore 実装 ----------

impl MemoryStore for FsMemoryStore {
    fn save_entity(&self, entity: &EntityMeta) -> Result<()> {
        // 1. ディレクトリ + _entity.md を書き出す
        let entity_dir = self.root.join("entities").join(&entity.name);
        std::fs::create_dir_all(&entity_dir)?;

        let fm = EntityFrontmatter {
            id: entity.id.0.clone(),
            scope: entity.scope,
            tags: entity.tags.clone(),
            created_at: entity.created_at,
            updated_at: entity.updated_at,
        };
        let body = entity.description.as_deref().unwrap_or("");
        std::fs::write(entity_dir.join("_entity.md"), render_md(&fm, body)?)?;

        // 2. SQLite INSERT
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO entities (id, name, scope, description, tags, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                entity.id.0,
                entity.name,
                scope_to_str(entity.scope),
                entity.description,
                tags_to_json(&entity.tags)?,
                entity.created_at.to_rfc3339(),
                entity.updated_at.to_rfc3339(),
            ],
        )
        .map_err(|e| map_constraint_err(e, &entity.name))?;

        Ok(())
    }

    fn get_entity_by_name(&self, name: &str) -> Result<Option<EntityMeta>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT id, name, scope, description, tags, created_at, updated_at
             FROM entities WHERE name = ?1",
            params![name],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        );

        match result {
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(ScrapwellError::Database(e)),
            Ok((id, name, scope_str, description, tags_json, created_str, updated_str)) => {
                Ok(Some(EntityMeta {
                    id: MemoryId(id),
                    name,
                    scope: str_to_scope(&scope_str)?,
                    description,
                    tags: json_to_tags(&tags_json)?,
                    created_at: parse_datetime(&created_str)?,
                    updated_at: parse_datetime(&updated_str)?,
                }))
            }
        }
    }

    fn save(&self, entry: &MemoryEntry) -> Result<()> {
        // 1. entity_id を取得（別スコープでロック解放）
        let entity_id: String = {
            let conn = self.conn.lock().unwrap();
            conn.query_row(
                "SELECT id FROM entities WHERE name = ?1",
                params![entry.entity],
                |row| row.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    ScrapwellError::NotFound(format!("entity '{}'", entry.entity))
                }
                other => ScrapwellError::Database(other),
            })?
        };

        // 2. Markdown ファイルを書き出す（ロックなし）
        let path = MemoryPath::new(&entry.entity, entry.topic.as_deref(), &entry.name)?;
        let fs_path = path.to_fs_path(&self.root);
        if let Some(parent) = fs_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let fm = DocumentFrontmatter {
            id: entry.id.0.clone(),
            title: entry.title.clone(),
            tags: entry.tags.clone(),
            created_at: entry.created_at,
            updated_at: entry.updated_at,
        };
        std::fs::write(&fs_path, render_md(&fm, &entry.content)?)?;

        // 3. SQLite INSERT
        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO documents (id, name, entity_id, topic, title, tags, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    entry.id.0,
                    entry.name,
                    entity_id,
                    entry.topic,
                    entry.title,
                    tags_to_json(&entry.tags)?,
                    entry.created_at.to_rfc3339(),
                    entry.updated_at.to_rfc3339(),
                ],
            )
            .map_err(|e| map_constraint_err(e, &entry.name))?;
        }

        Ok(())
    }

    fn get(&self, id: &MemoryId) -> Result<Option<MemoryEntry>> {
        // メタデータを SQLite から取得
        let row = {
            let conn = self.conn.lock().unwrap();
            let result = conn.query_row(
                "SELECT d.id, e.name, d.topic, d.name, d.title, d.tags, d.created_at, d.updated_at
                 FROM documents d JOIN entities e ON d.entity_id = e.id
                 WHERE d.id = ?1",
                params![id.0],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            );
            match result {
                Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
                Err(e) => return Err(ScrapwellError::Database(e)),
                Ok(row) => row,
            }
        };

        let (doc_id, entity_name, topic, name, title, tags_json, created_str, updated_str) = row;

        // Markdown ファイルから本文を読む（ロックなし）
        let path = MemoryPath::new(&entity_name, topic.as_deref(), &name)?;
        let fs_path = path.to_fs_path(&self.root);
        let file_content = std::fs::read_to_string(&fs_path)?;
        let (_fm, content) = parse_frontmatter::<DocumentFrontmatter>(&file_content)?;

        Ok(Some(MemoryEntry {
            id: MemoryId(doc_id),
            entity: entity_name,
            topic,
            name,
            title,
            content,
            tags: json_to_tags(&tags_json)?,
            created_at: parse_datetime(&created_str)?,
            updated_at: parse_datetime(&updated_str)?,
        }))
    }

    fn list_tree(&self, entity: Option<&str>, depth: u32) -> Result<TreeNode> {
        let conn = self.conn.lock().unwrap();

        // (entity_name, topic_opt, count) の行を取得
        // stmt のライフタイムを確実にブロック内で終わらせるため、変数に束縛してから返す
        let rows: Vec<(String, Option<String>, usize)> = if let Some(entity_name) = entity {
            let mut stmt = conn.prepare(
                "SELECT e.name, d.topic, COUNT(d.id) as cnt
                 FROM entities e
                 LEFT JOIN documents d ON d.entity_id = e.id
                 WHERE e.name = ?1
                 GROUP BY e.id, e.name, d.topic
                 ORDER BY d.topic",
            )?;
            let result = stmt
                .query_map(params![entity_name], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, usize>(2)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            result
        } else {
            let mut stmt = conn.prepare(
                "SELECT e.name, d.topic, COUNT(d.id) as cnt
                 FROM entities e
                 LEFT JOIN documents d ON d.entity_id = e.id
                 GROUP BY e.id, e.name, d.topic
                 ORDER BY e.name, d.topic",
            )?;
            let result = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, usize>(2)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            result
        };

        // entity_name → (total_count, topic_children)
        let mut entity_map: BTreeMap<String, (usize, Vec<TreeNode>)> = BTreeMap::new();

        for (entity_name, topic_opt, count) in rows {
            let entry = entity_map.entry(entity_name).or_insert((0, vec![]));
            entry.0 += count;
            if let Some(topic_name) = topic_opt {
                if depth >= 2 {
                    entry.1.push(TreeNode {
                        name: topic_name,
                        document_count: count,
                        children: vec![],
                    });
                }
            }
        }

        if let Some(entity_name) = entity {
            let (count, children) = entity_map.remove(entity_name).unwrap_or_default();
            Ok(TreeNode {
                name: entity_name.to_string(),
                document_count: count,
                children,
            })
        } else {
            let children = entity_map
                .into_iter()
                .map(|(name, (count, topics))| TreeNode {
                    name,
                    document_count: count,
                    children: topics,
                })
                .collect();
            Ok(TreeNode {
                name: String::new(),
                document_count: 0,
                children,
            })
        }
    }

    fn check_name_unique(&self, name: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM documents WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )?;
        Ok(count == 0)
    }

    fn list_entity_names(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT name FROM entities ORDER BY name")?;
        let result = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(result)
    }

    fn update_entity(&self, id: &MemoryId, patch: &EntityPatch) -> Result<()> {
        // 1. 現在の Entity 情報を SQLite から取得
        let (name, current_scope_str, current_description, current_tags_json, created_at_str) = {
            let conn = self.conn.lock().unwrap();
            conn.query_row(
                "SELECT name, scope, description, tags, created_at FROM entities WHERE id = ?1",
                params![id.0],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => ScrapwellError::NotFound(id.0.clone()),
                other => ScrapwellError::Database(other),
            })?
        };

        // 2. パッチを適用
        let new_scope = patch.scope.unwrap_or_else(|| str_to_scope(&current_scope_str).unwrap());
        let new_description = patch
            .description
            .as_ref()
            .map(|s| s.clone())
            .or(current_description);
        let new_tags = patch
            .tags
            .as_ref()
            .cloned()
            .unwrap_or_else(|| json_to_tags(&current_tags_json).unwrap_or_default());
        let created_at = parse_datetime(&created_at_str)?;
        let now = chrono::Utc::now();

        // 3. _entity.md を更新
        let entity_dir = self.root.join("entities").join(&name);
        let fm = EntityFrontmatter {
            id: id.0.clone(),
            scope: new_scope,
            tags: new_tags.clone(),
            created_at,
            updated_at: now,
        };
        let body = new_description.as_deref().unwrap_or("");
        std::fs::write(entity_dir.join("_entity.md"), render_md(&fm, body)?)?;

        // 4. SQLite を更新
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE entities SET scope=?1, description=?2, tags=?3, updated_at=?4 WHERE id=?5",
            params![
                scope_to_str(new_scope),
                new_description,
                tags_to_json(&new_tags)?,
                now.to_rfc3339(),
                id.0,
            ],
        )?;

        Ok(())
    }

    fn delete_entity(&self, id: &MemoryId) -> Result<()> {
        // 1. Entity 名を取得（ディレクトリパスの構築に必要）
        let entity_name: String = {
            let conn = self.conn.lock().unwrap();
            conn.query_row(
                "SELECT name FROM entities WHERE id = ?1",
                params![id.0],
                |row| row.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => ScrapwellError::NotFound(id.0.clone()),
                other => ScrapwellError::Database(other),
            })?
        };

        // 2. SQLite から削除（ON DELETE CASCADE でドキュメントも削除される）
        {
            let conn = self.conn.lock().unwrap();
            conn.execute("DELETE FROM entities WHERE id = ?1", params![id.0])?;
        }

        // 3. エンティティディレクトリをまるごと削除
        let entity_dir = self.root.join("entities").join(&entity_name);
        if entity_dir.exists() {
            std::fs::remove_dir_all(&entity_dir)?;
        }

        Ok(())
    }

    fn update(&self, id: &MemoryId, patch: &MemoryPatch) -> Result<()> {
        // 1. ドキュメントのメタデータを SQLite から取得
        let (entity_name, topic, name, current_title, current_tags_json, created_at_str) = {
            let conn = self.conn.lock().unwrap();
            conn.query_row(
                "SELECT e.name, d.topic, d.name, d.title, d.tags, d.created_at
                 FROM documents d JOIN entities e ON d.entity_id = e.id
                 WHERE d.id = ?1",
                params![id.0],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => ScrapwellError::NotFound(id.0.clone()),
                other => ScrapwellError::Database(other),
            })?
        };

        // 2. Markdown ファイルから現在の本文を読む
        let mem_path = MemoryPath::new(&entity_name, topic.as_deref(), &name)?;
        let fs_path = mem_path.to_fs_path(&self.root);
        let file_content = std::fs::read_to_string(&fs_path)?;
        let (_fm, current_content) = parse_frontmatter::<DocumentFrontmatter>(&file_content)?;

        // 3. パッチを適用
        let new_title = patch.title.as_ref().unwrap_or(&current_title).clone();
        let new_content = patch.content.as_ref().unwrap_or(&current_content).clone();
        let new_tags = patch
            .tags
            .as_ref()
            .cloned()
            .unwrap_or_else(|| json_to_tags(&current_tags_json).unwrap_or_default());
        let created_at = parse_datetime(&created_at_str)?;
        let now = chrono::Utc::now();

        // 4. Markdown ファイルを更新
        let fm = DocumentFrontmatter {
            id: id.0.clone(),
            title: new_title.clone(),
            tags: new_tags.clone(),
            created_at,
            updated_at: now,
        };
        std::fs::write(&fs_path, render_md(&fm, &new_content)?)?;

        // 5. SQLite を更新
        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "UPDATE documents SET title=?1, tags=?2, updated_at=?3 WHERE id=?4",
                params![
                    new_title,
                    tags_to_json(&new_tags)?,
                    now.to_rfc3339(),
                    id.0,
                ],
            )?;
        }

        Ok(())
    }

    fn delete(&self, id: &MemoryId) -> Result<()> {
        // 1. ドキュメントの場所を SQLite から取得
        let (entity_name, topic, name): (String, Option<String>, String) = {
            let conn = self.conn.lock().unwrap();
            conn.query_row(
                "SELECT e.name, d.topic, d.name
                 FROM documents d JOIN entities e ON d.entity_id = e.id
                 WHERE d.id = ?1",
                params![id.0],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => ScrapwellError::NotFound(id.0.clone()),
                other => ScrapwellError::Database(other),
            })?
        };

        // 2. SQLite から削除
        {
            let conn = self.conn.lock().unwrap();
            conn.execute("DELETE FROM documents WHERE id = ?1", params![id.0])?;
        }

        // 3. Markdown ファイルを削除
        let path = MemoryPath::new(&entity_name, topic.as_deref(), &name)?;
        let fs_path = path.to_fs_path(&self.root);
        if fs_path.exists() {
            std::fs::remove_file(&fs_path)?;
        }

        Ok(())
    }

    fn iter_all(&self) -> Result<Vec<MemoryEntry>> {
        // 関数スコープで conn/stmt を宣言し、collect 後に明示的に drop してから
        // ファイル読み込みを行う（ネストブロック内の ? は一時値の借用問題を引き起こすため回避）
        type Row = (String, String, Option<String>, String, String, String, String, String);

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT d.id, e.name, d.topic, d.name, d.title, d.tags, d.created_at, d.updated_at
             FROM documents d JOIN entities e ON d.entity_id = e.id
             ORDER BY e.name, d.topic, d.name",
        )?;
        let rows: Vec<Row> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        // ? のセミコロンで一時値が破棄された後にロックを解放
        drop(stmt);
        drop(conn);

        let mut entries = Vec::with_capacity(rows.len());
        for (doc_id, entity_name, topic, name, title, tags_json, created_str, updated_str) in rows {
            let path = MemoryPath::new(&entity_name, topic.as_deref(), &name)?;
            let fs_path = path.to_fs_path(&self.root);
            // ファイルが見つからない場合はスキップ（SQLite と FS の不整合）
            let file_content = match std::fs::read_to_string(&fs_path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let (_fm, content) = parse_frontmatter::<DocumentFrontmatter>(&file_content)?;
            entries.push(MemoryEntry {
                id: MemoryId(doc_id),
                entity: entity_name,
                topic,
                name,
                title,
                content,
                tags: json_to_tags(&tags_json)?,
                created_at: parse_datetime(&created_str)?,
                updated_at: parse_datetime(&updated_str)?,
            });
        }
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ScrapwellError;
    use chrono::Utc;
    use tempfile::TempDir;

    // ---------- テスト用ヘルパー ----------

    fn make_store() -> (FsMemoryStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = FsMemoryStore::new(dir.path().to_path_buf()).unwrap();
        (store, dir)
    }

    fn sample_entity(name: &str) -> EntityMeta {
        let now = Utc::now();
        EntityMeta {
            id: MemoryId::new(),
            name: name.to_string(),
            scope: Scope::Knowledge,
            description: Some(format!("About {}", name)),
            tags: vec!["test-tag".to_string()],
            created_at: now,
            updated_at: now,
        }
    }

    fn sample_entry(entity: &str, name: &str, topic: Option<&str>) -> MemoryEntry {
        let now = Utc::now();
        MemoryEntry {
            id: MemoryId::new(),
            entity: entity.to_string(),
            topic: topic.map(|t| t.to_string()),
            name: name.to_string(),
            title: format!("Title of {}", name),
            content: format!("Content of {}", name),
            tags: vec!["entry-tag".to_string()],
            created_at: now,
            updated_at: now,
        }
    }

    // ---------- CRUD 往復 ----------

    #[test]
    fn save_entity_and_get_by_name() {
        let (store, _dir) = make_store();
        let entity = sample_entity("elasticsearch");

        store.save_entity(&entity).unwrap();

        let got = store.get_entity_by_name("elasticsearch").unwrap().unwrap();
        assert_eq!(got.id, entity.id);
        assert_eq!(got.name, "elasticsearch");
        assert_eq!(got.scope, Scope::Knowledge);
        assert_eq!(got.description, Some("About elasticsearch".to_string()));
        assert_eq!(got.tags, entity.tags);
    }

    #[test]
    fn get_entity_by_name_returns_none_when_missing() {
        let (store, _dir) = make_store();
        let result = store.get_entity_by_name("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn save_and_get_document_roundtrip() {
        let (store, _dir) = make_store();
        let entity = sample_entity("rust");
        store.save_entity(&entity).unwrap();

        let entry = MemoryEntry {
            id: MemoryId::new(),
            entity: "rust".to_string(),
            topic: Some("async".to_string()),
            name: "tokio-basics".to_string(),
            title: "Tokio basics".to_string(),
            content: "# Tokio\n\nThis is about tokio.\n\n[[anyhow-vs-thiserror]]".to_string(),
            tags: vec!["async".to_string(), "runtime".to_string()],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let id = entry.id.clone();

        store.save(&entry).unwrap();

        let got = store.get(&id).unwrap().unwrap();
        assert_eq!(got.id, entry.id);
        assert_eq!(got.entity, "rust");
        assert_eq!(got.topic, Some("async".to_string()));
        assert_eq!(got.name, "tokio-basics");
        assert_eq!(got.title, "Tokio basics");
        assert_eq!(got.content, entry.content);
        assert_eq!(got.tags, entry.tags);
    }

    #[test]
    fn get_returns_none_when_missing() {
        let (store, _dir) = make_store();
        let result = store.get(&MemoryId("NONEXISTENT".to_string())).unwrap();
        assert!(result.is_none());
    }

    // ---------- ファイル構造 ----------

    #[test]
    fn save_entity_creates_correct_files() {
        let (store, dir) = make_store();
        let entity = sample_entity("elasticsearch");
        store.save_entity(&entity).unwrap();

        let entity_dir = dir.path().join("entities/elasticsearch");
        assert!(entity_dir.is_dir(), "entity directory should exist");

        let entity_md = entity_dir.join("_entity.md");
        assert!(entity_md.exists(), "_entity.md should exist");

        let content = std::fs::read_to_string(&entity_md).unwrap();
        assert!(content.contains("scope: knowledge"), "frontmatter should have scope");
        assert!(content.contains("About elasticsearch"), "body should have description");
    }

    #[test]
    fn save_document_creates_correct_files() {
        let (store, dir) = make_store();
        store.save_entity(&sample_entity("elasticsearch")).unwrap();

        let entry = MemoryEntry {
            id: MemoryId::new(),
            entity: "elasticsearch".to_string(),
            topic: Some("mapping".to_string()),
            name: "nested-dense-vector".to_string(),
            title: "Nested + Dense Vector".to_string(),
            content: "Elasticsearch でのネスト構造の説明".to_string(),
            tags: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.save(&entry).unwrap();

        let doc_path = dir
            .path()
            .join("entities/elasticsearch/mapping/nested-dense-vector.md");
        assert!(doc_path.exists(), "document file should exist");

        let content = std::fs::read_to_string(&doc_path).unwrap();
        assert!(content.contains("Nested + Dense Vector"), "title should be in frontmatter");
        assert!(
            content.contains("Elasticsearch でのネスト構造の説明"),
            "body should be present"
        );
    }

    // ---------- list_tree ----------

    #[test]
    fn list_tree_all_entities() {
        let (store, _dir) = make_store();
        store.save_entity(&sample_entity("rust")).unwrap();
        store.save_entity(&sample_entity("elasticsearch")).unwrap();

        store.save(&sample_entry("rust", "anyhow", None)).unwrap();
        store.save(&sample_entry("rust", "thiserror", None)).unwrap();
        store.save(&sample_entry("elasticsearch", "shards", None)).unwrap();

        let tree = store.list_tree(None, 2).unwrap();

        assert_eq!(tree.children.len(), 2);
        let rust = tree.children.iter().find(|n| n.name == "rust").unwrap();
        let es = tree.children.iter().find(|n| n.name == "elasticsearch").unwrap();
        assert_eq!(rust.document_count, 2);
        assert_eq!(es.document_count, 1);
    }

    #[test]
    fn list_tree_with_topics() {
        let (store, _dir) = make_store();
        store.save_entity(&sample_entity("elasticsearch")).unwrap();

        store.save(&sample_entry("elasticsearch", "nested-vector", Some("mapping"))).unwrap();
        store.save(&sample_entry("elasticsearch", "dynamic-templates", Some("mapping"))).unwrap();
        store.save(&sample_entry("elasticsearch", "reindex-strategy", None)).unwrap();

        let tree = store.list_tree(Some("elasticsearch"), 2).unwrap();

        assert_eq!(tree.name, "elasticsearch");
        assert_eq!(tree.document_count, 3);
        assert_eq!(tree.children.len(), 1, "topic 'mapping' should appear as child");
        assert_eq!(tree.children[0].name, "mapping");
        assert_eq!(tree.children[0].document_count, 2);
    }

    #[test]
    fn list_tree_entity_filter() {
        let (store, _dir) = make_store();
        store.save_entity(&sample_entity("rust")).unwrap();
        store.save_entity(&sample_entity("elasticsearch")).unwrap();
        store.save(&sample_entry("rust", "anyhow", None)).unwrap();
        store.save(&sample_entry("elasticsearch", "shards", None)).unwrap();

        let tree = store.list_tree(Some("rust"), 2).unwrap();

        assert_eq!(tree.name, "rust");
        assert_eq!(tree.document_count, 1);
        assert!(tree.children.is_empty(), "no topics expected");
    }

    #[test]
    fn list_tree_depth_1_hides_topics() {
        let (store, _dir) = make_store();
        store.save_entity(&sample_entity("elasticsearch")).unwrap();
        store.save(&sample_entry("elasticsearch", "nested-vector", Some("mapping"))).unwrap();

        let tree = store.list_tree(Some("elasticsearch"), 1).unwrap();

        assert_eq!(tree.document_count, 1);
        assert!(tree.children.is_empty(), "depth=1 should not show topics");
    }

    // ---------- 一意性チェック ----------

    #[test]
    fn check_name_unique_returns_true_for_new_name() {
        let (store, _dir) = make_store();
        assert!(store.check_name_unique("nonexistent-doc").unwrap());
    }

    #[test]
    fn check_name_unique_returns_false_after_save() {
        let (store, _dir) = make_store();
        store.save_entity(&sample_entity("rust")).unwrap();
        store.save(&sample_entry("rust", "anyhow", None)).unwrap();

        assert!(!store.check_name_unique("anyhow").unwrap());
    }

    // ---------- エラーケース ----------

    #[test]
    fn save_to_nonexistent_entity_is_not_found() {
        let (store, _dir) = make_store();
        let entry = sample_entry("nonexistent", "foo", None);
        let err = store.save(&entry).unwrap_err();
        assert!(matches!(err, ScrapwellError::NotFound(_)));
    }

    #[test]
    fn duplicate_entity_name_returns_error() {
        let (store, _dir) = make_store();
        store.save_entity(&sample_entity("rust")).unwrap();
        let err = store.save_entity(&sample_entity("rust")).unwrap_err();
        assert!(matches!(err, ScrapwellError::DuplicateName(_)));
    }

    #[test]
    fn duplicate_document_name_returns_error() {
        let (store, _dir) = make_store();
        store.save_entity(&sample_entity("rust")).unwrap();
        store.save(&sample_entry("rust", "anyhow", None)).unwrap();
        let err = store.save(&sample_entry("rust", "anyhow", None)).unwrap_err();
        assert!(matches!(err, ScrapwellError::DuplicateName(_)));
    }

    // ---------- frontmatter ヘルパー ----------

    #[test]
    fn frontmatter_roundtrip_with_body() {
        let ts = "2026-04-12T00:00:00+00:00"
            .parse::<chrono::DateTime<Utc>>()
            .unwrap();
        let fm = DocumentFrontmatter {
            id: "01JX123456".to_string(),
            title: "Test Title".to_string(),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            created_at: ts,
            updated_at: ts,
        };
        let body = "Body content here.\n\nSecond paragraph.";

        let rendered = render_md(&fm, body).unwrap();
        let (parsed, got_body): (DocumentFrontmatter, String) =
            parse_frontmatter(&rendered).unwrap();

        assert_eq!(parsed.id, fm.id);
        assert_eq!(parsed.title, fm.title);
        assert_eq!(parsed.tags, fm.tags);
        assert_eq!(got_body, body);
    }

    #[test]
    fn frontmatter_roundtrip_empty_body() {
        let ts = "2026-04-12T00:00:00+00:00"
            .parse::<chrono::DateTime<Utc>>()
            .unwrap();
        let fm = DocumentFrontmatter {
            id: "01JX654321".to_string(),
            title: "Empty Body".to_string(),
            tags: vec![],
            created_at: ts,
            updated_at: ts,
        };

        let rendered = render_md(&fm, "").unwrap();
        let (parsed, got_body): (DocumentFrontmatter, String) =
            parse_frontmatter(&rendered).unwrap();

        assert_eq!(parsed.id, fm.id);
        assert_eq!(got_body, "");
    }

    // ---------- Phase 2: list_entity_names ----------

    #[test]
    fn list_entity_names_returns_all() {
        let (store, _dir) = make_store();
        store.save_entity(&sample_entity("rust")).unwrap();
        store.save_entity(&sample_entity("elasticsearch")).unwrap();

        let mut names = store.list_entity_names().unwrap();
        names.sort();
        assert_eq!(names, vec!["elasticsearch", "rust"]);
    }

    // ---------- Phase 2: update_entity ----------

    #[test]
    fn update_entity_changes_fields() {
        let (store, dir) = make_store();
        let entity = sample_entity("rust");
        let id = entity.id.clone();
        store.save_entity(&entity).unwrap();

        let patch = EntityPatch {
            scope: Some(Scope::Project),
            description: Some("updated description".to_string()),
            tags: Some(vec!["new-tag".to_string()]),
        };
        store.update_entity(&id, &patch).unwrap();

        let updated = store.get_entity_by_name("rust").unwrap().unwrap();
        assert_eq!(updated.scope, Scope::Project);
        assert_eq!(updated.description, Some("updated description".to_string()));
        assert_eq!(updated.tags, vec!["new-tag".to_string()]);

        // _entity.md にも反映されているか確認
        let content = std::fs::read_to_string(
            dir.path().join("entities/rust/_entity.md")
        ).unwrap();
        assert!(content.contains("scope: project"));
        assert!(content.contains("updated description"));
    }

    // ---------- Phase 2: delete_entity ----------

    #[test]
    fn delete_entity_removes_directory_and_records() {
        let (store, dir) = make_store();
        let entity = sample_entity("rust");
        let id = entity.id.clone();
        store.save_entity(&entity).unwrap();
        store.save(&sample_entry("rust", "anyhow", None)).unwrap();

        store.delete_entity(&id).unwrap();

        // ディレクトリが消えている
        assert!(!dir.path().join("entities/rust").exists());
        // SQLite からも消えている
        assert!(store.get_entity_by_name("rust").unwrap().is_none());
        // ドキュメントの一意性チェックを再利用：name が再び使えるようになっている
        assert!(store.check_name_unique("anyhow").unwrap());
    }

    // ---------- Phase 2: update document ----------

    #[test]
    fn update_document_changes_content() {
        let (store, _dir) = make_store();
        store.save_entity(&sample_entity("rust")).unwrap();

        let entry = MemoryEntry {
            id: MemoryId::new(),
            entity: "rust".to_string(),
            topic: None,
            name: "anyhow".to_string(),
            title: "Old Title".to_string(),
            content: "Old content".to_string(),
            tags: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let id = entry.id.clone();
        store.save(&entry).unwrap();

        let patch = MemoryPatch {
            title: Some("New Title".to_string()),
            content: Some("New content".to_string()),
            tags: Some(vec!["updated".to_string()]),
        };
        store.update(&id, &patch).unwrap();

        let updated = store.get(&id).unwrap().unwrap();
        assert_eq!(updated.title, "New Title");
        assert_eq!(updated.content, "New content");
        assert_eq!(updated.tags, vec!["updated".to_string()]);
    }

    // ---------- Phase 2: delete document ----------

    #[test]
    fn delete_document_removes_file_and_record() {
        let (store, dir) = make_store();
        store.save_entity(&sample_entity("rust")).unwrap();

        let entry = sample_entry("rust", "anyhow", None);
        let id = entry.id.clone();
        store.save(&entry).unwrap();

        store.delete(&id).unwrap();

        // ファイルが消えている
        assert!(!dir.path().join("entities/rust/anyhow.md").exists());
        // SQLite からも消えている
        assert!(store.get(&id).unwrap().is_none());
        // 名前が再利用可能
        assert!(store.check_name_unique("anyhow").unwrap());
    }

    // ---------- Phase 4: iter_all ----------

    #[test]
    fn iter_all_returns_all_documents_with_content() {
        let (store, _dir) = make_store();
        store.save_entity(&sample_entity("rust")).unwrap();
        store.save_entity(&sample_entity("elasticsearch")).unwrap();

        store.save(&sample_entry("rust", "anyhow", None)).unwrap();
        store.save(&sample_entry("rust", "thiserror", None)).unwrap();
        store.save(&sample_entry("elasticsearch", "nested-vector", Some("mapping"))).unwrap();

        let entries = store.iter_all().unwrap();
        assert_eq!(entries.len(), 3);

        // コンテンツが読み込まれている
        for entry in &entries {
            assert!(!entry.content.is_empty(), "content should be loaded for {}", entry.name);
        }
    }

    #[test]
    fn iter_all_on_empty_store_returns_empty() {
        let (store, _dir) = make_store();
        let entries = store.iter_all().unwrap();
        assert!(entries.is_empty());
    }
}
