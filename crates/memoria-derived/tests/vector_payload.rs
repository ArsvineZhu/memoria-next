use std::fs;

use memoria_derived::{EmbeddingNormalization, ProjectionInputHash, VectorPayloadV1};
use tempfile::tempdir;

fn payload(producer: &str) -> VectorPayloadV1 {
    VectorPayloadV1::new(
        vec![0.25, 0.5, 0.25],
        EmbeddingNormalization::L2,
        producer,
        ProjectionInputHash::new(
            memoria_derived::ProjectionKind::LocalEmbedding,
            b"career",
            "memoria-derived-v1",
        ),
    )
    .unwrap()
}

#[test]
fn equal_vector_payloads_have_equal_hash_and_bytes() {
    let first = payload("provider:model:v1");
    let second = payload("provider:model:v1");

    assert_eq!(first.canonical_bytes(), second.canonical_bytes());
    assert_eq!(first.hash(), second.hash());
}

#[test]
fn producer_or_projection_signature_changes_payload_hash() {
    let first = payload("provider:model:v1");
    let producer_changed = payload("provider:model:v2");
    let projection_changed = VectorPayloadV1::new(
        first.values().to_vec(),
        first.normalization(),
        "provider:model:v1",
        ProjectionInputHash::new(
            memoria_derived::ProjectionKind::LocalEmbedding,
            b"different projection",
            "memoria-derived-v1",
        ),
    )
    .unwrap();

    assert_ne!(first.hash(), producer_changed.hash());
    assert_ne!(first.hash(), projection_changed.hash());
}

#[test]
fn corrupt_vector_payload_fails_checksum_or_hash_validation() {
    let directory = tempdir().unwrap();
    let payload = payload("provider:model:v1");
    let hash = payload.put(directory.path()).unwrap();
    let path = directory
        .path()
        .join("objects/vector")
        .join(&hash.as_hex()[..2])
        .join(format!("{hash}.vec"));
    let mut bytes = fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    fs::write(&path, bytes).unwrap();

    assert!(VectorPayloadV1::verify(directory.path(), hash).is_err());
}

#[test]
fn non_finite_vector_cannot_be_persisted() {
    let error = VectorPayloadV1::new(
        vec![f32::NAN],
        EmbeddingNormalization::None,
        "provider:model:v1",
        ProjectionInputHash::new(
            memoria_derived::ProjectionKind::LocalEmbedding,
            b"career",
            "memoria-derived-v1",
        ),
    )
    .expect_err("non-finite payload values must be rejected");
    assert!(error.to_string().contains("finite"));
}
