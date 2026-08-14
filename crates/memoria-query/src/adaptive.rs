use memoria_adaptive::{AdaptiveStateV1, QueryAdaptiveSignature};
use memoria_types::{MemoryId, SpaceId, Timestamp};

use crate::consolidate::MemoryResult;

pub const MAX_ADAPTIVE_PRIOR: f32 = 0.1;

/// Apply Adaptive only to already admissible query results.
///
/// Base relevance remains the primary ordering key. The bounded prior breaks
/// ties and cannot manufacture a candidate or make a lower-relevance result
/// outrank a materially stronger result. Accessibility is context-conditioned
/// and does not participate in hard filtering.
pub fn rank_with_adaptive(
    mut results: Vec<MemoryResult>,
    state: &AdaptiveStateV1,
    signature: &QueryAdaptiveSignature,
    now: Timestamp,
) -> Vec<MemoryResult> {
    let scope = signature
        .scope
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let mut scored = results
        .drain(..)
        .map(|mut result| {
            let accessibility = if scope.contains(&result.space_id) {
                context_accessibility(state, signature, result.space_id, result.memory_id, now)
            } else {
                0.0
            };
            result.accessibility = accessibility;
            let prior = MAX_ADAPTIVE_PRIOR * accessibility;
            (result, prior)
        })
        .collect::<Vec<_>>();
    scored.sort_by(|(left, left_prior), (right, right_prior)| {
        right
            .relevance
            .total_cmp(&left.relevance)
            .then_with(|| right_prior.total_cmp(left_prior))
            .then_with(|| left.memory_id.cmp(&right.memory_id))
            .then_with(|| left.revision_id.cmp(&right.revision_id))
    });
    scored.into_iter().map(|(result, _)| result).collect()
}

#[must_use]
pub fn adaptive_prior(
    state: &AdaptiveStateV1,
    signature: &QueryAdaptiveSignature,
    space_id: SpaceId,
    memory_id: MemoryId,
    now: Timestamp,
) -> f32 {
    if !signature.scope.contains(&space_id) {
        return 0.0;
    }
    MAX_ADAPTIVE_PRIOR * context_accessibility(state, signature, space_id, memory_id, now)
}

fn context_accessibility(
    state: &AdaptiveStateV1,
    signature: &QueryAdaptiveSignature,
    space_id: SpaceId,
    memory_id: MemoryId,
    now: Timestamp,
) -> f32 {
    let target = state
        .familiarity(space_id, memory_id)
        .map_or(0.0, |familiarity| familiarity.accessibility_at(now));
    let mut affinity_sum = 0.0;
    let mut affinity_count = 0.0;
    for tag in &signature.explicit_tags {
        if let Some(affinity) = state.tag_affinity(space_id, tag, memory_id) {
            affinity_sum += (affinity.score() + 1.0) / 2.0;
            affinity_count += 1.0;
        }
    }
    if let Some(query_class) = signature.query_class.as_deref()
        && let Some(affinity) = state.query_class_affinity(space_id, query_class, memory_id)
    {
        affinity_sum += (affinity.score() + 1.0) / 2.0;
        affinity_count += 1.0;
    }
    let affinity = if affinity_count == 0.0 {
        0.0
    } else {
        affinity_sum / affinity_count
    };
    (0.75 * target + 0.25 * affinity).clamp(0.0, 1.0) as f32
}
