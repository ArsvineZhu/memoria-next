use std::time::Duration;

use memoria_query::MemoryQuery;
use memoria_runtime::{
    MemoriaRuntime, NeedWork, ProviderWorkResult, QueryStep, QueryWork, RuntimeError,
};
use tempfile::tempdir;

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
    let step = runtime.query_start(semantic_query(space)).unwrap();
    match step {
        QueryStep::Pending {
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
    assert!(matches!(
        runtime.provider_poll_work().unwrap(),
        Some(NeedWork::Embeddings(_))
    ));
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
            ProviderWorkResult {
                work_id: "wrong-work-id".to_owned(),
                accepted: true,
                scores: Vec::new(),
                tags: Vec::new(),
            },
        )
        .unwrap_err();
    assert!(matches!(error, RuntimeError::UnexpectedQueryWork { .. }));
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
            ProviderWorkResult {
                work_id,
                accepted: true,
                scores: Vec::new(),
                tags: Vec::new(),
            },
        )
        .unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::QueryOperationCancelled { .. }
    ));
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
            ProviderWorkResult {
                work_id,
                accepted: true,
                scores: Vec::new(),
                tags: Vec::new(),
            },
        )
        .unwrap_err();
    assert!(matches!(error, RuntimeError::QueryOperationExpired { .. }));
}
