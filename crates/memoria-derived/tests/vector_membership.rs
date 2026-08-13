use memoria_derived::{
    VectorArtifact, VectorFilter, VectorIndex, VectorMembership, default_content_signature,
};
use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};

fn artifact() -> VectorArtifact {
    let signature = default_content_signature("test", "model", 3).unwrap();
    let vector = memoria_derived::EmbeddingVector::new(signature, vec![1.0, 0.0, 0.0]).unwrap();
    VectorArtifact::from_embedding(vector)
}

fn membership(
    space: u8,
    memory: u8,
    revision: u8,
    generation: u64,
    artifact: &VectorArtifact,
) -> VectorMembership {
    VectorMembership::new(
        SpaceId::from_bytes([space; 16]),
        MemoryId::from_bytes([memory; 16]),
        RevisionId::from_bytes([revision; 32]),
        AuthorityGeneration::new(generation),
        artifact,
    )
}

#[test]
fn moving_membership_reuses_immutable_payload_and_filters_by_space() {
    let artifact = artifact();
    let first = membership(1, 2, 3, 7, &artifact);
    let moved = membership(4, 2, 3, 8, &artifact);
    assert_eq!(first.payload_hash(), moved.payload_hash());
    assert_ne!(first.space_id(), moved.space_id());
    assert_ne!(first.authority_generation(), moved.authority_generation());

    let mut index = VectorIndex::new(artifact.signature().clone()).unwrap();
    index.insert(artifact.clone(), first).unwrap();
    index.insert(artifact, moved).unwrap();
    assert_eq!(index.payload_count(), 1);
    assert_eq!(index.membership_count(), 2);

    let first_hits = index
        .search(
            &[1.0, 0.0, 0.0],
            10,
            &VectorFilter::any().with_space_id(first.space_id()),
        )
        .unwrap();
    assert_eq!(first_hits.len(), 1);
    assert_eq!(first_hits[0].membership(), first);
    assert!(first_hits[0].score() > 0.99);

    let moved_hits = index
        .search(
            &[1.0, 0.0, 0.0],
            10,
            &VectorFilter::any().with_space_id(moved.space_id()),
        )
        .unwrap();
    assert_eq!(moved_hits.len(), 1);
    assert_eq!(moved_hits[0].membership(), moved);
}

#[test]
fn membership_insert_is_idempotent_but_payload_conflicts_are_rejected() {
    let first_artifact = artifact();
    let first_membership = membership(1, 2, 3, 7, &first_artifact);
    let mut index = VectorIndex::new(first_artifact.signature().clone()).unwrap();
    index
        .insert(first_artifact.clone(), first_membership)
        .unwrap();
    index.insert(first_artifact, first_membership).unwrap();
    assert_eq!(index.payload_count(), 1);
    assert_eq!(index.membership_count(), 1);

    let other_vector = memoria_derived::EmbeddingVector::new(
        default_content_signature("test", "model", 3).unwrap(),
        vec![0.0, 1.0, 0.0],
    )
    .unwrap();
    let other_artifact = VectorArtifact::from_embedding(other_vector);
    let error = index
        .insert(other_artifact, first_membership)
        .expect_err("same authority membership must not change payload");
    assert!(error.to_string().contains("payload"));
}

#[test]
fn vector_queries_validate_dimensions_and_finite_values() {
    let artifact = artifact();
    let index = VectorIndex::new(artifact.signature().clone()).unwrap();
    let dimension_error = index
        .search(&[1.0, 0.0], 1, &VectorFilter::any())
        .expect_err("wrong dimension must be rejected");
    assert!(dimension_error.to_string().contains("dimension mismatch"));

    let non_finite_error = index
        .search(&[f32::NAN, 0.0, 0.0], 1, &VectorFilter::any())
        .expect_err("non-finite queries must be rejected");
    assert!(non_finite_error.to_string().contains("finite"));
}
