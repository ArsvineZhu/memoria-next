use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use memoria_query::{CompiledQuery, MemoryQuery, RetrievalResponse};
use memoria_types::MemoryId;

use crate::provider::{EmbeddingBatchRequest, NeedWork, RerankBatchRequest};

pub const QUERY_OPERATION_TTL: Duration = Duration::from_secs(5 * 60);
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

#[derive(Clone, Debug)]
pub struct QueryOperation {
    pub(crate) query: MemoryQuery,
    pub(crate) compiled: Option<CompiledQuery>,
    pub(crate) stage: QueryOperationStage,
    pub(crate) work: Option<QueryWork>,
    pub(crate) rerank_candidates: Vec<String>,
    pub(crate) query_vector: Option<Vec<f32>>,
    pub(crate) created_at: Instant,
    pub(crate) expires_at: Instant,
    pub(crate) readiness_deadline: Option<Instant>,
    pub(crate) deadline_unix_ms: Option<u64>,
    pub(crate) cancelled: bool,
}

impl QueryOperation {
    #[must_use]
    pub fn is_expired(&self, now: Instant) -> bool {
        now >= self.expires_at
            || now.saturating_duration_since(self.created_at) >= QUERY_OPERATION_TTL
    }
}

#[derive(Debug, Default)]
pub struct QueryOperationTable {
    operations: RefCell<BTreeMap<String, QueryOperation>>,
}

impl QueryOperationTable {
    pub fn insert_provider(
        &self,
        query: MemoryQuery,
        compiled: CompiledQuery,
        work: QueryWork,
        rerank_candidates: Vec<String>,
    ) -> String {
        let now = Instant::now();
        let mut operations = self.operations.borrow_mut();
        loop {
            let operation_id = MemoryId::new().to_string();
            if !operations.contains_key(&operation_id) {
                operations.insert(
                    operation_id.clone(),
                    QueryOperation {
                        query,
                        compiled: Some(compiled),
                        stage: match &work {
                            QueryWork::Embedding(_) => {
                                QueryOperationStage::WaitingForQueryEmbedding
                            }
                            QueryWork::Rerank(_) => QueryOperationStage::WaitingForRerank,
                        },
                        work: Some(work),
                        rerank_candidates,
                        query_vector: None,
                        created_at: now,
                        expires_at: now + QUERY_OPERATION_TTL,
                        readiness_deadline: None,
                        deadline_unix_ms: None,
                        cancelled: false,
                    },
                );
                return operation_id;
            }
        }
    }

    pub fn insert_readiness(&self, query: MemoryQuery, timeout: Duration) -> String {
        let now = Instant::now();
        let deadline = now + timeout;
        let deadline_unix_ms = unix_millis_after(timeout);
        let mut operations = self.operations.borrow_mut();
        loop {
            let operation_id = MemoryId::new().to_string();
            if !operations.contains_key(&operation_id) {
                operations.insert(
                    operation_id.clone(),
                    QueryOperation {
                        query,
                        compiled: None,
                        stage: QueryOperationStage::WaitingForCapabilities,
                        work: None,
                        rerank_candidates: Vec::new(),
                        query_vector: None,
                        created_at: now,
                        expires_at: now + QUERY_OPERATION_TTL,
                        readiness_deadline: Some(deadline),
                        deadline_unix_ms: Some(deadline_unix_ms),
                        cancelled: false,
                    },
                );
                return operation_id;
            }
        }
    }

    pub fn replace_provider(
        &self,
        operation_id: &str,
        compiled: CompiledQuery,
        work: QueryWork,
        rerank_candidates: Vec<String>,
    ) {
        let mut operations = self.operations.borrow_mut();
        let operation = operations
            .get_mut(operation_id)
            .expect("query continuation operation must exist");
        operation.query = compiled.query.clone();
        operation.compiled = Some(compiled);
        operation.stage = match &work {
            QueryWork::Embedding(_) => QueryOperationStage::WaitingForQueryEmbedding,
            QueryWork::Rerank(_) => QueryOperationStage::WaitingForRerank,
        };
        operation.work = Some(work);
        operation.rerank_candidates = rerank_candidates;
        operation.query_vector = None;
        operation.readiness_deadline = None;
        operation.deadline_unix_ms = None;
    }

    pub fn readiness_step(&self, operation_id: &str) -> Option<QueryStep> {
        let operation = self.operations.borrow().get(operation_id)?.clone();
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
        self.operations.borrow().get(operation_id).cloned()
    }

    pub fn remove(&self, operation_id: &str) -> Option<QueryOperation> {
        self.operations.borrow_mut().remove(operation_id)
    }

    pub fn replace(&self, operation_id: String, operation: QueryOperation) {
        self.operations.borrow_mut().insert(operation_id, operation);
    }

    pub fn cancel(&self, operation_id: &str) -> bool {
        if let Some(operation) = self.operations.borrow_mut().get_mut(operation_id) {
            operation.cancelled = true;
            true
        } else {
            false
        }
    }

    pub fn cleanup(&self, now: Instant) {
        self.operations
            .borrow_mut()
            .retain(|_, operation| !operation.is_expired(now));
    }

    pub fn expire_for_test(&self, operation_id: &str) {
        if let Some(operation) = self.operations.borrow_mut().get_mut(operation_id) {
            operation.expires_at = Instant::now() - Duration::from_secs(1);
        }
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
