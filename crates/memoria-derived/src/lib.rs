mod artifact;
mod catalog;
mod compiler;
mod dependency;
mod embedding;
mod gc;
mod lease;
mod manifest;
mod projection;
mod scheduler;
mod status;
mod vector;

use std::num::TryFromIntError;

use thiserror::Error;

pub use artifact::{ArtifactDescriptor, ArtifactId, ArtifactState};
pub use catalog::DerivedCatalog;
pub use compiler::{BASE_ARTIFACT_KINDS, BaseReadyReport, DerivedCompiler};
pub use dependency::{InvalidationPlan, ProjectionInputHash, ProjectionKind};
pub use embedding::{
    EmbeddingCacheKey, EmbeddingNormalization, EmbeddingPayloadCache, EmbeddingResult,
    EmbeddingResultItem, EmbeddingSignature, EmbeddingVector, EmbeddingWork, EmbeddingWorkItem,
    default_content_signature, validate_embedding_result,
};
pub use gc::{DerivedGc, GcReport};
pub use lease::ManifestLease;
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
pub use scheduler::DerivedScheduler;
pub use status::DerivedStatus;
pub use vector::{
    VectorArtifact, VectorFilter, VectorHit, VectorIndex, VectorMembership, VectorSearch,
};

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

    #[error("vector index error: {value}")]
    VectorIndex { value: String },

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

    #[error("derived scheduler has stopped")]
    SchedulerStopped,

    #[error("derived scheduler state was poisoned")]
    SchedulerPoisoned,

    #[error(
        "capability `{capability}` did not reach authority generation {generation} before timeout"
    )]
    CapabilityTimeout {
        capability: String,
        generation: AuthorityGeneration,
    },

    #[error("lease duration cannot be represented or added: {seconds} seconds")]
    LeaseDuration { seconds: u64 },
}

fn generation_to_sql(generation: AuthorityGeneration) -> Result<i64, DerivedError> {
    i64::try_from(generation.value()).map_err(DerivedError::IntegerConversion)
}
