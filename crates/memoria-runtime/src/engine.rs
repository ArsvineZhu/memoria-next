use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::string::FromUtf8Error;

use memoria_adaptive::{
    AdaptiveEventLog, AdaptiveReadSnapshot, AdaptiveStateV1, QueryAdaptiveSignature,
};
use memoria_authority::{
    AuthorityDb, ImportMemoryAllocation, MemoryLifecycle, PortableImportAllocation,
    PortableImportCommit, PortableImportResult, PurgeOperationRecord, SourceCas, SpaceProviderMode,
    SpaceProviderPolicy, StoreLayout, StoreWriterLock,
};
use memoria_derived::{
    AnnSegmentEntry, AnnSegmentV1, BaseReadyReport, BuildJobState, DerivedCatalog, DerivedCompiler,
    EmbeddingBuildIdentity, EmbeddingNormalization, EnrichmentProjection, EntityObservationBuilder,
    ExplicitTagBuilder, GeneratedTagArtifact, LatestMemoryState, LexicalArtifactHandle,
    LexicalDocument, LocalEmbeddingProjectionV1, ManifestId, ProjectionInputHash, ProjectionKind,
    QueryEmbeddingProjectionV1, SEMANTIC_ARTIFACT_KIND, SEMANTIC_ARTIFACT_VERSION,
    SemanticPublicationDecision, TagDictionary, TagGraph, TagId, TagMembershipInput, TagProvenance,
    VectorFilter, VectorMembership, VectorPayloadRecord, VectorPayloadV1,
    semantic_publication_target,
};
use memoria_mdx::{SemanticDiff, compile_ir};
use memoria_query::{
    AdaptiveSnapshotIdentity, AlgorithmChannelInputs, CandidateEvidence, CandidatePool, ExactIndex,
    ExactRecord, LexicalCandidate, LexicalCandidateIndex, LexicalOperator, MemoryQuery,
    PhysicalChannel, PhysicalQueryPlanner, QueryCompiler, QueryError, QueryOperatorTrace,
    ReadSession, ReadinessBehavior, RelationLink, RerankBatch, RerankScore as QueryRerankScore,
    RetrievalResponse, SemanticCandidateIndex, SemanticChannel, SemanticResidualOperator,
    SemanticResolution, TagSeedProvenance, TagVectorCandidate, apply_rerank, assess,
    build_rerank_batch, build_response, execute_algorithm_channels, execute_exact, execute_lexical,
    execute_semantic, fuse_candidate_pool, rank_with_adaptive, rerank_limit_for_quality,
    resolve_explicit_tag_seeds,
};
use memoria_types::{AuthorityGeneration, MemoriaError, MemoryId, RevisionId, SpaceId};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::backup;
use crate::limits::{ResourceLimits, check_source_bytes};
use crate::privacy::{
    ProviderCapability, ProviderEgressPolicy, ProviderRouteConfig, ProviderRouteDecision,
    resolve_provider_route,
};
use crate::provider::{
    EmbeddingBatchRequest, EmbeddingItem, NeedWork, ProviderWorkResult, RerankBatchRequest,
    validate_provider_result,
};
use crate::purge::{PurgeCoordinator, PurgePlan, PurgeState};
use crate::query_operation::{QueryOperationStage, QueryOperationTable, QueryStep, QueryWork};
use crate::receipt::{FeedbackCommit, FeedbackSubmission, ReceiptError, RetrievalReceipt};
use crate::status::RuntimeStatus;
use crate::transfer::{PortableImportRequest, PortableMemory, rewrite_memory_ref_source};

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("runtime is closed")]
    Closed,

    #[error("authority error: {0}")]
    Authority(#[from] MemoriaError),

    #[error("authority database error: {message}")]
    AuthorityDatabase { message: String },

    #[error("Derived error: {0}")]
    Derived(#[from] memoria_derived::DerivedError),

    #[error("MDX error: {0}")]
    Mdx(#[from] memoria_mdx::MdxError),

    #[error("query error: {0}")]
    Query(#[from] memoria_query::QueryError),

    #[error("consolidation error: {0}")]
    Consolidation(#[from] memoria_query::ConsolidationError),

    #[error("rerank error: {0}")]
    Rerank(#[from] memoria_query::RerankError),

    #[error("source is not valid UTF-8: {0}")]
    Utf8(#[from] FromUtf8Error),

    #[error("provider result `{work_id}` was not expected")]
    UnexpectedProviderWork { work_id: String },

    #[error("query provider result `{work_id}` was not expected")]
    UnexpectedQueryWork { work_id: String },

    #[error("query operation `{operation_id}` was not found")]
    QueryOperationNotFound { operation_id: String },

    #[error("query operation `{operation_id}` was cancelled")]
    QueryOperationCancelled { operation_id: String },

    #[error("query operation `{operation_id}` expired")]
    QueryOperationExpired { operation_id: String },

    #[error("query provider rejected work `{work_id}`")]
    QueryProviderRejected { work_id: String },

    #[error("provider result `{work_id}` is invalid: {message}")]
    InvalidProviderResult { work_id: String, message: String },

    #[error("provider failure for `{work_id}` ({code}, retryable={retryable}): {message}")]
    ProviderFailure {
        work_id: String,
        retryable: bool,
        code: String,
        message: String,
    },

    #[error("query operation `{operation_id}` is pending provider work")]
    QueryOperationPending { operation_id: String },

    #[error("adaptive error: {0}")]
    Adaptive(#[from] memoria_adaptive::AdaptiveError),

    #[error("adaptive snapshot is unavailable for the compiled query")]
    AdaptiveSnapshotUnavailable,

    #[error("CAPABILITY_NOT_READY: provider data egress denied for {capability}")]
    ProviderEgressDenied { capability: ProviderCapability },

    #[error("PROVIDER_POLICY_DENIED: provider {capability} is denied by the scoped Space policy")]
    ProviderPolicyDenied { capability: ProviderCapability },

    #[error("PURGE_CONFLICT: purge plan is unavailable or not resumable: {plan_id}")]
    PurgeConflict { plan_id: String },

    #[error("RESOURCE_LIMIT: {resource} size {actual} exceeds configured maximum {maximum}")]
    ResourceLimit {
        resource: &'static str,
        actual: usize,
        maximum: usize,
    },

    #[error("feedback receipt error: {0}")]
    Receipt(#[from] ReceiptError),

    #[error("runtime I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("backup error: {0}")]
    Backup(#[from] backup::BackupError),
}

impl RuntimeError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Closed => "STORE_CLOSED",
            Self::Authority(error) => error.code(),
            Self::AuthorityDatabase { .. } | Self::Derived(_) => "STORE_CORRUPT",
            Self::Mdx(_) | Self::Utf8(_) => "INVALID_MDX",
            Self::Query(_) | Self::Consolidation(_) | Self::Rerank(_) => "QUERY_ERROR",
            Self::UnexpectedProviderWork { .. } => "PROVIDER_UNAVAILABLE",
            Self::UnexpectedQueryWork { .. } | Self::QueryProviderRejected { .. } => {
                "PROVIDER_UNAVAILABLE"
            }
            Self::InvalidProviderResult { .. } | Self::ProviderFailure { .. } => {
                "PROVIDER_UNAVAILABLE"
            }
            Self::QueryOperationNotFound { .. } => "SNAPSHOT_UNAVAILABLE",
            Self::QueryOperationCancelled { .. } => "ABORTED",
            Self::QueryOperationExpired { .. } => "CONTINUATION_EXPIRED",
            Self::QueryOperationPending { .. } => "QUERY_ERROR",
            Self::Adaptive(_) => "ADAPTIVE_ERROR",
            Self::AdaptiveSnapshotUnavailable => "ADAPTIVE_ERROR",
            Self::ProviderEgressDenied { .. } => "CAPABILITY_NOT_READY",
            Self::ProviderPolicyDenied { .. } => "PROVIDER_POLICY_DENIED",
            Self::PurgeConflict { .. } => "PURGE_CONFLICT",
            Self::ResourceLimit { .. } => "RESOURCE_LIMIT",
            Self::Receipt(ReceiptError::NotFound { .. }) => "NOT_FOUND",
            Self::Receipt(ReceiptError::Expired { .. }) => "FEEDBACK_RECEIPT_EXPIRED",
            Self::Receipt(_) => "ADAPTIVE_ERROR",
            Self::Backup(_) => "STORE_CORRUPT",
            Self::Io(_) => "STORE_CORRUPT",
        }
    }
}

pub struct MemoriaRuntime {
    _writer_lock: StoreWriterLock,
    layout: StoreLayout,
    authority: AuthorityDb,
    cas: SourceCas,
    derived: DerivedCatalog,
    lexical_handles: BTreeMap<ManifestId, LexicalArtifactHandle>,
    compiler: DerivedCompiler,
    pending_provider_work: VecDeque<PendingProviderWork>,
    inflight_provider_work: BTreeMap<String, InflightProviderWork>,
    pending_by_generation: BTreeMap<AuthorityGeneration, usize>,
    query_operations: QueryOperationTable,
    semantic_build_coverage: AuthorityGeneration,
    tag_dictionary: TagDictionary,
    generated_tag_artifacts: BTreeMap<ProjectionInputHash, GeneratedTagArtifact>,
    adaptive_log: AdaptiveEventLog,
    provider_egress_policy: ProviderEgressPolicy,
    provider_routes: ProviderRouteConfig,
    purge: PurgeCoordinator,
    resource_limits: ResourceLimits,
    receipts: BTreeMap<String, RetrievalReceipt>,
    next_retrieval_id: u64,
    closed: bool,
    last_error: Option<String>,
}

struct PendingProviderWork {
    generation: AuthorityGeneration,
    work: NeedWork,
    embedding_projection: Option<LocalEmbeddingProjectionV1>,
    job_id: Option<String>,
}

struct InflightProviderWork {
    generation: AuthorityGeneration,
    expected_work: NeedWork,
    advances_semantic: bool,
    embedding_projection: Option<LocalEmbeddingProjectionV1>,
    enrichment_projection: Option<EnrichmentProjection>,
    job_id: Option<String>,
}

struct ManifestLexicalOperator<'a> {
    handle: &'a LexicalArtifactHandle,
    exact: &'a ExactIndex,
}

impl LexicalOperator for ManifestLexicalOperator<'_> {
    fn search(
        &self,
        query: &str,
        scope: &[SpaceId],
        limit: usize,
    ) -> Result<Vec<LexicalCandidate>, QueryError> {
        let hits = self.handle.search(query, scope, limit).map_err(|error| {
            QueryError::OperatorFailure {
                message: error.to_string(),
            }
        })?;
        Ok(LexicalCandidateIndex::from_derived_hits(hits, self.exact)
            .candidates()
            .to_vec())
    }
}

struct RuntimeSemanticResidualOperator<'a> {
    runtime: &'a MemoriaRuntime,
    compiled: &'a memoria_query::CompiledQuery,
    exact: &'a ExactIndex,
}

impl SemanticResidualOperator for RuntimeSemanticResidualOperator<'_> {
    fn search(
        &self,
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<CandidateEvidence>, QueryError> {
        let hits = self
            .runtime
            .semantic_vector_hits(self.compiled, query_vector, self.exact, limit)
            .map_err(|error| QueryError::OperatorFailure {
                message: error.to_string(),
            })?;
        Ok(execute_semantic(
            self.compiled,
            &SemanticCandidateIndex::from_vector_hits_with_channel(
                hits,
                self.exact,
                SemanticResolution::Leaf,
                SemanticChannel::Residual,
            ),
        )
        .results)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryMutation {
    pub memory_id: MemoryId,
    pub space_id: SpaceId,
    pub revision_id: RevisionId,
    pub generation: AuthorityGeneration,
}

impl MemoriaRuntime {
    pub fn open(data_dir: impl AsRef<std::path::Path>) -> Result<Self, RuntimeError> {
        Self::open_with_provider_egress_policy_and_routes(
            data_dir,
            ProviderEgressPolicy::default(),
            ProviderRouteConfig::default(),
        )
    }

    pub fn open_with_provider_egress_policy(
        data_dir: impl AsRef<std::path::Path>,
        provider_egress_policy: ProviderEgressPolicy,
    ) -> Result<Self, RuntimeError> {
        Self::open_with_provider_egress_policy_and_routes(
            data_dir,
            provider_egress_policy,
            ProviderRouteConfig::default(),
        )
    }

    pub fn open_with_provider_routes(
        data_dir: impl AsRef<std::path::Path>,
        provider_routes: ProviderRouteConfig,
    ) -> Result<Self, RuntimeError> {
        Self::open_with_provider_egress_policy_and_routes(
            data_dir,
            ProviderEgressPolicy::default(),
            provider_routes,
        )
    }

    pub fn open_with_provider_egress_policy_and_routes(
        data_dir: impl AsRef<std::path::Path>,
        provider_egress_policy: ProviderEgressPolicy,
        provider_routes: ProviderRouteConfig,
    ) -> Result<Self, RuntimeError> {
        let layout = StoreLayout::create(data_dir)?;
        let writer_lock = StoreWriterLock::acquire(layout.store_dir())?;
        let authority = AuthorityDb::open(layout.authority_database()).map_err(|error| {
            RuntimeError::AuthorityDatabase {
                message: error.to_string(),
            }
        })?;
        let derived = DerivedCatalog::open(layout.derived_dir().join("catalog.sqlite"))?;
        let adaptive_database = layout.adaptive_dir().join("adaptive.sqlite");
        let mut runtime = Self {
            _writer_lock: writer_lock,
            cas: SourceCas::new(&layout),
            layout,
            authority,
            derived,
            lexical_handles: BTreeMap::new(),
            compiler: DerivedCompiler::default(),
            pending_provider_work: VecDeque::new(),
            inflight_provider_work: BTreeMap::new(),
            pending_by_generation: BTreeMap::new(),
            query_operations: QueryOperationTable::default(),
            semantic_build_coverage: AuthorityGeneration::initial(),
            tag_dictionary: TagDictionary::new(),
            generated_tag_artifacts: BTreeMap::new(),
            adaptive_log: AdaptiveEventLog::open(adaptive_database)?,
            provider_egress_policy,
            provider_routes,
            purge: PurgeCoordinator::new(),
            resource_limits: ResourceLimits::default(),
            receipts: BTreeMap::new(),
            next_retrieval_id: 0,
            closed: false,
            last_error: None,
        };
        for (tag_id, normalized_value) in runtime.derived.tag_dictionary_entries()? {
            runtime.tag_dictionary.restore(tag_id, normalized_value)?;
        }
        runtime.recover_after_open()?;
        Ok(runtime)
    }

    pub fn close(&mut self) -> Result<(), RuntimeError> {
        self.closed = true;
        self.query_operations = QueryOperationTable::default();
        Ok(())
    }

    pub fn create_space(&mut self, space_key: impl AsRef<str>) -> Result<SpaceId, RuntimeError> {
        self.create_space_with_policy(space_key, SpaceProviderPolicy::default())
    }

    pub fn create_space_with_policy(
        &mut self,
        space_key: impl AsRef<str>,
        provider_policy: SpaceProviderPolicy,
    ) -> Result<SpaceId, RuntimeError> {
        self.ensure_open()?;
        Ok(self
            .authority
            .create_space_with_policy(space_key, provider_policy)?
            .into_value()
            .space_id)
    }

    pub fn update_space_provider_policy(
        &mut self,
        space_id: SpaceId,
        provider_policy: SpaceProviderPolicy,
        expected_generation: AuthorityGeneration,
    ) -> Result<AuthorityGeneration, RuntimeError> {
        self.ensure_open()?;
        Ok(self
            .authority
            .update_space_provider_policy(space_id, provider_policy, expected_generation)?
            .generation())
    }

    pub fn create_memory(
        &mut self,
        space_id: SpaceId,
        document_key: Option<&str>,
        source: &[u8],
    ) -> Result<MemoryId, RuntimeError> {
        self.ensure_open()?;
        check_source_bytes(self.resource_limits, source.len()).map_err(|error| {
            RuntimeError::ResourceLimit {
                resource: error.resource,
                actual: error.actual,
                maximum: error.maximum,
            }
        })?;
        let projection = local_embedding_projection(source)?;
        let result = self
            .authority
            .create_memory(&self.cas, space_id, document_key, source)?;
        let memory_id = result.value().memory_id;
        let provider_policy = self.space_provider_policy_at(space_id, result.generation())?;
        self.enqueue_embedding_work(memory_id, result.generation(), projection, provider_policy);
        self.rebuild_base_for_space(space_id, result.generation(), provider_policy);
        Ok(memory_id)
    }

    pub fn create_memory_idempotent(
        &mut self,
        space_id: SpaceId,
        document_key: Option<&str>,
        source: &[u8],
        idempotency_key: &str,
    ) -> Result<MemoryId, RuntimeError> {
        self.ensure_open()?;
        check_source_bytes(self.resource_limits, source.len()).map_err(|error| {
            RuntimeError::ResourceLimit {
                resource: error.resource,
                actual: error.actual,
                maximum: error.maximum,
            }
        })?;
        let projection = local_embedding_projection(source)?;
        let before = self.authority_generation()?;
        let result = self.authority.create_memory_idempotent(
            &self.cas,
            space_id,
            document_key,
            source,
            idempotency_key,
        )?;
        let memory_id = result.value().memory_id;
        if result.generation() > before {
            let provider_policy = self.space_provider_policy_at(space_id, result.generation())?;
            self.enqueue_embedding_work(
                memory_id,
                result.generation(),
                projection,
                provider_policy,
            );
            self.rebuild_base_for_space(space_id, result.generation(), provider_policy);
        }
        Ok(memory_id)
    }

    pub fn export_memories(&self, scope: &[SpaceId]) -> Result<Vec<PortableMemory>, RuntimeError> {
        self.ensure_open()?;
        let generation = self.authority_generation()?;
        let mut memories = Vec::new();
        for space_id in scope {
            for read in self
                .authority
                .list_memories_at(&self.cas, *space_id, generation)?
            {
                memories.push(PortableMemory {
                    source_id: read.memory.memory_id,
                    space_id: read.memory.space_id,
                    revision_id: read.revision.revision_id,
                    mdx: String::from_utf8(read.source)?,
                });
            }
        }
        memories.sort_by_key(|memory| (memory.space_id, memory.source_id));
        Ok(memories)
    }

    pub fn import_portable(
        &mut self,
        request: PortableImportRequest,
    ) -> Result<PortableImportResult, RuntimeError> {
        self.ensure_open()?;
        let PortableImportRequest {
            target_space_key,
            idempotency_key,
            request_fingerprint,
            origin_store_id,
            memories,
        } = request;
        if target_space_key.trim().is_empty() {
            return Err(RuntimeError::Authority(
                MemoriaError::InvalidMutationBatch {
                    message: "portable import target space key must not be empty".to_owned(),
                },
            ));
        }
        if memories.is_empty() {
            return Err(RuntimeError::Authority(
                MemoriaError::InvalidMutationBatch {
                    message: "portable import must contain at least one memory".to_owned(),
                },
            ));
        }

        let target_space_id = SpaceId::try_new()?;
        let mut mapping = BTreeMap::new();
        let mut allocated = Vec::with_capacity(memories.len());
        for memory in memories {
            let target_id = MemoryId::try_new()?;
            if mapping
                .insert(memory.source_id.to_string(), target_id.to_string())
                .is_some()
            {
                return Err(RuntimeError::Authority(
                    MemoriaError::InvalidMutationBatch {
                        message: format!(
                            "portable import source memory {} is listed more than once",
                            memory.source_id
                        ),
                    },
                ));
            }
            allocated.push((memory.source_id, target_id, memory.mdx));
        }

        // Allocate the complete local identity mapping before parsing and
        // rewriting any document. This keeps the document pass independent of
        // Authority commit order and makes all internal references resolvable.
        let mut unresolved = std::collections::BTreeSet::new();
        let mut import_memories = Vec::with_capacity(allocated.len());
        let mut rewritten_sources = Vec::with_capacity(allocated.len());
        for (source_id, target_id, source) in allocated {
            let rewritten = rewrite_memory_ref_source(&source, &mapping)?;
            unresolved.extend(rewritten.unresolved);
            let source_bytes = rewritten.rewritten.into_bytes();
            rewritten_sources.push((target_id, source_bytes.clone()));
            import_memories.push(ImportMemoryAllocation {
                source_id,
                memory_id: target_id,
                document_key: None,
                source: source_bytes,
            });
        }

        let allocation = PortableImportAllocation {
            target_space_id,
            memories: import_memories,
        };
        let before = self.authority_generation()?;
        let result = self.authority.import_portable(
            &self.cas,
            PortableImportCommit {
                target_space_key,
                allocation,
                idempotency_key,
                request_fingerprint,
                origin_store_id,
                unresolved_external_references: unresolved.into_iter().collect(),
            },
        )?;
        if result.generation() > before {
            let policy = self.space_provider_policy_at(target_space_id, result.generation())?;
            for (memory_id, source) in rewritten_sources {
                let projection = local_embedding_projection(&source)?;
                self.enqueue_embedding_work(memory_id, result.generation(), projection, policy);
            }
            self.rebuild_base_for_space(target_space_id, result.generation(), policy);
        }
        Ok(result.into_value())
    }

    pub fn create_backup(
        &mut self,
        destination: Option<&std::path::Path>,
        includes_adaptive: bool,
    ) -> Result<backup::BackupManifest, RuntimeError> {
        self.ensure_open()?;
        if let Some(destination) = destination
            && destination.starts_with(self.layout.store_dir())
        {
            return Err(RuntimeError::Backup(backup::BackupError::Invalid {
                message: "backup destination is inside the source Store".to_owned(),
            }));
        }
        Ok(backup::create_backup(
            &self.layout,
            &self.authority,
            &self.adaptive_log,
            destination,
            includes_adaptive,
        )?)
    }

    pub fn plan_purge(&mut self, memory_id: MemoryId) -> Result<PurgePlan, RuntimeError> {
        self.ensure_open()?;
        self.authority.get_memory(memory_id)?;
        if let Some(existing) = self
            .authority
            .list_purge_operations()?
            .into_iter()
            .find(|operation| operation.memory_id == memory_id)
        {
            let plan = purge_plan_from_authority(existing)?;
            self.purge.restore(plan.clone());
            return Ok(plan);
        }
        let plan_id = format!("PURGE_{}_{}", memory_id, unique_purge_suffix());
        let plan = self.purge.plan_with_id(plan_id, memory_id);
        let persisted = self.authority.create_purge_operation(&plan.id, memory_id)?;
        let plan = purge_plan_from_authority(persisted)?;
        self.purge.restore(plan.clone());
        Ok(plan)
    }

    pub fn execute_purge(&mut self, plan_id: &str) -> Result<PurgePlan, RuntimeError> {
        self.ensure_open()?;
        let planned =
            self.purge
                .plan_for(plan_id)
                .cloned()
                .ok_or_else(|| RuntimeError::PurgeConflict {
                    plan_id: plan_id.to_owned(),
                })?;
        if planned.state == PurgeState::Completed {
            return Ok(planned);
        }
        if planned.state == PurgeState::Planned {
            self.authority
                .transition_purge_operation(plan_id, "planned", "committed")?;
            self.purge
                .transition(plan_id, PurgeState::Planned, PurgeState::Committed)
                .map_err(|_| RuntimeError::PurgeConflict {
                    plan_id: plan_id.to_owned(),
                })?;
        }
        if self.purge.state(plan_id) == Some(PurgeState::Committed) {
            match self.authority.purge_memory(planned.memory_id) {
                Ok(_) | Err(MemoriaError::NotFound { .. }) => {}
                Err(error) => return Err(error.into()),
            }
            self.authority
                .transition_purge_operation(plan_id, "committed", "cleaning")?;
            self.purge
                .transition(plan_id, PurgeState::Committed, PurgeState::Cleaning)
                .map_err(|_| RuntimeError::PurgeConflict {
                    plan_id: plan_id.to_owned(),
                })?;
        }
        if self.purge.state(plan_id) == Some(PurgeState::Cleaning) {
            self.adaptive_log
                .rewrite_without_memory(planned.memory_id)?;
            self.receipts.clear();
            self.derived.delete_all_derived()?;
            self.collect_unreferenced_source_objects()?;
            let integrity = self.authority.verify_full(&self.cas)?;
            if !integrity.is_clean() {
                let message = integrity
                    .issues()
                    .iter()
                    .map(|issue| format!("{}: {}", issue.code, issue.message))
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(MemoriaError::Corruption { message }.into());
            }
            self.authority
                .transition_purge_operation(plan_id, "cleaning", "completed")?;
            return self
                .purge
                .transition(plan_id, PurgeState::Cleaning, PurgeState::Completed)
                .map_err(|_| RuntimeError::PurgeConflict {
                    plan_id: plan_id.to_owned(),
                });
        }
        Err(RuntimeError::PurgeConflict {
            plan_id: plan_id.to_owned(),
        })
    }

    pub fn revise_memory(
        &mut self,
        memory_id: MemoryId,
        expected_head: RevisionId,
        source: &[u8],
    ) -> Result<MemoryMutation, RuntimeError> {
        self.ensure_open()?;
        check_source_bytes(self.resource_limits, source.len()).map_err(|error| {
            RuntimeError::ResourceLimit {
                resource: error.resource,
                actual: error.actual,
                maximum: error.maximum,
            }
        })?;
        let previous_source = self.authority.read_memory(&self.cas, memory_id)?.source;
        let requires_content_embedding = self.requires_content_embedding(&previous_source, source);
        let projection = if requires_content_embedding {
            Some(local_embedding_projection(source)?)
        } else {
            None
        };
        let result = self
            .authority
            .revise_memory(&self.cas, memory_id, expected_head, source)?;
        let record = result.value();
        let provider_policy =
            self.space_provider_policy_at(record.space_id, result.generation())?;
        if let Some(projection) = projection {
            self.supersede_embedding_work_for_memory(memory_id)?;
            self.enqueue_embedding_work(
                record.memory_id,
                result.generation(),
                projection,
                provider_policy,
            );
        } else {
            self.advance_semantic_build_coverage();
        }
        self.rebuild_base_for_space(record.space_id, result.generation(), provider_policy);
        Ok(MemoryMutation {
            memory_id: record.memory_id,
            space_id: record.space_id,
            revision_id: record.head_revision_id,
            generation: result.generation(),
        })
    }

    pub fn move_memory(
        &mut self,
        memory_id: MemoryId,
        space_id: SpaceId,
        document_key: Option<&str>,
        expected_generation: AuthorityGeneration,
    ) -> Result<MemoryMutation, RuntimeError> {
        self.ensure_open()?;
        let previous_space_id = self.authority.get_memory(memory_id)?.space_id;
        let result =
            self.authority
                .move_memory(memory_id, space_id, document_key, expected_generation)?;
        let record = result.value();
        let generation = result.generation();
        let previous_policy = self.space_provider_policy_at(previous_space_id, generation)?;
        self.rebuild_base_for_space(previous_space_id, generation, previous_policy);
        if previous_space_id != record.space_id {
            let target_policy = self.space_provider_policy_at(record.space_id, generation)?;
            self.rebuild_base_for_space(record.space_id, generation, target_policy);
        }
        self.advance_semantic_build_coverage();
        Ok(MemoryMutation {
            memory_id: record.memory_id,
            space_id: record.space_id,
            revision_id: record.head_revision_id,
            generation,
        })
    }

    pub fn query(&mut self, query: MemoryQuery) -> Result<RetrievalResponse, RuntimeError> {
        match self.query_start(query)? {
            QueryStep::Complete(response) => Ok(response),
            QueryStep::ProviderPending { operation_id, .. }
            | QueryStep::ReadinessPending { operation_id, .. } => {
                Err(RuntimeError::QueryOperationPending { operation_id })
            }
        }
    }

    pub fn query_start(&mut self, query: MemoryQuery) -> Result<QueryStep, RuntimeError> {
        self.ensure_open()?;
        self.query_operations.cleanup(std::time::Instant::now());
        query.validate()?;

        let compiled = match self.compile_query(query.clone()) {
            Ok(compiled) => compiled,
            Err(RuntimeError::Query(QueryError::CapabilityNotReady { .. }))
                if matches!(query.consistency.readiness, ReadinessBehavior::Wait(_)) =>
            {
                let ReadinessBehavior::Wait(timeout) = query.consistency.readiness else {
                    unreachable!("readiness wait guard must hold");
                };
                let operation_id = self.query_operations.insert_readiness(query, timeout);
                return Ok(QueryStep::ReadinessPending {
                    deadline_unix_ms: self
                        .query_operations
                        .get(&operation_id)
                        .and_then(|operation| operation.deadline_unix_ms)
                        .unwrap_or_default(),
                    operation_id,
                    retry_after_ms: crate::query_operation::READINESS_RETRY_AFTER_MS,
                });
            }
            Err(error) => return Err(error),
        };

        self.start_compiled_query(compiled, None)
    }

    fn start_compiled_query(
        &mut self,
        compiled: memoria_query::CompiledQuery,
        continuation_operation_id: Option<String>,
    ) -> Result<QueryStep, RuntimeError> {
        let query = compiled.query.clone();
        let semantic_required = query
            .required_capabilities
            .iter()
            .any(|capability| capability == "semantic");
        let requested_semantic = semantic_required
            || query
                .preferred_capabilities
                .iter()
                .any(|capability| capability == "semantic");

        let space_policy = self.space_provider_policy_at_scope(
            &compiled.query.scope.spaces,
            compiled.snapshot.authority_generation,
        )?;
        let semantic_route = self
            .provider_routes
            .route(ProviderCapability::Embedding)
            .clone();
        let compiled =
            match resolve_provider_route(space_policy, &semantic_route, semantic_required) {
                ProviderRouteDecision::RequiredDenied => {
                    return Err(RuntimeError::ProviderPolicyDenied {
                        capability: ProviderCapability::Embedding,
                    });
                }
                ProviderRouteDecision::PreferredDegraded
                    if requested_semantic && compiled.execution.used("semantic") =>
                {
                    self.compile_without_capability(&query, "semantic")?
                }
                ProviderRouteDecision::Allowed(_) | ProviderRouteDecision::PreferredDegraded => {
                    compiled
                }
            };

        let adaptive_snapshot = self.adaptive_snapshot_for(&compiled)?;
        let semantic_pending = requested_semantic && compiled.execution.used("semantic");
        if semantic_pending {
            let work = query_embedding_work(&query, space_policy, semantic_route);
            let operation_id = if let Some(operation_id) = continuation_operation_id {
                self.query_operations.replace_provider(
                    &operation_id,
                    compiled.clone(),
                    work.clone(),
                    None,
                    None,
                    adaptive_snapshot.clone(),
                );
                operation_id
            } else {
                self.query_operations.insert_provider(
                    compiled.query.clone(),
                    compiled,
                    work.clone(),
                    None,
                    None,
                    adaptive_snapshot.clone(),
                )
            };
            return Ok(QueryStep::ProviderPending { operation_id, work });
        }

        let response = self.execute_compiled_query_stage(&compiled, None)?;
        if compiled.execution.used("reranking") {
            return self.begin_rerank_barrier(
                compiled.query.clone(),
                compiled,
                response,
                continuation_operation_id,
                space_policy,
                adaptive_snapshot,
            );
        }

        if let Some(operation_id) = continuation_operation_id {
            self.query_operations.remove(&operation_id);
        }
        Ok(QueryStep::Complete(self.finalize_query_response(
            &compiled,
            response,
            adaptive_snapshot.as_ref(),
        )?))
    }

    fn begin_rerank_barrier(
        &mut self,
        query: MemoryQuery,
        compiled: memoria_query::CompiledQuery,
        mut response: RetrievalResponse,
        continuation_operation_id: Option<String>,
        space_policy: SpaceProviderPolicy,
        adaptive_snapshot: Option<AdaptiveReadSnapshot>,
    ) -> Result<QueryStep, RuntimeError> {
        let batch = build_rerank_batch(
            query.cue.text.join(" "),
            &response.results,
            &query.scope.spaces,
            rerank_limit_for_quality(query.quality.level),
        )?;
        if batch.views.is_empty() {
            if let Some(operation_id) = continuation_operation_id {
                self.query_operations.remove(&operation_id);
            }
            return Ok(QueryStep::Complete(self.finalize_query_response(
                &compiled,
                response,
                adaptive_snapshot.as_ref(),
            )?));
        }
        let rerank_required = query
            .required_capabilities
            .iter()
            .any(|capability| capability == "reranking");
        let rerank_route = self
            .provider_routes
            .route(ProviderCapability::Rerank)
            .clone();
        match resolve_provider_route(space_policy, &rerank_route, rerank_required) {
            ProviderRouteDecision::RequiredDenied => Err(RuntimeError::ProviderPolicyDenied {
                capability: ProviderCapability::Rerank,
            }),
            ProviderRouteDecision::PreferredDegraded => {
                let mut compiled = compiled;
                compiled
                    .execution
                    .used_capabilities
                    .retain(|capability| capability != "reranking");
                compiled.execution.degraded = true;
                if !compiled
                    .execution
                    .degraded_capabilities
                    .iter()
                    .any(|capability| capability == "reranking")
                {
                    compiled
                        .execution
                        .degraded_capabilities
                        .push("reranking".to_owned());
                }
                response.execution = compiled.execution.clone();
                response.trace.capability_degraded = true;
                response.trace.rerank_requested = false;
                if let Some(operation_id) = continuation_operation_id {
                    self.query_operations.remove(&operation_id);
                }
                Ok(QueryStep::Complete(self.finalize_query_response(
                    &compiled,
                    response,
                    adaptive_snapshot.as_ref(),
                )?))
            }
            ProviderRouteDecision::Allowed(rerank_route) => {
                let work = query_rerank_work(&query, &batch, space_policy, rerank_route);
                let operation_id = if let Some(operation_id) = continuation_operation_id {
                    self.query_operations.replace_provider(
                        &operation_id,
                        compiled,
                        work.clone(),
                        Some(response),
                        Some(batch),
                        adaptive_snapshot.clone(),
                    );
                    operation_id
                } else {
                    self.query_operations.insert_provider(
                        query,
                        compiled,
                        work.clone(),
                        Some(response),
                        Some(batch),
                        adaptive_snapshot,
                    )
                };
                Ok(QueryStep::ProviderPending { operation_id, work })
            }
        }
    }

    pub fn query_continue(&mut self, operation_id: &str) -> Result<QueryStep, RuntimeError> {
        self.ensure_open()?;
        let now = std::time::Instant::now();
        let Some(operation) = self.query_operations.get(operation_id) else {
            return Err(RuntimeError::QueryOperationNotFound {
                operation_id: operation_id.to_owned(),
            });
        };
        if operation.is_expired(now) {
            self.query_operations.remove(operation_id);
            return Err(RuntimeError::QueryOperationExpired {
                operation_id: operation_id.to_owned(),
            });
        }
        if operation.cancelled {
            self.query_operations.remove(operation_id);
            return Err(RuntimeError::QueryOperationCancelled {
                operation_id: operation_id.to_owned(),
            });
        }
        if operation.stage != QueryOperationStage::WaitingForCapabilities {
            if let Some(work) = operation.work {
                return Ok(QueryStep::ProviderPending {
                    operation_id: operation_id.to_owned(),
                    work,
                });
            }
            return Err(RuntimeError::QueryOperationPending {
                operation_id: operation_id.to_owned(),
            });
        }

        let query = operation.query.clone();
        match self.compile_query(query) {
            Ok(compiled) => self
                .start_compiled_query(compiled, Some(operation_id.to_owned()))
                .inspect_err(|_| {
                    self.query_operations.remove(operation_id);
                }),
            Err(error @ RuntimeError::Query(QueryError::CapabilityNotReady { .. })) => {
                let deadline = operation
                    .readiness_deadline
                    .expect("capability readiness operation must have a deadline");
                if now >= deadline {
                    self.query_operations.remove(operation_id);
                    Err(error)
                } else {
                    Ok(self
                        .query_operations
                        .readiness_step(operation_id)
                        .expect("capability readiness operation must remain pending"))
                }
            }
            Err(error) => Err(error),
        }
    }

    pub fn query_resume(
        &mut self,
        operation_id: &str,
        result: ProviderWorkResult,
    ) -> Result<QueryStep, RuntimeError> {
        self.ensure_open()?;
        let now = std::time::Instant::now();
        let Some(operation) = self.query_operations.get(operation_id) else {
            return Err(RuntimeError::QueryOperationNotFound {
                operation_id: operation_id.to_owned(),
            });
        };
        if operation.is_expired(now) {
            self.query_operations.remove(operation_id);
            self.query_operations.cleanup(now);
            return Err(RuntimeError::QueryOperationExpired {
                operation_id: operation_id.to_owned(),
            });
        }
        if operation.cancelled {
            self.query_operations.remove(operation_id);
            return Err(RuntimeError::QueryOperationCancelled {
                operation_id: operation_id.to_owned(),
            });
        }
        let Some(expected_work) = operation.work.as_ref() else {
            return Err(RuntimeError::UnexpectedQueryWork {
                work_id: result.work_id().to_owned(),
            });
        };
        if expected_work.work_id() != result.work_id() {
            return Err(RuntimeError::UnexpectedQueryWork {
                work_id: result.work_id().to_owned(),
            });
        }
        let expected_work = expected_work.as_need_work();
        let result = validate_provider_result(&expected_work, result).map_err(|error| {
            RuntimeError::InvalidProviderResult {
                work_id: work_id(&expected_work).to_owned(),
                message: error.to_string(),
            }
        })?;
        if let ProviderWorkResult::Failure {
            work_id,
            retryable,
            code,
            message,
        } = result
        {
            if code == "PROVIDER_POLICY_DENIED" {
                let capability = match operation.work.as_ref() {
                    Some(QueryWork::Embedding(_)) => "semantic",
                    Some(QueryWork::Rerank(_)) => "reranking",
                    None => unreachable!("provider work was checked above"),
                };
                let required = operation
                    .compiled
                    .as_ref()
                    .expect("provider operation must have a compiled query")
                    .query
                    .required_capabilities
                    .iter()
                    .any(|item| item == capability);
                if !required {
                    let operation =
                        self.query_operations.remove(operation_id).ok_or_else(|| {
                            RuntimeError::QueryOperationNotFound {
                                operation_id: operation_id.to_owned(),
                            }
                        })?;
                    let mut fallback_query = operation
                        .compiled
                        .as_ref()
                        .expect("provider operation must have a compiled query")
                        .query
                        .clone();
                    fallback_query
                        .required_capabilities
                        .retain(|item| item != capability);
                    fallback_query
                        .preferred_capabilities
                        .retain(|item| item != capability);
                    let mut fallback = self.compile_query(fallback_query)?;
                    fallback.execution.degraded = true;
                    if !fallback
                        .execution
                        .degraded_capabilities
                        .iter()
                        .any(|item| item == capability)
                    {
                        fallback
                            .execution
                            .degraded_capabilities
                            .push(capability.to_owned());
                    }
                    self.query_operations.cleanup(now);
                    return Ok(QueryStep::Complete(
                        self.execute_compiled_query(fallback, None)?,
                    ));
                }
            }
            return Err(RuntimeError::ProviderFailure {
                work_id,
                retryable,
                code,
                message,
            });
        }
        if matches!(operation.work, Some(QueryWork::Rerank(_))) {
            let ProviderWorkResult::Rerank { scores, .. } = result else {
                unreachable!("validated rerank work must return rerank scores");
            };
            let rerank_scores = scores
                .into_iter()
                .map(|score| QueryRerankScore {
                    handle: score.handle,
                    score: score.score,
                })
                .collect::<Vec<_>>();
            let mut stored = operation.clone();
            stored.rerank_scores = Some(rerank_scores.clone());
            stored.stage = QueryOperationStage::Reranked;
            self.query_operations
                .replace(operation_id.to_owned(), stored);

            let compiled = operation
                .compiled
                .as_ref()
                .expect("rerank operation must have a compiled query");
            let batch = operation
                .rerank_batch
                .as_ref()
                .expect("rerank operation must have a stored batch");
            let mut response =
                operation
                    .pending_response
                    .ok_or(RuntimeError::QueryOperationPending {
                        operation_id: operation_id.to_owned(),
                    })?;
            response.results = apply_rerank(
                response.results,
                batch,
                &rerank_scores,
                &compiled.query.scope.spaces,
            )?;
            response.trace.rerank_applied = true;
            let finalized = self.finalize_query_response(
                compiled,
                response,
                operation.adaptive_snapshot.as_ref(),
            )?;
            self.query_operations.remove(operation_id);
            self.query_operations.cleanup(now);
            return Ok(QueryStep::Complete(finalized));
        }

        let query_vector = match &result {
            ProviderWorkResult::Embeddings { vectors, .. } => {
                vectors.first().map(|vector| vector.values.clone())
            }
            _ => None,
        };
        let compiled = operation
            .compiled
            .as_ref()
            .expect("embedding operation must have a compiled query")
            .clone();
        let response = self.execute_compiled_query_stage(&compiled, query_vector.clone())?;
        if compiled.execution.used("reranking") {
            return self.begin_rerank_barrier(
                compiled.query.clone(),
                compiled,
                response,
                Some(operation_id.to_owned()),
                operation
                    .work
                    .as_ref()
                    .expect("embedding operation must have provider work")
                    .space_policy(),
                operation.adaptive_snapshot.clone(),
            );
        }

        self.query_operations.remove(operation_id);
        self.query_operations.cleanup(now);
        Ok(QueryStep::Complete(self.finalize_query_response(
            &compiled,
            response,
            operation.adaptive_snapshot.as_ref(),
        )?))
    }

    pub fn cancel_operation(&mut self, operation_id: &str) -> Result<(), RuntimeError> {
        self.ensure_open()?;
        self.query_operations.cleanup(std::time::Instant::now());
        if self.query_operations.cancel(operation_id) {
            Ok(())
        } else {
            Err(RuntimeError::QueryOperationNotFound {
                operation_id: operation_id.to_owned(),
            })
        }
    }

    pub fn expire_query_operation_for_test(&mut self, operation_id: &str) {
        self.query_operations.expire_for_test(operation_id);
    }

    fn compile_query(
        &self,
        query: MemoryQuery,
    ) -> Result<memoria_query::CompiledQuery, RuntimeError> {
        let generation = self.authority.current_generation().map_err(|error| {
            RuntimeError::AuthorityDatabase {
                message: error.to_string(),
            }
        })?;
        let manifest = self.derived.serving_manifest()?.unwrap_or_else(|| {
            memoria_derived::DerivedManifest::empty_for_lexical_query(generation)
        });
        let mut compiled = QueryCompiler::new(generation, Some(manifest))
            .with_adaptive_snapshot(AdaptiveSnapshotIdentity::Disabled)
            .compile(query)
            .map_err(RuntimeError::from)?;
        if compiled.execution.used("adaptive") {
            compiled.snapshot.adaptive = AdaptiveSnapshotIdentity::Enabled {
                generation: self.adaptive_log.current_generation(),
                model_version: AdaptiveStateV1::model_version().to_owned(),
            };
        }
        Ok(compiled)
    }

    fn compile_without_capability(
        &self,
        query: &MemoryQuery,
        capability: &str,
    ) -> Result<memoria_query::CompiledQuery, RuntimeError> {
        let mut fallback = query.clone();
        fallback
            .required_capabilities
            .retain(|item| item != capability);
        fallback
            .preferred_capabilities
            .retain(|item| item != capability);
        let mut compiled = self.compile_query(fallback)?;
        compiled.execution.degraded = true;
        if !compiled
            .execution
            .degraded_capabilities
            .iter()
            .any(|item| item == capability)
        {
            compiled
                .execution
                .degraded_capabilities
                .push(capability.to_owned());
        }
        Ok(compiled)
    }

    fn adaptive_snapshot_for(
        &self,
        compiled: &memoria_query::CompiledQuery,
    ) -> Result<Option<AdaptiveReadSnapshot>, RuntimeError> {
        let generation = match &compiled.snapshot.adaptive {
            AdaptiveSnapshotIdentity::Enabled { generation, .. } => *generation,
            AdaptiveSnapshotIdentity::Disabled => return Ok(None),
        };
        Ok(Some(self.adaptive_log.snapshot_at(generation)?))
    }

    fn execute_compiled_query(
        &mut self,
        compiled: memoria_query::CompiledQuery,
        query_vector: Option<Vec<f32>>,
    ) -> Result<RetrievalResponse, RuntimeError> {
        let adaptive_snapshot = self.adaptive_snapshot_for(&compiled)?;
        let response = self.execute_compiled_query_stage(&compiled, query_vector)?;
        self.finalize_query_response(&compiled, response, adaptive_snapshot.as_ref())
    }

    fn execute_compiled_query_stage(
        &mut self,
        compiled: &memoria_query::CompiledQuery,
        query_vector: Option<Vec<f32>>,
    ) -> Result<RetrievalResponse, RuntimeError> {
        let records =
            self.records_for_query(&compiled.query, compiled.snapshot.authority_generation)?;
        let exact = ExactIndex::new(records);
        let plan = PhysicalQueryPlanner::plan(compiled);
        let mut pool = CandidatePool::new();
        let mut trace = QueryOperatorTrace {
            authority_generation: compiled.snapshot.authority_generation,
            capability_degraded: compiled.execution.degraded,
            ..QueryOperatorTrace::default()
        };

        if plan.channels.contains(&PhysicalChannel::Exact) {
            pool.insert_response(PhysicalChannel::Exact, execute_exact(compiled, &exact));
            record_runtime_channel(&mut trace, &pool, PhysicalChannel::Exact, "exact");
        }

        if plan.channels.contains(&PhysicalChannel::Lexical) && !exact.records().is_empty() {
            let lexical_candidates = {
                let handle = self.lexical_handle(compiled.snapshot.derived_manifest)?;
                let operator = ManifestLexicalOperator {
                    handle,
                    exact: &exact,
                };
                operator.search(
                    &compiled.query.cue.text.join(" "),
                    &compiled.query.scope.spaces,
                    plan.profile.lexical_candidates,
                )?
            };
            pool.insert_response(
                PhysicalChannel::Lexical,
                execute_lexical(compiled, &LexicalCandidateIndex::new(lexical_candidates)),
            );
            record_runtime_channel(&mut trace, &pool, PhysicalChannel::Lexical, "lexical");
        }

        if plan.channels.contains(&PhysicalChannel::SemanticDirect) {
            let query_vector = query_vector
                .as_deref()
                .ok_or(QueryError::QueryEmbeddingRequired)?;
            let hits = self.semantic_vector_hits(
                compiled,
                query_vector,
                &exact,
                plan.profile.semantic_direct_candidates,
            )?;
            pool.insert_response(
                PhysicalChannel::SemanticDirect,
                execute_semantic(
                    compiled,
                    &SemanticCandidateIndex::from_vector_hits(
                        hits,
                        &exact,
                        SemanticResolution::Leaf,
                    ),
                ),
            );
            record_runtime_channel(
                &mut trace,
                &pool,
                PhysicalChannel::SemanticDirect,
                "semantic-direct",
            );
        }

        if compiled.execution.used("semantic") || compiled.execution.used("associative") {
            let inputs = self.algorithm_channel_inputs(compiled, &exact)?;
            let residual_operator = RuntimeSemanticResidualOperator {
                runtime: self,
                compiled,
                exact: &exact,
            };
            trace.merge(execute_algorithm_channels(
                compiled,
                query_vector.as_deref().unwrap_or(&[]),
                &exact,
                &mut pool,
                &inputs,
                Some(&residual_operator),
            )?);
        }

        let fused = fuse_candidate_pool(
            &pool,
            compiled.query.budget.max_candidates,
            &compiled.query.scope.spaces,
        );
        trace.independent_support_count = fused
            .iter()
            .map(|candidate| candidate.independent_support_count)
            .sum();
        trace.correlation_suppressed_evidence = fused
            .iter()
            .map(|candidate| candidate.correlation_suppressed_evidence)
            .sum();
        let candidates = fused;
        self.next_retrieval_id = self.next_retrieval_id.saturating_add(1);
        let mut response = build_response(
            format!("RET_{}", self.next_retrieval_id),
            compiled,
            candidates,
        )?;
        response.trace = trace;
        response.trace.rerank_requested = compiled.execution.used("reranking");
        response.assessment = assess(&response.results);
        Ok(response)
    }

    fn finalize_query_response(
        &mut self,
        compiled: &memoria_query::CompiledQuery,
        mut response: RetrievalResponse,
        adaptive_snapshot: Option<&AdaptiveReadSnapshot>,
    ) -> Result<RetrievalResponse, RuntimeError> {
        let query_signature = adaptive_signature(&compiled.query);
        let now = memoria_types::Timestamp::now()?;
        if compiled.execution.used("adaptive") {
            let adaptive_state =
                adaptive_snapshot.ok_or(RuntimeError::AdaptiveSnapshotUnavailable)?;
            response.results = rank_with_adaptive(
                response.results,
                adaptive_state.state(),
                &query_signature,
                now,
            );
            response.trace = response
                .trace
                .with_channel("adaptive", response.results.len());
        }
        response.assessment = assess(&response.results);
        let receipt = RetrievalReceipt::from_results(
            response.retrieval_id.clone(),
            response.snapshot.clone(),
            query_signature,
            &response.results,
            now,
        )?;
        self.receipts.insert(response.retrieval_id.clone(), receipt);
        Ok(response)
    }

    fn algorithm_channel_inputs(
        &mut self,
        compiled: &memoria_query::CompiledQuery,
        exact: &ExactIndex,
    ) -> Result<AlgorithmChannelInputs, RuntimeError> {
        let memberships = self.derived.tag_memberships()?;
        let mut tag_dictionary = self.tag_dictionary.clone();
        let mut tag_graph = TagGraph::new();
        for membership in &memberships {
            let mut input = TagMembershipInput::new(
                membership.space_id,
                membership.memory_id,
                membership.revision_id,
                membership.normalized_value.clone(),
                membership.provenance,
            );
            if let Some(node_id) = membership.node_id.as_deref() {
                input = input.with_node_id(node_id.to_owned());
            }
            tag_graph.insert_membership(&mut tag_dictionary, input)?;
        }
        let tag_seeds = resolve_explicit_tag_seeds(&mut tag_dictionary, &compiled.query.cue.tags)?;
        let tag_vectors = self.tag_vector_candidates(compiled, exact, &memberships)?;
        let exact_candidates = execute_exact(compiled, exact).results;
        let relation_links = exact
            .records()
            .iter()
            .flat_map(|record| {
                let local_relations = record
                    .relations
                    .iter()
                    .filter(|relation| !relation.starts_with("memory-ref:"))
                    .filter_map(|relation| {
                        let candidate = exact_candidates
                            .iter()
                            .find(|candidate| candidate.target == record.target)?;
                        let mut evidence = candidate.clone();
                        evidence.relations.clear();
                        Some(RelationLink::new(
                            record.target,
                            record.target,
                            relation.clone(),
                            evidence,
                        ))
                    });
                let memory_relations = record.memory_reference_ids().filter_map(|memory_id| {
                    let target_memory = memory_id.parse::<MemoryId>().ok()?;
                    let target_record = exact
                        .records()
                        .iter()
                        .find(|candidate| candidate.target.memory_id == target_memory)?;
                    let mut evidence = exact_candidates
                        .iter()
                        .find(|candidate| candidate.target == target_record.target)?
                        .clone();
                    evidence.relations.clear();
                    Some(RelationLink::new(
                        record.target,
                        target_record.target,
                        "references",
                        evidence,
                    ))
                });
                local_relations.chain(memory_relations).collect::<Vec<_>>()
            })
            .collect();
        Ok(AlgorithmChannelInputs {
            tag_dictionary,
            tag_graph,
            tag_vectors,
            tag_seeds,
            relation_links,
        })
    }

    fn tag_vector_candidates(
        &self,
        compiled: &memoria_query::CompiledQuery,
        exact: &ExactIndex,
        memberships: &[memoria_derived::TagMembershipRecord],
    ) -> Result<Vec<TagVectorCandidate>, RuntimeError> {
        let manifest = self.derived.manifest(compiled.snapshot.derived_manifest)?;
        let mut vectors_by_target = BTreeMap::<(SpaceId, MemoryId, RevisionId), Vec<f32>>::new();
        for artifact_id in manifest.artifacts() {
            let artifact = self.derived.artifact(artifact_id)?;
            if !artifact.is_compatible_semantic() {
                continue;
            }
            for membership in self.derived.vector_memberships_for_artifact(artifact_id)? {
                if !compiled.query.scope.spaces.contains(&membership.space_id)
                    || exact
                        .record_for_target(memoria_query::CandidateTarget {
                            space_id: membership.space_id,
                            memory_id: membership.memory_id,
                            revision_id: membership.revision_id,
                        })
                        .is_none()
                {
                    continue;
                }
                let payload =
                    VectorPayloadV1::get(self.layout.derived_dir(), membership.payload_hash)?;
                vectors_by_target
                    .entry((
                        membership.space_id,
                        membership.memory_id,
                        membership.revision_id,
                    ))
                    .or_insert_with(|| payload.values().to_vec());
            }
        }

        let mut candidates = BTreeMap::<TagId, TagVectorCandidate>::new();
        for membership in memberships {
            let key = (
                membership.space_id,
                membership.memory_id,
                membership.revision_id,
            );
            let Some(vector) = vectors_by_target.get(&key) else {
                continue;
            };
            let provenance = match membership.provenance {
                TagProvenance::Explicit => TagSeedProvenance::ExactSupport,
                TagProvenance::Generated => TagSeedProvenance::Generated,
            };
            let candidate = TagVectorCandidate {
                tag_id: membership.tag_id,
                vector: vector.clone(),
                provenance,
                score: provenance.weight(),
            };
            candidates
                .entry(candidate.tag_id)
                .and_modify(|existing| {
                    if candidate.score > existing.score {
                        *existing = candidate.clone();
                    }
                })
                .or_insert(candidate);
        }
        Ok(candidates.into_values().collect())
    }

    fn lexical_handle(
        &mut self,
        manifest_id: ManifestId,
    ) -> Result<&LexicalArtifactHandle, RuntimeError> {
        if !self.lexical_handles.contains_key(&manifest_id) {
            let record = self.derived.lexical_artifact_for_manifest(manifest_id)?;
            let handle =
                LexicalArtifactHandle::open(self.layout.derived_dir().join(record.object_path))?;
            if *handle.hash().as_bytes() != record.object_hash {
                return Err(RuntimeError::Derived(
                    memoria_derived::DerivedError::InvalidProjectionValue {
                        value: format!("lexical artifact hash mismatch for manifest {manifest_id}"),
                    },
                ));
            }
            self.lexical_handles.insert(manifest_id, handle);
        }
        Ok(self
            .lexical_handles
            .get(&manifest_id)
            .expect("lexical handle was inserted or cached"))
    }

    fn semantic_vector_hits(
        &self,
        compiled: &memoria_query::CompiledQuery,
        query_vector: &[f32],
        exact: &ExactIndex,
        limit: usize,
    ) -> Result<Vec<memoria_derived::VectorHit>, RuntimeError> {
        let manifest = self.derived.manifest(compiled.snapshot.derived_manifest)?;
        let mut hits = Vec::new();
        for artifact_id in manifest.artifacts() {
            let artifact = self.derived.artifact(artifact_id)?;
            if !artifact.is_compatible_semantic() {
                continue;
            }
            let tombstones = self
                .derived
                .ann_tombstones(artifact_id)?
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>();
            let segment_records = self.derived.ann_segments_for_artifact(artifact_id)?;
            let mut segments = Vec::with_capacity(segment_records.len());
            for record in segment_records.iter().rev() {
                segments.push(memoria_derived::AnnSegmentV1::get(
                    self.layout.derived_dir(),
                    record.object_hash,
                )?);
            }
            let references = segments.iter().collect::<Vec<_>>();
            hits.extend(memoria_derived::AnnSegmentV1::merge_search(
                references,
                &tombstones,
                query_vector,
                limit,
                &VectorFilter::any(),
            )?);
        }
        hits.retain(|hit| {
            let membership = hit.membership();
            exact
                .record_for_target(memoria_query::CandidateTarget {
                    space_id: membership.space_id(),
                    memory_id: membership.memory_id(),
                    revision_id: membership.revision_id(),
                })
                .is_some()
        });
        Ok(hits)
    }

    pub fn submit_feedback(
        &mut self,
        submission: FeedbackSubmission,
    ) -> Result<FeedbackCommit, RuntimeError> {
        self.ensure_open()?;
        let now = memoria_types::Timestamp::now()?;
        let inputs = if let Some(receipt) = self.receipts.get(&submission.retrieval_id) {
            receipt.resolve_feedback(&submission, now)?
        } else {
            submission
                .events
                .iter()
                .map(|event| {
                    let key = format!("{}:{}", submission.idempotency_key, event.result_id);
                    let input = self
                        .adaptive_log
                        .input_for_idempotency_key(&key)
                        .ok_or_else(|| ReceiptError::NotFound {
                            retrieval_id: submission.retrieval_id.clone(),
                        })?;
                    if input.retrieval_id != submission.retrieval_id
                        || input.outcome != event.outcome
                    {
                        return Err(ReceiptError::NotFound {
                            retrieval_id: submission.retrieval_id.clone(),
                        });
                    }
                    Ok(input)
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        let committed = self.adaptive_log.append_batch(inputs)?;
        Ok(FeedbackCommit {
            generation: committed.generation,
            events: committed.events,
        })
    }

    pub fn open_read_session(
        &mut self,
        query: MemoryQuery,
        ttl: std::time::Duration,
    ) -> Result<ReadSession, RuntimeError> {
        self.ensure_open()?;
        query.validate()?;
        let generation = self.authority.current_generation().map_err(|error| {
            RuntimeError::AuthorityDatabase {
                message: error.to_string(),
            }
        })?;
        let manifest = self.derived.serving_manifest().ok().flatten().ok_or(
            RuntimeError::AuthorityDatabase {
                message: "no Derived Manifest is serving".to_owned(),
            },
        )?;
        let compiled = QueryCompiler::new(generation, Some(manifest)).compile(query)?;
        ReadSession::open(&mut self.derived, &compiled, ttl).map_err(|error| {
            RuntimeError::AuthorityDatabase {
                message: error.to_string(),
            }
        })
    }

    #[must_use]
    pub fn status(&self) -> RuntimeStatus {
        self.query_operations.cleanup(std::time::Instant::now());
        let authority_generation = self
            .authority
            .current_generation()
            .unwrap_or_else(|_| AuthorityGeneration::initial());
        let (base_coverage, semantic_coverage, last_error) = self
            .derived
            .serving_manifest()
            .ok()
            .flatten()
            .map(|manifest| {
                let semantic = manifest.capability("semantic");
                (
                    manifest.capability("base-search").coverage(),
                    semantic.serving_coverage(),
                    self.last_error.clone(),
                )
            })
            .unwrap_or((
                AuthorityGeneration::initial(),
                AuthorityGeneration::initial(),
                self.last_error.clone(),
            ));
        RuntimeStatus {
            authority_generation,
            base_coverage,
            semantic_coverage,
            semantic_build_coverage: self.semantic_build_coverage,
            active_read_leases: 0,
            closed: self.closed,
            last_error,
        }
    }

    pub fn provider_poll_work(&mut self) -> Result<Option<NeedWork>, RuntimeError> {
        self.ensure_open()?;
        if let Some(pending) = self.pending_provider_work.front()
            && !self
                .provider_egress_policy
                .allows(pending.work.capability())
        {
            return Err(RuntimeError::ProviderEgressDenied {
                capability: pending.work.capability(),
            });
        }
        if let Some(pending) = self.pending_provider_work.front() {
            let capability = pending.work.capability();
            if pending.work.route().capability != capability
                || !matches!(
                    resolve_provider_route(pending.work.space_policy(), pending.work.route(), true),
                    ProviderRouteDecision::Allowed(_)
                )
            {
                return Err(RuntimeError::ProviderPolicyDenied { capability });
            }
        }
        let Some(pending) = self.pending_provider_work.pop_front() else {
            return Ok(None);
        };
        if let Some(job_id) = &pending.job_id {
            self.derived.mark_build_job_running(job_id)?;
        }
        let work_id = work_id(&pending.work).to_owned();
        let metadata = match &pending.work {
            NeedWork::Embeddings(_) => InflightProviderWork {
                generation: pending.generation,
                expected_work: pending.work.clone(),
                advances_semantic: true,
                embedding_projection: pending.embedding_projection.clone(),
                enrichment_projection: None,
                job_id: pending.job_id.clone(),
            },
            NeedWork::Enrichment(request) => InflightProviderWork {
                generation: pending.generation,
                expected_work: pending.work.clone(),
                advances_semantic: false,
                embedding_projection: None,
                enrichment_projection: Some(request.projection.clone()),
                job_id: pending.job_id.clone(),
            },
            NeedWork::Rerank(_) => InflightProviderWork {
                generation: pending.generation,
                expected_work: pending.work.clone(),
                advances_semantic: false,
                embedding_projection: None,
                enrichment_projection: None,
                job_id: pending.job_id.clone(),
            },
        };
        self.inflight_provider_work.insert(work_id, metadata);
        Ok(Some(pending.work))
    }

    pub fn provider_submit_result(
        &mut self,
        result: ProviderWorkResult,
    ) -> Result<(), RuntimeError> {
        self.ensure_open()?;
        let Some(inflight) = self.inflight_provider_work.get(result.work_id()) else {
            return Err(RuntimeError::UnexpectedProviderWork {
                work_id: result.work_id().to_owned(),
            });
        };
        let result =
            validate_provider_result(&inflight.expected_work, result).map_err(|error| {
                RuntimeError::InvalidProviderResult {
                    work_id: work_id(&inflight.expected_work).to_owned(),
                    message: error.to_string(),
                }
            })?;
        let pending = self
            .inflight_provider_work
            .remove(result.work_id())
            .ok_or_else(|| RuntimeError::UnexpectedProviderWork {
                work_id: result.work_id().to_owned(),
            })?;
        if let Some(job_id) = pending.job_id.as_deref()
            && self.derived.build_job(job_id)?.state == BuildJobState::Superseded
        {
            if pending.advances_semantic {
                if let Some(count) = self.pending_by_generation.get_mut(&pending.generation) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.pending_by_generation.remove(&pending.generation);
                    }
                }
                self.advance_semantic_build_coverage();
            }
            return Ok(());
        }
        match result {
            ProviderWorkResult::Enrichment { tags, .. } => {
                let Some(projection) = pending.enrichment_projection else {
                    return Err(RuntimeError::InvalidProviderResult {
                        work_id: work_id(&pending.expected_work).to_owned(),
                        message: "enrichment result did not have an enrichment request".to_owned(),
                    });
                };
                let artifact = GeneratedTagArtifact::from_candidates(
                    projection,
                    &mut self.tag_dictionary,
                    tags,
                )?;
                for record in artifact.records() {
                    self.derived
                        .register_tag_dictionary_value(record.tag_id, &record.value)?;
                    self.derived.insert_tag_membership(
                        record.space_id,
                        record.memory_id,
                        record.revision_id,
                        record.tag_id,
                        &record.value,
                        record.semantic_node_id.as_deref(),
                        record.provenance,
                        Some(&record.producer_signature),
                        Some(record.projection_input_hash.clone()),
                        record.score,
                        record.confidence,
                    )?;
                }
                self.generated_tag_artifacts
                    .insert(artifact.projection_input_hash().clone(), artifact);
                if let Some(job_id) = pending.job_id.as_deref() {
                    self.derived.mark_build_job_succeeded(job_id)?;
                }
            }
            ProviderWorkResult::Embeddings { vectors, .. } => {
                if pending.advances_semantic {
                    let projection = pending.embedding_projection.as_ref().ok_or_else(|| {
                        RuntimeError::InvalidProviderResult {
                            work_id: work_id(&pending.expected_work).to_owned(),
                            message: "embedding work did not retain its projection identity"
                                .to_owned(),
                        }
                    })?;
                    self.persist_embedding_result(
                        pending.generation,
                        &pending.expected_work,
                        projection,
                        &vectors,
                        pending.job_id.as_deref(),
                    )?;
                    if let Some(count) = self.pending_by_generation.get_mut(&pending.generation) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            self.pending_by_generation.remove(&pending.generation);
                        }
                    }
                    self.advance_semantic_build_coverage();
                }
            }
            ProviderWorkResult::Rerank { .. } => {}
            ProviderWorkResult::Failure {
                work_id,
                retryable,
                code,
                message,
            } => {
                if let Some(job_id) = pending.job_id.as_deref() {
                    self.derived
                        .mark_build_job_failed(job_id, &code, &message, retryable)?;
                }
                self.last_error = Some(format!(
                    "provider failure for `{work_id}` ({code}, retryable={retryable}): {message}"
                ));
            }
        }
        Ok(())
    }

    fn rebuild_base_for_space(
        &mut self,
        space_id: SpaceId,
        generation: AuthorityGeneration,
        provider_policy: SpaceProviderPolicy,
    ) {
        match self.try_rebuild_base_for_space(space_id, generation) {
            Ok(report) => {
                self.enqueue_enrichment_work(
                    generation,
                    report.enrichment_projections(),
                    provider_policy,
                );
            }
            Err(error) => {
                self.last_error = Some(error.to_string());
            }
        }
    }

    fn recover_after_open(&mut self) -> Result<(), RuntimeError> {
        let purge_operations = self.authority.list_purge_operations()?;
        for operation in purge_operations {
            let plan = purge_plan_from_authority(operation)?;
            let plan_id = plan.id.clone();
            let state = plan.state;
            let memory_id = plan.memory_id;
            self.purge.restore(plan);
            match state {
                PurgeState::Planned => {
                    // Planning is durable, but remains an explicit admin action.
                    // Reopening validates the target without silently starting a
                    // destructive operation.
                    self.authority.get_memory(memory_id)?;
                }
                PurgeState::Committed | PurgeState::Cleaning => {
                    self.execute_purge(&plan_id)?;
                }
                PurgeState::Completed => {}
            }
        }
        let generation = self.authority_generation()?;
        let spaces = self.authority.list_active_space_ids_at(generation)?;
        let serving_manifest = self.derived.serving_manifest()?;
        let base_ready = serving_manifest.as_ref().is_some_and(|manifest| {
            manifest.authority_generation() >= generation
                && manifest.capability("base-search").is_ready()
        });
        let semantic_ready = serving_manifest.as_ref().is_some_and(|manifest| {
            manifest.authority_generation() >= generation
                && manifest.capability("semantic").is_ready()
        });
        for space_id in spaces {
            let policy = self.space_provider_policy_at(space_id, generation)?;
            if !base_ready {
                self.rebuild_base_for_space(space_id, generation, policy);
            }
            if semantic_ready || policy.embedding == SpaceProviderMode::Deny {
                continue;
            }
            for read in self
                .authority
                .list_memories_at(&self.cas, space_id, generation)?
            {
                let projection = local_embedding_projection(&read.source)?;
                self.enqueue_embedding_work(read.memory.memory_id, generation, projection, policy);
            }
        }
        Ok(())
    }

    fn collect_unreferenced_source_objects(&self) -> Result<(), RuntimeError> {
        let entries = fs::read_dir(self.layout.objects_dir())?;
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Ok(hash) = name.parse() else {
                continue;
            };
            if !self.authority.source_blob_is_referenced(hash)? {
                let _ = self.cas.remove_if_exists(hash)?;
            }
        }
        Ok(())
    }

    fn requires_content_embedding(&self, previous_source: &[u8], source: &[u8]) -> bool {
        let Ok(previous_source) = std::str::from_utf8(previous_source) else {
            return true;
        };
        let Ok(previous_ir) = compile_ir(previous_source) else {
            return true;
        };
        let Ok(source) = std::str::from_utf8(source) else {
            return true;
        };
        let Ok(source_ir) = compile_ir(source) else {
            return true;
        };
        let diff = SemanticDiff::between(&previous_ir, &source_ir);
        self.compiler
            .plan(&diff)
            .rebuilds(ProjectionKind::LocalEmbedding)
    }

    fn enqueue_enrichment_work(
        &mut self,
        generation: AuthorityGeneration,
        projections: &[EnrichmentProjection],
        space_policy: SpaceProviderPolicy,
    ) {
        let route = self
            .provider_routes
            .route(ProviderCapability::Enrichment)
            .clone();
        let ProviderRouteDecision::Allowed(route) =
            resolve_provider_route(space_policy, &route, false)
        else {
            return;
        };
        for (index, projection) in projections.iter().enumerate() {
            let input_hash = hex_lower(projection.input_hash().as_bytes());
            let job_id = match self.derived.enqueue_build_job(
                "enrichment",
                &input_hash,
                projection.producer_signature(),
                generation,
            ) {
                Ok(job) => Some(job.job_id),
                Err(error) => {
                    self.last_error = Some(error.to_string());
                    continue;
                }
            };
            let work = NeedWork::Enrichment(crate::EnrichmentBatchRequest {
                work_id: format!("TG_{}_{}_{}", projection.memory_id(), generation, index),
                signature: projection.producer_signature().to_owned(),
                route: route.clone(),
                projection: projection.clone(),
                space_policy,
            });
            self.pending_provider_work.push_back(PendingProviderWork {
                generation,
                work,
                embedding_projection: None,
                job_id,
            });
        }
    }

    fn enqueue_embedding_work(
        &mut self,
        memory_id: MemoryId,
        generation: AuthorityGeneration,
        projection: LocalEmbeddingProjectionV1,
        space_policy: SpaceProviderPolicy,
    ) {
        let route = self
            .provider_routes
            .route(ProviderCapability::Embedding)
            .clone();
        let ProviderRouteDecision::Allowed(route) =
            resolve_provider_route(space_policy, &route, false)
        else {
            return;
        };
        let input_hash = hex_lower(projection.input_hash().as_bytes());
        let job = match self.derived.enqueue_build_job(
            "embedding",
            &input_hash,
            projection.producer_signature(),
            generation,
        ) {
            Ok(job) => job,
            Err(error) => {
                self.last_error = Some(error.to_string());
                return;
            }
        };
        if matches!(
            job.state,
            BuildJobState::Succeeded | BuildJobState::Superseded
        ) {
            return;
        }
        let work_id = format!("EW_{memory_id}_{generation}");
        let work = NeedWork::Embeddings(crate::EmbeddingBatchRequest {
            work_id,
            signature: projection.producer_signature().to_owned(),
            route,
            dimensions: 3,
            items: vec![crate::EmbeddingItem {
                key: memory_id.to_string(),
                text: projection.content().to_owned(),
            }],
            space_policy,
        });
        self.pending_provider_work.push_back(PendingProviderWork {
            generation,
            work,
            embedding_projection: Some(projection),
            job_id: Some(job.job_id),
        });
        *self.pending_by_generation.entry(generation).or_default() += 1;
    }

    fn supersede_embedding_work_for_memory(
        &mut self,
        memory_id: MemoryId,
    ) -> Result<(), RuntimeError> {
        let mut retained = VecDeque::with_capacity(self.pending_provider_work.len());
        while let Some(pending) = self.pending_provider_work.pop_front() {
            let is_target = matches!(
                &pending.work,
                NeedWork::Embeddings(request)
                    if request.items.iter().any(|item| item.key == memory_id.to_string())
            );
            if is_target {
                if let Some(job_id) = pending.job_id.as_deref() {
                    self.derived.mark_build_job_superseded(job_id)?;
                }
                if let Some(count) = self.pending_by_generation.get_mut(&pending.generation) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.pending_by_generation.remove(&pending.generation);
                    }
                }
            } else {
                retained.push_back(pending);
            }
        }
        self.pending_provider_work = retained;
        let inflight_job_ids = self
            .inflight_provider_work
            .values()
            .filter_map(|inflight| {
                let is_target = matches!(
                    &inflight.expected_work,
                    NeedWork::Embeddings(request)
                        if request.items.iter().any(|item| item.key == memory_id.to_string())
                );
                is_target.then(|| inflight.job_id.clone()).flatten()
            })
            .collect::<Vec<_>>();
        for job_id in inflight_job_ids {
            self.derived.mark_build_job_superseded(&job_id)?;
        }
        Ok(())
    }

    fn persist_embedding_result(
        &mut self,
        generation: AuthorityGeneration,
        expected_work: &NeedWork,
        projection: &LocalEmbeddingProjectionV1,
        vectors: &[crate::EmbeddingVector],
        job_id: Option<&str>,
    ) -> Result<(), RuntimeError> {
        let NeedWork::Embeddings(request) = expected_work else {
            return Err(RuntimeError::InvalidProviderResult {
                work_id: work_id(expected_work).to_owned(),
                message: "embedding result was paired with non-embedding work".to_owned(),
            });
        };

        let first_vector = vectors
            .first()
            .ok_or_else(|| RuntimeError::InvalidProviderResult {
                work_id: request.work_id.clone(),
                message: "embedding result contained no vectors".to_owned(),
            })?;
        let memory_id = first_vector.key.parse::<MemoryId>()?;
        let original_memory = self.authority.get_memory_at(memory_id, generation)?;
        let latest_generation = self.authority_generation()?;
        let latest_read = match self.authority.read_memory(&self.cas, memory_id) {
            Ok(read) => read,
            Err(MemoriaError::NotFound { .. }) => {
                if let Some(job_id) = job_id {
                    self.derived.mark_build_job_superseded(job_id)?;
                }
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };
        let latest_projection = local_embedding_projection(&latest_read.source)?;
        let decision = semantic_publication_target(
            &EmbeddingBuildIdentity {
                authority_generation: generation,
                memory_id: original_memory.memory_id,
                revision_id: original_memory.head_revision_id,
                space_id: original_memory.space_id,
                projection_input_hash: projection.input_hash().clone(),
            },
            &LatestMemoryState {
                authority_generation: latest_generation,
                memory_id: latest_read.memory.memory_id,
                revision_id: latest_read.memory.head_revision_id,
                space_id: latest_read.memory.space_id,
                projection_input_hash: latest_projection.input_hash().clone(),
            },
        );
        let (publication_generation, target_memory_id, target_revision_id) = match decision {
            SemanticPublicationDecision::PublishAt {
                authority_generation,
                memory_id,
                revision_id,
            } => (authority_generation, memory_id, revision_id),
            SemanticPublicationDecision::Superseded => {
                if let Some(job_id) = job_id {
                    self.derived.mark_build_job_superseded(job_id)?;
                }
                return Ok(());
            }
        };

        let mut persisted = Vec::with_capacity(vectors.len());
        for vector in vectors {
            let vector_memory_id = vector.key.parse::<MemoryId>()?;
            if vector_memory_id != target_memory_id {
                return Err(RuntimeError::InvalidProviderResult {
                    work_id: request.work_id.clone(),
                    message: format!(
                        "embedding result key `{}` does not match target memory `{target_memory_id}`",
                        vector.key
                    ),
                });
            }
            let payload = VectorPayloadV1::new(
                vector.values.clone(),
                EmbeddingNormalization::L2,
                request.signature.as_bytes(),
                projection.input_hash().clone(),
            )?;
            let payload_hash = payload.put(self.layout.derived_dir())?;
            let payload_bytes = payload.canonical_bytes();
            let payload_hex = payload_hash.as_hex();
            self.derived.register_vector_payload(&VectorPayloadRecord {
                payload_hash,
                dimension: payload.dimension(),
                normalization: payload.normalization(),
                producer_signature: request.signature.clone(),
                projection_input_hash: projection.input_hash().clone(),
                object_path: format!("objects/vector/{}/{}.vec", &payload_hex[..2], payload_hex),
                byte_length: u64::try_from(payload_bytes.len()).map_err(|_| {
                    memoria_derived::DerivedError::InvalidProjectionValue {
                        value: "vector payload byte length is out of range".to_owned(),
                    }
                })?,
                checksum: *payload_hash.as_bytes(),
                created_at: 0,
            })?;
            persisted.push((payload_hash, vector.values.clone()));
        }

        let semantic_artifact = self.derived.stage_artifact(
            SEMANTIC_ARTIFACT_KIND,
            SEMANTIC_ARTIFACT_VERSION,
            generation,
        )?;
        let mut ann_entries = Vec::with_capacity(persisted.len());
        for (payload_hash, values) in persisted {
            self.derived.insert_vector_membership(
                semantic_artifact.id(),
                latest_read.memory.space_id,
                target_memory_id,
                target_revision_id,
                format!("memory:{target_memory_id}"),
                None,
                "leaf",
                payload_hash,
                publication_generation,
            )?;
            ann_entries.push(AnnSegmentEntry::new(
                format!("memory:{target_memory_id}"),
                VectorMembership::from_parts(
                    latest_read.memory.space_id,
                    target_memory_id,
                    target_revision_id,
                    publication_generation,
                    *payload_hash.as_bytes(),
                ),
                values,
            )?);
        }
        let input_hash = hex_lower(projection.input_hash().as_bytes());
        let job = self.derived.enqueue_build_job(
            "embedding",
            &input_hash,
            &request.signature,
            generation,
        )?;
        self.derived.mark_build_job_succeeded(&job.job_id)?;
        let ann_job = self.derived.enqueue_build_job(
            "ann-segment",
            input_hash,
            &request.signature,
            generation,
        )?;
        let segment = AnnSegmentV1::build(request.signature.clone(), ann_entries)?;
        let object_hash = segment.put(self.layout.derived_dir())?;
        self.derived.register_ann_segment(
            semantic_artifact.id(),
            object_hash,
            u64::try_from(segment.vector_count()).map_err(memoria_derived::DerivedError::from)?,
            segment.dimension(),
            segment.producer_signature(),
        )?;
        self.derived.validate_artifact(semantic_artifact.id())?;
        self.derived.mark_build_job_succeeded(&ann_job.job_id)?;
        if let Some(manifest) = self.derived.serving_manifest()?
            && manifest.authority_generation() >= publication_generation
        {
            let mut artifact_ids = manifest.artifacts_with(semantic_artifact.id());
            artifact_ids.retain(|artifact_id| {
                *artifact_id == semantic_artifact.id()
                    || self
                        .derived
                        .artifact(*artifact_id)
                        .is_ok_and(|artifact| artifact.kind() != SEMANTIC_ARTIFACT_KIND)
            });
            self.derived.publish_manifest_rebased_at_generation(
                artifact_ids,
                manifest.authority_generation(),
                Vec::new(),
            )?;
        }
        Ok(())
    }

    fn advance_semantic_build_coverage(&mut self) {
        let mut candidate = self.semantic_build_coverage.next();
        while candidate <= self.authority_generation_or_initial() {
            if self
                .pending_by_generation
                .get(&candidate)
                .is_some_and(|count| *count > 0)
            {
                break;
            }
            self.semantic_build_coverage = candidate;
            candidate = candidate.next();
        }
    }

    fn authority_generation_or_initial(&self) -> AuthorityGeneration {
        self.authority_generation()
            .unwrap_or_else(|_| AuthorityGeneration::initial())
    }

    fn authority_generation(&self) -> Result<AuthorityGeneration, RuntimeError> {
        self.authority
            .current_generation()
            .map_err(|error| RuntimeError::AuthorityDatabase {
                message: error.to_string(),
            })
    }

    fn space_provider_policy_at(
        &self,
        space_id: SpaceId,
        generation: AuthorityGeneration,
    ) -> Result<SpaceProviderPolicy, RuntimeError> {
        Ok(self
            .authority
            .get_space_at(space_id, generation)?
            .provider_policy)
    }

    fn space_provider_policy_at_scope(
        &self,
        space_ids: &[SpaceId],
        generation: AuthorityGeneration,
    ) -> Result<SpaceProviderPolicy, RuntimeError> {
        let mut policy = SpaceProviderPolicy::default();
        for space_id in space_ids {
            policy = policy.most_restrictive(self.space_provider_policy_at(*space_id, generation)?);
        }
        Ok(policy)
    }

    fn try_rebuild_base_for_space(
        &mut self,
        space_id: SpaceId,
        generation: AuthorityGeneration,
    ) -> Result<BaseReadyReport, RuntimeError> {
        let reads = self
            .authority
            .list_memories_at(&self.cas, space_id, generation)?;
        let mut documents = Vec::with_capacity(reads.len());
        for read in &reads {
            let source = String::from_utf8(read.source.clone())?;
            documents.push(LexicalDocument::new(
                space_id,
                read.memory.memory_id,
                read.memory.head_revision_id,
                compile_ir(&source)?,
            ));
        }
        for document in &documents {
            let target = memoria_derived::ProjectionTarget {
                space_id: Some(document.space_id()),
                memory_id: Some(document.memory_id()),
                revision_id: Some(document.revision_id()),
            };
            for membership in ExplicitTagBuilder::build_for(document.ir(), target)?.memberships() {
                let tag_id = self.tag_dictionary.intern(&membership.value)?;
                let normalized = self
                    .tag_dictionary
                    .value(tag_id)
                    .ok_or_else(|| memoria_derived::DerivedError::InvalidProjectionValue {
                        value: "Tag dictionary lost explicit Tag identity".to_owned(),
                    })?
                    .to_owned();
                self.derived.insert_tag_membership(
                    document.space_id(),
                    document.memory_id(),
                    document.revision_id(),
                    tag_id,
                    normalized,
                    membership.node_id.as_deref(),
                    membership.provenance,
                    None,
                    None,
                    None,
                    None,
                )?;
            }
        }
        Ok(self
            .compiler
            .compile_base(&mut self.derived, generation, documents)?)
    }

    fn records_for_query(
        &mut self,
        query: &MemoryQuery,
        generation: AuthorityGeneration,
    ) -> Result<Vec<ExactRecord>, RuntimeError> {
        if matches!(query.history.mode, memoria_query::QueryHistoryMode::Current) {
            let manifest = match self.derived.serving_manifest()? {
                Some(manifest) => manifest,
                None => {
                    let has_active_memory =
                        query
                            .scope
                            .spaces
                            .iter()
                            .try_fold(false, |found, space_id| {
                                self.authority
                                    .has_active_memories_at(*space_id, generation)
                                    .map(|active| found || active)
                            })?;
                    if !has_active_memory {
                        return Ok(Vec::new());
                    }
                    return Err(RuntimeError::Query(QueryError::CapabilityNotReady {
                        capability: "base-search".to_owned(),
                        required: generation,
                        available: AuthorityGeneration::initial(),
                    }));
                }
            };
            let base = manifest.capability("base-search");
            if !base.is_ready() || base.coverage() < generation {
                return Err(RuntimeError::Query(QueryError::CapabilityNotReady {
                    capability: "base-search".to_owned(),
                    required: generation,
                    available: base.coverage(),
                }));
            }
            return self
                .derived
                .serving_records_for_spaces(&query.scope.spaces, generation)?
                .into_iter()
                .map(|record| {
                    let entities = record
                        .entity_refs
                        .iter()
                        .map(|entity| {
                            memoria_query::EntityRef::new(entity.clone()).map_err(|_| {
                                QueryError::InvalidEntityRef {
                                    value: entity.clone(),
                                }
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(ExactRecord::new(
                        record.space_id,
                        record.memory_id,
                        record.revision_id,
                        record.text,
                    )
                    .with_entities(entities)
                    .with_tags(record.tags)
                    .with_node_ids(record.node_ids)
                    .with_relations(record.relations)
                    .with_authority_generation(record.authority_generation)
                    .with_current(record.current)
                    .with_retired(record.retired))
                })
                .collect();
        }

        let mut records = Vec::new();
        for space_id in &query.scope.spaces {
            let reads = self
                .authority
                .list_memories_at(&self.cas, *space_id, generation)?;
            for read in reads {
                let source = String::from_utf8(read.source)?;
                let ir = compile_ir(&source)?;
                let target = memoria_derived::ProjectionTarget {
                    space_id: Some(*space_id),
                    memory_id: Some(read.memory.memory_id),
                    revision_id: Some(read.memory.head_revision_id),
                };
                let entities = EntityObservationBuilder::build_for(&ir, target)?
                    .observations()
                    .iter()
                    .map(|observation| {
                        memoria_query::EntityRef::new(observation.entity_ref.as_str()).map_err(
                            |_| QueryError::InvalidEntityRef {
                                value: observation.entity_ref.to_string(),
                            },
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let tags = ExplicitTagBuilder::build_for(&ir, target)?
                    .memberships()
                    .iter()
                    .map(|membership| membership.value.clone())
                    .collect();
                let node_ids = ir
                    .nodes()
                    .filter_map(|node| node.id().map(ToString::to_string))
                    .collect();
                let relations = memoria_derived::RelationBuilder::build_for(&ir, target)?
                    .relations()
                    .iter()
                    .map(|relation| relation.kind.clone())
                    .chain(ir.nodes().filter_map(|node| {
                        if node.kind() != memoria_mdx::SemanticKind::MemoryRef {
                            return None;
                        }
                        ["memoryId", "memory_id", "ref"]
                            .into_iter()
                            .find_map(|name| {
                                node.attributes()
                                    .iter()
                                    .find(|(candidate, _)| candidate == name)
                                    .map(|(_, value)| format!("memory-ref:{value}"))
                            })
                    }))
                    .collect();
                records.push(
                    ExactRecord::new(
                        *space_id,
                        read.memory.memory_id,
                        read.memory.head_revision_id,
                        source,
                    )
                    .with_entities(entities)
                    .with_tags(tags)
                    .with_node_ids(node_ids)
                    .with_relations(relations)
                    .with_authority_generation(read.memory.generation)
                    .with_retired(read.memory.lifecycle == MemoryLifecycle::Retired),
                );
            }
        }
        Ok(records)
    }

    fn ensure_open(&self) -> Result<(), RuntimeError> {
        if self.closed {
            Err(RuntimeError::Closed)
        } else {
            Ok(())
        }
    }

    #[must_use]
    pub fn data_dir(&self) -> &std::path::Path {
        self.layout.store_dir()
    }
}

fn purge_plan_from_authority(operation: PurgeOperationRecord) -> Result<PurgePlan, RuntimeError> {
    let state = match operation.state.as_str() {
        "planned" => PurgeState::Planned,
        "committed" => PurgeState::Committed,
        "cleaning" => PurgeState::Cleaning,
        "completed" => PurgeState::Completed,
        value => {
            return Err(RuntimeError::AuthorityDatabase {
                message: format!("invalid purge operation state `{value}`"),
            });
        }
    };
    Ok(PurgePlan {
        id: operation.purge_id,
        memory_id: operation.memory_id,
        state,
    })
}

fn record_runtime_channel(
    trace: &mut QueryOperatorTrace,
    pool: &CandidatePool,
    channel: PhysicalChannel,
    name: &str,
) {
    *trace = std::mem::take(trace).with_channel(name, pool.keys_for_channel(channel).len());
}

fn unique_purge_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

fn local_embedding_projection(source: &[u8]) -> Result<LocalEmbeddingProjectionV1, RuntimeError> {
    let source = String::from_utf8(source.to_vec())?;
    let ir = compile_ir(&source)?;
    Ok(LocalEmbeddingProjectionV1::build(
        &ir,
        "memoria-embedding-v1",
    )?)
}

fn query_embedding_work(
    query: &MemoryQuery,
    space_policy: SpaceProviderPolicy,
    route: crate::privacy::ProviderRoute,
) -> QueryWork {
    let projection = QueryEmbeddingProjectionV1::build(query.cue.text.join("\n"));
    QueryWork::Embedding(EmbeddingBatchRequest {
        work_id: format!("QW_{}", MemoryId::new()),
        signature: "query-embedding-v1".to_owned(),
        route,
        dimensions: 3,
        items: vec![EmbeddingItem {
            key: "query".to_owned(),
            text: projection.content().to_owned(),
        }],
        space_policy,
    })
}

fn query_rerank_work(
    query: &MemoryQuery,
    batch: &RerankBatch,
    space_policy: SpaceProviderPolicy,
    route: crate::privacy::ProviderRoute,
) -> QueryWork {
    QueryWork::Rerank(RerankBatchRequest {
        work_id: format!("QW_{}", MemoryId::new()),
        signature: "query-rerank-v1".to_owned(),
        route,
        query: query.cue.text.join("\n"),
        candidates: batch.views.iter().map(|view| view.handle.clone()).collect(),
        space_policy,
    })
}

fn adaptive_signature(query: &MemoryQuery) -> QueryAdaptiveSignature {
    let mut hasher = Sha256::new();
    for cue in &query.cue.text {
        hasher.update(cue.as_bytes());
        hasher.update([0]);
    }
    QueryAdaptiveSignature {
        scope: query.scope.spaces.clone(),
        entity_refs: query.cue.entities.iter().map(ToString::to_string).collect(),
        explicit_tags: query.cue.tags.clone(),
        query_class: if query.constraints.valid_at.is_some() {
            Some("historical".to_owned())
        } else {
            Some("current".to_owned())
        },
        text_projection_hash: Some(format!("sha256:{}", hex_lower(&hasher.finalize()))),
        vector_identity: None,
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn work_id(work: &NeedWork) -> &str {
    match work {
        NeedWork::Embeddings(request) => &request.work_id,
        NeedWork::Rerank(request) => &request.work_id,
        NeedWork::Enrichment(request) => &request.work_id,
    }
}
