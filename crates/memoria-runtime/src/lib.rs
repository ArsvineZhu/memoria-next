mod backup;
mod diagnostics;
mod discovery;
mod engine;
mod limits;
mod privacy;
mod provider;
mod purge;
mod query_operation;
mod receipt;
mod status;
mod transfer;

pub use backup::{BackupError, BackupManifest, create_backup, inspect_backup, restore_backup};
pub use diagnostics::{RedactedDiagnostic, redact_text};
pub use discovery::{EntityObservation, discover_scoped};
pub use engine::{MemoriaRuntime, MemoryMutation, RuntimeError};
pub use limits::{ResourceLimitError, ResourceLimits, check_source_bytes};
pub use memoria_authority::{
    PortableImportMapping, PortableImportResult, SpaceProviderMode, SpaceProviderPolicy,
};
pub use memoria_query::{MemoryQuery, RetrievalResponse};
pub use privacy::{
    ProviderCapability, ProviderEgressPolicy, ProviderRoute, ProviderRouteConfig,
    ProviderRouteDecision, ProviderTrust, allows_space_provider_mode, allows_space_provider_policy,
    resolve_provider_route, space_provider_mode,
};
pub use provider::{
    EmbeddingBatchRequest, EmbeddingItem, EmbeddingVector, EnrichmentBatchRequest, NeedWork,
    ProviderResultValidationError, ProviderWorkResult, RerankBatchRequest, RerankScore,
    validate_provider_result,
};
pub use purge::{PurgeCoordinator, PurgePlan, PurgeState, PurgeTransitionError};
pub use query_operation::{QueryOperationStage, QueryStep, QueryWork, READINESS_RETRY_AFTER_MS};
pub use receipt::{
    FeedbackCommit, FeedbackSubmission, FeedbackSubmissionEvent, ReceiptError, RetrievalReceipt,
};
pub use status::RuntimeStatus;
pub use transfer::{
    PortableImportRequest, PortableMemory, ReferenceRewrite, SourceReferenceRewrite,
    rewrite_memory_ref_source, rewrite_reference_ids,
};

pub fn restore_store_backup(
    backup_dir: impl AsRef<std::path::Path>,
    target_dir: impl AsRef<std::path::Path>,
) -> Result<BackupManifest, RuntimeError> {
    Ok(restore_backup(backup_dir, target_dir)?)
}
