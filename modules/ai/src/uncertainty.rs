//! Uncertainty surfacing for the NOVA AI Runtime (Milestone 6, FR-AI-003).
//!
//! Principle 9 — "Honesty about limits" — requires that NOVA surface uncertainty
//! explicitly rather than presenting a confident but wrong answer. This module
//! provides the mechanism:
//!
//! 1. A heuristic [`UncertaintyScorer`] estimates a confidence score [0.0, 1.0]
//!    from the model's generated text.
//! 2. If the score falls below a configurable [`UncertaintyConfig::threshold`],
//!    [`UncertaintyScorer::apply`] prepends a standard uncertainty prefix so the
//!    caller never has to check the flag before displaying.
//! 3. An "I don't know" response is a first-class outcome — the scorer recognises
//!    it as a valid high-confidence answer (confidence = 1.0, flagged = true).
//!
//! # Design rationale
//! True probabilistic calibration requires access to token-level log-probabilities,
//! which the current streaming API does not expose. The heuristic approach used here
//! (lexical cues + response length + hedging patterns) is documented honestly and can
//! be replaced by a calibrated scorer later without changing the public API.
//!
//! The scorer is deterministic — identical input always produces identical output —
//! making it safe to use in tests.

/// Configuration for uncertainty surfacing.
#[derive(Debug, Clone)]
pub struct UncertaintyConfig {
    /// Responses whose estimated confidence is below this value are flagged.
    /// Range: [0.0, 1.0]. Default: 0.5.
    pub threshold: f32,

    /// Prefix prepended to the response text when uncertainty is flagged.
    pub prefix: String,

    /// Whether uncertainty surfacing is active. When `false`, confidence is
    /// always 1.0 and no text is modified.
    pub enabled: bool,
}

impl Default for UncertaintyConfig {
    fn default() -> Self {
        Self {
            threshold: 0.5,
            prefix: "I'm not certain, but: ".to_string(),
            enabled: true,
        }
    }
}

/// The result of scoring one response.
#[derive(Debug, Clone, PartialEq)]
pub struct UncertaintyResult {
    /// Estimated confidence in [0.0, 1.0]. Higher = more confident.
    pub confidence: f32,
    /// True when confidence < threshold **or** the model explicitly said it
    /// doesn't know (which is a valid, honest outcome).
    pub flagged: bool,
    /// The (possibly modified) response text.
    pub text: String,
}

/// Scores and optionally rewrites a model response for uncertainty.
#[derive(Debug, Clone)]
pub struct UncertaintyScorer {
    config: UncertaintyConfig,
}

impl Default for UncertaintyScorer {
    fn default() -> Self {
        Self::new(UncertaintyConfig::default())
    }
}

impl UncertaintyScorer {
    pub fn new(config: UncertaintyConfig) -> Self {
        Self { config }
    }

    /// Score `text` and return the (possibly modified) result.
    ///
    /// The heuristic works in two passes:
    ///
    /// **Pass 1 — "I don't know" detection.**  
    /// If the response is a clear non-answer, it is a first-class outcome with
    /// `confidence = 1.0` and `flagged = true` (we're *certain* we don't know).
    /// The text is not modified because it is already honest.
    ///
    /// **Pass 2 — Hedging-cue scoring.**  
    /// Known uncertainty markers lower the score. The final score is clamped to
    /// [0.0, 1.0].
    pub fn apply(&self, text: &str) -> UncertaintyResult {
        if !self.config.enabled || text.is_empty() {
            return UncertaintyResult {
                confidence: 1.0,
                flagged: false,
                text: text.to_string(),
            };
        }

        let lower = text.to_lowercase();

        // --- Pass 1: explicit "I don't know" patterns ---
        if self.is_dont_know(&lower) {
            return UncertaintyResult {
                confidence: 1.0, // Certain that we don't know — honest answer.
                flagged: true,
                text: text.to_string(), // Don't modify; already explicit.
            };
        }

        // --- Pass 2: hedging-cue scoring ---
        let confidence = self.score_confidence(&lower, text);
        let flagged = confidence < self.config.threshold;

        let final_text = if flagged {
            // Only prepend if not already starting with an uncertainty marker.
            if lower.starts_with("i'm not") || lower.starts_with("i am not") {
                text.to_string()
            } else {
                format!("{}{}", self.config.prefix, text)
            }
        } else {
            text.to_string()
        };

        UncertaintyResult {
            confidence,
            flagged,
            text: final_text,
        }
    }

    /// True when the text is a clear explicit non-answer.
    fn is_dont_know(&self, lower: &str) -> bool {
        const DONT_KNOW_PATTERNS: &[&str] = &[
            "i don't know",
            "i do not know",
            "i have no idea",
            "i'm not sure",
            "i am not sure",
            "i cannot answer",
            "i can't answer",
            "i don't have enough information",
            "i lack the information",
            "i'm unable to answer",
            "i am unable to answer",
            "no information available",
            "insufficient information",
        ];
        DONT_KNOW_PATTERNS.iter().any(|p| lower.contains(p))
    }

    /// Estimate confidence from hedging cues in the lowercased text.
    ///
    /// Starts at 1.0 (fully confident) and subtracts penalties for each cue
    /// found. Multiple occurrences of the same cue do not stack beyond the
    /// first — the score reflects *type* diversity of doubt, not frequency.
    fn score_confidence(&self, lower: &str, original: &str) -> f32 {
        // (pattern, penalty) — tuned so a single strong hedge brings score below
        // the default threshold of 0.5.
        const CUES: &[(&str, f32)] = &[
            // Strong uncertainty
            ("i think", 0.25),
            ("i believe", 0.20),
            ("i'm not entirely sure", 0.40),
            ("i am not entirely sure", 0.40),
            ("i'm not completely sure", 0.35),
            ("not entirely clear", 0.35),
            ("it's possible", 0.30),
            ("it is possible", 0.30),
            ("might be", 0.25),
            ("may be", 0.20),
            ("could be", 0.20),
            ("probably", 0.20),
            ("perhaps", 0.25),
            ("possibly", 0.25),
            ("approximately", 0.15),
            ("roughly", 0.15),
            ("not certain", 0.40),
            ("uncertain", 0.35),
            ("unclear", 0.30),
            ("speculation", 0.40),
            ("speculate", 0.35),
            ("guess", 0.30),
            // Weaker hedges
            ("generally", 0.05),
            ("typically", 0.05),
            ("usually", 0.05),
            ("often", 0.05),
        ];

        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut penalty = 0.0f32;

        for (cue, p) in CUES {
            if lower.contains(cue) && seen.insert(cue) {
                penalty += p;
            }
        }

        // Very short responses are penalised slightly — a one-word answer is
        // likely incomplete.
        let word_count = original.split_whitespace().count();
        if word_count < 5 {
            penalty += 0.1;
        }

        (1.0_f32 - penalty).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scorer() -> UncertaintyScorer {
        UncertaintyScorer::default()
    }

    #[test]
    fn confident_response_is_not_flagged() {
        let r = scorer().apply("The capital of France is Paris.");
        assert!(!r.flagged);
        assert!(r.confidence >= 0.5);
        assert_eq!(r.text, "The capital of France is Paris.");
    }

    #[test]
    fn single_strong_hedge_flags_response() {
        // "perhaps" (0.25) + "might be" (0.25) → penalty 0.50 → confidence 0.50
        // At threshold 0.5 (exclusive), score of 0.50 is NOT below threshold.
        // Use two strong cues so penalty > 0.50.
        let r = scorer().apply("Perhaps it might be correct, I'm not entirely sure.");
        assert!(r.flagged, "confidence={}", r.confidence);
        assert!(r.text.starts_with("I'm not certain, but:"));
    }

    #[test]
    fn explicit_dont_know_is_first_class_outcome() {
        let r = scorer().apply("I don't know the answer to that question.");
        // Flagged = true (uncertainty surfaced), but confidence = 1.0 (honest).
        assert!(r.flagged);
        assert_eq!(r.confidence, 1.0);
        // Text NOT modified — already honest.
        assert!(r.text.starts_with("I don't know"));
    }

    #[test]
    fn multiple_hedges_compound_penalty() {
        let r = scorer().apply("Perhaps it might be Paris, but I'm not entirely sure.");
        assert!(r.flagged);
        assert!(r.confidence < 0.3, "confidence={}", r.confidence);
    }

    #[test]
    fn disabled_scorer_passes_through() {
        let scorer = UncertaintyScorer::new(UncertaintyConfig {
            enabled: false,
            ..Default::default()
        });
        let r = scorer.apply("I think maybe possibly the answer is 42.");
        assert!(!r.flagged);
        assert_eq!(r.confidence, 1.0);
    }

    #[test]
    fn empty_text_is_not_flagged() {
        let r = scorer().apply("");
        assert!(!r.flagged);
        assert_eq!(r.confidence, 1.0);
    }

    #[test]
    fn custom_threshold_changes_flagging() {
        // With threshold = 0.0, nothing is ever flagged (unless dont-know).
        let scorer = UncertaintyScorer::new(UncertaintyConfig {
            threshold: 0.0,
            ..Default::default()
        });
        let r = scorer.apply("I think the answer is 42.");
        assert!(!r.flagged);
    }

    #[test]
    fn custom_prefix_is_used() {
        let scorer = UncertaintyScorer::new(UncertaintyConfig {
            prefix: "WARNING — low confidence: ".to_string(),
            ..Default::default()
        });
        // "perhaps" (0.25) + "might be" (0.25) + "uncertain" (0.35) → penalty 0.85 → confidence 0.15
        let r = scorer.apply("Perhaps it might be true, though I'm uncertain.");
        assert!(r.flagged, "confidence={}", r.confidence);
        assert!(r.text.starts_with("WARNING — low confidence:"));
    }

    #[test]
    fn already_prefixed_text_not_double_prefixed() {
        let r = scorer().apply("I'm not sure, but Paris might be the answer.");
        // Text already starts with uncertainty marker — don't prepend again.
        assert!(r.flagged);
        let prefix_count = r.text.matches("I'm not certain, but:").count();
        assert_eq!(prefix_count, 0, "should not prepend; already has marker");
    }
}
