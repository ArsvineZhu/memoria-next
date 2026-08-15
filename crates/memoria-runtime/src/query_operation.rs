use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use memoria_adaptive::AdaptiveReadSnapshot;
use memoria_query::{
    CandidateEvidence, CandidatePool, CompiledQuery, MemoryQuery, MemoryResult, PhysicalQueryPlan,
    QueryOperatorTrace, RerankBatch, RerankScore, RetrievalResponse, TagBasisResult,
};
use memoria_types::MemoryId;

use crate::privacy::ProviderRoute;
use crate::provider::{EmbeddingBatchRequest, NeedWork, RerankBatchRequest};
use moka::sync::Cache;

pub const READINESS_RETRY_AFTER_MS: u32 = 25;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryWork {
    Embedding(EmbeddingBatchRequest),
    Rerank(RerankBatchRequest),
}

impl QueryWork {
    #[must_use]
    pub fn work_id(&self) -> &str {
        match self {
            Self::Embedding(work) => &work.work_id,
            Self::Rerank(work) => &work.work_id,
        }
    }

    #[must_use]
    pub fn space_policy(&self) -> memoria_authority::SpaceProviderPolicy {
        match self {
            Self::Embedding(work) => work.space_policy,
            Self::Rerank(work) => work.space_policy,
        }
    }

    #[must_use]
    pub fn route(&self) -> &ProviderRoute {
        match self {
            Self::Embedding(work) => &work.route,
            Self::Rerank(work) => &work.route,
        }
    }

    #[must_use]
    pub(crate) fn as_need_work(&self) -> NeedWork {
        match self {
            Self::Embedding(work) => NeedWork::Embeddings(work.clone()),
            Self::Rerank(work) => NeedWork::Rerank(work.clone()),
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum QueryStep {
    Complete(RetrievalResponse),
    ProviderPending {
        operation_id: String,
        work: QueryWork,
    },
    ReadinessPending {
        operation_id: String,
        retry_after_ms: u32,
        deadline_unix_ms: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryOperationStage {
    Planned,
    WaitingForCapabilities,
    WaitingForQueryEmbedding,
    LocalCandidatesReady,
    CandidatesFused,
    WaitingForRerank,
    Reranked,
    Complete,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct QueryOperationState {
    pub(crate) plan: Option<PhysicalQueryPlan>,
    pub(crate) query_vector: Option<Vec<f32>>,
    pub(crate) tag_basis: Option<TagBasisResult>,
    pub(crate) candidate_pool: CandidatePool,
    pub(crate) fused_candidates: Vec<CandidateEvidence>,
    pub(crate) consolidated: Vec<MemoryResult>,
    pub(crate) trace: Option<QueryOperatorTrace>,
}

impl QueryOperationState {
    #[must_use]
    pub(crate) fn with_plan(plan: PhysicalQueryPlan) -> Self {
        Self {
            plan: Some(plan),
            ..Self::default()
        }
    }
}

pub(crate) struct QueryOperationPayload {
    pub(crate) pending_response: Option<RetrievalResponse>,
    pub(crate) rerank_batch: Option<RerankBatch>,
    pub(crate) adaptive_snapshot: Option<AdaptiveReadSnapshot>,
    pub(crate) state: QueryOperationState,
}

#[derive(Clone, Debug)]
pub struct QueryOperation {
    pub(crate) query: MemoryQuery,
    pub(crate) compiled: Option<CompiledQuery>,
    pub(crate) plan: Option<PhysicalQueryPlan>,
    pub(crate) stage: QueryOperationStage,
    pub(crate) work: Option<QueryWork>,
    pub(crate) pending_response: Option<RetrievalResponse>,
    pub(crate) query_vector: Option<Vec<f32>>,
    pub(crate) tag_basis: Option<TagBasisResult>,
    pub(crate) candidate_pool: CandidatePool,
    pub(crate) fused_candidates: Vec<CandidateEvidence>,
    pub(crate) consolidated: Vec<MemoryResult>,
    pub(crate) rerank_batch: Option<RerankBatch>,
    pub(crate) rerank_scores: Option<Vec<RerankScore>>,
    pub(crate) trace: Option<QueryOperatorTrace>,
    pub(crate) adaptive_snapshot: Option<AdaptiveReadSnapshot>,
    pub(crate) readiness_deadline: Option<Instant>,
    pub(crate) deadline_unix_ms: Option<u64>,
    pub(crate) cancelled: bool,
    pub(crate) expired_for_test: bool,
}

impl QueryOperation {
    #[must_use]
    pub fn is_expired(&self, _now: Instant) -> bool {
        self.expired_for_test
    }

    fn apply_state(&mut self, state: QueryOperationState) {
        self.plan = state.plan;
        self.query_vector = state.query_vector;
        self.tag_basis = state.tag_basis;
        self.candidate_pool = state.candidate_pool;
        self.fused_candidates = state.fused_candidates;
        self.consolidated = state.consolidated;
        self.trace = state.trace;
    }
}

#[derive(Debug)]
pub struct QueryOperationTable {
    operations: Cache<String, Arc<Mutex<QueryOperation>>>,
    expired_for_test: Mutex<HashSet<String>>,
}

impl Default for QueryOperationTable {
    fn default() -> Self {
        Self::from_cache(crate::cache::RuntimeCaches::new().query_operation_cache())
    }
}

impl QueryOperationTable {
    pub(crate) fn from_cache(operations: Cache<String, Arc<Mutex<QueryOperation>>>) -> Self {
        Self {
            operations,
            expired_for_test: Mutex::new(HashSet::new()),
        }
    }

    pub fn insert_provider(
        &self,
        query: MemoryQuery,
        compiled: CompiledQuery,
        work: QueryWork,
        payload: QueryOperationPayload,
    ) -> String {
        let operation_id = MemoryId::new().to_string();
        let mut operation = QueryOperation {
            query,
            compiled: Some(compiled),
            plan: None,
            stage: match &work {
                QueryWork::Embedding(_) => QueryOperationStage::WaitingForQueryEmbedding,
                QueryWork::Rerank(_) => QueryOperationStage::WaitingForRerank,
            },
            work: Some(work),
            pending_response: payload.pending_response,
            query_vector: None,
            tag_basis: None,
            candidate_pool: CandidatePool::default(),
            fused_candidates: Vec::new(),
            consolidated: Vec::new(),
            rerank_batch: payload.rerank_batch,
            rerank_scores: None,
            trace: None,
            adaptive_snapshot: payload.adaptive_snapshot,
            readiness_deadline: None,
            deadline_unix_ms: None,
            cancelled: false,
            expired_for_test: false,
        };
        operation.apply_state(payload.state);
        self.operations
            .insert(operation_id.clone(), Arc::new(Mutex::new(operation)));
        operation_id
    }

    pub fn insert_readiness(&self, query: MemoryQuery, timeout: Duration) -> String {
        let now = Instant::now();
        let deadline = now + timeout;
        let deadline_unix_ms = unix_millis_after(timeout);
        let operation_id = MemoryId::new().to_string();
        self.operations.insert(
            operation_id.clone(),
            Arc::new(Mutex::new(QueryOperation {
                query,
                compiled: None,
                plan: None,
                stage: QueryOperationStage::WaitingForCapabilities,
                work: None,
                pending_response: None,
                query_vector: None,
                tag_basis: None,
                candidate_pool: CandidatePool::default(),
                fused_candidates: Vec::new(),
                consolidated: Vec::new(),
                rerank_batch: None,
                rerank_scores: None,
                trace: None,
                adaptive_snapshot: None,
                readiness_deadline: Some(deadline),
                deadline_unix_ms: Some(deadline_unix_ms),
                cancelled: false,
                expired_for_test: false,
            })),
        );
        operation_id
    }

    pub fn replace_provider(
        &self,
        operation_id: &str,
        compiled: CompiledQuery,
        work: QueryWork,
        payload: QueryOperationPayload,
    ) {
        let operation = self
            .operations
            .get(operation_id)
            .expect("query continuation operation must exist");
        let mut operation = operation
            .lock()
            .expect("query operation mutex must not be poisoned");
        operation.query = compiled.query.clone();
        operation.compiled = Some(compiled);
        operation.stage = match &work {
            QueryWork::Embedding(_) => QueryOperationStage::WaitingForQueryEmbedding,
            QueryWork::Rerank(_) => QueryOperationStage::WaitingForRerank,
        };
        operation.work = Some(work);
        operation.pending_response = payload.pending_response;
        operation.rerank_batch = payload.rerank_batch;
        operation.rerank_scores = None;
        operation.adaptive_snapshot = payload.adaptive_snapshot;
        operation.apply_state(payload.state);
        operation.readiness_deadline = None;
        operation.deadline_unix_ms = None;
    }

    pub fn readiness_step(&self, operation_id: &str) -> Option<QueryStep> {
        let operation = self.operations.get(operation_id)?;
        let operation = operation
            .lock()
            .expect("query operation mutex must not be poisoned")
            .clone();
        if operation.stage != QueryOperationStage::WaitingForCapabilities {
            return None;
        }
        Some(QueryStep::ReadinessPending {
            operation_id: operation_id.to_owned(),
            retry_after_ms: READINESS_RETRY_AFTER_MS,
            deadline_unix_ms: operation.deadline_unix_ms.unwrap_or_default(),
        })
    }

    pub fn get(&self, operation_id: &str) -> Option<QueryOperation> {
        self.operations.get(operation_id).map(|operation| {
            operation
                .lock()
                .expect("query operation mutex must not be poisoned")
                .clone()
        })
    }

    pub fn remove(&self, operation_id: &str) -> Option<QueryOperation> {
        let operation = self.operations.get(operation_id)?;
        let operation = operation
            .lock()
            .expect("query operation mutex must not be poisoned")
            .clone();
        self.operations.invalidate(operation_id);
        self.operations.run_pending_tasks();
        Some(operation)
    }

    pub fn replace(&self, operation_id: String, operation: QueryOperation) {
        self.operations
            .insert(operation_id, Arc::new(Mutex::new(operation)));
    }

    pub fn cancel(&self, operation_id: &str) -> bool {
        if let Some(operation) = self.operations.get(operation_id) {
            operation
                .lock()
                .expect("query operation mutex must not be poisoned")
                .cancelled = true;
            true
        } else {
            false
        }
    }

    pub fn expire_for_test(&self, operation_id: &str) {
        self.expired_for_test
            .lock()
            .expect("query operation test mutex must not be poisoned")
            .insert(operation_id.to_owned());
        if let Some(operation) = self.operations.get(operation_id) {
            operation
                .lock()
                .expect("query operation mutex must not be poisoned")
                .expired_for_test = true;
        }
    }

    pub fn purge_expired_for_test(&self) {
        let operation_ids = std::mem::take(
            &mut *self
                .expired_for_test
                .lock()
                .expect("query operation test mutex must not be poisoned"),
        );
        for operation_id in operation_ids {
            self.operations.invalidate(&operation_id);
        }
        self.operations.run_pending_tasks();
    }
}

fn unix_millis_after(timeout: Duration) -> u64 {
    let deadline = std::time::SystemTime::now()
        .checked_add(timeout)
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    deadline
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}
