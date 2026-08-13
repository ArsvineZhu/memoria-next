mod assessment;
mod compile;
mod consolidate;
mod continuation;
mod evidence;
mod exact;
mod fusion;
mod history;
mod lexical;
mod model;
mod planner;
mod response;
mod semantic;
mod snapshot;
mod validate;

pub use assessment::{RecallAssessment, assess};
pub use compile::{CompiledQuery, QueryCompiler};
pub use consolidate::{
    ConsolidationCandidate, ConsolidationError, MemoryMatch, MemoryResult, consolidate,
};
pub use continuation::{Continuation, ReadSession, SessionError};
pub use evidence::{
    CandidateEvidence, CandidateResponse, CandidateTarget, ExactEvidence, HistoryEvidence,
    LexicalEvidence, PropagationEvidence, RelationEvidence, SemanticEvidence, TagEvidence,
};
pub use exact::{
    ExactIndex, ExactRecord, MemoryReference, ReferenceStatus, ResolvedReference, execute_exact,
};
pub use fusion::{FusedCandidate, fuse_channels, rrf};
pub use history::execute_history;
pub use lexical::{LexicalCandidate, LexicalCandidateIndex, execute_lexical};
pub use model::{
    AuthorityConsistency, EntityRef, EntityRefParseError, MemoryQuery, MemoryQueryBuilder,
    QueryBudget, QueryConsistency, QueryConstraints, QueryCue, QueryQuality, QueryScope,
    ReadinessBehavior,
};
pub use planner::{CapabilityExecution, CapabilityPlanner, CapabilityTarget};
pub use response::{RetrievalResponse, build_response};
pub use semantic::{
    SemanticCandidate, SemanticCandidateIndex, SemanticResolution, execute_semantic,
};
pub use snapshot::{AdaptiveSnapshotIdentity, QuerySnapshot};
pub use validate::QueryError;
