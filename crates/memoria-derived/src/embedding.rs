use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::dependency::{ProjectionInputHash, ProjectionKind};
use crate::{DerivedError, PROJECTION_SCHEMA_VERSION};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EmbeddingNormalization {
    None,
    L2,
}

impl EmbeddingNormalization {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::L2 => "l2",
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EmbeddingSignature {
    provider: String,
    model: String,
    task: String,
    dimensions: usize,
    normalization: EmbeddingNormalization,
    projection_version: u32,
}

impl EmbeddingSignature {
    pub fn new(
        provider: impl Into<String>,
        model: impl Into<String>,
        task: impl Into<String>,
        dimensions: usize,
        normalization: EmbeddingNormalization,
        projection_version: u32,
    ) -> Result<Self, DerivedError> {
        if dimensions == 0 {
            return Err(DerivedError::InvalidProjectionValue {
                value: "embedding dimensions must be positive".to_owned(),
            });
        }
        if projection_version == 0 {
            return Err(DerivedError::InvalidProjectionValue {
                value: "embedding projection version must be positive".to_owned(),
            });
        }
        Ok(Self {
            provider: provider.into(),
            model: model.into(),
            task: task.into(),
            dimensions,
            normalization,
            projection_version,
        })
    }

    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub fn task(&self) -> &str {
        &self.task
    }

    #[must_use]
    pub const fn dimensions(&self) -> usize {
        self.dimensions
    }

    #[must_use]
    pub const fn normalization(&self) -> EmbeddingNormalization {
        self.normalization
    }

    #[must_use]
    pub const fn projection_version(&self) -> u32 {
        self.projection_version
    }

    #[must_use]
    pub fn cache_key(&self, input_hash: ProjectionInputHash) -> EmbeddingCacheKey {
        EmbeddingCacheKey {
            input_hash,
            signature: self.clone(),
        }
    }

    #[must_use]
    pub fn identity_hash(&self, input_hash: &ProjectionInputHash) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"memoria-embedding-identity-v1\0");
        hasher.update(input_hash.as_bytes());
        put_string(&mut hasher, self.provider.as_bytes());
        put_string(&mut hasher, self.model.as_bytes());
        put_string(&mut hasher, self.task.as_bytes());
        hasher.update((self.dimensions as u64).to_be_bytes());
        put_string(&mut hasher, self.normalization.as_str().as_bytes());
        hasher.update(self.projection_version.to_be_bytes());
        hasher.finalize().into()
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EmbeddingCacheKey {
    input_hash: ProjectionInputHash,
    signature: EmbeddingSignature,
}

impl EmbeddingCacheKey {
    #[must_use]
    pub fn input_hash(&self) -> &ProjectionInputHash {
        &self.input_hash
    }

    #[must_use]
    pub const fn signature(&self) -> &EmbeddingSignature {
        &self.signature
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddingVector {
    signature: EmbeddingSignature,
    values: Vec<f32>,
}

impl EmbeddingVector {
    pub fn new(signature: EmbeddingSignature, values: Vec<f32>) -> Result<Self, DerivedError> {
        validate_values(signature.dimensions(), &values)?;
        Ok(Self { signature, values })
    }

    #[must_use]
    pub const fn signature(&self) -> &EmbeddingSignature {
        &self.signature
    }

    #[must_use]
    pub fn values(&self) -> &[f32] {
        &self.values
    }

    #[must_use]
    pub fn payload_hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"memoria-embedding-payload-v1\0");
        for value in &self.values {
            hasher.update(value.to_bits().to_be_bytes());
        }
        hasher.finalize().into()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingWorkItem {
    pub key: String,
    pub input_hash: ProjectionInputHash,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingWork {
    pub work_id: String,
    pub signature: EmbeddingSignature,
    pub items: Vec<EmbeddingWorkItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddingResultItem {
    pub key: String,
    pub values: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddingResult {
    pub work_id: String,
    pub signature: EmbeddingSignature,
    pub items: Vec<EmbeddingResultItem>,
}

pub fn validate_embedding_result(
    work: &EmbeddingWork,
    result: &EmbeddingResult,
) -> Result<Vec<EmbeddingVector>, DerivedError> {
    if work.work_id != result.work_id {
        return Err(DerivedError::InvalidProjectionValue {
            value: "embedding result work id does not match request".to_owned(),
        });
    }
    if work.signature != result.signature {
        return Err(DerivedError::InvalidProjectionValue {
            value: "embedding result signature does not match request".to_owned(),
        });
    }
    if work.items.len() != result.items.len() {
        return Err(DerivedError::InvalidProjectionValue {
            value: "embedding result item count does not match request".to_owned(),
        });
    }
    let expected = work
        .items
        .iter()
        .map(|item| item.key.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let actual = result
        .items
        .iter()
        .map(|item| item.key.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if expected != actual {
        return Err(DerivedError::InvalidProjectionValue {
            value: "embedding result keys do not match request".to_owned(),
        });
    }
    result
        .items
        .iter()
        .map(|item| EmbeddingVector::new(result.signature.clone(), item.values.clone()))
        .collect()
}

#[derive(Clone, Debug, Default)]
pub struct EmbeddingPayloadCache {
    values: BTreeMap<EmbeddingCacheKey, EmbeddingVector>,
}

impl EmbeddingPayloadCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    #[must_use]
    pub fn get(&self, key: &EmbeddingCacheKey) -> Option<&EmbeddingVector> {
        self.values.get(key)
    }

    pub fn insert(
        &mut self,
        key: EmbeddingCacheKey,
        value: EmbeddingVector,
    ) -> Result<(), DerivedError> {
        if key.signature != value.signature {
            return Err(DerivedError::InvalidProjectionValue {
                value: "embedding cache key and payload signatures differ".to_owned(),
            });
        }
        if let Some(existing) = self.values.get(&key)
            && existing.payload_hash() != value.payload_hash()
        {
            return Err(DerivedError::InvalidProjectionValue {
                value: "embedding cache key already contains a different payload".to_owned(),
            });
        }
        self.values.insert(key, value);
        Ok(())
    }
}

pub fn default_content_signature(
    provider: impl Into<String>,
    model: impl Into<String>,
    dimensions: usize,
) -> Result<EmbeddingSignature, DerivedError> {
    EmbeddingSignature::new(
        provider,
        model,
        "content",
        dimensions,
        EmbeddingNormalization::L2,
        ProjectionKind::LocalEmbedding
            .version()
            .max(PROJECTION_SCHEMA_VERSION),
    )
}

fn validate_values(dimensions: usize, values: &[f32]) -> Result<(), DerivedError> {
    if values.len() != dimensions {
        return Err(DerivedError::InvalidProjectionValue {
            value: format!(
                "embedding dimension mismatch: expected {dimensions}, got {}",
                values.len()
            ),
        });
    }
    if values.iter().any(|value| !value.is_finite()) {
        return Err(DerivedError::InvalidProjectionValue {
            value: "embedding values must be finite".to_owned(),
        });
    }
    Ok(())
}

fn put_string(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}
