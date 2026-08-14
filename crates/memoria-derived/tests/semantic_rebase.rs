use memoria_derived::{
    ArtifactId, DerivedCatalog, EmbeddingBuildIdentity, EmbeddingNormalization, LatestMemoryState,
    ProjectionInputHash, SemanticPublicationDecision, VectorPayloadHash, VectorPayloadRecord,
    semantic_publication_target,
};
use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};
use tempfile::tempdir;

fn identity(
    generation: u64,
    memory_id: MemoryId,
    revision_id: RevisionId,
    space_id: SpaceId,
    projection_byte: u8,
) -> EmbeddingBuildIdentity {
    EmbeddingBuildIdentity {
        authority_generation: AuthorityGeneration::new(generation),
        memory_id,
        revision_id,
        space_id,
        projection_input_hash: ProjectionInputHash::from_bytes([projection_byte; 32]),
    }
}

fn latest(
    generation: u64,
    memory_id: MemoryId,
    revision_id: RevisionId,
    space_id: SpaceId,
    projection_byte: u8,
) -> LatestMemoryState {
    LatestMemoryState {
        authority_generation: AuthorityGeneration::new(generation),
        memory_id,
        revision_id,
        space_id,
        projection_input_hash: ProjectionInputHash::from_bytes([projection_byte; 32]),
    }
}

#[test]
fn unchanged_projection_rebases_to_latest_revision_and_space() {
    let memory_id = MemoryId::from_bytes([1; 16]);
    let original = identity(
        5,
        memory_id,
        RevisionId::from_bytes([2; 32]),
        SpaceId::from_bytes([3; 16]),
        7,
    );
    let current = latest(
        8,
        memory_id,
        RevisionId::from_bytes([4; 32]),
        SpaceId::from_bytes([5; 16]),
        7,
    );

    assert_eq!(
        semantic_publication_target(&original, &current),
        SemanticPublicationDecision::PublishAt {
            authority_generation: AuthorityGeneration::new(8),
            memory_id,
            revision_id: RevisionId::from_bytes([4; 32]),
        }
    );
}

#[test]
fn changed_projection_supersedes_old_completion() {
    let memory_id = MemoryId::from_bytes([6; 16]);
    let original = identity(
        5,
        memory_id,
        RevisionId::from_bytes([7; 32]),
        SpaceId::from_bytes([8; 16]),
        9,
    );
    let current = latest(
        6,
        memory_id,
        RevisionId::from_bytes([10; 32]),
        SpaceId::from_bytes([8; 16]),
        11,
    );

    assert_eq!(
        semantic_publication_target(&original, &current),
        SemanticPublicationDecision::Superseded
    );
}

fn payload_record() -> VectorPayloadRecord {
    let payload_hash = VectorPayloadHash::from_bytes([12; 32]);
    VectorPayloadRecord {
        payload_hash,
        dimension: 3,
        normalization: EmbeddingNormalization::L2,
        producer_signature: "provider:model:v1".to_owned(),
        projection_input_hash: ProjectionInputHash::from_bytes([13; 32]),
        object_path: "objects/vector/0c/payload.vec".to_owned(),
        byte_length: 99,
        checksum: [12; 32],
        created_at: 0,
    }
}

fn add_membership(
    catalog: &mut DerivedCatalog,
    artifact_id: ArtifactId,
    payload_hash: VectorPayloadHash,
    generation: AuthorityGeneration,
) {
    catalog
        .insert_vector_membership(
            artifact_id,
            SpaceId::from_bytes([14; 16]),
            MemoryId::from_bytes([15; 16]),
            RevisionId::from_bytes([16; 32]),
            "memory:rebase",
            None,
            "leaf",
            payload_hash,
            generation,
        )
        .unwrap();
}

#[test]
fn rebased_manifest_accepts_older_compatible_semantic_artifact() {
    let directory = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(directory.path().join("catalog.sqlite")).unwrap();
    let payload = payload_record();
    catalog.register_vector_payload(&payload).unwrap();

    let base = catalog
        .stage_artifact("lexical", 1, AuthorityGeneration::new(8))
        .unwrap();
    catalog.validate_artifact(base.id()).unwrap();
    let semantic = catalog
        .stage_artifact("semantic", 1, AuthorityGeneration::new(5))
        .unwrap();
    add_membership(
        &mut catalog,
        semantic.id(),
        payload.payload_hash,
        AuthorityGeneration::new(8),
    );
    catalog.validate_artifact(semantic.id()).unwrap();

    let manifest = catalog
        .publish_manifest_rebased_at_generation(
            vec![base.id(), semantic.id()],
            AuthorityGeneration::new(8),
            Vec::new(),
        )
        .unwrap();

    assert_eq!(manifest.authority_generation(), AuthorityGeneration::new(8));
    assert!(manifest.capability("semantic").is_ready());
}

#[test]
fn semantic_payload_is_not_recomputed_when_projection_hash_is_unchanged() {
    let directory = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(directory.path().join("catalog.sqlite")).unwrap();
    let payload = payload_record();

    catalog.register_vector_payload(&payload).unwrap();
    catalog.register_vector_payload(&payload).unwrap();

    assert_eq!(catalog.vector_payload_count().unwrap(), 1);
}
