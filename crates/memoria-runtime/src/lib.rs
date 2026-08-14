mod backup;
mod discovery;
mod engine;
mod privacy;
mod provider;
mod receipt;
mod status;

pub use discovery::{EntityObservation, discover_scoped};
pub use backup::{BackupManifest, inspect_backup};
pub use engine::{MemoriaRuntime, MemoryMutation, RuntimeError};
pub use memoria_query::{MemoryQuery, RetrievalResponse};
pub use privacy::{ProviderCapability, ProviderEgressPolicy};
pub use provider::{
    EmbeddingBatchRequest, EmbeddingItem, EnrichmentBatchRequest, NeedWork, ProviderWorkResult,
    RerankBatchRequest, RerankScore,
};
pub use receipt::{
    FeedbackCommit, FeedbackSubmission, FeedbackSubmissionEvent, ReceiptError, RetrievalReceipt,
};
pub use status::RuntimeStatus;
