use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RankedResult {
    pub entity_id: String,
    pub name: String,
    pub score: f64,
    pub entity_relevance: f64,
    pub graph_distance: f64,
    pub embedding_score: f64,
    pub keyword_score: f64,
    pub recency_score: f64,
    pub confidence_score: f64,
    pub details: HashMap<String, f64>,
}

pub trait Ranker: Send + Sync {
    fn rank(&self, results: Vec<RankedResult>) -> Vec<RankedResult>;
    fn name(&self) -> &str;
}

pub struct CombinedRanker {
    weights: RankWeights,
}

#[derive(Debug, Clone)]
pub struct RankWeights {
    pub entity_relevance: f64,
    pub graph_distance: f64,
    pub embedding_score: f64,
    pub keyword_score: f64,
    pub recency: f64,
    pub confidence: f64,
}

impl Default for RankWeights {
    fn default() -> Self {
        Self {
            entity_relevance: 0.25,
            graph_distance: 0.15,
            embedding_score: 0.25,
            keyword_score: 0.15,
            recency: 0.10,
            confidence: 0.10,
        }
    }
}

impl Default for CombinedRanker {
    fn default() -> Self {
        Self::new()
    }
}

impl CombinedRanker {
    pub fn new() -> Self {
        Self {
            weights: RankWeights::default(),
        }
    }

    pub fn with_weights(weights: RankWeights) -> Self {
        Self { weights }
    }
}

impl Ranker for CombinedRanker {
    fn rank(&self, mut results: Vec<RankedResult>) -> Vec<RankedResult> {
        for r in &mut results {
            r.score = r.entity_relevance * self.weights.entity_relevance
                + (1.0 - r.graph_distance) * self.weights.graph_distance
                + r.embedding_score * self.weights.embedding_score
                + r.keyword_score * self.weights.keyword_score
                + r.recency_score * self.weights.recency
                + r.confidence_score * self.weights.confidence;
            r.details.insert("weighted_score".to_string(), r.score);
        }
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    fn name(&self) -> &str {
        "combined_ranker"
    }
}

pub struct RecencyRanker;

impl Ranker for RecencyRanker {
    fn rank(&self, mut results: Vec<RankedResult>) -> Vec<RankedResult> {
        results.sort_by(|a, b| {
            b.recency_score
                .partial_cmp(&a.recency_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    fn name(&self) -> &str {
        "recency_ranker"
    }
}

pub fn compute_recency_score(timestamp_ms: i64, now_ms: i64) -> f64 {
    let diff = now_ms.saturating_sub(timestamp_ms).max(0);
    if diff == 0 {
        return 1.0;
    }
    let days = diff as f64 / 86_400_000.0;
    (1.0 / (1.0 + days)).clamp(0.0, 1.0)
}

#[allow(dead_code)]
pub fn compute_graph_distance_score(distance: usize, max_depth: usize) -> f64 {
    if distance == 0 {
        return 1.0;
    }
    let normalized = distance as f64 / max_depth.max(1) as f64;
    (1.0 - normalized).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[allow(clippy::too_many_arguments)]
    fn make_result(
        id: &str,
        name: &str,
        entity_rel: f64,
        graph_dist: f64,
        emb: f64,
        kw: f64,
        recency: f64,
        conf: f64,
    ) -> RankedResult {
        RankedResult {
            entity_id: id.to_string(),
            name: name.to_string(),
            score: 0.0,
            entity_relevance: entity_rel,
            graph_distance: graph_dist,
            embedding_score: emb,
            keyword_score: kw,
            recency_score: recency,
            confidence_score: conf,
            details: HashMap::new(),
        }
    }

    #[test]
    fn test_combined_ranker_sorts_by_score() {
        let ranker = CombinedRanker::new();
        let results = vec![
            make_result("1", "Low", 0.1, 0.9, 0.1, 0.1, 0.1, 0.1),
            make_result("2", "High", 0.9, 0.1, 0.9, 0.9, 0.9, 0.9),
        ];
        let ranked = ranker.rank(results);
        assert_eq!(ranked[0].name, "High");
        assert_eq!(ranked[1].name, "Low");
    }

    #[test]
    fn test_recency_ranker() {
        let ranker = RecencyRanker;
        let results = vec![
            make_result("1", "Old", 0.0, 0.0, 0.0, 0.0, 0.1, 0.0),
            make_result("2", "Recent", 0.0, 0.0, 0.0, 0.0, 0.9, 0.0),
        ];
        let ranked = ranker.rank(results);
        assert_eq!(ranked[0].name, "Recent");
    }

    #[test]
    fn test_custom_weights() {
        let weights = RankWeights {
            entity_relevance: 1.0,
            graph_distance: 0.0,
            embedding_score: 0.0,
            keyword_score: 0.0,
            recency: 0.0,
            confidence: 0.0,
        };
        let ranker = CombinedRanker::with_weights(weights);
        let results = vec![
            make_result("1", "Low", 0.1, 0.0, 0.0, 0.0, 0.0, 0.0),
            make_result("2", "High", 0.9, 0.0, 0.0, 0.0, 0.0, 0.0),
        ];
        let ranked = ranker.rank(results);
        assert_eq!(ranked[0].name, "High");
        assert!(ranked[0].score > ranked[1].score);
    }

    #[test]
    fn test_compute_recency_score_now() {
        let now = chrono::Utc::now().timestamp_millis();
        assert!((compute_recency_score(now, now) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_recency_score_old() {
        let now = chrono::Utc::now().timestamp_millis();
        let old = now - 30 * 24 * 60 * 60 * 1000;
        let score = compute_recency_score(old, now);
        assert!(score < 0.5 && score > 0.0);
    }

    #[test]
    fn test_compute_graph_distance_score() {
        assert!((compute_graph_distance_score(0, 5) - 1.0).abs() < 0.01);
        assert!((compute_graph_distance_score(5, 5) - 0.0).abs() < 0.01);
        assert!((compute_graph_distance_score(2, 5) - 0.6).abs() < 0.01);
    }

    #[test]
    fn test_combined_ranker_score_calculation() {
        let ranker = CombinedRanker::new();
        let results = vec![make_result("1", "Test", 1.0, 0.0, 1.0, 1.0, 1.0, 1.0)];
        let ranked = ranker.rank(results);
        assert!((ranked[0].score - 1.0).abs() < 0.01);
    }
}
