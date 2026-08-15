use std::sync::{Arc, Mutex};
use std::time::Duration;

use memoria_derived::ManifestId;
use memoria_query::AssociationGraphView;
use memoria_types::AuthorityGeneration;
use moka::sync::Cache;

use crate::query_operation::QueryOperation;

const QUERY_OPERATION_CAPACITY: u64 = 1024;
const QUERY_OPERATION_TTL: Duration = Duration::from_secs(5 * 60);
const QUERY_EMBEDDING_CAPACITY: u64 = 256;
const QUERY_EMBEDDING_TTI: Duration = Duration::from_secs(15 * 60);
const ASSOCIATION_VIEW_CAPACITY: u64 = 64;
const ASSOCIATION_VIEW_TTI: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct QueryEmbeddingCacheKey {
    pub authority_generation: AuthorityGeneration,
    pub manifest_id: ManifestId,
    pub query_signature: [u8; 32],
}

impl QueryEmbeddingCacheKey {
    #[must_use]
    pub const fn new(
        authority_generation: AuthorityGeneration,
        manifest_id: ManifestId,
        query_signature: [u8; 32],
    ) -> Self {
        Self {
            authority_generation,
            manifest_id,
            query_signature,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct AssociationCacheKey {
    pub authority_generation: AuthorityGeneration,
    pub manifest_id: ManifestId,
    pub view_signature: [u8; 32],
}

impl AssociationCacheKey {
    #[must_use]
    pub const fn new(
        authority_generation: AuthorityGeneration,
        manifest_id: ManifestId,
        view_signature: [u8; 32],
    ) -> Self {
        Self {
            authority_generation,
            manifest_id,
            view_signature,
        }
    }
}

/// Process-local caches with explicit bounded policies.
///
/// Snapshot and manifest identity live in the retrieval cache keys. A new
/// immutable identity therefore creates a new entry naturally; no broad
/// expiration scan or hand-maintained LRU state is needed.
#[derive(Clone, Debug)]
pub struct RuntimeCaches {
    pub(crate) query_operations: Cache<String, Arc<Mutex<QueryOperation>>>,
    query_embeddings: Cache<QueryEmbeddingCacheKey, Arc<Vec<f32>>>,
    association_views: Cache<AssociationCacheKey, Arc<AssociationGraphView>>,
}

impl RuntimeCaches {
    #[must_use]
    pub fn new() -> Self {
        Self {
            query_operations: Cache::builder()
                .max_capacity(QUERY_OPERATION_CAPACITY)
                .time_to_live(QUERY_OPERATION_TTL)
                .build(),
            query_embeddings: Cache::builder()
                .max_capacity(QUERY_EMBEDDING_CAPACITY)
                .time_to_idle(QUERY_EMBEDDING_TTI)
                .build(),
            association_views: Cache::builder()
                .max_capacity(ASSOCIATION_VIEW_CAPACITY)
                .time_to_idle(ASSOCIATION_VIEW_TTI)
                .build(),
        }
    }

    #[must_use]
    pub const fn query_operation_ttl() -> Duration {
        QUERY_OPERATION_TTL
    }

    #[must_use]
    pub const fn query_operation_max_capacity() -> u64 {
        QUERY_OPERATION_CAPACITY
    }

    #[must_use]
    pub fn query_operation_policy(&self) -> moka::policy::Policy {
        self.query_operations.policy()
    }

    #[must_use]
    pub(crate) fn query_operation_cache(&self) -> Cache<String, Arc<Mutex<QueryOperation>>> {
        self.query_operations.clone()
    }

    pub fn insert_query_embedding(&self, key: QueryEmbeddingCacheKey, value: Arc<Vec<f32>>) {
        self.query_embeddings.insert(key, value);
    }

    #[must_use]
    pub fn get_query_embedding(&self, key: &QueryEmbeddingCacheKey) -> Option<Arc<Vec<f32>>> {
        self.query_embeddings.get(key)
    }

    #[must_use]
    pub fn query_embedding_entry_count(&self) -> u64 {
        self.query_embeddings.entry_count()
    }

    pub fn insert_association_view(
        &self,
        key: AssociationCacheKey,
        value: Arc<AssociationGraphView>,
    ) {
        self.association_views.insert(key, value);
    }

    #[must_use]
    pub fn get_association_view(
        &self,
        key: &AssociationCacheKey,
    ) -> Option<Arc<AssociationGraphView>> {
        self.association_views.get(key)
    }

    pub fn run_pending_tasks(&self) {
        self.query_operations.run_pending_tasks();
        self.query_embeddings.run_pending_tasks();
        self.association_views.run_pending_tasks();
    }

    pub fn clear(&self) {
        self.query_operations.invalidate_all();
        self.query_embeddings.invalidate_all();
        self.association_views.invalidate_all();
        self.run_pending_tasks();
    }
}

impl Default for RuntimeCaches {
    fn default() -> Self {
        Self::new()
    }
}
