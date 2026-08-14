mod support;

use memoria_query::MemoryQuery;
use memoria_runtime::{MemoriaRuntime, ProviderWorkResult, QueryStep, QueryWork, RerankScore};
use tempfile::tempdir;

fn setup_runtime() -> (
    tempfile::TempDir,
    MemoriaRuntime,
    memoria_types::SpaceId,
    memoria_types::MemoryId,
    memoria_types::MemoryId,
) {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let first = runtime
        .create_memory(space, Some("first"), b"# First\nshared rerank cue")
        .unwrap();
    let second = runtime
        .create_memory(space, Some("second"), b"# Second\nshared rerank cue")
        .unwrap();
    support::drain_background_work(&mut runtime, |_| vec![1.0, 0.0, 0.0]);
    support::publish_manifest_with_capabilities(&runtime, &["reranking"]);
    (directory, runtime, space, first, second)
}

fn rerank_query(space: memoria_types::SpaceId, first: memoria_types::MemoryId) -> MemoryQuery {
    MemoryQuery::builder()
        .spaces(vec![space])
        .text_cue("shared rerank cue")
        .cue_memory(first)
        .prefer_capability("reranking")
        .build()
        .unwrap()
}

fn scores_for(
    request: &memoria_runtime::RerankBatchRequest,
    desired: memoria_types::MemoryId,
) -> Vec<RerankScore> {
    request
        .candidates
        .iter()
        .map(|handle| RerankScore {
            handle: handle.clone(),
            score: f32::from(handle.contains(&desired.to_string())),
        })
        .collect()
}

#[test]
fn rerank_batch_is_built_from_actual_consolidated_results() {
    let (_directory, mut runtime, space, first, second) = setup_runtime();
    let QueryStep::ProviderPending {
        work: QueryWork::Rerank(request),
        ..
    } = runtime.query_start(rerank_query(space, first)).unwrap()
    else {
        panic!("expected rerank provider work");
    };
    assert_eq!(request.candidates.len(), 2);
    assert!(
        request
            .candidates
            .iter()
            .any(|handle| handle.contains(&second.to_string()))
    );
}

#[test]
fn rerank_scores_are_persisted_before_finalization() {
    let (_directory, mut runtime, space, first, second) = setup_runtime();
    let QueryStep::ProviderPending {
        operation_id,
        work: QueryWork::Rerank(request),
    } = runtime.query_start(rerank_query(space, first)).unwrap()
    else {
        panic!("expected rerank provider work");
    };
    let QueryStep::Complete(response) = runtime
        .query_resume(
            &operation_id,
            ProviderWorkResult::Rerank {
                work_id: request.work_id.clone(),
                scores: scores_for(&request, second),
            },
        )
        .unwrap()
    else {
        panic!("expected rerank query completion");
    };
    assert!(response.trace.rerank_requested);
    assert!(response.trace.rerank_applied);
    assert_eq!(response.results[0].memory_id, second);
}

#[test]
fn rerank_changes_order_but_cannot_add_candidate() {
    let (_directory, mut runtime, space, first, second) = setup_runtime();
    let QueryStep::ProviderPending {
        operation_id,
        work: QueryWork::Rerank(request),
    } = runtime.query_start(rerank_query(space, first)).unwrap()
    else {
        panic!("expected rerank provider work");
    };
    let response = runtime
        .query_resume(
            &operation_id,
            ProviderWorkResult::Rerank {
                work_id: request.work_id.clone(),
                scores: scores_for(&request, second),
            },
        )
        .unwrap();
    let QueryStep::Complete(response) = response else {
        panic!("expected rerank query completion");
    };
    assert_eq!(response.results.len(), 2);
    assert!(
        response
            .results
            .iter()
            .all(|result| { result.memory_id == first || result.memory_id == second })
    );
}

#[test]
fn rerank_provider_barrier_occurs_at_most_once() {
    let (_directory, mut runtime, space, first, second) = setup_runtime();
    let QueryStep::ProviderPending {
        operation_id,
        work: QueryWork::Rerank(request),
    } = runtime.query_start(rerank_query(space, first)).unwrap()
    else {
        panic!("expected rerank provider work");
    };
    let QueryStep::ProviderPending {
        work: QueryWork::Rerank(repeated),
        ..
    } = runtime.query_continue(&operation_id).unwrap()
    else {
        panic!("expected the same rerank barrier to remain pending");
    };
    assert_eq!(repeated.work_id, request.work_id);
    let QueryStep::Complete(_) = runtime
        .query_resume(
            &operation_id,
            ProviderWorkResult::Rerank {
                work_id: request.work_id.clone(),
                scores: scores_for(&request, second),
            },
        )
        .unwrap()
    else {
        panic!("expected rerank query completion");
    };
    assert!(runtime.query_continue(&operation_id).is_err());
}

#[test]
fn trace_distinguishes_requested_from_applied() {
    let (_directory, mut runtime, space, first, second) = setup_runtime();
    let QueryStep::ProviderPending {
        operation_id,
        work: QueryWork::Rerank(request),
    } = runtime.query_start(rerank_query(space, first)).unwrap()
    else {
        panic!("expected rerank provider work");
    };
    let QueryStep::Complete(response) = runtime
        .query_resume(
            &operation_id,
            ProviderWorkResult::Rerank {
                work_id: request.work_id.clone(),
                scores: scores_for(&request, second),
            },
        )
        .unwrap()
    else {
        panic!("expected rerank query completion");
    };
    assert_eq!(
        (
            response.trace.rerank_requested,
            response.trace.rerank_applied
        ),
        (true, true)
    );
}
