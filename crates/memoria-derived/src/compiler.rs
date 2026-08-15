use std::path::Path;

use memoria_mdx::SemanticDiff;
use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};

use crate::dependency::{InvalidationPlan, ProjectionInputHash, ProjectionKind};
use crate::enrichment::{ENRICHMENT_PROJECTION_VERSION, EnrichmentProjection};
use crate::projection::ProjectionTarget;
use crate::{
    AnnSegmentEntry, AnnSegmentRecord, AnnSegmentV1, ArtifactId, DerivedCatalog, DerivedError,
    DerivedManifest, EntityObservationBuilder, ExplicitTagBuilder, LexicalArtifactV1,
    LexicalDocument, RelationBuilder, ServingRecord, StructuralBuilder, TemporalBuilder,
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
pub struct EmbeddingBuildIdentity {
    pub authority_generation: AuthorityGeneration,
    pub memory_id: MemoryId,
    pub revision_id: RevisionId,
    pub space_id: SpaceId,
    pub projection_input_hash: ProjectionInputHash,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LatestMemoryState {
    pub authority_generation: AuthorityGeneration,
    pub memory_id: MemoryId,
    pub revision_id: RevisionId,
    pub space_id: SpaceId,
    pub projection_input_hash: ProjectionInputHash,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SemanticPublicationDecision {
    PublishAt {
        authority_generation: AuthorityGeneration,
        memory_id: MemoryId,
        revision_id: RevisionId,
    },
    Superseded,
}

#[must_use]
pub fn semantic_publication_target(
    original: &EmbeddingBuildIdentity,
    latest: &LatestMemoryState,
) -> SemanticPublicationDecision {
    if original.memory_id != latest.memory_id
        || original.projection_input_hash != latest.projection_input_hash
    {
        return SemanticPublicationDecision::Superseded;
    }
    SemanticPublicationDecision::PublishAt {
        authority_generation: latest.authority_generation,
        memory_id: latest.memory_id,
        revision_id: latest.revision_id,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BaseReadyReport {
    pub provider_work_items: usize,
    pub coverage: AuthorityGeneration,
    pub manifest: DerivedManifest,
    enrichment_projections: Vec<EnrichmentProjection>,
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
    pub fn enrichment_projections(&self) -> &[EnrichmentProjection] {
        &self.enrichment_projections
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

    pub fn persist_ann_segment(
        &self,
        catalog: &mut DerivedCatalog,
        derived_dir: impl AsRef<Path>,
        artifact_id: ArtifactId,
        entries: Vec<AnnSegmentEntry>,
    ) -> Result<AnnSegmentRecord, DerivedError> {
        let segment = AnnSegmentV1::build(self.producer_signature.clone(), entries)?;
        let object_hash = segment.put(derived_dir)?;
        catalog.register_ann_segment(
            artifact_id,
            object_hash,
            u64::try_from(segment.vector_count())?,
            segment.dimension(),
            segment.producer_signature(),
        )
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
        self.compile_base_internal(catalog, generation, documents)
    }

    fn compile_base_internal<I>(
        &self,
        catalog: &mut DerivedCatalog,
        generation: AuthorityGeneration,
        documents: I,
    ) -> Result<BaseReadyReport, DerivedError>
    where
        I: IntoIterator<Item = LexicalDocument>,
    {
        let documents = documents.into_iter().collect::<Vec<_>>();
        let _build_span = tracing::info_span!(
            "memoria.derived.build",
            authority_generation = generation.value(),
            producer_signature = %self.producer_signature,
            document_count = documents.len(),
        )
        .entered();
        let current_targets = documents
            .iter()
            .map(|document| {
                (
                    document.space_id(),
                    document.memory_id(),
                    document.revision_id(),
                )
            })
            .collect::<std::collections::BTreeSet<_>>();
        let mut compatible_semantic_artifacts = Vec::new();
        if let Some(manifest) = catalog.serving_manifest()? {
            for artifact_id in manifest.artifacts() {
                let artifact = catalog.artifact(artifact_id)?;
                if !artifact.is_compatible_semantic() {
                    continue;
                }
                let memberships = catalog.vector_memberships_for_artifact(artifact_id)?;
                let has_current_membership = memberships.iter().any(|membership| {
                    current_targets.contains(&(
                        membership.space_id,
                        membership.memory_id,
                        membership.revision_id,
                    ))
                });
                if has_current_membership {
                    compatible_semantic_artifacts.push(artifact_id);
                }
            }
        }
        let enrichment_signature = format!(
            "{}:generated-tags-v{}",
            self.producer_signature, ENRICHMENT_PROJECTION_VERSION
        );
        let enrichment_projections = documents
            .iter()
            .map(|document| EnrichmentProjection::from_document(document, &enrichment_signature))
            .collect::<Result<Vec<_>, DerivedError>>()?;

        let mut serving_by_space = std::collections::BTreeMap::<_, Vec<ServingRecord>>::new();
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
            serving_by_space
                .entry(document.space_id())
                .or_default()
                .push(ServingRecord::from_document(document, generation)?);
        }
        for (space_id, records) in serving_by_space {
            catalog.replace_serving_records(space_id, &records)?;
        }

        let lexical_artifact =
            LexicalArtifactV1::build(catalog.derived_dir(), generation, &documents)?;

        let mut artifact_ids = Vec::with_capacity(BASE_ARTIFACT_KINDS.len());
        for kind in BASE_ARTIFACT_KINDS {
            let artifact =
                catalog.stage_artifact(kind, crate::PROJECTION_SCHEMA_VERSION, generation)?;
            if kind == "lexical" {
                catalog.register_lexical_artifact(
                    artifact.id(),
                    *lexical_artifact.hash().as_bytes(),
                    lexical_artifact.object_path(),
                    u64::try_from(lexical_artifact.document_count())?,
                )?;
            }
            catalog.validate_artifact(artifact.id())?;
            artifact_ids.push(artifact.id());
        }
        artifact_ids.extend(compatible_semantic_artifacts);
        let manifest = if artifact_ids.len() == BASE_ARTIFACT_KINDS.len() {
            catalog.publish_manifest_at_generation(artifact_ids, generation, Vec::new())?
        } else {
            catalog.publish_manifest_rebased_at_generation(artifact_ids, generation, Vec::new())?
        };
        tracing::debug!(
            target: "memoria.derived.build",
            authority_generation = generation.value(),
            document_count = documents.len(),
            "derived base build completed"
        );
        Ok(BaseReadyReport {
            provider_work_items: 0,
            coverage: generation,
            manifest,
            enrichment_projections,
        })
    }
}

impl Default for DerivedCompiler {
    fn default() -> Self {
        Self::new("memoria-derived-v1")
    }
}
