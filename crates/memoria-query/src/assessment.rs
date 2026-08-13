use crate::consolidate::MemoryResult;

#[derive(Clone, Debug, PartialEq)]
pub struct RecallAssessment {
    pub result_count: usize,
    pub top_relevance: Option<f32>,
    pub mean_confidence: f32,
    pub channel_coverage: f32,
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
        channel_coverage,
    }
}
