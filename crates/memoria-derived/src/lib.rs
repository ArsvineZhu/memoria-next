mod artifact;
mod catalog;
mod manifest;

use std::num::TryFromIntError;

use thiserror::Error;

pub use artifact::{ArtifactDescriptor, ArtifactId, ArtifactState};
pub use catalog::DerivedCatalog;
pub use manifest::{DerivedManifest, ManifestId};
pub use memoria_types::AuthorityGeneration;

#[derive(Debug, Error)]
pub enum DerivedError {
    #[error("derived catalog I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("derived catalog SQLite error: {0}")]
    Sql(#[from] rusqlite::Error),

    #[error("artifact {id} is not validated (state: {state:?})")]
    ArtifactNotValidated {
        id: ArtifactId,
        state: ArtifactState,
    },

    #[error("artifact {id} was not found")]
    ArtifactNotFound { id: ArtifactId },

    #[error("manifest {id} is immutable")]
    ManifestImmutable { id: ManifestId },

    #[error("manifest {id} was not found")]
    ManifestNotFound { id: ManifestId },

    #[error("invalid artifact state in catalog: {value}")]
    InvalidCatalogState { value: String },

    #[error("invalid artifact state transition for {id}: {state:?}")]
    InvalidArtifactTransition {
        id: ArtifactId,
        state: ArtifactState,
    },

    #[error("authority generation cannot be represented as SQLite integer: {value}")]
    InvalidGeneration { value: i64 },

    #[error("integer conversion failed: {0}")]
    IntegerConversion(#[from] TryFromIntError),
}

fn generation_to_sql(generation: AuthorityGeneration) -> Result<i64, DerivedError> {
    i64::try_from(generation.value()).map_err(DerivedError::IntegerConversion)
}
