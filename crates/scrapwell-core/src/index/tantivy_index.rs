use std::path::PathBuf;

use tantivy::{
    collector::TopDocs,
    directory::MmapDirectory,
    query::{BooleanQuery, Occur, QueryParser, TermQuery},
    schema::{IndexRecordOption, Schema, Value, STORED, STRING, TEXT},
    snippet::SnippetGenerator,
    Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term,
};

use crate::{
    error::{Result, ScrapwellError},
    index::SearchIndex,
    model::{MemoryEntry, MemoryId, SearchHit, SearchQuery},
};

// ---------- Schema ----------

fn build_schema() -> Schema {
    let mut b = Schema::builder();
    // STRING: no tokenization (exact match / deletion key), STORED: for retrieval
    b.add_text_field("id", STRING | STORED);
    b.add_text_field("entity", STRING | STORED);
    b.add_text_field("topic", STRING | STORED);
    b.add_text_field("name", STRING | STORED);
    // TEXT: full-text searchable, STORED: for snippet generation and retrieval
    b.add_text_field("title", TEXT | STORED);
    b.add_text_field("content", TEXT | STORED);
    b.add_text_field("tags", TEXT | STORED);
    b.build()
}

// ---------- Helpers ----------

fn into_search_err(e: impl std::fmt::Display) -> ScrapwellError {
    ScrapwellError::SearchIndex(e.to_string())
}

/// Add a document to the writer (delete by ID first if it exists = upsert).
fn add_entry_doc(schema: &Schema, writer: &mut IndexWriter, entry: &MemoryEntry) -> Result<()> {
    let id_field = schema.get_field("id").unwrap();
    writer.delete_term(Term::from_field_text(id_field, &entry.id.0));

    let mut doc = TantivyDocument::default();
    doc.add_text(id_field, &entry.id.0);
    doc.add_text(schema.get_field("entity").unwrap(), &entry.entity);
    doc.add_text(
        schema.get_field("topic").unwrap(),
        entry.topic.as_deref().unwrap_or(""),
    );
    doc.add_text(schema.get_field("name").unwrap(), &entry.name);
    doc.add_text(schema.get_field("title").unwrap(), &entry.title);
    doc.add_text(schema.get_field("content").unwrap(), &entry.content);
    doc.add_text(schema.get_field("tags").unwrap(), entry.tags.join(" "));

    writer.add_document(doc).map_err(into_search_err)?;
    Ok(())
}

/// Convert a Tantivy Snippet into <<highlight>> format.
fn format_snippet(snippet: &tantivy::snippet::Snippet) -> Vec<String> {
    let fragment = snippet.fragment();
    if fragment.is_empty() {
        return vec![];
    }

    let mut result = String::new();
    let mut prev = 0usize;

    for r in snippet.highlighted() {
        result.push_str(&fragment[prev..r.start]);
        result.push_str("<<");
        result.push_str(&fragment[r.start..r.end]);
        result.push_str(">>");
        prev = r.end;
    }
    result.push_str(&fragment[prev..]);

    vec![result]
}

// ---------- TantivySearchIndex ----------

pub struct TantivySearchIndex {
    schema: Schema,
    index: Index,
    reader: IndexReader,
}

impl TantivySearchIndex {
    pub fn new(index_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&index_dir)?;

        let schema = build_schema();
        let dir = MmapDirectory::open(&index_dir).map_err(into_search_err)?;
        let index = Index::open_or_create(dir, schema.clone()).map_err(into_search_err)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(into_search_err)?;

        Ok(Self {
            schema,
            index,
            reader,
        })
    }

    /// Acquire a short-lived writer, run the closure, then drop the writer to
    /// release the Tantivy lockfile. This allows multiple scrapwell processes
    /// to coexist — each holds the lock only for the duration of a single
    /// write operation.
    fn with_writer<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut IndexWriter) -> Result<T>,
    {
        let mut writer = self.index.writer(50_000_000).map_err(into_search_err)?;
        let result = f(&mut writer);
        // Drop the writer to release the lockfile regardless of success/failure.
        drop(writer);
        result
    }
}

impl SearchIndex for TantivySearchIndex {
    fn upsert(&self, entry: &MemoryEntry) -> Result<()> {
        self.with_writer(|w| {
            add_entry_doc(&self.schema, w, entry)?;
            w.commit().map_err(into_search_err)
        })?;
        self.reader.reload().map_err(into_search_err)?;
        Ok(())
    }

    fn search(&self, query: &SearchQuery) -> Result<Vec<SearchHit>> {
        let searcher = self.reader.searcher();
        let schema = &self.schema;

        let title = schema.get_field("title").unwrap();
        let content = schema.get_field("content").unwrap();
        let tags = schema.get_field("tags").unwrap();

        // Keyword search query across title, content, and tags
        let parser = QueryParser::for_index(&self.index, vec![title, content, tags]);
        let kw_query = parser.parse_query(&query.query).map_err(into_search_err)?;

        // If an entity filter is provided, AND it together with BooleanQuery
        let final_query: Box<dyn tantivy::query::Query> = if let Some(entity) = &query.entity {
            let entity_field = schema.get_field("entity").unwrap();
            let entity_term = Term::from_field_text(entity_field, entity);
            let entity_query = TermQuery::new(entity_term, IndexRecordOption::Basic);
            Box::new(BooleanQuery::new(vec![
                (Occur::Must, kw_query),
                (Occur::Must, Box::new(entity_query)),
            ]))
        } else {
            kw_query
        };

        let top_docs = searcher
            .search(&*final_query, &TopDocs::with_limit(query.limit))
            .map_err(into_search_err)?;

        let snippet_gen =
            SnippetGenerator::create(&searcher, &*final_query, content).map_err(into_search_err)?;

        let mut hits = Vec::new();
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address).map_err(into_search_err)?;

            // Closure to extract a STORED field value as a string
            let get_str = |field_name: &str| -> String {
                schema
                    .get_field(field_name)
                    .ok()
                    .and_then(|f| doc.get_first(f))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };

            let topic_str = get_str("topic");
            let topic = if topic_str.is_empty() {
                None
            } else {
                Some(topic_str)
            };

            let tags_str = get_str("tags");
            let tags: Vec<String> = if tags_str.is_empty() {
                vec![]
            } else {
                tags_str.split_whitespace().map(str::to_string).collect()
            };

            let snippet = snippet_gen.snippet_from_doc(&doc);
            let snippets = format_snippet(&snippet);

            hits.push(SearchHit {
                id: MemoryId(get_str("id")),
                entity: get_str("entity"),
                topic,
                name: get_str("name"),
                title: get_str("title"),
                tags,
                snippets,
                score,
            });
        }

        Ok(hits)
    }

    fn remove(&self, id: &MemoryId) -> Result<()> {
        self.with_writer(|w| {
            let id_field = self.schema.get_field("id").unwrap();
            w.delete_term(Term::from_field_text(id_field, &id.0));
            w.commit().map_err(into_search_err)
        })?;
        self.reader.reload().map_err(into_search_err)?;
        Ok(())
    }

    fn rebuild(&self, entries: &mut dyn Iterator<Item = MemoryEntry>) -> Result<()> {
        self.with_writer(|w| {
            w.delete_all_documents().map_err(into_search_err)?;
            for entry in entries {
                add_entry_doc(&self.schema, w, &entry)?;
            }
            w.commit().map_err(into_search_err)
        })?;
        self.reader.reload().map_err(into_search_err)?;
        Ok(())
    }
}

// ---------- テスト ----------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;

    fn make_entry(
        id: &str,
        entity: &str,
        topic: Option<&str>,
        name: &str,
        title: &str,
        content: &str,
        tags: Vec<&str>,
    ) -> MemoryEntry {
        MemoryEntry {
            id: MemoryId(id.to_string()),
            entity: entity.to_string(),
            topic: topic.map(str::to_string),
            name: name.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn make_index(dir: &TempDir) -> TantivySearchIndex {
        TantivySearchIndex::new(dir.path().join("index")).unwrap()
    }

    #[test]
    fn upsert_and_search_basic() {
        let dir = TempDir::new().unwrap();
        let idx = make_index(&dir);

        // The default SimpleTokenizer splits on whitespace, so use ASCII text
        let entry = make_entry(
            "01HZZZZ00001",
            "rust",
            None,
            "anyhow-guide",
            "Anyhow Guide",
            "anyhow is a Rust crate that simplifies error handling",
            vec!["error-handling"],
        );
        idx.upsert(&entry).unwrap();

        let hits = idx
            .search(&SearchQuery {
                query: "error handling".to_string(),
                entity: None,
                limit: 10,
            })
            .unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id.0, "01HZZZZ00001");
        assert_eq!(hits[0].name, "anyhow-guide");
        assert_eq!(hits[0].entity, "rust");
    }

    #[test]
    fn search_no_match_returns_empty() {
        let dir = TempDir::new().unwrap();
        let idx = make_index(&dir);

        let entry = make_entry("01", "rust", None, "doc", "Title", "Rust content", vec![]);
        idx.upsert(&entry).unwrap();

        let hits = idx
            .search(&SearchQuery {
                query: "python".to_string(),
                entity: None,
                limit: 10,
            })
            .unwrap();

        assert!(hits.is_empty());
    }

    #[test]
    fn search_with_entity_filter() {
        let dir = TempDir::new().unwrap();
        let idx = make_index(&dir);

        idx.upsert(&make_entry(
            "01",
            "rust",
            None,
            "trait-guide",
            "Trait Guide",
            "Rust traits",
            vec![],
        ))
        .unwrap();
        idx.upsert(&make_entry(
            "02",
            "go",
            None,
            "interface-guide",
            "Interface Guide",
            "Go interfaces and traits",
            vec![],
        ))
        .unwrap();

        let hits = idx
            .search(&SearchQuery {
                query: "traits".to_string(),
                entity: Some("rust".to_string()),
                limit: 10,
            })
            .unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].entity, "rust");
    }

    #[test]
    fn upsert_overwrites_existing_doc() {
        let dir = TempDir::new().unwrap();
        let idx = make_index(&dir);

        let mut entry = make_entry(
            "01",
            "rust",
            None,
            "doc",
            "Old Title",
            "old content here",
            vec![],
        );
        idx.upsert(&entry).unwrap();

        entry.title = "New Title".to_string();
        entry.content = "new content here".to_string();
        idx.upsert(&entry).unwrap();

        let hits = idx
            .search(&SearchQuery {
                query: "new content".to_string(),
                entity: None,
                limit: 10,
            })
            .unwrap();

        assert_eq!(
            hits.len(),
            1,
            "upsert should result in exactly one document"
        );
        assert_eq!(hits[0].title, "New Title");
    }

    #[test]
    fn remove_deletes_document() {
        let dir = TempDir::new().unwrap();
        let idx = make_index(&dir);

        let entry = make_entry(
            "01",
            "rust",
            None,
            "doc",
            "Title",
            "searchable content",
            vec![],
        );
        idx.upsert(&entry).unwrap();
        idx.remove(&MemoryId("01".to_string())).unwrap();

        let hits = idx
            .search(&SearchQuery {
                query: "searchable".to_string(),
                entity: None,
                limit: 10,
            })
            .unwrap();

        assert!(hits.is_empty());
    }

    #[test]
    fn rebuild_replaces_all_documents() {
        let dir = TempDir::new().unwrap();
        let idx = make_index(&dir);

        // Initial data
        idx.upsert(&make_entry(
            "01",
            "rust",
            None,
            "old-doc",
            "Old",
            "old content",
            vec![],
        ))
        .unwrap();

        // Rebuild with new data
        let new_entries = vec![make_entry(
            "02",
            "rust",
            None,
            "new-doc",
            "New",
            "new content here",
            vec![],
        )];
        idx.rebuild(&mut new_entries.into_iter()).unwrap();

        let old_hits = idx
            .search(&SearchQuery {
                query: "old".to_string(),
                entity: None,
                limit: 10,
            })
            .unwrap();
        let new_hits = idx
            .search(&SearchQuery {
                query: "new".to_string(),
                entity: None,
                limit: 10,
            })
            .unwrap();

        assert!(
            old_hits.is_empty(),
            "old documents should be gone after rebuild"
        );
        assert_eq!(new_hits.len(), 1);
    }

    #[test]
    fn two_instances_can_coexist_on_same_directory() {
        let dir = TempDir::new().unwrap();
        let idx1 = make_index(&dir);
        let idx2 = make_index(&dir);

        // Instance 1 writes a document
        idx1.upsert(&make_entry(
            "01",
            "rust",
            None,
            "from-idx1",
            "Written by idx1",
            "hello from the first instance",
            vec![],
        ))
        .unwrap();

        // Instance 2 can also write without LockBusy error
        idx2.upsert(&make_entry(
            "02",
            "go",
            None,
            "from-idx2",
            "Written by idx2",
            "hello from the second instance",
            vec![],
        ))
        .unwrap();

        // Reload readers to pick up cross-instance commits
        idx1.reader.reload().unwrap();
        idx2.reader.reload().unwrap();

        // Both instances can search and see all documents
        let hits1 = idx1
            .search(&SearchQuery {
                query: "hello".to_string(),
                entity: None,
                limit: 10,
            })
            .unwrap();
        let hits2 = idx2
            .search(&SearchQuery {
                query: "hello".to_string(),
                entity: None,
                limit: 10,
            })
            .unwrap();

        assert_eq!(hits1.len(), 2, "idx1 should see both documents");
        assert_eq!(hits2.len(), 2, "idx2 should see both documents");
    }

    #[test]
    fn two_instances_sequential_writes_are_consistent() {
        let dir = TempDir::new().unwrap();
        let idx1 = make_index(&dir);
        let idx2 = make_index(&dir);

        // Alternating writes between instances
        idx1.upsert(&make_entry(
            "01", "rust", None, "doc-a", "A", "alpha content", vec![],
        ))
        .unwrap();
        idx2.upsert(&make_entry(
            "02", "rust", None, "doc-b", "B", "beta content", vec![],
        ))
        .unwrap();
        idx1.upsert(&make_entry(
            "03", "rust", None, "doc-c", "C", "gamma content", vec![],
        ))
        .unwrap();

        // Remove via a different instance than the one that wrote
        idx2.remove(&MemoryId("01".to_string())).unwrap();

        // Reload reader to pick up cross-instance changes
        idx1.reader.reload().unwrap();

        let hits = idx1
            .search(&SearchQuery {
                query: "content".to_string(),
                entity: None,
                limit: 10,
            })
            .unwrap();

        assert_eq!(hits.len(), 2, "should have 2 docs after removal");
        let ids: Vec<&str> = hits.iter().map(|h| h.id.0.as_str()).collect();
        assert!(!ids.contains(&"01"), "removed doc should be gone");
    }

    #[test]
    fn snippet_contains_highlight_markers() {
        let dir = TempDir::new().unwrap();
        let idx = make_index(&dir);

        let entry = make_entry(
            "01",
            "rust",
            None,
            "doc",
            "Title",
            "The quick brown fox jumps over the lazy dog",
            vec![],
        );
        idx.upsert(&entry).unwrap();

        let hits = idx
            .search(&SearchQuery {
                query: "fox".to_string(),
                entity: None,
                limit: 10,
            })
            .unwrap();

        assert_eq!(hits.len(), 1);
        if let Some(snippet) = hits[0].snippets.first() {
            assert!(
                snippet.contains("<<"),
                "snippet should contain opening marker"
            );
            assert!(
                snippet.contains(">>"),
                "snippet should contain closing marker"
            );
        }
    }
}
