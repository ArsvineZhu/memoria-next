use std::collections::{HashMap, HashSet};

use memoria_types::SpaceId;
use thiserror::Error;

use crate::consolidate::MemoryResult;
use crate::evidence::CandidateTarget;

pub const MAX_RERANK_CANDIDATES: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub struct RerankView {
    pub handle: String,
    pub target: CandidateTarget,
    pub text: String,
    pub base_score: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RerankBatch {
    pub query: String,
    pub views: Vec<RerankView>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RerankScore {
    pub handle: String,
    pub score: f32,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum RerankError {
    #[error("rerank candidate limit must be greater than zero")]
    InvalidLimit,

    #[error("rerank candidate limit {actual} exceeds {limit}")]
    LimitExceeded { actual: usize, limit: usize },

    #[error("rerank view handles must be unique")]
    DuplicateHandle,

    #[error("rerank result count {actual} does not match requested count {expected}")]
    ScoreCount { actual: usize, expected: usize },

    #[error("rerank returned an unknown or duplicate handle `{handle}`")]
    InvalidHandle { handle: String },

    #[error("rerank score for `{handle}` must be finite and between 0 and 1")]
    InvalidScore { handle: String },
}

pub fn build_rerank_batch(
    query: impl Into<String>,
    results: &[MemoryResult],
    allowed_spaces: &[SpaceId],
    limit: usize,
) -> Result<RerankBatch, RerankError> {
    if limit == 0 {
        return Err(RerankError::InvalidLimit);
    }
    if limit > MAX_RERANK_CANDIDATES {
        return Err(RerankError::LimitExceeded {
            actual: limit,
            limit: MAX_RERANK_CANDIDATES,
        });
    }
    let scope = allowed_spaces.iter().copied().collect::<HashSet<_>>();
    let mut views = results
        .iter()
        .filter(|result| scope.contains(&result.space_id))
        .map(to_view)
        .collect::<Vec<_>>();
    views.sort_by(|left, right| {
        right
            .base_score
            .total_cmp(&left.base_score)
            .then_with(|| left.handle.cmp(&right.handle))
    });
    views.truncate(limit);
    validate_unique_handles(&views)?;
    Ok(RerankBatch {
        query: query.into(),
        views,
    })
}

pub fn apply_rerank(
    results: Vec<MemoryResult>,
    batch: &RerankBatch,
    scores: &[RerankScore],
    allowed_spaces: &[SpaceId],
) -> Result<Vec<MemoryResult>, RerankError> {
    if scores.len() != batch.views.len() {
        return Err(RerankError::ScoreCount {
            actual: scores.len(),
            expected: batch.views.len(),
        });
    }
    validate_unique_handles(&batch.views)?;
    let expected = batch
        .views
        .iter()
        .map(|view| view.handle.as_str())
        .collect::<HashSet<_>>();
    let mut by_handle = HashMap::new();
    for score in scores {
        if !expected.contains(score.handle.as_str()) || by_handle.contains_key(&score.handle) {
            return Err(RerankError::InvalidHandle {
                handle: score.handle.clone(),
            });
        }
        if !score.score.is_finite() || !(0.0..=1.0).contains(&score.score) {
            return Err(RerankError::InvalidScore {
                handle: score.handle.clone(),
            });
        }
        by_handle.insert(score.handle.clone(), score.score);
    }

    let scope = allowed_spaces.iter().copied().collect::<HashSet<_>>();
    let mut admissible = results
        .into_iter()
        .filter(|result| scope.contains(&result.space_id))
        .collect::<Vec<_>>();
    admissible.sort_by(|left, right| {
        let left_score = rerank_score(left, batch, &by_handle);
        let right_score = rerank_score(right, batch, &by_handle);
        right_score
            .is_some()
            .cmp(&left_score.is_some())
            .then_with(|| {
                right_score
                    .unwrap_or(f32::NEG_INFINITY)
                    .total_cmp(&left_score.unwrap_or(f32::NEG_INFINITY))
            })
            .then_with(|| right.relevance.total_cmp(&left.relevance))
            .then_with(|| left.memory_id.cmp(&right.memory_id))
    });
    Ok(admissible)
}

#[must_use]
pub fn rerank_handle(target: CandidateTarget) -> String {
    format!(
        "{}:{}:{}",
        target.space_id, target.memory_id, target.revision_id
    )
}

fn to_view(result: &MemoryResult) -> RerankView {
    let target = CandidateTarget {
        space_id: result.space_id,
        memory_id: result.memory_id,
        revision_id: result.revision_id,
    };
    RerankView {
        handle: rerank_handle(target),
        target,
        text: result
            .matches
            .first()
            .map(|item| item.evidence.text.clone())
            .unwrap_or_default(),
        base_score: result.relevance,
    }
}

fn validate_unique_handles(views: &[RerankView]) -> Result<(), RerankError> {
    let mut handles = HashSet::new();
    for view in views {
        if !handles.insert(view.handle.as_str()) {
            return Err(RerankError::DuplicateHandle);
        }
    }
    Ok(())
}

fn rerank_score(
    result: &MemoryResult,
    batch: &RerankBatch,
    scores: &HashMap<String, f32>,
) -> Option<f32> {
    let target = CandidateTarget {
        space_id: result.space_id,
        memory_id: result.memory_id,
        revision_id: result.revision_id,
    };
    batch
        .views
        .iter()
        .find(|view| view.target == target)
        .and_then(|view| scores.get(&view.handle).copied())
}
