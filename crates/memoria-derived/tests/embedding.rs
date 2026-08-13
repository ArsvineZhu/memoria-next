use memoria_derived::{
    EmbeddingNormalization, EmbeddingPayloadCache, EmbeddingResult, EmbeddingResultItem,
    EmbeddingSignature, EmbeddingVector, EmbeddingWork, EmbeddingWorkItem, ProjectionInputHash,
    ProjectionKind, validate_embedding_result,
};

fn signature(dimensions: usize) -> EmbeddingSignature {
    EmbeddingSignature::new(
        "test-provider",
        "test-model",
        "content",
        dimensions,
        EmbeddingNormalization::L2,
        1,
    )
    .unwrap()
}

fn work(signature: EmbeddingSignature) -> EmbeddingWork {
    EmbeddingWork {
        work_id: "W1".to_owned(),
        signature,
        items: vec![EmbeddingWorkItem {
            key: "u1".to_owned(),
            input_hash: ProjectionInputHash::new(ProjectionKind::Ir, b"source", "producer"),
        }],
    }
}

#[test]
fn wrong_embedding_dimension_is_rejected_before_cache_insert() {
    let work = work(signature(3));
    let result = EmbeddingResult {
        work_id: "W1".to_owned(),
        signature: work.signature.clone(),
        items: vec![EmbeddingResultItem {
            key: "u1".to_owned(),
            values: vec![1.0, 2.0],
        }],
    };
    assert!(validate_embedding_result(&work, &result).is_err());
}

#[test]
fn cache_identity_includes_provider_model_dimension_and_input_hash() {
    let input = ProjectionInputHash::new(ProjectionKind::Ir, b"source", "producer");
    let first = signature(3);
    let second = EmbeddingSignature::new(
        "test-provider",
        "other-model",
        "content",
        3,
        EmbeddingNormalization::L2,
        1,
    )
    .unwrap();
    assert_ne!(first.identity_hash(&input), second.identity_hash(&input));
    assert_ne!(first.cache_key(input.clone()), second.cache_key(input));
}

#[test]
fn reusable_payload_cache_rejects_conflicting_payload_for_same_identity() {
    let input = ProjectionInputHash::new(ProjectionKind::Ir, b"source", "producer");
    let signature = signature(2);
    let key = signature.cache_key(input);
    let first = EmbeddingVector::new(signature.clone(), vec![1.0, 0.0]).unwrap();
    let second = EmbeddingVector::new(signature, vec![0.0, 1.0]).unwrap();
    let mut cache = EmbeddingPayloadCache::new();
    cache.insert(key.clone(), first).unwrap();
    assert!(cache.insert(key.clone(), second).is_err());
    assert_eq!(cache.len(), 1);
}
