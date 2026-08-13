use memoria_mdx::SemanticDiff;

use crate::dependency::{InvalidationPlan, ProjectionInputHash, ProjectionKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivedCompiler {
    producer_signature: String,
}

impl DerivedCompiler {
    #[must_use]
    pub fn new(producer_signature: impl Into<String>) -> Self {
        Self {
            producer_signature: producer_signature.into(),
        }
    }

    #[must_use]
    pub fn plan(&self, diff: &SemanticDiff) -> InvalidationPlan {
        InvalidationPlan::from_diff(diff)
    }

    #[must_use]
    pub fn input_hash(
        &self,
        kind: ProjectionKind,
        canonical_projection_bytes: &[u8],
    ) -> ProjectionInputHash {
        ProjectionInputHash::new(kind, canonical_projection_bytes, &self.producer_signature)
    }

    #[must_use]
    pub fn producer_signature(&self) -> &str {
        &self.producer_signature
    }
}

impl Default for DerivedCompiler {
    fn default() -> Self {
        Self::new("memoria-derived-v1")
    }
}
