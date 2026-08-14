use crate::consolidate::MemoryResult;

#[derive(Clone, Debug, PartialEq)]
pub struct RecallAssessment {
    pub result_count: usize,
    pub top_relevance: Option<f32>,
    pub mean_confidence: f32,
    pub mean_accessibility: f32,
    pub mean_effort: f32,
    pub channel_coverage: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RetrievalCost {
    pub ann_searches: usize,
    pub candidate_count: usize,
    pub graph_edge_visits: usize,
    pub relation_expansions: usize,
    pub provider_barriers: usize,
}

/// Normalize operator work into an effort signal. This is an accessibility
/// cost indicator, not a relevance or factual-confidence score.
#[must_use]
pub fn effort_from_cost(cost: RetrievalCost) -> f32 {
    let raw = cost.ann_searches as f32 / 4.0
        + cost.candidate_count as f32 / 256.0
        + cost.graph_edge_visits as f32 / 1024.0
        + cost.relation_expansions as f32 / 64.0
        + cost.provider_barriers as f32 / 2.0;
    raw.clamp(0.0, 1.0)
}

#[must_use]
pub fn assess(results: &[MemoryResult]) -> RecallAssessment {
    let result_count = results.len();
    let top_relevance = results.first().map(|result| result.relevance);
    let mean_confidence = if results.is_empty() {
        0.0
    } else {
        results.iter().map(|result| result.confidence).sum::<f32>() / results.len() as f32
    };
    let mean_accessibility = if results.is_empty() {
        0.0
    } else {
        results
            .iter()
            .map(|result| result.accessibility)
            .sum::<f32>()
            / results.len() as f32
    };
    let mean_effort = if results.is_empty() {
        0.0
    } else {
        results.iter().map(|result| result.effort).sum::<f32>() / results.len() as f32
    };
    let channel_coverage = if results.is_empty() {
        0.0
    } else {
        results
            .iter()
            .flat_map(|result| result.matches.iter())
            .map(|item| {
                let evidence = &item.evidence;
                [
                    !evidence.exact.is_empty(),
                    !evidence.lexical.is_empty(),
                    !evidence.semantic.is_empty(),
                    !evidence.tags.is_empty(),
                    !evidence.propagation.is_empty(),
                    !evidence.relations.is_empty(),
                    !evidence.history.is_empty(),
                ]
                .into_iter()
                .filter(|present| *present)
                .count() as f32
                    / 7.0
            })
            .sum::<f32>()
            / results.len() as f32
    };
    RecallAssessment {
        result_count,
        top_relevance,
        mean_confidence,
        mean_accessibility,
        mean_effort,
        channel_coverage,
    }
}
