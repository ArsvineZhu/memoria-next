use std::collections::BTreeMap;

use memoria_adaptive::{
    AdaptiveError, AdaptiveEvent, FeedbackEventInput, FeedbackOutcome, QueryAdaptiveSignature,
};
use memoria_query::{MemoryResult, QuerySnapshot, result_id_for};
use memoria_types::{AdaptiveGeneration, MemoryId, RevisionId, SpaceId, Timestamp};
use thiserror::Error;

const RECEIPT_TTL_SECONDS: i64 = 15 * 60;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeedbackSubmission {
    pub retrieval_id: String,
    pub idempotency_key: String,
    pub events: Vec<FeedbackSubmissionEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeedbackSubmissionEvent {
    pub result_id: String,
    pub outcome: FeedbackOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeedbackCommit {
    pub generation: AdaptiveGeneration,
    pub events: Vec<AdaptiveEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetrievalReceipt {
    pub retrieval_id: String,
    pub snapshot: QuerySnapshot,
    pub query_signature: QueryAdaptiveSignature,
    pub expires_at: Timestamp,
    results: BTreeMap<String, PinnedEvidence>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PinnedEvidence {
    space_id: SpaceId,
    memory_id: MemoryId,
    revision_id: RevisionId,
    semantic_node_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum ReceiptError {
    #[error("retrieval receipt `{retrieval_id}` was not found")]
    NotFound { retrieval_id: String },

    #[error("retrieval receipt `{retrieval_id}` has expired")]
    Expired { retrieval_id: String },

    #[error("feedback submission field `{field}` is invalid")]
    InvalidSubmission { field: &'static str },

    #[error("feedback result `{result_id}` is not present in retrieval `{retrieval_id}`")]
    ResultNotFound {
        retrieval_id: String,
        result_id: String,
    },

    #[error("feedback result `{result_id}` was submitted more than once")]
    DuplicateResult { result_id: String },

    #[error("adaptive error: {0}")]
    Adaptive(#[from] AdaptiveError),

    #[error("receipt expiration timestamp overflowed")]
    ExpirationOverflow,
}

impl RetrievalReceipt {
    pub fn from_results(
        retrieval_id: impl Into<String>,
        snapshot: QuerySnapshot,
        query_signature: QueryAdaptiveSignature,
        results: &[MemoryResult],
        now: Timestamp,
    ) -> Result<Self, ReceiptError> {
        let retrieval_id = retrieval_id.into();
        if retrieval_id.trim().is_empty() {
            return Err(ReceiptError::InvalidSubmission {
                field: "retrieval_id",
            });
        }
        let expires_at = Timestamp::from_unix_seconds(
            now.unix_seconds()
                .checked_add(RECEIPT_TTL_SECONDS)
                .ok_or(ReceiptError::ExpirationOverflow)?,
        )
        .map_err(|_| ReceiptError::ExpirationOverflow)?;
        let mut pinned = BTreeMap::new();
        for (index, result) in results.iter().enumerate() {
            let result_id = result_id_for(&retrieval_id, index);
            pinned.insert(
                result_id,
                PinnedEvidence {
                    space_id: result.space_id,
                    memory_id: result.memory_id,
                    revision_id: result.revision_id,
                    semantic_node_id: None,
                },
            );
        }
        Ok(Self {
            retrieval_id,
            snapshot,
            query_signature,
            expires_at,
            results: pinned,
        })
    }

    pub fn resolve_feedback(
        &self,
        submission: &FeedbackSubmission,
        now: Timestamp,
    ) -> Result<Vec<FeedbackEventInput>, ReceiptError> {
        if submission.retrieval_id != self.retrieval_id {
            return Err(ReceiptError::NotFound {
                retrieval_id: submission.retrieval_id.clone(),
            });
        }
        if now > self.expires_at {
            return Err(ReceiptError::Expired {
                retrieval_id: self.retrieval_id.clone(),
            });
        }
        if submission.idempotency_key.trim().is_empty() {
            return Err(ReceiptError::InvalidSubmission {
                field: "idempotency_key",
            });
        }
        if submission.events.is_empty() {
            return Err(ReceiptError::InvalidSubmission { field: "events" });
        }

        let mut seen = std::collections::BTreeSet::new();
        let mut resolved = Vec::with_capacity(submission.events.len());
        for item in &submission.events {
            if !seen.insert(item.result_id.as_str()) {
                return Err(ReceiptError::DuplicateResult {
                    result_id: item.result_id.clone(),
                });
            }
            let evidence =
                self.results
                    .get(&item.result_id)
                    .ok_or_else(|| ReceiptError::ResultNotFound {
                        retrieval_id: self.retrieval_id.clone(),
                        result_id: item.result_id.clone(),
                    })?;
            let event_key = format!("{}:{}", submission.idempotency_key, item.result_id);
            resolved.push(FeedbackEventInput {
                event_id: format!("FE_{event_key}"),
                retrieval_id: self.retrieval_id.clone(),
                space_id: evidence.space_id,
                memory_id: evidence.memory_id,
                revision_id: evidence.revision_id,
                semantic_node_id: evidence.semantic_node_id.clone(),
                query_signature: self.query_signature.clone(),
                outcome: item.outcome.clone(),
                occurred_at: now,
                idempotency_key: event_key,
            });
        }
        Ok(resolved)
    }
}
