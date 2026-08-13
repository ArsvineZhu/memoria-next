use std::path::PathBuf;

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

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(String),
}
