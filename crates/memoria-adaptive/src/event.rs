use std::{fmt, str::FromStr};

use memoria_types::{AdaptiveGeneration, MemoryId, RevisionId, SpaceId, Timestamp};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QueryAdaptiveSignature {
    pub scope: Vec<SpaceId>,
    pub entity_refs: Vec<String>,
    pub explicit_tags: Vec<String>,
    pub query_class: Option<String>,
    pub text_projection_hash: Option<String>,
    pub vector_identity: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackOutcome {
    Used,
    Rejected,
    CorrectForQuery,
    IncorrectForQuery,
    PreferredOver,
    Sufficient,
    Insufficient,
}

impl fmt::Display for FeedbackOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Used => "used",
            Self::Rejected => "rejected",
            Self::CorrectForQuery => "correct_for_query",
            Self::IncorrectForQuery => "incorrect_for_query",
            Self::PreferredOver => "preferred_over",
            Self::Sufficient => "sufficient",
            Self::Insufficient => "insufficient",
        };
        formatter.write_str(value)
    }
}

impl FromStr for FeedbackOutcome {
    type Err = AdaptiveError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "used" => Ok(Self::Used),
            "rejected" => Ok(Self::Rejected),
            "correct_for_query" => Ok(Self::CorrectForQuery),
            "incorrect_for_query" => Ok(Self::IncorrectForQuery),
            "preferred_over" => Ok(Self::PreferredOver),
            "sufficient" => Ok(Self::Sufficient),
            "insufficient" => Ok(Self::Insufficient),
            value => Err(AdaptiveError::InvalidFeedbackOutcome {
                value: value.to_owned(),
            }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FeedbackEventInput {
    pub event_id: String,
    pub retrieval_id: String,
    pub space_id: SpaceId,
    pub memory_id: MemoryId,
    pub revision_id: RevisionId,
    pub semantic_node_id: Option<String>,
    pub query_signature: QueryAdaptiveSignature,
    pub outcome: FeedbackOutcome,
    pub occurred_at: Timestamp,
    pub idempotency_key: String,
}

impl FeedbackEventInput {
    pub(crate) fn validate(&self) -> Result<(), AdaptiveError> {
        if self.event_id.trim().is_empty() {
            return Err(AdaptiveError::InvalidEvent { field: "event_id" });
        }
        if self.retrieval_id.trim().is_empty() {
            return Err(AdaptiveError::InvalidEvent {
                field: "retrieval_id",
            });
        }
        if self.idempotency_key.trim().is_empty() {
            return Err(AdaptiveError::InvalidEvent {
                field: "idempotency_key",
            });
        }
        if self
            .query_signature
            .entity_refs
            .iter()
            .chain(self.query_signature.explicit_tags.iter())
            .any(|value| value.trim().is_empty())
        {
            return Err(AdaptiveError::InvalidEvent {
                field: "query_signature",
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveEvent {
    pub event_id: String,
    pub generation: AdaptiveGeneration,
    pub retrieval_id: String,
    pub space_id: SpaceId,
    pub memory_id: MemoryId,
    pub revision_id: RevisionId,
    pub semantic_node_id: Option<String>,
    pub query_signature: QueryAdaptiveSignature,
    pub outcome: FeedbackOutcome,
    pub occurred_at: Timestamp,
    pub idempotency_key: String,
}

impl AdaptiveEvent {
    pub(crate) fn from_input(input: FeedbackEventInput, generation: AdaptiveGeneration) -> Self {
        Self {
            event_id: input.event_id,
            generation,
            retrieval_id: input.retrieval_id,
            space_id: input.space_id,
            memory_id: input.memory_id,
            revision_id: input.revision_id,
            semantic_node_id: input.semantic_node_id,
            query_signature: input.query_signature,
            outcome: input.outcome,
            occurred_at: input.occurred_at,
            idempotency_key: input.idempotency_key,
        }
    }

    pub(crate) fn as_input(&self) -> FeedbackEventInput {
        FeedbackEventInput {
            event_id: self.event_id.clone(),
            retrieval_id: self.retrieval_id.clone(),
            space_id: self.space_id,
            memory_id: self.memory_id,
            revision_id: self.revision_id,
            semantic_node_id: self.semantic_node_id.clone(),
            query_signature: self.query_signature.clone(),
            outcome: self.outcome.clone(),
            occurred_at: self.occurred_at,
            idempotency_key: self.idempotency_key.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AdaptiveError {
    #[error("invalid adaptive event field `{field}`")]
    InvalidEvent { field: &'static str },

    #[error("invalid feedback outcome `{value}`")]
    InvalidFeedbackOutcome { value: String },

    #[error("idempotency key `{key}` conflicts with an existing feedback event")]
    IdempotencyConflict { key: String },

    #[error("event id `{event_id}` conflicts with an existing feedback event")]
    EventIdConflict { event_id: String },

    #[error("adaptive generation is exhausted")]
    GenerationExhausted,
}
