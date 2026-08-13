use memoria_mdx::SemanticDiff;
use memoria_types::AuthorityGeneration;

use crate::dependency::{InvalidationPlan, ProjectionInputHash, ProjectionKind};
use crate::projection::ProjectionTarget;
use crate::{
    DerivedCatalog, DerivedError, DerivedManifest, EntityObservationBuilder, ExplicitTagBuilder,
    LexicalDocument, RelationBuilder, StructuralBuilder, TemporalBuilder,
};

pub const BASE_ARTIFACT_KINDS: [&str; 7] = [
    "ir",
    "structural",
    "temporal",
    "relations",
    "entity_observations",
    "explicit_tags",
    "lexical",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BaseReadyReport {
    pub provider_work_items: usize,
    pub coverage: AuthorityGeneration,
    pub manifest: DerivedManifest,
}

impl BaseReadyReport {
    #[must_use]
    pub const fn provider_work_items(&self) -> usize {
        self.provider_work_items
    }

    #[must_use]
    pub const fn coverage(&self) -> AuthorityGeneration {
        self.coverage
    }

    #[must_use]
    pub const fn manifest(&self) -> &DerivedManifest {
        &self.manifest
    }
}

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

    pub fn compile_base<I>(
        &self,
        catalog: &mut DerivedCatalog,
        generation: AuthorityGeneration,
        documents: I,
    ) -> Result<BaseReadyReport, DerivedError>
    where
        I: IntoIterator<Item = LexicalDocument>,
    {
        let documents = documents.into_iter().collect::<Vec<_>>();

        for document in &documents {
            let target = ProjectionTarget {
                space_id: Some(document.space_id()),
                memory_id: Some(document.memory_id()),
                revision_id: Some(document.revision_id()),
            };
            let _ = StructuralBuilder::build_for(document.ir(), target)?;
            let _ = TemporalBuilder::build_for(document.ir(), target)?;
            let _ = RelationBuilder::build_for(document.ir(), target)?;
            let _ = EntityObservationBuilder::build_for(document.ir(), target)?;
            let _ = ExplicitTagBuilder::build_for(document.ir(), target)?;
            let _ = crate::build_lexical(document)?;
        }

        let mut artifact_ids = Vec::with_capacity(BASE_ARTIFACT_KINDS.len());
        for kind in BASE_ARTIFACT_KINDS {
            let artifact =
                catalog.stage_artifact(kind, crate::PROJECTION_SCHEMA_VERSION, generation)?;
            catalog.validate_artifact(artifact.id())?;
            artifact_ids.push(artifact.id());
        }
        let manifest =
            catalog.publish_manifest_at_generation(artifact_ids, generation, Vec::new())?;
        Ok(BaseReadyReport {
            provider_work_items: 0,
            coverage: generation,
            manifest,
        })
    }
}

impl Default for DerivedCompiler {
    fn default() -> Self {
        Self::new("memoria-derived-v1")
    }
}
