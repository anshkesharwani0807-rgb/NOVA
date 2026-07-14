//! The SQLite-backed universal search index (Milestone 5, ADR-0006).
//!
//! Stores a normalized, plaintext searchable projection of documents in a local SQLite
//! database (bundled, offline-only). Metadata filters run in SQL; ranking is computed in
//! Rust. The index is a derived cache: its confidentiality at rest is provided by the
//! same future whole-DB/OS-disk encryption path as the memory store (the `embedding`
//! column and the `document` module's trait seams reserve room for semantic search).

use crate::document::{Combine, IndexDocument, MatchMode, SearchQuery, SearchResult};
use crate::vector::VectorStore;
use nova_kernel::{log_activity, ErrorCategory, NovaError, Result};
use rusqlite::{params, params_from_iter, Connection};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Current index schema version.
pub const SCHEMA_VERSION: i64 = 2;

/// Embedding dimension for on-device semantic models (M5 default).
pub const EMBEDDING_DIM: usize = 384;

const SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS index_entries (
    doc_id       TEXT PRIMARY KEY NOT NULL,
    source       TEXT NOT NULL,
    source_id    TEXT NOT NULL,
    category     TEXT NOT NULL DEFAULT '',
    title        TEXT NOT NULL DEFAULT '',
    content      TEXT NOT NULL DEFAULT '',
    tags         TEXT NOT NULL DEFAULT '',
    source_field TEXT NOT NULL DEFAULT '',
    metadata     TEXT NOT NULL DEFAULT '',
    norm_text    TEXT NOT NULL DEFAULT '',
    norm_tags    TEXT NOT NULL DEFAULT '',
    created_at   INTEGER NOT NULL DEFAULT 0,
    updated_at   INTEGER NOT NULL DEFAULT 0,
    importance   INTEGER NOT NULL DEFAULT 0,
    embedding    BLOB
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_entries_source ON index_entries(source, source_id);
CREATE INDEX IF NOT EXISTS idx_entries_category ON index_entries(category);
CREATE INDEX IF NOT EXISTS idx_entries_created ON index_entries(created_at);
CREATE INDEX IF NOT EXISTS idx_entries_importance ON index_entries(importance);
CREATE INDEX IF NOT EXISTS idx_entries_norm ON index_entries(norm_text);
";

const SCHEMA_V2: &str = "
CREATE TABLE IF NOT EXISTS doc_id_mapping (
    doc_id TEXT PRIMARY KEY NOT NULL,
    internal_id INTEGER NOT NULL
);
";

const COLUMNS: &str =
    "doc_id,source,source_id,category,title,content,tags,source_field,metadata,created_at,updated_at,importance";

fn search_err(code: &'static str, detail: impl std::fmt::Display) -> NovaError {
    NovaError::new(ErrorCategory::Internal, code, &detail.to_string())
}

/// Escape LIKE wildcards so user text is matched literally (used with `ESCAPE '\\'`).
fn like_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Aggregate index statistics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexStats {
    pub total: usize,
    pub sources: Vec<(String, usize)>,
}

/// The universal search index over a local SQLite database.
#[derive(Clone)]
pub struct SearchEngine {
    conn: Arc<Mutex<Connection>>,
    path: PathBuf,
    vector_store: Option<Arc<VectorStore>>,
}

impl SearchEngine {
    /// Open (creating if needed) the index database and migrate to the current version.
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| search_err("ERR_SEARCH_OPEN", e))?;
        }
        let conn = Connection::open(db_path).map_err(|e| search_err("ERR_SEARCH_OPEN", e))?;
        let _ = conn.pragma_update(None, "journal_mode", "WAL");

        let engine = Self {
            conn: Arc::new(Mutex::new(conn)),
            path: db_path.to_path_buf(),
            vector_store: None,
        };
        engine.migrate()?;

        // Initialize VectorStore (dimension 384 for on-device embedding models).
        let vs = VectorStore::open(EMBEDDING_DIM)
            .map_err(|e| search_err("ERR_SEARCH_VECTOR_OPEN", e))?;

        let engine_with_vs = Self {
            conn: engine.conn,
            path: engine.path,
            vector_store: Some(Arc::new(vs)),
        };

        // Rebuild the in-memory vector index from embeddings persisted in SQLite so that
        // semantic search survives restarts (SQLite is the source of truth, the index a
        // derived cache).
        engine_with_vs.load_vectors_from_store()?;

        log_activity("search", "search.index_created", "search index ready", None);
        Ok(engine_with_vs)
    }

    /// Repopulate the vector index from the embeddings stored in SQLite. Rows whose
    /// embedding does not match the expected dimension are skipped rather than failing
    /// the whole open (the index is a best-effort derived cache).
    fn load_vectors_from_store(&self) -> Result<()> {
        let vs = match &self.vector_store {
            Some(vs) => vs.clone(),
            None => return Ok(()),
        };
        let conn = self
            .conn
            .lock()
            .map_err(|e| search_err("ERR_SEARCH_VECTOR_LOAD", e))?;
        let mut stmt = conn
            .prepare("SELECT doc_id, embedding FROM index_entries WHERE embedding IS NOT NULL")
            .map_err(|e| search_err("ERR_SEARCH_VECTOR_LOAD", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(|e| search_err("ERR_SEARCH_VECTOR_LOAD", e))?;
        for row in rows {
            let (doc_id, blob) = row.map_err(|e| search_err("ERR_SEARCH_VECTOR_LOAD", e))?;
            if blob.len() % 4 != 0 {
                continue;
            }
            let emb: Vec<f32> = blob
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            let internal_id = self.get_or_create_internal_id_locked(&conn, &doc_id)?;
            // Skip vectors of an unexpected dimension instead of aborting open.
            let _ = vs.upsert(internal_id, &emb);
        }
        Ok(())
    }

    fn migrate(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| search_err("ERR_SEARCH_MIGRATE", e))?;
        let current: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(|e| search_err("ERR_SEARCH_MIGRATE", e))?;
        if current < 1 {
            conn.execute_batch(SCHEMA_V1)
                .map_err(|e| search_err("ERR_SEARCH_MIGRATE", e))?;
        }
        if current < 2 {
            conn.execute_batch(SCHEMA_V2)
                .map_err(|e| search_err("ERR_SEARCH_MIGRATE", e))?;
        }
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(|e| search_err("ERR_SEARCH_MIGRATE", e))?;
        Ok(())
    }

    /// The index schema version.
    pub fn schema_version(&self) -> Result<i64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| search_err("ERR_SEARCH_QUERY", e))?;
        conn.query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(|e| search_err("ERR_SEARCH_QUERY", e))
    }

    /// The index database path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Look up an existing internal id using an already-held connection lock.
    fn get_internal_id_locked(&self, conn: &Connection, doc_id: &str) -> Result<usize> {
        conn.query_row(
            "SELECT internal_id FROM doc_id_mapping WHERE doc_id=?",
            [doc_id],
            |r| r.get(0),
        )
        .map_err(|e| search_err("ERR_SEARCH_ID", e))
    }

    fn get_doc_id_by_internal_id(&self, internal_id: usize) -> Result<String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| search_err("ERR_SEARCH_ID", e))?;
        conn.query_row(
            "SELECT doc_id FROM doc_id_mapping WHERE internal_id=?",
            [internal_id],
            |r| r.get(0),
        )
        .map_err(|e| search_err("ERR_SEARCH_ID", e))
    }

    fn get_doc_by_id(&self, doc_id: &str) -> Option<IndexDocument> {
        let conn = self.conn.lock().ok()?;
        let res = conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM index_entries WHERE doc_id=?"),
                [doc_id],
                Self::row_to_doc,
            )
            .ok();
        res
    }

    /// Get or allocate an internal id using an already-held connection lock.
    fn get_or_create_internal_id_locked(&self, conn: &Connection, doc_id: &str) -> Result<usize> {
        let existing: Option<usize> = conn
            .query_row(
                "SELECT internal_id FROM doc_id_mapping WHERE doc_id=?",
                [doc_id],
                |r| r.get(0),
            )
            .ok();
        if let Some(id) = existing {
            return Ok(id);
        }
        let next_id: usize = conn
            .query_row(
                "SELECT IFNULL(MAX(internal_id), 0) + 1 FROM doc_id_mapping",
                [],
                |r| r.get(0),
            )
            .map_err(|e| search_err("ERR_SEARCH_ID", e))?;
        conn.execute(
            "INSERT INTO doc_id_mapping (doc_id, internal_id) VALUES (?, ?)",
            params![doc_id, next_id],
        )
        .map_err(|e| search_err("ERR_SEARCH_ID", e))?;
        Ok(next_id)
    }

    fn write_doc(conn: &Connection, doc: &IndexDocument) -> Result<()> {
        let norm_tags = format!(
            " {} ",
            doc.tags
                .iter()
                .map(|t| t.to_lowercase())
                .collect::<Vec<_>>()
                .join(" ")
        );
        const SQL: &str = "INSERT INTO index_entries
                 (doc_id,source,source_id,category,title,content,tags,source_field,metadata,norm_text,norm_tags,created_at,updated_at,importance,embedding)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
                 ON CONFLICT(doc_id) DO UPDATE SET
                 category=excluded.category, title=excluded.title, content=excluded.content,
                 tags=excluded.tags, source_field=excluded.source_field, metadata=excluded.metadata,
                 norm_text=excluded.norm_text, norm_tags=excluded.norm_tags, updated_at=excluded.updated_at,
                 importance=excluded.importance, embedding=excluded.embedding";

        let embedding_blob = doc
            .embedding
            .as_ref()
            .map(|v| v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>());

        conn.execute(
            SQL,
            params![
                doc.doc_id(),
                doc.source,
                doc.source_id,
                doc.category,
                doc.title,
                doc.content,
                doc.tags.join(" "),
                doc.source_field,
                doc.metadata,
                doc.norm_text(),
                norm_tags,
                doc.created_at,
                doc.updated_at,
                doc.importance,
                embedding_blob,
            ],
        )
        .map_err(|e| search_err("ERR_SEARCH_WRITE", e))?;
        Ok(())
    }

    pub fn insert(&self, doc: &IndexDocument) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| search_err("ERR_SEARCH_WRITE", e))?;
        Self::write_doc(&conn, doc)?;

        if let Some(emb) = &doc.embedding {
            let internal_id = self.get_or_create_internal_id_locked(&conn, &doc.doc_id())?;
            if let Some(vs) = &self.vector_store {
                vs.upsert(internal_id, emb)?;
            }
        }

        log_activity(
            "search",
            "search.record_indexed",
            &format!("doc={}", doc.doc_id()),
            None,
        );
        Ok(())
    }

    pub fn update(&self, doc: &IndexDocument) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| search_err("ERR_SEARCH_WRITE", e))?;
        Self::write_doc(&conn, doc)?;

        if let Some(emb) = &doc.embedding {
            let internal_id = self.get_or_create_internal_id_locked(&conn, &doc.doc_id())?;
            if let Some(vs) = &self.vector_store {
                vs.upsert(internal_id, emb)?;
            }
        }

        log_activity(
            "search",
            "search.record_updated",
            &format!("doc={}", doc.doc_id()),
            None,
        );
        Ok(())
    }

    pub fn delete(&self, source: &str, source_id: &str) -> Result<()> {
        let doc_id = format!("{source}:{source_id}");
        let conn = self
            .conn
            .lock()
            .map_err(|e| search_err("ERR_SEARCH_DELETE", e))?;
        // Resolve the internal id before the row is gone so the vector can be revoked too.
        let internal_id = self.get_internal_id_locked(&conn, &doc_id).ok();

        conn.execute("DELETE FROM index_entries WHERE doc_id=?1", params![doc_id])
            .map_err(|e| search_err("ERR_SEARCH_DELETE", e))?;

        if let Some(id) = internal_id {
            if let Some(vs) = &self.vector_store {
                vs.remove(id)?;
            }
            conn.execute(
                "DELETE FROM doc_id_mapping WHERE doc_id=?",
                [doc_id.as_str()],
            )
            .map_err(|e| search_err("ERR_SEARCH_DELETE", e))?;
        }

        log_activity(
            "search",
            "search.record_removed",
            &format!("doc={doc_id}"),
            None,
        );
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| search_err("ERR_SEARCH_CLEAR", e))?;
        conn.execute("DELETE FROM index_entries", [])
            .map_err(|e| search_err("ERR_SEARCH_CLEAR", e))?;
        conn.execute("DELETE FROM doc_id_mapping", [])
            .map_err(|e| search_err("ERR_SEARCH_CLEAR", e))?;

        if let Some(vs) = &self.vector_store {
            vs.clear()?;
        }
        Ok(())
    }

    pub fn rebuild(&mut self, docs: &[IndexDocument]) -> Result<usize> {
        log_activity("search", "search.rebuild_started", "rebuild started", None);
        let mut conn = self
            .conn
            .lock()
            .map_err(|e| search_err("ERR_SEARCH_REBUILD", e))?;
        let tx = conn
            .transaction()
            .map_err(|e| search_err("ERR_SEARCH_REBUILD", e))?;
        tx.execute("DELETE FROM index_entries", [])
            .map_err(|e| search_err("ERR_SEARCH_REBUILD", e))?;
        tx.execute("DELETE FROM doc_id_mapping", [])
            .map_err(|e| search_err("ERR_SEARCH_REBUILD", e))?;
        for doc in docs {
            Self::write_doc(&tx, doc)?;
        }
        tx.commit()
            .map_err(|e| search_err("ERR_SEARCH_REBUILD", e))?;

        for doc in docs {
            if let Some(emb) = &doc.embedding {
                let internal_id = self.get_or_create_internal_id_locked(&conn, &doc.doc_id())?;
                if let Some(vs) = &self.vector_store {
                    vs.upsert(internal_id, emb)?;
                }
            }
        }

        log_activity(
            "search",
            "search.rebuild_completed",
            &format!("count={}", docs.len()),
            None,
        );
        Ok(docs.len())
    }

    pub fn count(&self) -> Result<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| search_err("ERR_SEARCH_QUERY", e))?;
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM index_entries", [], |r| r.get(0))
            .map_err(|e| search_err("ERR_SEARCH_QUERY", e))?;
        Ok(n as usize)
    }

    pub fn stats(&self) -> Result<IndexStats> {
        let total = self.count()?;
        let conn = self
            .conn
            .lock()
            .map_err(|e| search_err("ERR_SEARCH_QUERY", e))?;
        let mut stmt = conn
            .prepare("SELECT source, COUNT(*) FROM index_entries GROUP BY source ORDER by source")
            .map_err(|e| search_err("ERR_SEARCH_QUERY", e))?;
        let sources = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })
            .map_err(|e| search_err("ERR_SEARCH_QUERY", e))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| search_err("ERR_SEARCH_QUERY", e))?;
        Ok(IndexStats { total, sources })
    }

    fn build_where(query: &SearchQuery) -> (String, Vec<String>) {
        let mut sql = String::from(" WHERE 1=1");
        let mut binds: Vec<String> = Vec::new();

        if let Some(src) = &query.source {
            sql.push_str(" AND source = ?");
            binds.push(src.clone());
        }
        if let Some(cat) = &query.category {
            sql.push_str(" AND category = ?");
            binds.push(cat.clone());
        }
        if let Some(from) = query.date_from {
            sql.push_str(&format!(" AND created_at >= {from}"));
        }
        if let Some(to) = query.date_to {
            sql.push_str(&format!(" AND created_at <= {to}"));
        }
        for tag in &query.tags {
            sql.push_str(" AND norm_tags LIKE ? ESCAPE '\\'");
            binds.push(format!(
                "%{}%",
                like_escape(&format!(" {} ", tag.to_lowercase()))
            ));
        }

        if let Some(text) = &query.text {
            let text = text.trim();
            if !text.is_empty() {
                match query.mode {
                    MatchMode::Phrase => {
                        sql.push_str(" AND norm_text LIKE ? ESCAPE '\\'");
                        binds.push(format!("%{}%", like_escape(&text.to_lowercase())));
                    }
                    MatchMode::Exact => {
                        sql.push_str(" AND (LOWER(title) = ? OR LOWER(content) = ?)");
                        binds.push(text.to_lowercase());
                        binds.push(text.to_lowercase());
                    }
                    MatchMode::Partial | MatchMode::Prefix => {
                        let tokens: Vec<&str> = text.split_whitespace().collect();
                        let joiner = match query.combine {
                            Combine::And => " AND ",
                            Combine::Or => " OR ",
                        };
                        let mut parts: Vec<String> = Vec::new();
                        for tok in tokens {
                            let esc = like_escape(&tok.to_lowercase());
                            if query.mode == MatchMode::Prefix {
                                parts.push(
                                    "(norm_text LIKE ? ESCAPE '\\' OR norm_text LIKE ? ESCAPE '\\')"
                                        .to_string(),
                                );
                                binds.push(format!("{esc}%"));
                                binds.push(format!("% {esc}%"));
                            } else {
                                parts.push("norm_text LIKE ? ESCAPE '\\'".to_string());
                                binds.push(format!("%{esc}%"));
                            }
                        }
                        if !parts.is_empty() {
                            sql.push_str(" AND (");
                            sql.push_str(&parts.join(joiner));
                            sql.push(')');
                        }
                    }
                }
            }
        }
        (sql, binds)
    }

    fn row_to_doc(row: &rusqlite::Row<'_>) -> rusqlite::Result<IndexDocument> {
        let tags: String = row.get(6)?;
        Ok(IndexDocument {
            source: row.get(1)?,
            source_id: row.get(2)?,
            category: row.get(3)?,
            title: row.get(4)?,
            content: row.get(5)?,
            tags: if tags.is_empty() {
                Vec::new()
            } else {
                tags.split(' ').map(str::to_string).collect()
            },
            source_field: row.get(7)?,
            metadata: row.get(8)?,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
            importance: row.get(11)?,
            embedding: None,
        })
    }

    fn score(doc: &IndexDocument, query: &SearchQuery) -> f64 {
        let mut score = 0.0_f64;
        let title = doc.title.to_lowercase();
        let content = doc.content.to_lowercase();
        let tags = doc.tags.join(" ").to_lowercase();
        if let Some(text) = &query.text {
            for tok in text.to_lowercase().split_whitespace() {
                if title.contains(tok) {
                    score += 3.0;
                }
                if content.contains(tok) {
                    score += 1.0;
                }
                if tags.contains(tok) {
                    score += 2.0;
                }
            }
            if title == text.to_lowercase() {
                score += 5.0;
            }
        }
        score += f64::from(doc.importance) * 0.01;
        score
    }

    pub fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let mut semantic_results = Vec::new();
        if let Some(emb) = &query.embedding {
            if let Some(vs) = &self.vector_store {
                let k = query.limit.unwrap_or(100);
                let nn = vs
                    .search(emb, k)
                    .map_err(|e| search_err("ERR_SEARCH_VECTOR_QUERY", e))?;
                for (int_id, sim) in nn {
                    // Skip ids whose mapping/row no longer exists (e.g. concurrently
                    // deleted) instead of failing the whole query.
                    if let Ok(doc_id) = self.get_doc_id_by_internal_id(int_id) {
                        if let Some(doc) = self.get_doc_by_id(&doc_id) {
                            semantic_results.push(SearchResult {
                                document: doc,
                                score: sim as f64,
                            });
                        }
                    }
                }
            }
        }

        let (where_sql, binds) = Self::build_where(query);
        let sql = format!("SELECT {COLUMNS} FROM index_entries{where_sql}");
        let conn = self
            .conn
            .lock()
            .map_err(|e| search_err("ERR_SEARCH_QUERY", e))?;
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| search_err("ERR_SEARCH_QUERY", e))?;
        let mut docs = stmt
            .query_map(params_from_iter(binds.iter()), Self::row_to_doc)
            .map_err(|e| search_err("ERR_SEARCH_QUERY", e))?
            .collect::<rusqlite::Result<Vec<IndexDocument>>>()
            .map_err(|e| search_err("ERR_SEARCH_QUERY", e))?;

        if !query.case_insensitive {
            if let Some(text) = &query.text {
                let text = text.clone();
                docs.retain(|d| {
                    let hay = format!("{} {} {}", d.title, d.content, d.tags.join(" "));
                    match query.mode {
                        MatchMode::Exact => d.title == text || d.content == text,
                        MatchMode::Prefix => hay.split_whitespace().any(|w| w.starts_with(&text)),
                        _ => hay.contains(&text),
                    }
                });
            }
        }

        let lexical_results: Vec<SearchResult> = docs
            .into_iter()
            .map(|document| {
                let score = Self::score(&document, query);
                SearchResult { document, score }
            })
            .collect();

        // Fuse lexical and semantic scores. Lexical scores are unbounded (per-token
        // weights) while cosine similarity is in [-1, 1], so blending them raw would let
        // lexical magnitude dominate. When semantic results are present we min-max the
        // lexical scores to [0, 1] and clamp similarity to [0, 1] before the 0.4/0.6 mix;
        // for lexical-only queries we keep the raw lexical score (and ordering) unchanged.
        let has_semantic = !semantic_results.is_empty();
        let max_lex = lexical_results
            .iter()
            .map(|r| r.score)
            .fold(0.0_f64, f64::max);
        let sem_by_id: HashMap<String, f64> = semantic_results
            .iter()
            .map(|s| (s.document.doc_id(), s.score.clamp(0.0, 1.0)))
            .collect();

        let mut seen: HashSet<String> = HashSet::new();
        let mut final_results: Vec<SearchResult> = Vec::new();
        for res in lexical_results {
            let doc_id = res.document.doc_id();
            let score = if has_semantic {
                let lex_norm = if max_lex > 0.0 {
                    res.score / max_lex
                } else {
                    0.0
                };
                let sem = sem_by_id.get(&doc_id).copied().unwrap_or(0.0);
                (0.4 * lex_norm) + (0.6 * sem)
            } else {
                res.score
            };
            seen.insert(doc_id);
            final_results.push(SearchResult {
                document: res.document,
                score,
            });
        }

        for sem in semantic_results {
            if !seen.contains(&sem.document.doc_id()) {
                final_results.push(sem);
            }
        }

        final_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.document.updated_at.cmp(&a.document.updated_at))
        });

        let out = final_results
            .into_iter()
            .skip(query.offset)
            .take(query.limit.unwrap_or(usize::MAX))
            .collect();
        Ok(out)
    }
}
