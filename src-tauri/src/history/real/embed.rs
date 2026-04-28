//! Embedding client — POST text to an OpenAI-compatible
//! `/v1/embeddings` endpoint. Default target is local Ollama with the
//! `nomic-embed-text` model (768 dims).

use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::error::{BoothError, Result};

pub const DEFAULT_ENDPOINT: &str = "http://localhost:11434/v1/embeddings";
pub const DEFAULT_MODEL: &str = "nomic-embed-text";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

pub struct EmbeddingClient {
    endpoint: String,
    model: String,
    api_key: Option<String>,
    client: Client,
}

impl EmbeddingClient {
    pub fn new(endpoint: String, model: String, api_key: Option<String>) -> Result<Self> {
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| BoothError::internal(format!("embed client: {e}")))?;
        Ok(Self {
            endpoint,
            model,
            api_key,
            client,
        })
    }

    pub fn from_env() -> Option<Result<Self>> {
        if std::env::var("BOOTHRFLOW_HISTORY_DISABLED").is_ok() {
            return None;
        }
        let endpoint =
            std::env::var("BOOTHRFLOW_EMBED_ENDPOINT").unwrap_or_else(|_| DEFAULT_ENDPOINT.into());
        let model =
            std::env::var("BOOTHRFLOW_EMBED_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.into());
        let api_key = std::env::var("BOOTHRFLOW_EMBED_API_KEY").ok();
        Some(Self::new(endpoint, model, api_key))
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Compute an embedding for the given text. Returns the raw vector;
    /// caller decides how to store it.
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let started = Instant::now();
        let body = EmbedRequest {
            model: &self.model,
            input: text,
        };
        let mut req = self.client.post(&self.endpoint).json(&body);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        let res = req
            .send()
            .map_err(|e| BoothError::internal(format!("embed http: {e}")))?;

        let status = res.status();
        if !status.is_success() {
            let body = res.text().unwrap_or_default();
            return Err(BoothError::internal(format!("embed http {status}: {body}")));
        }

        let parsed: EmbedResponse = res
            .json()
            .map_err(|e| BoothError::internal(format!("embed json: {e}")))?;

        let embedding = parsed
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| BoothError::internal("embed: empty data"))?;

        tracing::debug!(
            "embed: {} dims in {} ms",
            embedding.len(),
            started.elapsed().as_millis()
        );
        Ok(embedding)
    }
}

#[derive(Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    input: &'a str,
}

#[derive(Deserialize)]
struct EmbedResponse {
    data: Vec<EmbedDatum>,
}

#[derive(Deserialize)]
struct EmbedDatum {
    embedding: Vec<f32>,
}

/// Pack a `&[f32]` as little-endian bytes for storage in a SQLite BLOB.
pub fn pack(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// Unpack a SQLite BLOB into `Vec<f32>`. Trailing partial floats are dropped.
pub fn unpack(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_roundtrip() {
        let original = vec![0.0, 1.5, -2.25, 42.0, f32::MIN_POSITIVE];
        let packed = pack(&original);
        let unpacked = unpack(&packed);
        assert_eq!(unpacked.len(), original.len());
        for (a, b) in original.iter().zip(&unpacked) {
            assert_eq!(a.to_bits(), b.to_bits());
        }
    }
}
