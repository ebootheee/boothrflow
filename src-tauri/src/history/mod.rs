//! Persistent dictation history with hybrid lexical + semantic search.
//!
//! Storage: SQLite (rusqlite, bundled) + FTS5 virtual table for lexical
//! BM25 search. Embeddings come from Ollama (or any OpenAI-compatible
//! `/v1/embeddings` endpoint) and are stored as `BLOB` columns of packed
//! little-endian f32 bytes. Semantic search is a brute-force cosine scan
//! against the BLOB column — fine for sub-10k entries; swap in `sqlite-vec`
//! when histories actually grow.
//!
//! The session daemon calls [`HistoryStore::record`] after every dictation;
//! `record` returns immediately after the SQL insert and queues a background
//! task that hits the embedding endpoint and `UPDATE`s the row's `embedding`
//! column. Search APIs return entries even if the embedding hasn't backfilled
//! yet (lexical works without it).
//!
//! ## Configuration (env vars)
//!
//! - `BOOTHRFLOW_EMBED_ENDPOINT` (default `http://localhost:11434/v1/embeddings`)
//! - `BOOTHRFLOW_EMBED_MODEL`    (default `nomic-embed-text`)
//! - `BOOTHRFLOW_EMBED_API_KEY`  (optional bearer for cloud BYOK)
//! - `BOOTHRFLOW_HISTORY_DISABLED=1` to skip persistence entirely

#[cfg(feature = "real-engines")]
mod real {
    pub mod embed;
    pub mod search;
    pub mod store;
    pub use store::{HistoryEntry, HistoryStats, HistoryStore, RecordRequest, SearchResult};
}

#[cfg(feature = "real-engines")]
pub use real::{HistoryEntry, HistoryStats, HistoryStore, RecordRequest, SearchResult};
