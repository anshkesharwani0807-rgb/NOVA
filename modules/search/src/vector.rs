//! Vector store for semantic search (Milestone 5).
//!
//! An on-device, single-user index that holds embeddings in memory and answers
//! k-nearest-neighbour queries by **exact** cosine similarity. At personal-memory scale
//! (thousands to low tens of thousands of records) an exact linear scan is fast and
//! returns perfect recall, avoiding the approximate-index tradeoffs — and, crucially, the
//! missing removal/persistence support — of the ANN crates evaluated for M5.
//!
//! The index is a *derived* artifact: the authoritative embeddings live in the SQLite
//! store, so the index is rebuilt from there on open (see `SearchEngine::open`). This is
//! what makes semantic search survive restarts without a separate on-disk index format.
//! `remove` is exact, which Principle 2 (privacy) and permission-scoped source revocation
//! require: a revoked source's vectors must actually leave the searchable set.

use nova_kernel::{ErrorCategory, NovaError, Result};
use parking_lot::RwLock;
use std::collections::HashMap;

/// The semantic vector store: `internal_id -> embedding`.
pub struct VectorStore {
    vectors: RwLock<HashMap<usize, Vec<f32>>>,
    dimension: usize,
}

impl VectorStore {
    /// Open an empty vector store for `dimension`-length embeddings. The caller is
    /// expected to repopulate it from the persistent store (SQLite) after opening.
    pub fn open(dimension: usize) -> Result<Self> {
        Ok(Self {
            vectors: RwLock::new(HashMap::new()),
            dimension,
        })
    }

    fn check_dim(&self, embedding: &[f32]) -> Result<()> {
        if embedding.len() != self.dimension {
            return Err(NovaError::new(
                ErrorCategory::Internal,
                "ERR_SEARCH_VECTOR_DIM",
                &format!(
                    "Expected dimension {}, got {}",
                    self.dimension,
                    embedding.len()
                ),
            ));
        }
        Ok(())
    }

    /// Insert or replace the embedding for a document (a true upsert — no duplicates).
    pub fn upsert(&self, id: usize, embedding: &[f32]) -> Result<()> {
        self.check_dim(embedding)?;
        self.vectors.write().insert(id, embedding.to_vec());
        Ok(())
    }

    /// Remove a document's embedding from the index (exact; used for source revocation).
    pub fn remove(&self, id: usize) -> Result<()> {
        self.vectors.write().remove(&id);
        Ok(())
    }

    /// Number of indexed vectors.
    pub fn len(&self) -> usize {
        self.vectors.read().len()
    }

    /// Whether the index holds no vectors.
    pub fn is_empty(&self) -> bool {
        self.vectors.read().is_empty()
    }

    /// The k nearest neighbours to `embedding` by cosine similarity, returned as
    /// `(id, similarity)` with similarity in `[-1, 1]`, most-similar first.
    pub fn search(&self, embedding: &[f32], k: usize) -> Result<Vec<(usize, f32)>> {
        self.check_dim(embedding)?;
        let q_norm = norm(embedding);
        if q_norm == 0.0 || k == 0 {
            return Ok(Vec::new());
        }
        let vectors = self.vectors.read();
        let mut scored: Vec<(usize, f32)> = vectors
            .iter()
            .filter_map(|(id, v)| {
                let v_norm = norm(v);
                if v_norm == 0.0 {
                    return None;
                }
                Some((*id, dot(embedding, v) / (q_norm * v_norm)))
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        Ok(scored)
    }

    /// Persistence is provided by the SQLite store (the index is rebuilt on open), so
    /// there is no separate index file to flush. Kept for API symmetry.
    pub fn save(&self) -> Result<()> {
        Ok(())
    }

    /// Drop all vectors from the index.
    pub fn clear(&self) -> Result<()> {
        self.vectors.write().clear();
        Ok(())
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> VectorStore {
        VectorStore::open(3).unwrap()
    }

    #[test]
    fn upsert_replaces_not_duplicates() {
        let vs = store();
        vs.upsert(1, &[1.0, 0.0, 0.0]).unwrap();
        vs.upsert(1, &[0.0, 1.0, 0.0]).unwrap();
        assert_eq!(vs.len(), 1);
    }

    #[test]
    fn nearest_neighbour_ranks_by_cosine() {
        let vs = store();
        vs.upsert(1, &[1.0, 0.0, 0.0]).unwrap();
        vs.upsert(2, &[0.9, 0.1, 0.0]).unwrap();
        vs.upsert(3, &[0.0, 0.0, 1.0]).unwrap();
        let hits = vs.search(&[1.0, 0.0, 0.0], 2).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].0, 1);
        assert_eq!(hits[1].0, 2);
    }

    #[test]
    fn remove_takes_vector_out_of_results() {
        let vs = store();
        vs.upsert(1, &[1.0, 0.0, 0.0]).unwrap();
        vs.remove(1).unwrap();
        assert!(vs.search(&[1.0, 0.0, 0.0], 5).unwrap().is_empty());
    }

    #[test]
    fn dimension_mismatch_is_rejected() {
        let vs = store();
        assert!(vs.upsert(1, &[1.0, 0.0]).is_err());
        assert!(vs.search(&[1.0, 0.0], 1).is_err());
    }
}
