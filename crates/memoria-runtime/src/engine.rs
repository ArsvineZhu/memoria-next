use std::collections::{BTreeMap, VecDeque};
use std::string::FromUtf8Error;

use memoria_adaptive::{AdaptiveEventLog, AdaptiveStateV1, QueryAdaptiveSignature};
use memoria_authority::{AuthorityDb, MemoryLifecycle, SourceCas, StoreLayout, StoreWriterLock};
use memoria_derived::{
    BaseReadyReport, DerivedCatalog, DerivedCompiler, EnrichmentProjection,
    EntityObservationBuilder, ExplicitTagBuilder, GeneratedTagArtifact, LexicalDocument,
    ProjectionInputHash, ProjectionKind, TagDictionary,
};
use memoria_mdx::{SemanticDiff, compile_ir};
use memoria_query::{
    AdaptiveSnapshotIdentity, ExactIndex, ExactRecord, LexicalCandidate, LexicalCandidateIndex,
    MemoryQuery, QueryCompiler, QueryError, ReadSession, ReadinessBehavior, RetrievalResponse,
    assess, build_response, execute_exact, execute_lexical, rank_with_adaptive,
};
use memoria_types::{AuthorityGeneration, MemoriaError, MemoryId, RevisionId, SpaceId};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::limits::{ResourceLimits, check_source_bytes};
use crate::privacy::{ProviderCapability, ProviderEgressPolicy};
use crate::provider::{
    EmbeddingBatchRequest, EmbeddingItem, NeedWork, ProviderWorkResult, RerankBatchRequest,
    validate_provider_result,
};
use crate::purge::{PurgeCoordinator, PurgePlan, PurgeState};
use crate::query_operation::{QueryOperationTable, QueryStep, QueryWork};
use crate::receipt::{FeedbackCommit, FeedbackSubmission, ReceiptError, RetrievalReceipt};
use crate::status::RuntimeStatus;
use crate::transfer::PortableMemory;

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

    #[error("CAPABILITY_NOT_READY: provider data egress denied for {capability}")]
    ProviderEgressDenied { capability: ProviderCapability },

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
}

impl RuntimeError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Closed => "STORE_CLOSED",
            Self::Authority(error) => error.code(),
            Self::AuthorityDatabase { .. } | Self::Derived(_) => "STORE_CORRUPT",
            Self::Mdx(_) | Self::Utf8(_) => "INVALID_MDX",
            Self::Query(_) | Self::Consolidation(_) => "QUERY_ERROR",
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
            Self::ProviderEgressDenied { .. } => "CAPABILITY_NOT_READY",
            Self::PurgeConflict { .. } => "PURGE_CONFLICT",
            Self::ResourceLimit { .. } => "RESOURCE_LIMIT",
            Self::Receipt(ReceiptError::NotFound { .. }) => "NOT_FOUND",
            Self::Receipt(ReceiptError::Expired { .. }) => "FEEDBACK_RECEIPT_EXPIRED",
            Self::Receipt(_) => "ADAPTIVE_ERROR",
        }
    }
}

pub struct MemoriaRuntime {
    _writer_lock: StoreWriterLock,
    layout: StoreLayout,
    authority: AuthorityDb,
    cas: SourceCas,
    derived: DerivedCatalog,
    compiler: DerivedCompiler,
    pending_provider_work: VecDeque<PendingProviderWork>,
    inflight_provider_work: BTreeMap<String, InflightProviderWork>,
    pending_by_generation: BTreeMap<AuthorityGeneration, usize>,
    query_operations: QueryOperationTable,
    semantic_coverage: AuthorityGeneration,
    tag_dictionary: TagDictionary,
    generated_tag_artifacts: BTreeMap<ProjectionInputHash, GeneratedTagArtifact>,
    adaptive_log: AdaptiveEventLog,
    provider_egress_policy: ProviderEgressPolicy,
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
}

struct InflightProviderWork {
    generation: AuthorityGeneration,
    expected_work: NeedWork,
    advances_semantic: bool,
    enrichment_projection: Option<EnrichmentProjection>,
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
        Self::open_with_provider_egress_policy(data_dir, ProviderEgressPolicy::default())
    }

    pub fn open_with_provider_egress_policy(
        data_dir: impl AsRef<std::path::Path>,
        provider_egress_policy: ProviderEgressPolicy,
    ) -> Result<Self, RuntimeError> {
        let layout = StoreLayout::create(data_dir)?;
        let writer_lock = StoreWriterLock::acquire(layout.store_dir())?;
        let authority = AuthorityDb::open(layout.authority_database()).map_err(|error| {
            RuntimeError::AuthorityDatabase {
                message: error.to_string(),
            }
        })?;
        let derived = DerivedCatalog::open(layout.derived_dir().join("catalog.sqlite"))?;
        Ok(Self {
            _writer_lock: writer_lock,
            cas: SourceCas::new(&layout),
            layout,
            authority,
            derived,
            compiler: DerivedCompiler::default(),
            pending_provider_work: VecDeque::new(),
            inflight_provider_work: BTreeMap::new(),
            pending_by_generation: BTreeMap::new(),
            query_operations: QueryOperationTable::default(),
            semantic_coverage: AuthorityGeneration::initial(),
            tag_dictionary: TagDictionary::new(),
            generated_tag_artifacts: BTreeMap::new(),
            adaptive_log: AdaptiveEventLog::new(),
            provider_egress_policy,
            purge: PurgeCoordinator::new(),
            resource_limits: ResourceLimits::default(),
            receipts: BTreeMap::new(),
            next_retrieval_id: 0,
            closed: false,
            last_error: None,
        })
    }

    pub fn close(&mut self) -> Result<(), RuntimeError> {
        self.closed = true;
        self.query_operations = QueryOperationTable::default();
        Ok(())
    }

    pub fn create_space(&mut self, space_key: impl AsRef<str>) -> Result<SpaceId, RuntimeError> {
        self.ensure_open()?;
        Ok(self
            .authority
            .create_space(space_key)?
            .into_value()
            .space_id)
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
        let result = self
            .authority
            .create_memory(&self.cas, space_id, document_key, source)?;
        let memory_id = result.value().memory_id;
        self.enqueue_embedding_work(memory_id, result.generation(), source);
        self.rebuild_base_for_space(space_id, result.generation());
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
            self.enqueue_embedding_work(memory_id, result.generation(), source);
            self.rebuild_base_for_space(space_id, result.generation());
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

    pub fn plan_purge(&mut self, memory_id: MemoryId) -> Result<PurgePlan, RuntimeError> {
        self.ensure_open()?;
        self.authority.get_memory(memory_id)?;
        Ok(self.purge.plan(memory_id))
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
            self.purge
                .transition(plan_id, PurgeState::Planned, PurgeState::Committed)
                .map_err(|_| RuntimeError::PurgeConflict {
                    plan_id: plan_id.to_owned(),
                })?;
        }
        if self.purge.state(plan_id) == Some(PurgeState::Committed) {
            let hashes = self.authority.purge_memory(planned.memory_id)?;
            self.purge
                .transition(plan_id, PurgeState::Committed, PurgeState::Cleaning)
                .map_err(|_| RuntimeError::PurgeConflict {
                    plan_id: plan_id.to_owned(),
                })?;
            self.adaptive_log
                .rewrite_without_memory(planned.memory_id)?;
            self.receipts.clear();
            self.derived.delete_all_derived()?;
            for hash in hashes {
                if !self.authority.source_blob_is_referenced(hash)? {
                    let _ = self.cas.remove_if_exists(hash)?;
                }
            }
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
        let result = self
            .authority
            .revise_memory(&self.cas, memory_id, expected_head, source)?;
        let record = result.value();
        if requires_content_embedding {
            self.enqueue_embedding_work(record.memory_id, result.generation(), source);
        } else {
            self.advance_semantic_coverage();
        }
        self.rebuild_base_for_space(record.space_id, result.generation());
        Ok(MemoryMutation {
            memory_id: record.memory_id,
            space_id: record.space_id,
            revision_id: record.head_revision_id,
            generation: result.generation(),
        })
    }

    pub fn query(&mut self, query: MemoryQuery) -> Result<RetrievalResponse, RuntimeError> {
        match self.query_start(query)? {
            QueryStep::Complete(response) => Ok(response),
            QueryStep::Pending { operation_id, .. } => {
                Err(RuntimeError::QueryOperationPending { operation_id })
            }
        }
    }

    pub fn query_start(&mut self, query: MemoryQuery) -> Result<QueryStep, RuntimeError> {
        self.ensure_open()?;
        self.query_operations.cleanup(std::time::Instant::now());
        query.validate()?;

        let semantic_required = query
            .required_capabilities
            .iter()
            .any(|capability| capability == "semantic");
        let requested_semantic = semantic_required
            || query
                .preferred_capabilities
                .iter()
                .any(|capability| capability == "semantic");
        let compiled = match self.compile_query(query.clone()) {
            Ok(compiled) => compiled,
            Err(RuntimeError::Query(QueryError::CapabilityNotReady { capability, .. }))
                if capability == "semantic"
                    && semantic_required
                    && matches!(query.consistency.readiness, ReadinessBehavior::Wait(_)) =>
            {
                let mut fallback = query.clone();
                fallback
                    .required_capabilities
                    .retain(|capability| capability != "semantic");
                fallback
                    .preferred_capabilities
                    .retain(|capability| capability != "semantic");
                let mut compiled = self.compile_query(fallback)?;
                compiled.execution.degraded = true;
                if !compiled
                    .execution
                    .degraded_capabilities
                    .iter()
                    .any(|item| item == "semantic")
                {
                    compiled
                        .execution
                        .degraded_capabilities
                        .push("semantic".to_owned());
                }
                compiled
            }
            Err(error) => return Err(error),
        };

        let semantic_ready = compiled.execution.used("semantic");
        let semantic_pending = requested_semantic && (semantic_required || semantic_ready);
        let rerank_candidates = if compiled.execution.used("reranking") {
            query_rerank_candidates(&query)
        } else {
            Vec::new()
        };
        let work = if semantic_pending {
            Some(query_embedding_work(&query))
        } else if !rerank_candidates.is_empty() {
            Some(query_rerank_work(&query, rerank_candidates.clone()))
        } else {
            None
        };
        if let Some(work) = work {
            let operation_id =
                self.query_operations
                    .insert(compiled, work.clone(), rerank_candidates);
            return Ok(QueryStep::Pending { operation_id, work });
        }

        Ok(QueryStep::Complete(self.execute_compiled_query(compiled)?))
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
        if operation.work.work_id() != result.work_id() {
            return Err(RuntimeError::UnexpectedQueryWork {
                work_id: result.work_id().to_owned(),
            });
        }
        let expected_work = operation.work.as_need_work();
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
            return Err(RuntimeError::ProviderFailure {
                work_id,
                retryable,
                code,
                message,
            });
        }
        let operation = self.query_operations.remove(operation_id).ok_or_else(|| {
            RuntimeError::QueryOperationNotFound {
                operation_id: operation_id.to_owned(),
            }
        })?;
        if matches!(operation.work, QueryWork::Embedding(_))
            && !operation.rerank_candidates.is_empty()
        {
            let mut operation = operation;
            let candidates = std::mem::take(&mut operation.rerank_candidates);
            let work = query_rerank_work(&operation.compiled.query, candidates);
            operation.work = work.clone();
            self.query_operations
                .replace(operation_id.to_owned(), operation);
            self.query_operations.cleanup(now);
            return Ok(QueryStep::Pending {
                operation_id: operation_id.to_owned(),
                work,
            });
        }
        self.query_operations.cleanup(now);
        Ok(QueryStep::Complete(
            self.execute_compiled_query(operation.compiled)?,
        ))
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
        let serving_manifest = self.derived.serving_manifest()?;
        let has_real_manifest = serving_manifest.is_some();
        let manifest = serving_manifest.unwrap_or_else(|| {
            memoria_derived::DerivedManifest::empty_for_lexical_query(generation)
        });
        let semantic_artifact_ready = manifest.capability("semantic").is_ready();
        let compiler = QueryCompiler::new(generation, Some(manifest)).with_adaptive_snapshot(
            AdaptiveSnapshotIdentity::Enabled {
                generation: self.adaptive_log.current_generation(),
                model_version: "adaptive-v1".to_owned(),
            },
        );
        let compiler = if has_real_manifest && semantic_artifact_ready {
            compiler.with_semantic_coverage(self.semantic_coverage)
        } else {
            compiler
        };
        compiler.compile(query).map_err(RuntimeError::from)
    }

    fn execute_compiled_query(
        &mut self,
        compiled: memoria_query::CompiledQuery,
    ) -> Result<RetrievalResponse, RuntimeError> {
        let query_signature = adaptive_signature(&compiled.query);
        let records =
            self.records_for_query(&compiled.query, compiled.snapshot.authority_generation)?;
        let candidates = if compiled.query.cue.text.is_empty() {
            execute_exact(&compiled, &ExactIndex::new(records)).results
        } else {
            let lexical = records
                .iter()
                .map(|record| LexicalCandidate {
                    target: record.target,
                    score: text_score(record, &compiled.query.cue.text),
                    text: record.text.clone(),
                    entity_refs: record.entity_refs.clone(),
                    tags: record.tags.clone(),
                    current: record.current,
                    retired: record.retired,
                })
                .collect();
            execute_lexical(&compiled, &LexicalCandidateIndex::new(lexical)).results
        };
        self.next_retrieval_id = self.next_retrieval_id.saturating_add(1);
        let mut response = build_response(
            format!("RET_{}", self.next_retrieval_id),
            &compiled,
            candidates,
        )?;
        let now = memoria_types::Timestamp::now()?;
        let adaptive_state = AdaptiveStateV1::replay(self.adaptive_log.events());
        response.results =
            rank_with_adaptive(response.results, &adaptive_state, &query_signature, now);
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

    pub fn submit_feedback(
        &mut self,
        submission: FeedbackSubmission,
    ) -> Result<FeedbackCommit, RuntimeError> {
        self.ensure_open()?;
        let now = memoria_types::Timestamp::now()?;
        let receipt =
            self.receipts
                .get(&submission.retrieval_id)
                .ok_or_else(|| ReceiptError::NotFound {
                    retrieval_id: submission.retrieval_id.clone(),
                })?;
        let inputs = receipt.resolve_feedback(&submission, now)?;
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
        let compiled = QueryCompiler::new(generation, Some(manifest))
            .with_semantic_coverage(self.semantic_coverage)
            .compile(query)?;
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
        let (base_coverage, last_error) = self
            .derived
            .serving_manifest()
            .ok()
            .flatten()
            .map(|manifest| {
                (
                    manifest.capability("base-search").coverage(),
                    self.last_error.clone(),
                )
            })
            .unwrap_or((AuthorityGeneration::initial(), self.last_error.clone()));
        RuntimeStatus {
            authority_generation,
            base_coverage,
            semantic_coverage: self.semantic_coverage,
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
        let Some(pending) = self.pending_provider_work.pop_front() else {
            return Ok(None);
        };
        let work_id = work_id(&pending.work).to_owned();
        let metadata = match &pending.work {
            NeedWork::Embeddings(_) => InflightProviderWork {
                generation: pending.generation,
                expected_work: pending.work.clone(),
                advances_semantic: true,
                enrichment_projection: None,
            },
            NeedWork::Enrichment(request) => InflightProviderWork {
                generation: pending.generation,
                expected_work: pending.work.clone(),
                advances_semantic: false,
                enrichment_projection: Some(request.projection.clone()),
            },
            NeedWork::Rerank(_) => InflightProviderWork {
                generation: pending.generation,
                expected_work: pending.work.clone(),
                advances_semantic: false,
                enrichment_projection: None,
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
                self.generated_tag_artifacts
                    .insert(artifact.projection_input_hash().clone(), artifact);
            }
            ProviderWorkResult::Embeddings { .. } => {
                if pending.advances_semantic {
                    if let Some(count) = self.pending_by_generation.get_mut(&pending.generation) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            self.pending_by_generation.remove(&pending.generation);
                        }
                    }
                    self.advance_semantic_coverage();
                }
            }
            ProviderWorkResult::Rerank { .. } => {}
            ProviderWorkResult::Failure {
                work_id,
                retryable,
                code,
                message,
            } => {
                self.last_error = Some(format!(
                    "provider failure for `{work_id}` ({code}, retryable={retryable}): {message}"
                ));
            }
        }
        Ok(())
    }

    fn rebuild_base_for_space(&mut self, space_id: SpaceId, generation: AuthorityGeneration) {
        match self.try_rebuild_base_for_space(space_id, generation) {
            Ok(report) => {
                self.enqueue_enrichment_work(generation, report.enrichment_projections());
            }
            Err(error) => {
                self.last_error = Some(error.to_string());
            }
        }
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
    ) {
        for (index, projection) in projections.iter().enumerate() {
            let work = NeedWork::Enrichment(crate::EnrichmentBatchRequest {
                work_id: format!("TG_{}_{}_{}", projection.memory_id(), generation, index),
                signature: projection.producer_signature().to_owned(),
                projection: projection.clone(),
            });
            self.pending_provider_work
                .push_back(PendingProviderWork { generation, work });
        }
    }

    fn enqueue_embedding_work(
        &mut self,
        memory_id: MemoryId,
        generation: AuthorityGeneration,
        source: &[u8],
    ) {
        let work_id = format!("EW_{memory_id}_{generation}");
        let work = NeedWork::Embeddings(crate::EmbeddingBatchRequest {
            work_id,
            signature: "memoria-embedding-v1".to_owned(),
            dimensions: 3,
            items: vec![crate::EmbeddingItem {
                key: memory_id.to_string(),
                text: String::from_utf8_lossy(source).into_owned(),
            }],
        });
        self.pending_provider_work
            .push_back(PendingProviderWork { generation, work });
        *self.pending_by_generation.entry(generation).or_default() += 1;
    }

    fn advance_semantic_coverage(&mut self) {
        let mut candidate = self.semantic_coverage.next();
        while candidate <= self.authority_generation_or_initial() {
            if self
                .pending_by_generation
                .get(&candidate)
                .is_some_and(|count| *count > 0)
            {
                break;
            }
            self.semantic_coverage = candidate;
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
        Ok(self
            .compiler
            .compile_base(&mut self.derived, generation, documents)?)
    }

    fn records_for_query(
        &mut self,
        query: &MemoryQuery,
        generation: AuthorityGeneration,
    ) -> Result<Vec<ExactRecord>, RuntimeError> {
        let mut records = Vec::new();
        for space_id in &query.scope.spaces {
            let reads = self
                .authority
                .list_memories_at(&self.cas, *space_id, generation)?;
            if self.derived.serving_manifest()?.is_none_or(|manifest| {
                manifest.authority_generation() < generation
                    || !manifest.capability("base-search").is_ready()
            }) {
                let _ = self.try_rebuild_base_for_space(*space_id, generation)?;
            }
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
                        memoria_query::EntityRef::new(observation.entity_ref.as_str())
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

fn text_score(record: &ExactRecord, cues: &[String]) -> f32 {
    cues.iter()
        .filter(|cue| {
            record
                .text
                .to_ascii_lowercase()
                .contains(&cue.to_ascii_lowercase())
        })
        .count() as f32
}

fn query_embedding_work(query: &MemoryQuery) -> QueryWork {
    QueryWork::Embedding(EmbeddingBatchRequest {
        work_id: format!("QW_{}", MemoryId::new()),
        signature: "query-embedding-v1".to_owned(),
        dimensions: 3,
        items: vec![EmbeddingItem {
            key: "query".to_owned(),
            text: query.cue.text.join("\n"),
        }],
    })
}

fn query_rerank_work(query: &MemoryQuery, candidates: Vec<String>) -> QueryWork {
    QueryWork::Rerank(RerankBatchRequest {
        work_id: format!("QW_{}", MemoryId::new()),
        signature: "query-rerank-v1".to_owned(),
        query: query.cue.text.join("\n"),
        candidates,
    })
}

fn query_rerank_candidates(query: &MemoryQuery) -> Vec<String> {
    let mut candidates = query
        .cue
        .memories
        .iter()
        .map(|memory| memory.memory_id.to_string())
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.dedup();
    candidates
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
