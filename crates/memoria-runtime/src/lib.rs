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

pub use backup::{BackupManifest, inspect_backup};
pub use diagnostics::{RedactedDiagnostic, redact_text};
pub use discovery::{EntityObservation, discover_scoped};
pub use engine::{MemoriaRuntime, MemoryMutation, RuntimeError};
pub use limits::{ResourceLimitError, ResourceLimits, check_source_bytes};
pub use memoria_query::{MemoryQuery, RetrievalResponse};
pub use privacy::{ProviderCapability, ProviderEgressPolicy};
pub use provider::{
    EmbeddingBatchRequest, EmbeddingItem, EnrichmentBatchRequest, NeedWork, ProviderWorkResult,
    RerankBatchRequest, RerankScore,
};
pub use purge::{PurgeCoordinator, PurgePlan, PurgeState, PurgeTransitionError};
pub use query_operation::{QueryStep, QueryWork};
pub use receipt::{
    FeedbackCommit, FeedbackSubmission, FeedbackSubmissionEvent, ReceiptError, RetrievalReceipt,
};
pub use status::RuntimeStatus;
pub use transfer::{PortableMemory, ReferenceRewrite, rewrite_reference_ids};
