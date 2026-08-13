mod artifact;
mod catalog;
mod compiler;
mod dependency;
mod manifest;
mod projection;

use std::num::TryFromIntError;

use thiserror::Error;

pub use artifact::{ArtifactDescriptor, ArtifactId, ArtifactState};
pub use catalog::DerivedCatalog;
pub use compiler::{BASE_ARTIFACT_KINDS, BaseReadyReport, DerivedCompiler};
pub use dependency::{InvalidationPlan, ProjectionInputHash, ProjectionKind};
pub use manifest::{CapabilityStatus, DerivedManifest, ManifestId};
pub use memoria_types::AuthorityGeneration;
pub use projection::PROJECTION_SCHEMA_VERSION;
pub use projection::ProjectionTarget;
pub use projection::entities::{
    EntityObservation, EntityObservationArtifact, EntityObservationBuilder, EntityRef,
};
pub use projection::lexical::{
    LexicalDocument, LexicalHit, LexicalIndex, build_lexical, lexical_projection_hash,
};
pub use projection::relations::{RelationArtifact, RelationBuilder, RelationRecord};
pub use projection::structural::{StructuralArtifact, StructuralBuilder, StructuralEntry};
pub use projection::tags::{ExplicitTagArtifact, ExplicitTagBuilder, TagMembership, TagProvenance};
pub use projection::temporal::{TemporalArtifact, TemporalAssertion, TemporalBuilder};

#[derive(Debug, Error)]
pub enum DerivedError {
    #[error("derived catalog I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("derived catalog SQLite error: {0}")]
    Sql(#[from] rusqlite::Error),

    #[error("MDX projection error: {0}")]
    Mdx(#[from] memoria_mdx::MdxError),

    #[error("memoria type conversion error: {0}")]
    Types(#[from] memoria_types::MemoriaError),

    #[error("lexical index error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),

    #[error("lexical query error: {0}")]
    TantivyQuery(#[from] tantivy::query::QueryParserError),

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

    #[error(
        "artifact {id} belongs to authority generation {actual}, expected manifest generation {expected}"
    )]
    ArtifactGenerationMismatch {
        id: ArtifactId,
        expected: AuthorityGeneration,
        actual: AuthorityGeneration,
    },

    #[error("authority generation cannot be represented as SQLite integer: {value}")]
    InvalidGeneration { value: i64 },

    #[error("integer conversion failed: {0}")]
    IntegerConversion(#[from] TryFromIntError),

    #[error("invalid projection value: {value}")]
    InvalidProjectionValue { value: String },
}

fn generation_to_sql(generation: AuthorityGeneration) -> Result<i64, DerivedError> {
    i64::try_from(generation.value()).map_err(DerivedError::IntegerConversion)
}
