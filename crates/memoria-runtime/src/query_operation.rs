use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use memoria_query::{CompiledQuery, RetrievalResponse};
use memoria_types::MemoryId;

use crate::provider::{EmbeddingBatchRequest, RerankBatchRequest};

pub const QUERY_OPERATION_TTL: Duration = Duration::from_secs(5 * 60);

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
}

#[derive(Debug)]
pub enum QueryStep {
    Complete(RetrievalResponse),
    Pending {
        operation_id: String,
        work: QueryWork,
    },
}

#[derive(Clone, Debug)]
pub struct QueryOperation {
    pub(crate) compiled: CompiledQuery,
    pub(crate) work: QueryWork,
    pub(crate) rerank_candidates: Vec<String>,
    pub(crate) created_at: Instant,
    pub(crate) expires_at: Instant,
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
    pub fn insert(
        &self,
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
                        compiled,
                        work,
                        rerank_candidates,
                        created_at: now,
                        expires_at: now + QUERY_OPERATION_TTL,
                        cancelled: false,
                    },
                );
                return operation_id;
            }
        }
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
