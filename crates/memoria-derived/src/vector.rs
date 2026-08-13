use std::collections::BTreeMap;
use std::sync::Arc;

use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

use crate::{DerivedError, EmbeddingSignature, EmbeddingVector};

/// An immutable, content-addressed vector payload.
///
/// The payload is independent from where a memory is currently attached. The
/// `Arc` keeps repeated memberships in the same in-process index from copying
/// the Rust-owned vector values.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorArtifact {
    signature: EmbeddingSignature,
    payload_hash: [u8; 32],
    values: Arc<[f32]>,
}

impl VectorArtifact {
    #[must_use]
    pub fn from_embedding(vector: EmbeddingVector) -> Self {
        let signature = vector.signature().clone();
        let payload_hash = vector.payload_hash();
        let values = Arc::<[f32]>::from(vector.values().to_vec().into_boxed_slice());
        Self {
            signature,
            payload_hash,
            values,
        }
    }

    #[must_use]
    pub const fn signature(&self) -> &EmbeddingSignature {
        &self.signature
    }

    #[must_use]
    pub const fn payload_hash(&self) -> &[u8; 32] {
        &self.payload_hash
    }

    #[must_use]
    pub fn values(&self) -> &[f32] {
        &self.values
    }
}

/// The authority-scoped membership of a vector payload.
///
/// Moving a memory creates a new membership while preserving the immutable
/// payload hash. The generation is part of the membership so stale derived
/// rows cannot be mistaken for the current authority view.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VectorMembership {
    space_id: SpaceId,
    memory_id: MemoryId,
    revision_id: RevisionId,
    authority_generation: AuthorityGeneration,
    payload_hash: [u8; 32],
}

impl VectorMembership {
    #[must_use]
    pub const fn new(
        space_id: SpaceId,
        memory_id: MemoryId,
        revision_id: RevisionId,
        authority_generation: AuthorityGeneration,
        artifact: &VectorArtifact,
    ) -> Self {
        Self {
            space_id,
            memory_id,
            revision_id,
            authority_generation,
            payload_hash: *artifact.payload_hash(),
        }
    }

    #[must_use]
    pub const fn space_id(self) -> SpaceId {
        self.space_id
    }

    #[must_use]
    pub const fn memory_id(self) -> MemoryId {
        self.memory_id
    }

    #[must_use]
    pub const fn revision_id(self) -> RevisionId {
        self.revision_id
    }

    #[must_use]
    pub const fn authority_generation(self) -> AuthorityGeneration {
        self.authority_generation
    }

    #[must_use]
    pub const fn payload_hash(self) -> [u8; 32] {
        self.payload_hash
    }

    #[must_use]
    fn identity(self) -> VectorMembershipIdentity {
        VectorMembershipIdentity {
            space_id: self.space_id,
            memory_id: self.memory_id,
            revision_id: self.revision_id,
            authority_generation: self.authority_generation,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VectorFilter {
    space_id: Option<SpaceId>,
    memory_id: Option<MemoryId>,
    revision_id: Option<RevisionId>,
    authority_generation: Option<AuthorityGeneration>,
}

impl VectorFilter {
    #[must_use]
    pub const fn any() -> Self {
        Self {
            space_id: None,
            memory_id: None,
            revision_id: None,
            authority_generation: None,
        }
    }

    #[must_use]
    pub const fn with_space_id(mut self, space_id: SpaceId) -> Self {
        self.space_id = Some(space_id);
        self
    }

    #[must_use]
    pub const fn with_memory_id(mut self, memory_id: MemoryId) -> Self {
        self.memory_id = Some(memory_id);
        self
    }

    #[must_use]
    pub const fn with_revision_id(mut self, revision_id: RevisionId) -> Self {
        self.revision_id = Some(revision_id);
        self
    }

    #[must_use]
    pub const fn with_authority_generation(
        mut self,
        authority_generation: AuthorityGeneration,
    ) -> Self {
        self.authority_generation = Some(authority_generation);
        self
    }

    #[must_use]
    fn matches(self, membership: &VectorMembership) -> bool {
        self.space_id
            .is_none_or(|value| value == membership.space_id)
            && self
                .memory_id
                .is_none_or(|value| value == membership.memory_id)
            && self
                .revision_id
                .is_none_or(|value| value == membership.revision_id)
            && self
                .authority_generation
                .is_none_or(|value| value == membership.authority_generation)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VectorHit {
    membership: VectorMembership,
    score: f32,
}

impl VectorHit {
    #[must_use]
    pub const fn membership(self) -> VectorMembership {
        self.membership
    }

    #[must_use]
    pub const fn score(self) -> f32 {
        self.score
    }
}

/// Provider-neutral vector search boundary.
///
/// Implementations own the ANN choice and must apply authority membership
/// filters before returning hits.
pub trait VectorSearch {
    fn search(
        &self,
        query: &[f32],
        limit: usize,
        filter: &VectorFilter,
    ) -> Result<Vec<VectorHit>, DerivedError>;
}

/// An immutable-payload vector index with authority-scoped memberships.
pub struct VectorIndex {
    signature: EmbeddingSignature,
    adapter: UsearchVectorSearch,
}

impl VectorIndex {
    pub fn new(signature: EmbeddingSignature) -> Result<Self, DerivedError> {
        let options = IndexOptions {
            dimensions: signature.dimensions(),
            metric: MetricKind::Cos,
            quantization: ScalarKind::F32,
            ..Default::default()
        };
        let index = Index::new(&options).map_err(vector_index_error)?;
        Ok(Self {
            signature,
            adapter: UsearchVectorSearch::new(index),
        })
    }

    #[must_use]
    pub const fn signature(&self) -> &EmbeddingSignature {
        &self.signature
    }

    #[must_use]
    pub fn payload_count(&self) -> usize {
        self.adapter.payloads.len()
    }

    #[must_use]
    pub fn membership_count(&self) -> usize {
        self.adapter.memberships.len()
    }

    pub fn insert(
        &mut self,
        artifact: VectorArtifact,
        membership: VectorMembership,
    ) -> Result<(), DerivedError> {
        if artifact.signature() != self.signature() {
            return Err(DerivedError::InvalidProjectionValue {
                value: "vector artifact signature does not match index signature".to_owned(),
            });
        }
        if membership.payload_hash() != *artifact.payload_hash() {
            return Err(DerivedError::InvalidProjectionValue {
                value: "vector membership payload hash does not match artifact".to_owned(),
            });
        }
        self.adapter.insert(artifact, membership)
    }

    pub fn search(
        &self,
        query: &[f32],
        limit: usize,
        filter: &VectorFilter,
    ) -> Result<Vec<VectorHit>, DerivedError> {
        self.adapter.search(query, limit, filter)
    }
}

impl VectorSearch for VectorIndex {
    fn search(
        &self,
        query: &[f32],
        limit: usize,
        filter: &VectorFilter,
    ) -> Result<Vec<VectorHit>, DerivedError> {
        self.search(query, limit, filter)
    }
}

struct UsearchVectorSearch {
    index: Index,
    memberships: BTreeMap<u64, VectorMembership>,
    membership_keys: BTreeMap<VectorMembershipIdentity, u64>,
    payloads: BTreeMap<[u8; 32], VectorArtifact>,
    next_key: u64,
}

impl UsearchVectorSearch {
    fn new(index: Index) -> Self {
        Self {
            index,
            memberships: BTreeMap::new(),
            membership_keys: BTreeMap::new(),
            payloads: BTreeMap::new(),
            next_key: 1,
        }
    }

    fn insert(
        &mut self,
        artifact: VectorArtifact,
        membership: VectorMembership,
    ) -> Result<(), DerivedError> {
        if let Some(key) = self.membership_keys.get(&membership.identity()) {
            let existing = self
                .memberships
                .get(key)
                .ok_or_else(|| vector_index_error("membership index is internally inconsistent"))?;
            if existing.payload_hash() != membership.payload_hash() {
                return Err(DerivedError::InvalidProjectionValue {
                    value: "vector membership already points to a different payload".to_owned(),
                });
            }
            return Ok(());
        }

        if let Some(existing) = self.payloads.get(artifact.payload_hash())
            && existing != &artifact
        {
            return Err(DerivedError::InvalidProjectionValue {
                value: "vector payload hash collision detected".to_owned(),
            });
        }

        let key = self.next_key;
        self.next_key = self
            .next_key
            .checked_add(1)
            .ok_or_else(|| vector_index_error("vector membership key space exhausted"))?;
        if self.index.capacity() <= self.memberships.len() {
            let capacity = self
                .memberships
                .len()
                .max(1)
                .checked_mul(2)
                .ok_or_else(|| vector_index_error("vector index capacity exhausted"))?;
            self.index.reserve(capacity).map_err(vector_index_error)?;
        }
        self.index
            .add(key, artifact.values())
            .map_err(vector_index_error)?;
        self.payloads
            .entry(*artifact.payload_hash())
            .or_insert_with(|| artifact.clone());
        self.memberships.insert(key, membership);
        self.membership_keys.insert(membership.identity(), key);
        Ok(())
    }
}

impl VectorSearch for UsearchVectorSearch {
    fn search(
        &self,
        query: &[f32],
        limit: usize,
        filter: &VectorFilter,
    ) -> Result<Vec<VectorHit>, DerivedError> {
        validate_query(self.index.dimensions(), query)?;
        if limit == 0 || self.memberships.is_empty() {
            return Ok(Vec::new());
        }

        let count = limit.min(self.memberships.len());
        let matches = self
            .index
            .filtered_search(query, count, |key| {
                self.memberships
                    .get(&key)
                    .is_some_and(|membership| filter.matches(membership))
            })
            .map_err(vector_index_error)?;

        let mut hits = matches
            .keys
            .iter()
            .zip(matches.distances.iter())
            .filter_map(|(key, distance)| {
                self.memberships.get(key).map(|membership| VectorHit {
                    membership: *membership,
                    score: 1.0 - distance,
                })
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.membership.cmp(&right.membership))
        });
        Ok(hits)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct VectorMembershipIdentity {
    space_id: SpaceId,
    memory_id: MemoryId,
    revision_id: RevisionId,
    authority_generation: AuthorityGeneration,
}

fn validate_query(dimensions: usize, query: &[f32]) -> Result<(), DerivedError> {
    if query.len() != dimensions {
        return Err(DerivedError::InvalidProjectionValue {
            value: format!(
                "vector query dimension mismatch: expected {dimensions}, got {}",
                query.len()
            ),
        });
    }
    if query.iter().any(|value| !value.is_finite()) {
        return Err(DerivedError::InvalidProjectionValue {
            value: "vector query values must be finite".to_owned(),
        });
    }
    let norm = query.iter().map(|value| value * value).sum::<f32>();
    if norm == 0.0 {
        return Err(DerivedError::InvalidProjectionValue {
            value: "vector query must not be zero".to_owned(),
        });
    }
    Ok(())
}

fn vector_index_error(error: impl std::fmt::Display) -> DerivedError {
    DerivedError::VectorIndex {
        value: error.to_string(),
    }
}
