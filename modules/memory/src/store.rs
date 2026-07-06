//! Encrypted SQLite store backing the Memory Engine (Milestone 4, ADR-0006).
//!
//! Uses a local SQLite database (bundled, offline-only, no cloud). Sensitive fields are
//! sealed with AES-256-GCM before they touch disk (see [`crate::crypto`]); operational
//! metadata (id, category, timestamps, importance, flags) is stored as plaintext columns
//! so it can be indexed and filtered. Schema is versioned via `PRAGMA user_version` and
//! migrated forward automatically.

use crate::crypto::{Cipher, KeyProvider};
use crate::record::{
    now_millis, MemoryCategory, MemoryOp, MemoryRecord, Query, SearchMode, SortBy,
};
use nova_kernel::{log_activity, ErrorCategory, NovaError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};

/// Current schema version. Bump and add a migration step for each future change.
pub const SCHEMA_VERSION: i64 = 1;

const SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS memories (
    id             TEXT PRIMARY KEY NOT NULL,
    category       TEXT NOT NULL,
    title          BLOB NOT NULL,
    content        BLOB NOT NULL,
    tags           BLOB NOT NULL,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    importance     INTEGER NOT NULL DEFAULT 0,
    source         BLOB NOT NULL,
    device_id      TEXT NOT NULL DEFAULT '',
    correlation_id TEXT,
    version        INTEGER NOT NULL DEFAULT 1,
    deleted        INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_memories_category ON memories(category);
CREATE INDEX IF NOT EXISTS idx_memories_deleted ON memories(deleted);
CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at);
CREATE INDEX IF NOT EXISTS idx_memories_importance ON memories(importance);
";

const COLUMNS: &str = "id,category,title,content,tags,created_at,updated_at,importance,source,device_id,correlation_id,version,deleted";

fn storage_err(code: &'static str, detail: impl std::fmt::Display) -> NovaError {
    NovaError::new(ErrorCategory::Storage, code, &detail.to_string())
}

/// Raw row as read from the database (encrypted blobs still sealed).
struct RawRow {
    id: String,
    category: String,
    title: Vec<u8>,
    content: Vec<u8>,
    tags: Vec<u8>,
    created_at: i64,
    updated_at: i64,
    importance: i32,
    source: Vec<u8>,
    device_id: String,
    correlation_id: Option<String>,
    version: i64,
    deleted: i64,
}

/// Sealed field blobs ready to write.
struct Encoded {
    title: Vec<u8>,
    content: Vec<u8>,
    tags: Vec<u8>,
    source: Vec<u8>,
}

fn encode(cipher: &Cipher, rec: &MemoryRecord) -> Result<Encoded> {
    let tags_json =
        serde_json::to_string(&rec.tags).map_err(|e| storage_err("ERR_MEM_ENCODE", e))?;
    Ok(Encoded {
        title: cipher.encrypt_str(&rec.title)?,
        content: cipher.encrypt_str(&rec.content)?,
        tags: cipher.encrypt_str(&tags_json)?,
        source: cipher.encrypt_str(&rec.source)?,
    })
}

fn decode(cipher: &Cipher, raw: RawRow) -> Result<MemoryRecord> {
    let tags_json = cipher.decrypt_str(&raw.tags)?;
    let tags: Vec<String> =
        serde_json::from_str(&tags_json).map_err(|e| storage_err("ERR_MEM_DECODE", e))?;
    Ok(MemoryRecord {
        id: raw.id,
        category: MemoryCategory::from_stored(&raw.category),
        title: cipher.decrypt_str(&raw.title)?,
        content: cipher.decrypt_str(&raw.content)?,
        tags,
        created_at: raw.created_at,
        updated_at: raw.updated_at,
        importance: raw.importance,
        source: cipher.decrypt_str(&raw.source)?,
        device_id: raw.device_id,
        correlation_id: raw.correlation_id,
        version: raw.version,
        deleted: raw.deleted != 0,
    })
}

fn order_clause(sort: Option<SortBy>) -> &'static str {
    match sort {
        Some(SortBy::CreatedAtAsc) => "created_at ASC",
        Some(SortBy::UpdatedAtDesc) => "updated_at DESC",
        Some(SortBy::ImportanceDesc) => "importance DESC, created_at DESC",
        Some(SortBy::CreatedAtDesc) | None => "created_at DESC",
    }
}

fn text_matches(rec: &MemoryRecord, needle: &str, mode: SearchMode, ci: bool) -> bool {
    let fields = [&rec.title, &rec.content];
    fields.iter().any(|raw| {
        let hay = if ci {
            raw.to_lowercase()
        } else {
            (*raw).clone()
        };
        match mode {
            SearchMode::Exact => hay == needle,
            SearchMode::Contains => hay.contains(needle),
            SearchMode::Prefix => hay.starts_with(needle),
        }
    })
}

fn tags_match(rec: &MemoryRecord, wanted: &[String], ci: bool) -> bool {
    wanted.iter().all(|want| {
        let want = if ci {
            want.to_lowercase()
        } else {
            want.clone()
        };
        rec.tags.iter().any(|t| {
            let t = if ci { t.to_lowercase() } else { t.clone() };
            t == want
        })
    })
}

/// A local, encrypted SQLite store of memory records.
pub struct Store {
    conn: Connection,
    cipher: Cipher,
    path: PathBuf,
}

impl Store {
    /// Open (creating if needed) the database and run migrations to the current version.
    pub fn open(db_path: &Path, key: &dyn KeyProvider) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| storage_err("ERR_MEM_OPEN", e))?;
        }
        let conn = Connection::open(db_path).map_err(|e| storage_err("ERR_MEM_OPEN", e))?;
        // WAL improves read/write concurrency; access is additionally serialized by the
        // engine's mutex so there are no data races.
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let cipher = Cipher::new(&key.key()?);
        let store = Self {
            conn,
            cipher,
            path: db_path.to_path_buf(),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let current: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(|e| storage_err("ERR_MEM_MIGRATE", e))?;
        if current < 1 {
            self.conn
                .execute_batch(SCHEMA_V1)
                .map_err(|e| storage_err("ERR_MEM_MIGRATE", e))?;
            self.conn
                .pragma_update(None, "user_version", SCHEMA_VERSION)
                .map_err(|e| storage_err("ERR_MEM_MIGRATE", e))?;
        }
        // Future migrations: `if current < 2 { ... set user_version = 2 }` etc.
        Ok(())
    }

    /// The schema version currently stored in the database.
    pub fn schema_version(&self) -> Result<i64> {
        self.conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(|e| storage_err("ERR_MEM_QUERY", e))
    }

    /// The database file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn log(&self, op: &str, id: &str, category: MemoryCategory, correlation: Option<&str>) {
        let corr = correlation.and_then(|s| uuid::Uuid::parse_str(s).ok());
        log_activity(
            "memory",
            &format!("memory.{op}"),
            &format!("id={id} category={}", category.as_str()),
            corr,
        );
    }

    /// Insert a new record.
    pub fn insert(&self, rec: &MemoryRecord) -> Result<()> {
        let enc = encode(&self.cipher, rec)?;
        self.conn
            .execute(
                &format!(
                    "INSERT INTO memories ({COLUMNS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"
                ),
                params![
                    rec.id,
                    rec.category.as_str(),
                    enc.title,
                    enc.content,
                    enc.tags,
                    rec.created_at,
                    rec.updated_at,
                    rec.importance,
                    enc.source,
                    rec.device_id,
                    rec.correlation_id,
                    rec.version,
                    rec.deleted as i64,
                ],
            )
            .map_err(|e| storage_err("ERR_MEM_INSERT", e))?;
        self.log(
            "insert",
            &rec.id,
            rec.category,
            rec.correlation_id.as_deref(),
        );
        Ok(())
    }

    /// Update an existing record (bumps `updated_at`). Errors if the id is absent.
    pub fn update(&self, rec: &MemoryRecord) -> Result<()> {
        let enc = encode(&self.cipher, rec)?;
        let now = now_millis();
        let rows = self
            .conn
            .execute(
                "UPDATE memories SET category=?2, title=?3, content=?4, tags=?5, updated_at=?6, \
                 importance=?7, source=?8, device_id=?9, correlation_id=?10, version=?11, deleted=?12 \
                 WHERE id=?1",
                params![
                    rec.id,
                    rec.category.as_str(),
                    enc.title,
                    enc.content,
                    enc.tags,
                    now,
                    rec.importance,
                    enc.source,
                    rec.device_id,
                    rec.correlation_id,
                    rec.version,
                    rec.deleted as i64,
                ],
            )
            .map_err(|e| storage_err("ERR_MEM_UPDATE", e))?;
        if rows == 0 {
            return Err(storage_err(
                "ERR_MEM_NOT_FOUND",
                format!("no record with id {}", rec.id),
            ));
        }
        self.log(
            "update",
            &rec.id,
            rec.category,
            rec.correlation_id.as_deref(),
        );
        Ok(())
    }

    /// Soft-delete a record (recoverable). Errors if the id is absent or already deleted.
    pub fn soft_delete(&self, id: &str) -> Result<()> {
        let now = now_millis();
        let rows = self
            .conn
            .execute(
                "UPDATE memories SET deleted=1, updated_at=?2 WHERE id=?1 AND deleted=0",
                params![id, now],
            )
            .map_err(|e| storage_err("ERR_MEM_DELETE", e))?;
        if rows == 0 {
            return Err(storage_err(
                "ERR_MEM_NOT_FOUND",
                format!("no active record with id {id}"),
            ));
        }
        log_activity("memory", "memory.soft_delete", &format!("id={id}"), None);
        Ok(())
    }

    /// Restore a soft-deleted record. Errors if the id is absent or not deleted.
    pub fn restore(&self, id: &str) -> Result<()> {
        let now = now_millis();
        let rows = self
            .conn
            .execute(
                "UPDATE memories SET deleted=0, updated_at=?2 WHERE id=?1 AND deleted=1",
                params![id, now],
            )
            .map_err(|e| storage_err("ERR_MEM_RESTORE", e))?;
        if rows == 0 {
            return Err(storage_err(
                "ERR_MEM_NOT_FOUND",
                format!("no deleted record with id {id}"),
            ));
        }
        log_activity("memory", "memory.restore", &format!("id={id}"), None);
        Ok(())
    }

    /// Permanently remove a single record (bypasses soft-delete).
    pub fn purge(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM memories WHERE id=?1", params![id])
            .map_err(|e| storage_err("ERR_MEM_DELETE", e))?;
        log_activity("memory", "memory.purge", &format!("id={id}"), None);
        Ok(())
    }

    /// Permanently remove all soft-deleted records. Returns the number removed.
    pub fn purge_deleted(&self) -> Result<usize> {
        let rows = self
            .conn
            .execute("DELETE FROM memories WHERE deleted=1", [])
            .map_err(|e| storage_err("ERR_MEM_DELETE", e))?;
        log_activity(
            "memory",
            "memory.purge_deleted",
            &format!("count={rows}"),
            None,
        );
        Ok(rows)
    }

    /// Fetch a single record by id (including soft-deleted).
    pub fn find_by_id(&self, id: &str) -> Result<Option<MemoryRecord>> {
        let raw = self
            .conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM memories WHERE id=?1"),
                params![id],
                Self::map_raw,
            )
            .optional()
            .map_err(|e| storage_err("ERR_MEM_QUERY", e))?;
        match raw {
            Some(raw) => Ok(Some(decode(&self.cipher, raw)?)),
            None => Ok(None),
        }
    }

    /// Whether an active (non-deleted) record with this id exists.
    pub fn exists(&self, id: &str) -> Result<bool> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM memories WHERE id=?1 AND deleted=0",
                params![id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| storage_err("ERR_MEM_QUERY", e))?;
        Ok(found.is_some())
    }

    fn map_raw(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
        Ok(RawRow {
            id: row.get(0)?,
            category: row.get(1)?,
            title: row.get(2)?,
            content: row.get(3)?,
            tags: row.get(4)?,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
            importance: row.get(7)?,
            source: row.get(8)?,
            device_id: row.get(9)?,
            correlation_id: row.get(10)?,
            version: row.get(11)?,
            deleted: row.get(12)?,
        })
    }

    /// Fetch candidate rows by metadata filters (category, deleted) in sort order.
    fn fetch(
        &self,
        category: Option<MemoryCategory>,
        include_deleted: bool,
        sort: Option<SortBy>,
    ) -> Result<Vec<MemoryRecord>> {
        let mut sql = format!("SELECT {COLUMNS} FROM memories");
        let mut clauses: Vec<String> = Vec::new();
        if !include_deleted {
            clauses.push("deleted = 0".to_string());
        }
        if let Some(cat) = category {
            // `cat.as_str()` is a fixed enum literal, not user input — safe to inline.
            clauses.push(format!("category = '{}'", cat.as_str()));
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY ");
        sql.push_str(order_clause(sort));

        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| storage_err("ERR_MEM_QUERY", e))?;
        let raws = stmt
            .query_map([], Self::map_raw)
            .map_err(|e| storage_err("ERR_MEM_QUERY", e))?
            .collect::<rusqlite::Result<Vec<RawRow>>>()
            .map_err(|e| storage_err("ERR_MEM_QUERY", e))?;
        raws.into_iter()
            .map(|raw| decode(&self.cipher, raw))
            .collect()
    }

    /// Run a query: metadata filters in SQL, then text/tag matching and pagination.
    pub fn query(&self, q: &Query) -> Result<Vec<MemoryRecord>> {
        let mut records = self.fetch(q.category, q.include_deleted, q.sort)?;
        if let Some(text) = &q.text {
            let mode = q.mode.unwrap_or(SearchMode::Contains);
            let needle = if q.case_insensitive {
                text.to_lowercase()
            } else {
                text.clone()
            };
            records.retain(|r| text_matches(r, &needle, mode, q.case_insensitive));
        }
        if !q.tags.is_empty() {
            records.retain(|r| tags_match(r, &q.tags, q.case_insensitive));
        }
        let out = records
            .into_iter()
            .skip(q.offset)
            .take(q.limit.unwrap_or(usize::MAX))
            .collect();
        Ok(out)
    }

    /// Count records matching a query (ignoring pagination).
    pub fn count(&self, q: &Query) -> Result<usize> {
        if q.text.is_none() && q.tags.is_empty() {
            let mut sql = String::from("SELECT COUNT(*) FROM memories");
            let mut clauses: Vec<String> = Vec::new();
            if !q.include_deleted {
                clauses.push("deleted = 0".to_string());
            }
            if let Some(cat) = q.category {
                clauses.push(format!("category = '{}'", cat.as_str()));
            }
            if !clauses.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&clauses.join(" AND "));
            }
            let n: i64 = self
                .conn
                .query_row(&sql, [], |r| r.get(0))
                .map_err(|e| storage_err("ERR_MEM_QUERY", e))?;
            return Ok(n as usize);
        }
        let mut counting = q.clone();
        counting.limit = None;
        counting.offset = 0;
        Ok(self.query(&counting)?.len())
    }

    /// Apply a set of operations atomically.
    pub fn transaction(&mut self, ops: &[MemoryOp]) -> Result<()> {
        let now = now_millis();
        let tx = self
            .conn
            .transaction()
            .map_err(|e| storage_err("ERR_MEM_TX", e))?;
        for op in ops {
            match op {
                MemoryOp::Insert(rec) => {
                    let enc = encode(&self.cipher, rec)?;
                    tx.execute(
                        &format!(
                            "INSERT INTO memories ({COLUMNS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"
                        ),
                        params![
                            rec.id,
                            rec.category.as_str(),
                            enc.title,
                            enc.content,
                            enc.tags,
                            rec.created_at,
                            rec.updated_at,
                            rec.importance,
                            enc.source,
                            rec.device_id,
                            rec.correlation_id,
                            rec.version,
                            rec.deleted as i64,
                        ],
                    )
                    .map_err(|e| storage_err("ERR_MEM_TX", e))?;
                }
                MemoryOp::Update(rec) => {
                    let enc = encode(&self.cipher, rec)?;
                    let rows = tx
                        .execute(
                            "UPDATE memories SET category=?2, title=?3, content=?4, tags=?5, \
                             updated_at=?6, importance=?7, source=?8, device_id=?9, \
                             correlation_id=?10, version=?11, deleted=?12 WHERE id=?1",
                            params![
                                rec.id,
                                rec.category.as_str(),
                                enc.title,
                                enc.content,
                                enc.tags,
                                now,
                                rec.importance,
                                enc.source,
                                rec.device_id,
                                rec.correlation_id,
                                rec.version,
                                rec.deleted as i64,
                            ],
                        )
                        .map_err(|e| storage_err("ERR_MEM_TX", e))?;
                    if rows == 0 {
                        return Err(storage_err(
                            "ERR_MEM_NOT_FOUND",
                            format!("no record {}", rec.id),
                        ));
                    }
                }
                MemoryOp::SoftDelete(id) => {
                    tx.execute(
                        "UPDATE memories SET deleted=1, updated_at=?2 WHERE id=?1 AND deleted=0",
                        params![id, now],
                    )
                    .map_err(|e| storage_err("ERR_MEM_TX", e))?;
                }
                MemoryOp::Restore(id) => {
                    tx.execute(
                        "UPDATE memories SET deleted=0, updated_at=?2 WHERE id=?1 AND deleted=1",
                        params![id, now],
                    )
                    .map_err(|e| storage_err("ERR_MEM_TX", e))?;
                }
            }
        }
        tx.commit().map_err(|e| storage_err("ERR_MEM_TX", e))?;
        log_activity(
            "memory",
            "memory.transaction",
            &format!("ops={}", ops.len()),
            None,
        );
        Ok(())
    }

    /// Total number of records (including soft-deleted).
    pub fn total(&self) -> Result<usize> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
            .map_err(|e| storage_err("ERR_MEM_QUERY", e))?;
        Ok(n as usize)
    }

    /// Write a consistent copy of the database to `dest` (a hot backup).
    pub fn backup(&self, dest: &Path) -> Result<()> {
        if dest.exists() {
            std::fs::remove_file(dest).map_err(|e| storage_err("ERR_MEM_BACKUP", e))?;
        }
        let escaped = dest.to_string_lossy().replace('\'', "''");
        self.conn
            .execute(&format!("VACUUM INTO '{escaped}'"), [])
            .map_err(|e| storage_err("ERR_MEM_BACKUP", e))?;
        log_activity(
            "memory",
            "memory.backup",
            &format!("dest={}", dest.display()),
            None,
        );
        Ok(())
    }

    /// Reclaim unused space and defragment the database file.
    pub fn vacuum(&self) -> Result<()> {
        self.conn
            .execute("VACUUM", [])
            .map_err(|e| storage_err("ERR_MEM_VACUUM", e))?;
        Ok(())
    }
}
