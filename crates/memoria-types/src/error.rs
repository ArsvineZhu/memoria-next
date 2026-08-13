use std::path::PathBuf;

use crate::{MemoryId, RevisionId};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MemoriaError {
    #[error("randomness provider failed: {0}")]
    Randomness(#[source] getrandom::Error),

    #[error("invalid {kind} `{value}`; expected prefix `{expected_prefix}`")]
    InvalidId {
        kind: &'static str,
        expected_prefix: &'static str,
        value: String,
    },

    #[error("invalid source blob hash `{value}`")]
    InvalidSourceBlobHash { value: String },

    #[error("invalid timestamp `{value}`")]
    InvalidTimestamp { value: String },

    #[error("invalid {kind} `{value}`")]
    InvalidGeneration { kind: &'static str, value: String },

    #[error("store is locked: {path}")]
    StoreLocked { path: PathBuf },

    #[error("HEAD_CONFLICT: memory {memory_id} expected {expected_head}, actual {actual_head}")]
    HeadConflict {
        memory_id: MemoryId,
        expected_head: RevisionId,
        actual_head: RevisionId,
    },

    #[error("{kind} not found: {id}")]
    NotFound { kind: &'static str, id: String },

    #[error("space is retired: {space_id}")]
    SpaceRetired { space_id: crate::SpaceId },

    #[error("memory is retired: {memory_id}")]
    MemoryRetired { memory_id: MemoryId },

    #[error("KEY_CONFLICT: {scope} key `{key}` is already in use")]
    KeyConflict { scope: &'static str, key: String },

    #[error("authority database error: {message}")]
    Database { message: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(String),
}

impl MemoriaError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Randomness(_) => "RANDOMNESS_FAILED",
            Self::InvalidId { .. } => "INVALID_ID",
            Self::InvalidSourceBlobHash { .. } => "INVALID_SOURCE_BLOB_HASH",
            Self::InvalidTimestamp { .. } => "INVALID_TIMESTAMP",
            Self::InvalidGeneration { .. } => "INVALID_GENERATION",
            Self::StoreLocked { .. } => "STORE_LOCKED",
            Self::HeadConflict { .. } => "HEAD_CONFLICT",
            Self::NotFound { .. } => "NOT_FOUND",
            Self::SpaceRetired { .. } => "SPACE_RETIRED",
            Self::MemoryRetired { .. } => "MEMORY_RETIRED",
            Self::KeyConflict { .. } => "KEY_CONFLICT",
            Self::Database { .. } => "DATABASE_ERROR",
            Self::Io(_) => "IO_ERROR",
            Self::Serialization(_) => "SERIALIZATION_ERROR",
        }
    }
}
