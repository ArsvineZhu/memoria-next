mod adaptive;
pub mod algorithms;
mod assessment;
mod association;
mod compile;
mod consolidate;
mod continuation;
mod evidence;
mod exact;
mod executor;
mod fusion;
mod history;
mod lexical;
mod lexical_operator;
mod model;
mod planner;
mod relation_expand;
mod rerank;
mod response;
mod semantic;
mod snapshot;
mod tags;
mod trace;
mod validate;

pub use adaptive::{MAX_ADAPTIVE_PRIOR, adaptive_prior, rank_with_adaptive};
pub use algorithms::{
    BALANCED_TAG_BASIS_VECTORS, DiffusionTrace, FAST_TAG_BASIS_VECTORS, MAX_TAG_BASIS_DIMENSIONS,
    MAX_TAG_BASIS_VECTORS, PropagatedTag, PropagationBudget, PropagationError, PropagationTrace,
    StructureEvidence, SupportEvidence, THOROUGH_TAG_BASIS_VECTORS, TagBasisError, TagBasisResult,
    activation_propagate, collect_structure, collect_support, correlated_resolutions,
    diffusion_propagate, l2_norm, project_tag_basis, project_tag_basis_with_limit, structure_bonus,
    support_bonus,
};
pub use assessment::{RecallAssessment, RetrievalCost, assess, effort_from_cost};
pub use association::{
    AssociationEdge, AssociationError, AssociationGraph, AssociationView, MAX_ASSOCIATION_EDGES,
};
pub use compile::{CompiledQuery, QueryCompiler};
pub use consolidate::{
    ConsolidationCandidate, ConsolidationError, MemoryMatch, MemoryResult, consolidate,
};
pub use continuation::{Continuation, ReadSession, SessionError};
pub use evidence::{
    CandidateEvidence, CandidateResponse, CandidateTarget, ExactEvidence, HistoryEvidence,
    LexicalEvidence, PropagationEvidence, RelationEvidence, SemanticChannel, SemanticEvidence,
    TagEvidence,
};
pub use exact::{
    ExactIndex, ExactRecord, MemoryReference, ReferenceStatus, ResolvedReference, execute_exact,
};
pub use executor::{
    AdaptivePolicy, AlgorithmChannelInputs, CandidateKey, CandidatePool, CompiledConstraints,
    DerivedUnitId, PhysicalChannel, PhysicalQueryPlan, PhysicalQueryPlanner, RerankPolicy,
    SemanticResidualOperator, execute_algorithm_channels,
};
pub use fusion::{
    FusedCandidate, RRF_K, fuse_candidate_pool, fuse_channels, fuse_channels_scoped,
    fuse_channels_with_adaptive, rrf,
};
pub use history::execute_history;
pub use lexical::{LexicalCandidate, LexicalCandidateIndex, execute_lexical};
pub use lexical_operator::LexicalOperator;
pub use model::{
    AuthorityConsistency, EntityRef, EntityRefParseError, MemoryQuery, MemoryQueryBuilder,
    QueryBudget, QueryConsistency, QueryConstraints, QueryCue, QueryHistory, QueryHistoryMode,
    QueryLifecycle, QueryQuality, QueryQualityLevel, QueryScope, ReadinessBehavior,
};
pub use planner::RetrievalProfile;
pub use planner::{CapabilityExecution, CapabilityPlanner, CapabilityTarget};
pub use relation_expand::{
    RelationExpansionBudget, RelationExpansionError, RelationLink, expand_relations, relation_bonus,
};
pub use rerank::{
    MAX_RERANK_CANDIDATES, RerankBatch, RerankError, RerankScore, RerankView, apply_rerank,
    build_rerank_batch, rerank_handle, rerank_limit_for_quality,
};
pub use response::{RetrievalResponse, build_response, result_id_for};
pub use semantic::{
    SemanticCandidate, SemanticCandidateIndex, SemanticQueryChannels, SemanticResolution,
    build_semantic_query_channels, execute_semantic,
};
pub use snapshot::{AdaptiveSnapshotIdentity, QuerySnapshot};
pub use tags::{
    CompositeTagAssociation, CompositeTagView, TagReadoutCandidate, TagSeed, TagSeedProvenance,
    TagVectorCandidate, readout_tag_candidates, resolve_explicit_tag_seeds,
    select_tag_basis_candidates,
};
pub use trace::QueryOperatorTrace;
pub use validate::QueryError;
