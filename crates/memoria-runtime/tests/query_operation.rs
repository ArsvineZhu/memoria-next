mod support;

use std::time::Duration;

use memoria_query::MemoryQuery;
use memoria_runtime::{
    EmbeddingBatchRequest, EmbeddingItem, EmbeddingVector, MemoriaRuntime, NeedWork,
    ProviderRouteConfig, ProviderTrust, ProviderWorkResult, QueryStep, QueryWork, RuntimeError,
    SpaceProviderMode, SpaceProviderPolicy, validate_provider_result,
};
use tempfile::tempdir;

fn embedding_vector(prefix: &[f32]) -> Vec<f32> {
    let mut values = prefix.to_vec();
    values.resize(64, 0.0);
    values
}

fn semantic_query(space: memoria_types::SpaceId) -> MemoryQuery {
    MemoryQuery::builder()
        .spaces(vec![space])
        .text_cue("career")
        .require_capability("semantic")
        .wait_for(Duration::from_secs(1))
        .build()
        .unwrap()
}

fn pending_embedding(
    runtime: &mut MemoriaRuntime,
    space: memoria_types::SpaceId,
) -> (String, String) {
    support::drain_background_work(runtime, |_| vec![1.0; 64]);
    let step = runtime.query_start(semantic_query(space)).unwrap();
    match step {
        QueryStep::ProviderPending {
            operation_id,
            work: QueryWork::Embedding(work),
        } => (operation_id, work.work_id),
        other => panic!("expected pending query embedding work, got {other:?}"),
    }
}

#[test]
fn semantic_query_returns_pending_query_embedding_work() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("career"), b"# Career\nRust systems work")
        .unwrap();

    let (operation_id, work_id) = pending_embedding(&mut runtime, space);
    assert!(!operation_id.starts_with("OP_"));
    assert!(work_id.starts_with("QW_"));

    // Query-time work is not placed on the background Derived provider queue.
    assert!(runtime.provider_poll_work().unwrap().is_none());
}

#[test]
fn semantic_work_uses_one_effective_policy_for_the_whole_scope() {
    let directory = tempdir().unwrap();
    let mut routes = ProviderRouteConfig::default();
    routes.embedding.trust = ProviderTrust::Local;
    let mut runtime = MemoriaRuntime::open_with_provider_routes(directory.path(), routes).unwrap();
    let external = runtime.create_space("external").unwrap();
    let local_only = runtime
        .create_space_with_policy(
            "local-only",
            SpaceProviderPolicy {
                embedding: SpaceProviderMode::LocalOnly,
                ..SpaceProviderPolicy::default()
            },
        )
        .unwrap();
    runtime
        .create_memory(external, Some("career"), b"# Career\nRust systems work")
        .unwrap();
    support::drain_background_work(&mut runtime, |_| vec![1.0; 64]);
    let query = MemoryQuery::builder()
        .spaces(vec![external, local_only])
        .text_cue("career")
        .require_capability("semantic")
        .wait_for(Duration::from_secs(1))
        .build()
        .unwrap();

    let step = runtime.query_start(query).unwrap();
    match step {
        QueryStep::ProviderPending {
            work: QueryWork::Embedding(work),
            ..
        } => assert_eq!(work.space_policy.embedding, SpaceProviderMode::LocalOnly),
        other => panic!("expected pending query embedding work, got {other:?}"),
    }
}

#[test]
fn wrong_work_id_cannot_resume_operation() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("career"), b"# Career\nRust systems work")
        .unwrap();
    let (operation_id, _) = pending_embedding(&mut runtime, space);

    let error = runtime
        .query_resume(
            &operation_id,
            ProviderWorkResult::Embeddings {
                work_id: "wrong-work-id".to_owned(),
                vectors: vec![EmbeddingVector {
                    key: "query".to_owned(),
                    values: vec![0.0; 64],
                }],
            },
        )
        .unwrap_err();
    assert!(matches!(&error, RuntimeError::UnexpectedQueryWork { .. }));
    assert_eq!(error.code(), "PROVIDER_UNAVAILABLE");
}

#[test]
fn invalid_embedding_payload_is_rejected_before_operation_resume() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("career"), b"# Career\nRust systems work")
        .unwrap();
    let (operation_id, work_id) = pending_embedding(&mut runtime, space);

    let error = runtime
        .query_resume(
            &operation_id,
            ProviderWorkResult::Embeddings {
                work_id: work_id.clone(),
                vectors: vec![EmbeddingVector {
                    key: "query".to_owned(),
                    values: vec![0.0, 0.0],
                }],
            },
        )
        .unwrap_err();
    assert!(matches!(&error, RuntimeError::InvalidProviderResult { .. }));
    assert_eq!(error.code(), "PROVIDER_UNAVAILABLE");

    let resumed = runtime
        .query_resume(
            &operation_id,
            ProviderWorkResult::Embeddings {
                work_id,
                vectors: vec![EmbeddingVector {
                    key: "query".to_owned(),
                    values: vec![0.0; 64],
                }],
            },
        )
        .unwrap();
    assert!(matches!(resumed, QueryStep::Complete(_)));
}

#[test]
fn query_provider_failure_preserves_typed_metadata() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("career"), b"# Career\nRust systems work")
        .unwrap();
    let (operation_id, work_id) = pending_embedding(&mut runtime, space);

    let error = runtime
        .query_resume(
            &operation_id,
            ProviderWorkResult::Failure {
                work_id,
                retryable: true,
                code: "RATE_LIMITED".to_owned(),
                message: "provider asked for backoff".to_owned(),
            },
        )
        .unwrap_err();
    assert!(matches!(
        &error,
        RuntimeError::ProviderFailure {
            retryable: true,
            code,
            message,
            ..
        } if code == "RATE_LIMITED" && message == "provider asked for backoff"
    ));
    assert_eq!(error.code(), "PROVIDER_UNAVAILABLE");
}

#[test]
fn rust_embedding_validation_uses_keys_and_reconstructs_request_order() {
    let work = NeedWork::Embeddings(EmbeddingBatchRequest {
        work_id: "EW_test".to_owned(),
        signature: "embedding-v1".to_owned(),
        route: memoria_runtime::ProviderRouteConfig::default().embedding,
        dimensions: 64,
        items: vec![
            EmbeddingItem {
                key: "u1".to_owned(),
                text: "one".to_owned(),
            },
            EmbeddingItem {
                key: "u2".to_owned(),
                text: "two".to_owned(),
            },
        ],
        space_policy: memoria_authority::SpaceProviderPolicy::default(),
    });
    let result = validate_provider_result(
        &work,
        ProviderWorkResult::Embeddings {
            work_id: "EW_test".to_owned(),
            vectors: vec![
                EmbeddingVector {
                    key: "u2".to_owned(),
                    values: embedding_vector(&[0.0, 1.0, 0.0]),
                },
                EmbeddingVector {
                    key: "u1".to_owned(),
                    values: vec![1.0; 64],
                },
            ],
        },
    )
    .unwrap();
    let ProviderWorkResult::Embeddings { vectors, .. } = result else {
        panic!("expected embedding result");
    };
    assert_eq!(vectors[0].key, "u1");
    assert_eq!(vectors[1].key, "u2");

    let duplicate = validate_provider_result(
        &work,
        ProviderWorkResult::Embeddings {
            work_id: "EW_test".to_owned(),
            vectors: vec![
                EmbeddingVector {
                    key: "u1".to_owned(),
                    values: vec![1.0; 64],
                },
                EmbeddingVector {
                    key: "u1".to_owned(),
                    values: embedding_vector(&[0.0, 1.0, 0.0]),
                },
            ],
        },
    )
    .unwrap_err();
    assert!(matches!(
        duplicate,
        memoria_runtime::ProviderResultValidationError::EmbeddingKeys
    ));
}

#[test]
fn cancelled_operation_cannot_resume() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("career"), b"# Career\nRust systems work")
        .unwrap();
    let (operation_id, work_id) = pending_embedding(&mut runtime, space);
    runtime.cancel_operation(&operation_id).unwrap();

    let error = runtime
        .query_resume(
            &operation_id,
            ProviderWorkResult::Embeddings {
                work_id,
                vectors: vec![EmbeddingVector {
                    key: "query".to_owned(),
                    values: vec![0.0; 64],
                }],
            },
        )
        .unwrap_err();
    assert!(matches!(
        &error,
        RuntimeError::QueryOperationCancelled { .. }
    ));
    assert_eq!(error.code(), "ABORTED");
}

#[test]
fn expired_operation_returns_query_operation_expired() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("career"), b"# Career\nRust systems work")
        .unwrap();
    let (operation_id, work_id) = pending_embedding(&mut runtime, space);
    runtime.expire_query_operation_for_test(&operation_id);

    let error = runtime
        .query_resume(
            &operation_id,
            ProviderWorkResult::Embeddings {
                work_id,
                vectors: vec![EmbeddingVector {
                    key: "query".to_owned(),
                    values: vec![0.0; 64],
                }],
            },
        )
        .unwrap_err();
    assert!(matches!(&error, RuntimeError::QueryOperationExpired { .. }));
    assert_eq!(error.code(), "CONTINUATION_EXPIRED");
}

#[test]
fn status_cleanup_removes_expired_operation() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("career"), b"# Career\nRust systems work")
        .unwrap();
    let (operation_id, work_id) = pending_embedding(&mut runtime, space);
    runtime.expire_query_operation_for_test(&operation_id);
    let _ = runtime.status();

    let error = runtime
        .query_resume(
            &operation_id,
            ProviderWorkResult::Embeddings {
                work_id,
                vectors: vec![EmbeddingVector {
                    key: "query".to_owned(),
                    values: vec![0.0; 64],
                }],
            },
        )
        .unwrap_err();
    assert!(matches!(
        &error,
        RuntimeError::QueryOperationNotFound { .. }
    ));
    assert_eq!(error.code(), "SNAPSHOT_UNAVAILABLE");
}
