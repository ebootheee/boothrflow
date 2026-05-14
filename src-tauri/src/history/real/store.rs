//! `HistoryStore` — the public face of the history module.
//!
//! Owns the SQLite connection (in a `Mutex` for thread-safe access from
//! the session daemon, the embedding worker thread, and Tauri commands)
//! and an embedding client. `record()` returns immediately after the
//! row insert; embedding fires-and-forgets on a background thread so
//! the dictation hot path stays unblocked.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use rusqlite_migration::{Migrations, M};
use serde::Serialize;

use crate::error::{BoothError, Result};
use crate::history::real::embed::{pack, unpack, EmbeddingClient};
use crate::history::real::search::{cosine, rrf_merge, SearchSource};
use crate::settings::Style;

/// One persisted dictation utterance.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct HistoryEntry {
    pub id: i64,
    pub captured_at: String,
    pub raw: String,
    pub formatted: String,
    pub style: Style,
    pub app_exe: Option<String>,
    pub window_title: Option<String>,
    pub duration_ms: u64,
    pub llm_ms: u64,
    pub has_embedding: bool,
}

/// What the session daemon hands us when a dictation completes.
#[derive(Debug, Clone)]
pub struct RecordRequest {
    pub raw: String,
    pub formatted: String,
    pub style: Style,
    pub app_exe: Option<String>,
    pub window_title: Option<String>,
    pub duration_ms: u64,
    pub llm_ms: u64,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SearchResult {
    pub entry: HistoryEntry,
    pub score: f32,
    /// "lexical" | "semantic" | "both"
    pub source: String,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct HistoryStats {
    pub total_entries: i64,
    pub embedded_entries: i64,
    pub db_path: String,
    pub embed_endpoint: Option<String>,
    pub embed_model: Option<String>,
}

pub struct HistoryStore {
    conn: Arc<Mutex<Connection>>,
    db_path: PathBuf,
    embedder: Option<Arc<EmbeddingClient>>,
}

impl HistoryStore {
    /// Open or create the history database in `%APPDATA%/boothrflow/history.db`,
    /// run migrations, and bring up the embedding client (best-effort).
    pub fn open_default() -> Result<Self> {
        Self::open_default_with_settings(&crate::settings::current_app_settings().embed)
    }

    pub fn open_default_with_settings(
        embed_settings: &crate::settings::EmbedSettings,
    ) -> Result<Self> {
        let db_path = default_db_path()
            .ok_or_else(|| BoothError::internal("could not resolve user data dir"))?;
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| BoothError::internal(format!("create db dir: {e}")))?;
        }

        let mut conn = Connection::open(&db_path)
            .map_err(|e| BoothError::internal(format!("open sqlite: {e}")))?;

        // Sensible pragmas for a desktop-app SQLite: WAL for concurrent
        // readers + writer, NORMAL sync (durable enough, faster than FULL).
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| BoothError::internal(format!("WAL: {e}")))?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|e| BoothError::internal(format!("synchronous: {e}")))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| BoothError::internal(format!("foreign_keys: {e}")))?;

        migrations()
            .to_latest(&mut conn)
            .map_err(|e| BoothError::internal(format!("migrate: {e}")))?;

        let embedder = match EmbeddingClient::from_settings(embed_settings) {
            None => {
                tracing::info!("history: embedding disabled");
                None
            }
            Some(Ok(client)) => {
                tracing::info!(
                    "history: embedding via {} ({})",
                    client.endpoint(),
                    client.model()
                );
                Some(Arc::new(client))
            }
            Some(Err(e)) => {
                tracing::warn!("history: embedding client init failed, lexical-only search: {e}");
                None
            }
        };

        tracing::info!("history: opened {}", db_path.display());

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path,
            embedder,
        })
    }

    /// Persist a dictation. Returns the new row's id. The embedding (if
    /// available) is computed on a background thread and `UPDATE`d into the
    /// row when ready.
    pub fn record(&self, req: RecordRequest) -> Result<i64> {
        let captured_at = Utc::now().to_rfc3339();
        let style_str = style_str(&req.style);

        let id = {
            let conn = self.conn.lock();
            conn.execute(
                "INSERT INTO utterances
                   (captured_at, raw, formatted, style, app_exe, window_title, duration_ms, llm_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    captured_at,
                    req.raw,
                    req.formatted,
                    style_str,
                    req.app_exe,
                    req.window_title,
                    req.duration_ms as i64,
                    req.llm_ms as i64,
                ],
            )
            .map_err(|e| BoothError::internal(format!("history insert: {e}")))?;
            conn.last_insert_rowid()
        };

        // Fire-and-forget embedding. Failures only affect future semantic
        // search recall; lexical search still works.
        if let Some(embedder) = self.embedder.clone() {
            let conn = self.conn.clone();
            let formatted = req.formatted.clone();
            std::thread::Builder::new()
                .name(format!("embed-{id}"))
                .spawn(move || {
                    if let Err(e) = embed_and_store(&embedder, &conn, id, &formatted) {
                        tracing::warn!("embed-{id}: {e}");
                    }
                })
                .ok();
        }

        Ok(id)
    }

    /// Most-recent entries, newest first.
    pub fn recent(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let conn = self.conn.lock();
        load_entries(
            &conn,
            "SELECT id, captured_at, raw, formatted, style, app_exe, window_title,
                    duration_ms, llm_ms, embedding IS NOT NULL
             FROM utterances
             ORDER BY id DESC
             LIMIT ?1",
            &[&(limit as i64) as &dyn rusqlite::ToSql],
        )
    }

    /// Hybrid search: lexical (FTS5 BM25) + semantic (cosine) → RRF.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let started = Instant::now();
        let conn = self.conn.lock();

        // Lexical: FTS5 on `formatted`. Take 4× limit as candidates so RRF
        // has more material to merge.
        let lex_take = (limit * 4).max(20);
        let lexical = lexical_search(&conn, query, lex_take)?;

        // Semantic: only run if we have an embedder + at least one
        // embedded row. Otherwise hand back lexical-only.
        let semantic = if let Some(embedder) = self.embedder.as_ref() {
            // Drop the lock around the network call so other writers don't
            // stall behind embedding.
            drop(conn);
            let q_emb = embedder.embed(query)?;
            let conn = self.conn.lock();
            semantic_search(&conn, &q_emb, lex_take)?
        } else {
            Vec::new()
        };
        let conn = self.conn.lock();

        let merged = rrf_merge(&lexical, &semantic, 60, limit);

        // Re-fetch entries by id, preserving the merged order.
        let mut results = Vec::with_capacity(merged.len());
        for (id, score, source) in merged {
            if let Some(entry) = load_entry_by_id(&conn, id)? {
                results.push(SearchResult {
                    entry,
                    score,
                    source: source_str(source).into(),
                });
            }
        }

        tracing::debug!(
            "history search '{}': {} results in {} ms",
            query,
            results.len(),
            started.elapsed().as_millis()
        );
        Ok(results)
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM utterances WHERE id = ?1", params![id])
            .map_err(|e| BoothError::internal(format!("delete: {e}")))?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM utterances", [])
            .map_err(|e| BoothError::internal(format!("clear: {e}")))?;
        Ok(())
    }

    pub fn stats(&self) -> Result<HistoryStats> {
        let conn = self.conn.lock();
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM utterances", [], |r| r.get(0))
            .map_err(|e| BoothError::internal(format!("count: {e}")))?;
        let embedded: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM utterances WHERE embedding IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .map_err(|e| BoothError::internal(format!("count embedded: {e}")))?;
        Ok(HistoryStats {
            total_entries: total,
            embedded_entries: embedded,
            db_path: self.db_path.display().to_string(),
            embed_endpoint: self.embedder.as_ref().map(|e| e.endpoint().to_string()),
            embed_model: self.embedder.as_ref().map(|e| e.model().to_string()),
        })
    }

    /// Look up the full formatted text by id (used by `history_paste`).
    pub fn get_formatted(&self, id: i64) -> Result<Option<String>> {
        let conn = self.conn.lock();
        match conn.query_row(
            "SELECT formatted FROM utterances WHERE id = ?1",
            params![id],
            |r| r.get::<_, String>(0),
        ) {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(BoothError::internal(format!("get_formatted: {e}"))),
        }
    }
}

fn embed_and_store(
    embedder: &EmbeddingClient,
    conn: &Mutex<Connection>,
    id: i64,
    text: &str,
) -> Result<()> {
    let v = embedder.embed(text)?;
    let dims = v.len() as i64;
    let blob = pack(&v);
    let conn = conn.lock();
    conn.execute(
        "UPDATE utterances SET embedding = ?1, embedding_dims = ?2 WHERE id = ?3",
        params![blob, dims, id],
    )
    .map_err(|e| BoothError::internal(format!("update embedding: {e}")))?;
    Ok(())
}

fn lexical_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<i64>> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare(
            "SELECT rowid FROM utterance_fts
             WHERE utterance_fts MATCH ?1
             ORDER BY bm25(utterance_fts) ASC
             LIMIT ?2",
        )
        .map_err(|e| BoothError::internal(format!("prep lexical: {e}")))?;
    let rows = stmt
        .query_map(params![sanitize_fts_query(query), limit as i64], |r| {
            r.get::<_, i64>(0)
        })
        .map_err(|e| BoothError::internal(format!("query lexical: {e}")))?;
    rows.collect::<rusqlite::Result<Vec<i64>>>()
        .map_err(|e| BoothError::internal(format!("collect lexical: {e}")))
}

fn semantic_search(conn: &Connection, query_emb: &[f32], limit: usize) -> Result<Vec<i64>> {
    let mut stmt = conn
        .prepare("SELECT id, embedding FROM utterances WHERE embedding IS NOT NULL")
        .map_err(|e| BoothError::internal(format!("prep semantic: {e}")))?;

    let rows = stmt
        .query_map([], |r| {
            let id: i64 = r.get(0)?;
            let blob: Vec<u8> = r.get(1)?;
            Ok((id, blob))
        })
        .map_err(|e| BoothError::internal(format!("query semantic: {e}")))?;

    let mut scored: Vec<(i64, f32)> = rows
        .filter_map(|r| r.ok())
        .map(|(id, blob)| {
            let emb = unpack(&blob);
            (id, cosine(query_emb, &emb))
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    Ok(scored.into_iter().map(|(id, _)| id).collect())
}

fn load_entries(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<HistoryEntry>> {
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| BoothError::internal(format!("prep: {e}")))?;
    let rows = stmt
        .query_map(params, row_to_entry)
        .map_err(|e| BoothError::internal(format!("query: {e}")))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| BoothError::internal(format!("collect: {e}")))
}

fn load_entry_by_id(conn: &Connection, id: i64) -> Result<Option<HistoryEntry>> {
    match conn.query_row(
        "SELECT id, captured_at, raw, formatted, style, app_exe, window_title,
                duration_ms, llm_ms, embedding IS NOT NULL
         FROM utterances WHERE id = ?1",
        params![id],
        row_to_entry,
    ) {
        Ok(e) => Ok(Some(e)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(BoothError::internal(format!("get by id: {e}"))),
    }
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
    let style_str: String = row.get(4)?;
    Ok(HistoryEntry {
        id: row.get(0)?,
        captured_at: row.get(1)?,
        raw: row.get(2)?,
        formatted: row.get(3)?,
        style: parse_style(&style_str),
        app_exe: row.get(5)?,
        window_title: row.get(6)?,
        duration_ms: row.get::<_, i64>(7)? as u64,
        llm_ms: row.get::<_, i64>(8)? as u64,
        has_embedding: row.get::<_, i64>(9)? != 0,
    })
}

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(
        "CREATE TABLE utterances (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            captured_at TEXT NOT NULL,
            raw TEXT NOT NULL,
            formatted TEXT NOT NULL,
            style TEXT NOT NULL,
            app_exe TEXT,
            window_title TEXT,
            duration_ms INTEGER NOT NULL,
            llm_ms INTEGER NOT NULL DEFAULT 0,
            embedding BLOB,
            embedding_dims INTEGER
        );

        CREATE INDEX idx_utterances_captured_at ON utterances(captured_at DESC);

        CREATE VIRTUAL TABLE utterance_fts USING fts5(
            formatted,
            content='utterances',
            content_rowid='id',
            tokenize='porter unicode61'
        );

        CREATE TRIGGER utterances_ai AFTER INSERT ON utterances BEGIN
            INSERT INTO utterance_fts(rowid, formatted) VALUES (new.id, new.formatted);
        END;

        CREATE TRIGGER utterances_ad AFTER DELETE ON utterances BEGIN
            INSERT INTO utterance_fts(utterance_fts, rowid, formatted)
                VALUES('delete', old.id, old.formatted);
        END;

        CREATE TRIGGER utterances_au AFTER UPDATE OF formatted ON utterances BEGIN
            INSERT INTO utterance_fts(utterance_fts, rowid, formatted)
                VALUES('delete', old.id, old.formatted);
            INSERT INTO utterance_fts(rowid, formatted) VALUES (new.id, new.formatted);
        END;",
    )])
}

fn default_db_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("boothrflow").join("history.db"))
}

fn style_str(s: &Style) -> &'static str {
    match s {
        Style::Raw => "raw",
        Style::Light => "light",
        Style::Moderate => "moderate",
        Style::Assertive => "assertive",
        Style::CaptainsLog => "captains-log",
    }
}

fn parse_style(s: &str) -> Style {
    match s {
        "raw" => Style::Raw,
        // Legacy values from before the Wave 6 styles overhaul. Map them
        // forward so old history rows display under their new bucket
        // rather than falling into the default and losing the user's
        // original intent.
        "moderate" | "formal" => Style::Moderate,
        "assertive" => Style::Assertive,
        "captains-log" => Style::CaptainsLog,
        // Light is the default + soaks up the legacy Casual / VeryCasual
        // / Excited tone variants.
        _ => Style::Light,
    }
}

fn source_str(s: SearchSource) -> &'static str {
    match s {
        SearchSource::Lexical => "lexical",
        SearchSource::Semantic => "semantic",
        SearchSource::Both => "both",
    }
}

/// Sanitize a free-form user query for FTS5. FTS5 has its own syntax
/// (operators like `AND`, `OR`, `NOT`, `*`, quotes, `:`) that can blow up
/// on user input. We wrap the whole thing in a phrase quote and escape
/// embedded double quotes, which gets us "everything as a tokenised
/// substring match" — close enough for v0.
fn sanitize_fts_query(q: &str) -> String {
    let escaped = q.replace('"', "\"\"");
    format!("\"{escaped}\"")
}
