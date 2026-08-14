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

#[derive(Debug)]
pub struct QueryOperation {
    pub(crate) compiled: CompiledQuery,
    pub(crate) work: QueryWork,
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
    operations: BTreeMap<String, QueryOperation>,
}

impl QueryOperationTable {
    pub fn insert(&mut self, compiled: CompiledQuery, work: QueryWork) -> String {
        let now = Instant::now();
        loop {
            let operation_id = MemoryId::new().to_string();
            if !self.operations.contains_key(&operation_id) {
                self.operations.insert(
                    operation_id.clone(),
                    QueryOperation {
                        compiled,
                        work,
                        created_at: now,
                        expires_at: now + QUERY_OPERATION_TTL,
                        cancelled: false,
                    },
                );
                return operation_id;
            }
        }
    }

    pub fn get(&self, operation_id: &str) -> Option<&QueryOperation> {
        self.operations.get(operation_id)
    }

    pub fn remove(&mut self, operation_id: &str) -> Option<QueryOperation> {
        self.operations.remove(operation_id)
    }

    pub fn cancel(&mut self, operation_id: &str) -> bool {
        if let Some(operation) = self.operations.get_mut(operation_id) {
            operation.cancelled = true;
            true
        } else {
            false
        }
    }

    pub fn cleanup(&mut self, now: Instant) {
        self.operations
            .retain(|_, operation| !operation.is_expired(now));
    }

    pub fn expire_for_test(&mut self, operation_id: &str) {
        if let Some(operation) = self.operations.get_mut(operation_id) {
            operation.expires_at = Instant::now() - Duration::from_secs(1);
        }
    }
}
