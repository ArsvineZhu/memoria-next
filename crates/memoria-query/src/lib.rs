mod compile;
mod evidence;
mod exact;
mod fusion;
mod lexical;
mod model;
mod planner;
mod snapshot;
mod validate;

pub use compile::{CompiledQuery, QueryCompiler};
pub use evidence::{
    CandidateEvidence, CandidateResponse, CandidateTarget, ExactEvidence, HistoryEvidence,
    LexicalEvidence, PropagationEvidence, RelationEvidence, SemanticEvidence, TagEvidence,
};
pub use exact::{
    ExactIndex, ExactRecord, MemoryReference, ReferenceStatus, ResolvedReference, execute_exact,
};
pub use fusion::{FusedCandidate, fuse_channels, rrf};
pub use lexical::{LexicalCandidate, LexicalCandidateIndex, execute_lexical};
pub use model::{
    AuthorityConsistency, EntityRef, EntityRefParseError, MemoryQuery, MemoryQueryBuilder,
    QueryBudget, QueryConsistency, QueryConstraints, QueryCue, QueryQuality, QueryScope,
    ReadinessBehavior,
};
pub use planner::{CapabilityExecution, CapabilityPlanner, CapabilityTarget};
pub use snapshot::{AdaptiveSnapshotIdentity, QuerySnapshot};
pub use validate::QueryError;
