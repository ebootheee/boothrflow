//! Hybrid search: BM25 (FTS5) + cosine (brute force) merged with
//! Reciprocal Rank Fusion.
//!
//! Why RRF: it's robust to the wildly different score scales of BM25 vs
//! cosine, requires no normalization, and does well empirically vs
//! weighted-sum schemes. Standard k=60.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub enum SearchSource {
    Lexical,
    Semantic,
    Both,
}

/// Cosine similarity in `[-1, 1]`. Returns 0 if either side is zero-norm.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let n = a.len().min(b.len());
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..n {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

/// RRF — given two ranked lists of `(id, score_unused)` pairs, merge into
/// a single ranking using `1 / (k + rank)`. Returns top-`limit` IDs paired
/// with the source(s) they came from and the merged RRF score.
pub fn rrf_merge(
    lexical: &[i64],
    semantic: &[i64],
    k: u32,
    limit: usize,
) -> Vec<(i64, f32, SearchSource)> {
    let mut scores: HashMap<i64, (f32, bool, bool)> = HashMap::new();

    for (rank, id) in lexical.iter().enumerate() {
        let entry = scores.entry(*id).or_insert((0.0, false, false));
        entry.0 += 1.0 / (k as f32 + rank as f32 + 1.0);
        entry.1 = true;
    }
    for (rank, id) in semantic.iter().enumerate() {
        let entry = scores.entry(*id).or_insert((0.0, false, false));
        entry.0 += 1.0 / (k as f32 + rank as f32 + 1.0);
        entry.2 = true;
    }

    let mut merged: Vec<(i64, f32, SearchSource)> = scores
        .into_iter()
        .map(|(id, (score, in_lex, in_sem))| {
            let src = match (in_lex, in_sem) {
                (true, true) => SearchSource::Both,
                (true, false) => SearchSource::Lexical,
                (false, true) => SearchSource::Semantic,
                (false, false) => SearchSource::Lexical, // unreachable
            };
            (id, score, src)
        })
        .collect();

    merged.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    merged.truncate(limit);
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_orthogonal_is_zero() {
        let a = [1.0f32, 0.0];
        let b = [0.0f32, 1.0];
        assert!(cosine(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_identical_is_one() {
        let a = [1.0f32, 2.0, 3.0];
        let b = [2.0f32, 4.0, 6.0]; // collinear
        assert!((cosine(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_opposite_is_neg_one() {
        let a = [1.0f32, 2.0, 3.0];
        let b = [-1.0f32, -2.0, -3.0];
        assert!((cosine(&a, &b) - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn rrf_promotes_overlap() {
        // id 1 appears at rank 0 in both lists → highest combined score.
        // id 2 only in lexical, id 3 only in semantic.
        let merged = rrf_merge(&[1, 2, 4], &[1, 3, 4], 60, 5);
        assert_eq!(merged[0].0, 1, "overlap should win");
        assert!(matches!(merged[0].2, SearchSource::Both));
    }

    #[test]
    fn rrf_respects_rank_decay() {
        let merged = rrf_merge(&[1, 2, 3], &[], 60, 5);
        let s1 = merged.iter().find(|(id, _, _)| *id == 1).unwrap().1;
        let s3 = merged.iter().find(|(id, _, _)| *id == 3).unwrap().1;
        assert!(s1 > s3);
    }
}
