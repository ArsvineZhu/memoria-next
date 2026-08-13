mod compile;
mod evidence;
mod exact;
mod model;
mod planner;
mod snapshot;
mod validate;

pub use compile::{CompiledQuery, QueryCompiler};
pub use evidence::{
    CandidateEvidence, CandidateResponse, CandidateTarget, ExactEvidence, HistoryEvidence,
    LexicalEvidence, PropagationEvidence, RelationEvidence, SemanticEvidence, TagEvidence,
};
pub use exact::{ExactIndex, ExactRecord, execute_exact};
pub use model::{
    AuthorityConsistency, EntityRef, EntityRefParseError, MemoryQuery, MemoryQueryBuilder,
    QueryBudget, QueryConsistency, QueryConstraints, QueryCue, QueryQuality, QueryScope,
    ReadinessBehavior,
};
pub use planner::{CapabilityExecution, CapabilityPlanner, CapabilityTarget};
pub use snapshot::{AdaptiveSnapshotIdentity, QuerySnapshot};
pub use validate::QueryError;
