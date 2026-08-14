use std::collections::BTreeSet;

use memoria_derived::EnrichmentProjection;
use thiserror::Error;

use crate::privacy::ProviderCapability;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingItem {
    pub key: String,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingBatchRequest {
    pub work_id: String,
    pub signature: String,
    pub dimensions: usize,
    pub items: Vec<EmbeddingItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RerankBatchRequest {
    pub work_id: String,
    pub signature: String,
    pub query: String,
    pub candidates: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnrichmentBatchRequest {
    pub work_id: String,
    pub signature: String,
    pub projection: EnrichmentProjection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NeedWork {
    Embeddings(EmbeddingBatchRequest),
    Rerank(RerankBatchRequest),
    Enrichment(EnrichmentBatchRequest),
}

impl NeedWork {
    #[must_use]
    pub const fn capability(&self) -> ProviderCapability {
        match self {
            Self::Embeddings(_) => ProviderCapability::Embedding,
            Self::Rerank(_) => ProviderCapability::Rerank,
            Self::Enrichment(_) => ProviderCapability::Enrichment,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RerankScore {
    pub handle: String,
    pub score: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddingVector {
    pub key: String,
    pub values: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProviderWorkResult {
    Embeddings {
        work_id: String,
        vectors: Vec<EmbeddingVector>,
    },
    Rerank {
        work_id: String,
        scores: Vec<RerankScore>,
    },
    Enrichment {
        work_id: String,
        tags: Vec<String>,
    },
    Failure {
        work_id: String,
        retryable: bool,
        code: String,
        message: String,
    },
}

impl ProviderWorkResult {
    #[must_use]
    pub fn work_id(&self) -> &str {
        match self {
            Self::Embeddings { work_id, .. }
            | Self::Rerank { work_id, .. }
            | Self::Enrichment { work_id, .. }
            | Self::Failure { work_id, .. } => work_id,
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ProviderResultValidationError {
    #[error("provider result work id does not match the requested work")]
    WorkIdMismatch,

    #[error("provider result variant does not match the requested work")]
    VariantMismatch,

    #[error("embedding result item count does not match the requested work")]
    EmbeddingItemCount,

    #[error("embedding result keys must contain exactly one item for every requested key")]
    EmbeddingKeys,

    #[error("embedding result for `{key}` has dimension {actual}; expected {expected}")]
    EmbeddingDimensions {
        key: String,
        actual: usize,
        expected: usize,
    },

    #[error("embedding result for `{key}` contains non-finite values")]
    EmbeddingValues { key: String },

    #[error("rerank result item count does not match the requested work")]
    RerankItemCount,

    #[error("rerank result handles must contain exactly one score for every requested handle")]
    RerankHandles,

    #[error("rerank result for `{handle}` contains an invalid score")]
    RerankScore { handle: String },

    #[error("enrichment result contains more than the requested tag bound")]
    EnrichmentBounds,
}

/// Validate a provider result against the exact Rust-owned work contract.
///
/// Embedding and rerank ordering is provider-defined. The returned result is
/// reconstructed in request order only after key/handle coverage has been
/// validated.
pub fn validate_provider_result(
    work: &NeedWork,
    result: ProviderWorkResult,
) -> Result<ProviderWorkResult, ProviderResultValidationError> {
    if work_id(work) != result.work_id() {
        return Err(ProviderResultValidationError::WorkIdMismatch);
    }

    if matches!(&result, ProviderWorkResult::Failure { .. }) {
        return Ok(result);
    }

    match (work, result) {
        (NeedWork::Embeddings(request), ProviderWorkResult::Embeddings { work_id, vectors }) => {
            if request.items.len() != vectors.len() {
                return Err(ProviderResultValidationError::EmbeddingItemCount);
            }
            let expected = request
                .items
                .iter()
                .map(|item| item.key.as_str())
                .collect::<BTreeSet<_>>();
            let actual = vectors
                .iter()
                .map(|vector| vector.key.as_str())
                .collect::<BTreeSet<_>>();
            if expected.len() != vectors.len() || expected != actual {
                return Err(ProviderResultValidationError::EmbeddingKeys);
            }
            let mut by_key = vectors
                .into_iter()
                .map(|vector| (vector.key, vector.values))
                .collect::<std::collections::BTreeMap<_, _>>();
            let mut ordered = Vec::with_capacity(request.items.len());
            for item in &request.items {
                let values = by_key
                    .remove(&item.key)
                    .ok_or(ProviderResultValidationError::EmbeddingKeys)?;
                if values.len() != request.dimensions {
                    return Err(ProviderResultValidationError::EmbeddingDimensions {
                        key: item.key.clone(),
                        actual: values.len(),
                        expected: request.dimensions,
                    });
                }
                if values.iter().any(|value| !value.is_finite()) {
                    return Err(ProviderResultValidationError::EmbeddingValues {
                        key: item.key.clone(),
                    });
                }
                ordered.push(EmbeddingVector {
                    key: item.key.clone(),
                    values,
                });
            }
            Ok(ProviderWorkResult::Embeddings {
                work_id,
                vectors: ordered,
            })
        }
        (NeedWork::Rerank(request), ProviderWorkResult::Rerank { work_id, scores }) => {
            if request.candidates.len() != scores.len() {
                return Err(ProviderResultValidationError::RerankItemCount);
            }
            let expected = request
                .candidates
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            let actual = scores
                .iter()
                .map(|score| score.handle.as_str())
                .collect::<BTreeSet<_>>();
            if expected.len() != scores.len() || expected != actual {
                return Err(ProviderResultValidationError::RerankHandles);
            }
            if scores
                .iter()
                .any(|score| !score.score.is_finite() || !(0.0..=1.0).contains(&score.score))
            {
                let handle = scores
                    .iter()
                    .find(|score| !score.score.is_finite() || !(0.0..=1.0).contains(&score.score))
                    .map_or_else(String::new, |score| score.handle.clone());
                return Err(ProviderResultValidationError::RerankScore { handle });
            }
            Ok(ProviderWorkResult::Rerank { work_id, scores })
        }
        (NeedWork::Enrichment(request), ProviderWorkResult::Enrichment { work_id, tags }) => {
            if tags.len() > request.projection.max_tags() {
                return Err(ProviderResultValidationError::EnrichmentBounds);
            }
            Ok(ProviderWorkResult::Enrichment { work_id, tags })
        }
        _ => Err(ProviderResultValidationError::VariantMismatch),
    }
}

fn work_id(work: &NeedWork) -> &str {
    match work {
        NeedWork::Embeddings(request) => &request.work_id,
        NeedWork::Rerank(request) => &request.work_id,
        NeedWork::Enrichment(request) => &request.work_id,
    }
}
