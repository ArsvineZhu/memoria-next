use memoria_derived::{
    DerivedCatalog, EmbeddingNormalization, ProjectionInputHash, VectorPayloadHash,
    VectorPayloadRecord,
};
use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};
use tempfile::tempdir;

fn payload_record(hash_byte: u8) -> VectorPayloadRecord {
    let hash = VectorPayloadHash::from_bytes([hash_byte; 32]);
    VectorPayloadRecord {
        payload_hash: hash,
        dimension: 3,
        normalization: EmbeddingNormalization::L2,
        producer_signature: "provider:model:v1".to_owned(),
        projection_input_hash: ProjectionInputHash::from_bytes([hash_byte.wrapping_add(1); 32]),
        object_path: format!("objects/vector/{hash_byte:02x}/payload.vec"),
        byte_length: 99,
        checksum: [hash_byte; 32],
        created_at: 0,
    }
}

fn add_membership(
    catalog: &mut DerivedCatalog,
    artifact_id: memoria_derived::ArtifactId,
    payload_hash: VectorPayloadHash,
) {
    catalog
        .insert_vector_membership(
            artifact_id,
            SpaceId::from_bytes([1; 16]),
            MemoryId::from_bytes([2; 16]),
            RevisionId::from_bytes([3; 32]),
            "memory:2",
            None,
            "leaf",
            payload_hash,
            AuthorityGeneration::new(5),
        )
        .unwrap();
}

#[test]
fn accepted_embedding_without_vector_artifact_cannot_publish_semantic() {
    let directory = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(directory.path().join("catalog.sqlite")).unwrap();
    let artifact = catalog
        .stage_artifact("semantic", 1, AuthorityGeneration::new(5))
        .unwrap();
    catalog.validate_artifact(artifact.id()).unwrap();
    let manifest = catalog
        .publish_manifest_at_generation(
            vec![artifact.id()],
            AuthorityGeneration::new(5),
            vec!["semantic".to_owned()],
        )
        .unwrap();

    assert!(!manifest.capability("semantic").is_ready());
}

#[test]
fn embedding_result_persists_payload_and_target_membership() {
    let directory = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(directory.path().join("catalog.sqlite")).unwrap();
    let payload = payload_record(0x41);
    catalog.register_vector_payload(&payload).unwrap();
    let artifact = catalog
        .stage_artifact("semantic", 1, AuthorityGeneration::new(5))
        .unwrap();
    add_membership(&mut catalog, artifact.id(), payload.payload_hash);

    assert_eq!(catalog.vector_payload_count().unwrap(), 1);
    let memberships = catalog
        .vector_memberships_for_artifact(artifact.id())
        .unwrap();
    assert_eq!(memberships.len(), 1);
    assert_eq!(memberships[0].payload_hash, payload.payload_hash);
}

#[test]
fn semantic_manifest_publication_happens_after_artifact_validation() {
    let directory = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(directory.path().join("catalog.sqlite")).unwrap();
    let payload = payload_record(0x51);
    catalog.register_vector_payload(&payload).unwrap();
    let artifact = catalog
        .stage_artifact("semantic", 1, AuthorityGeneration::new(5))
        .unwrap();
    add_membership(&mut catalog, artifact.id(), payload.payload_hash);
    assert!(
        catalog
            .publish_manifest_at_generation(
                vec![artifact.id()],
                AuthorityGeneration::new(5),
                Vec::new(),
            )
            .is_err()
    );

    catalog.validate_artifact(artifact.id()).unwrap();
    let manifest = catalog
        .publish_manifest_at_generation(
            vec![artifact.id()],
            AuthorityGeneration::new(5),
            Vec::new(),
        )
        .unwrap();
    assert!(manifest.capability("semantic").is_ready());
}

#[test]
fn tag_only_revision_reuses_existing_content_vector_payload() {
    let directory = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(directory.path().join("catalog.sqlite")).unwrap();
    let payload = payload_record(0x61);
    catalog.register_vector_payload(&payload).unwrap();
    let artifact = catalog
        .stage_artifact("semantic", 1, AuthorityGeneration::new(5))
        .unwrap();
    add_membership(&mut catalog, artifact.id(), payload.payload_hash);
    let second_artifact = catalog
        .stage_artifact("semantic", 1, AuthorityGeneration::new(6))
        .unwrap();
    catalog
        .insert_vector_membership(
            second_artifact.id(),
            SpaceId::from_bytes([1; 16]),
            MemoryId::from_bytes([2; 16]),
            RevisionId::from_bytes([4; 32]),
            "memory:2",
            None,
            "leaf",
            payload.payload_hash,
            AuthorityGeneration::new(6),
        )
        .unwrap();

    assert_eq!(catalog.vector_payload_count().unwrap(), 1);
}
