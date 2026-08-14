use memoria_derived::{
    BuildJobState, DerivedCatalog, EmbeddingNormalization, ProjectionInputHash, VectorPayloadHash,
    VectorPayloadRecord,
};
use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};
use tempfile::tempdir;

fn payload_record() -> VectorPayloadRecord {
    let hash = VectorPayloadHash::from_bytes([0x11; 32]);
    VectorPayloadRecord {
        payload_hash: hash,
        dimension: 3,
        normalization: EmbeddingNormalization::L2,
        producer_signature: "provider:model:v1".to_owned(),
        projection_input_hash: ProjectionInputHash::from_bytes([0x22; 32]),
        object_path:
            "objects/vector/11/1111111111111111111111111111111111111111111111111111111111111111.vec"
                .to_owned(),
        byte_length: 99,
        checksum: [0x11; 32],
        created_at: 1,
    }
}

#[test]
fn one_payload_can_have_memberships_in_multiple_revisions_or_spaces() {
    let directory = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(directory.path().join("catalog.sqlite")).unwrap();
    let payload = payload_record();
    catalog.register_vector_payload(&payload).unwrap();
    let artifact = catalog
        .stage_artifact("semantic", 1, AuthorityGeneration::new(2))
        .unwrap();

    catalog
        .insert_vector_membership(
            artifact.id(),
            SpaceId::from_bytes([1; 16]),
            MemoryId::from_bytes([2; 16]),
            RevisionId::from_bytes([3; 32]),
            "unit-a",
            Some("node-a"),
            "leaf",
            payload.payload_hash,
            AuthorityGeneration::new(2),
        )
        .unwrap();
    catalog
        .insert_vector_membership(
            artifact.id(),
            SpaceId::from_bytes([4; 16]),
            MemoryId::from_bytes([2; 16]),
            RevisionId::from_bytes([5; 32]),
            "unit-b",
            Some("node-b"),
            "section",
            payload.payload_hash,
            AuthorityGeneration::new(2),
        )
        .unwrap();

    assert_eq!(catalog.vector_payload_count().unwrap(), 1);
    assert_eq!(
        catalog
            .vector_memberships_for_artifact(artifact.id())
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn moving_memory_changes_membership_without_rewriting_payload() {
    let directory = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(directory.path().join("catalog.sqlite")).unwrap();
    let payload = payload_record();
    catalog.register_vector_payload(&payload).unwrap();
    let artifact = catalog
        .stage_artifact("semantic", 1, AuthorityGeneration::new(3))
        .unwrap();
    let first = catalog
        .insert_vector_membership(
            artifact.id(),
            SpaceId::from_bytes([1; 16]),
            MemoryId::from_bytes([2; 16]),
            RevisionId::from_bytes([3; 32]),
            "unit-a",
            None,
            "leaf",
            payload.payload_hash,
            AuthorityGeneration::new(3),
        )
        .unwrap();
    let moved = catalog
        .insert_vector_membership(
            artifact.id(),
            SpaceId::from_bytes([4; 16]),
            MemoryId::from_bytes([2; 16]),
            RevisionId::from_bytes([3; 32]),
            "unit-a",
            None,
            "leaf",
            payload.payload_hash,
            AuthorityGeneration::new(3),
        )
        .unwrap();

    assert_ne!(first.membership_id, moved.membership_id);
    assert_eq!(first.payload_hash, moved.payload_hash);
    assert_eq!(catalog.vector_payload_count().unwrap(), 1);
}

#[test]
fn running_job_is_requeued_after_reopen() {
    let directory = tempdir().unwrap();
    let job_id;
    {
        let mut catalog = DerivedCatalog::open(directory.path().join("catalog.sqlite")).unwrap();
        let job = catalog
            .enqueue_build_job(
                "local-embedding",
                "input-hash",
                "provider:model:v1",
                AuthorityGeneration::new(4),
            )
            .unwrap();
        job_id = job.job_id.clone();
        let running = catalog.mark_build_job_running(&job.job_id).unwrap();
        assert_eq!(running.state, BuildJobState::Running);
    }

    let catalog = DerivedCatalog::open(directory.path().join("catalog.sqlite")).unwrap();
    let recovered = catalog.build_job(&job_id).unwrap();
    assert_eq!(recovered.state, BuildJobState::Queued);
    assert_eq!(recovered.attempt_count, 1);
}

#[test]
fn build_job_key_deduplicates_same_projection_and_producer() {
    let directory = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(directory.path().join("catalog.sqlite")).unwrap();
    let first = catalog
        .enqueue_build_job(
            "local-embedding",
            "input-hash",
            "provider:model:v1",
            AuthorityGeneration::new(4),
        )
        .unwrap();
    let second = catalog
        .enqueue_build_job(
            "local-embedding",
            "input-hash",
            "provider:model:v1",
            AuthorityGeneration::new(5),
        )
        .unwrap();

    assert_eq!(first.job_id, second.job_id);
    assert_eq!(catalog.build_job(&first.job_id).unwrap().attempt_count, 0);
}
