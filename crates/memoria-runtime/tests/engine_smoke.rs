use memoria_query::MemoryQuery;
use memoria_runtime::{MemoriaRuntime, NeedWork, ProviderWorkResult};
use tempfile::tempdir;

#[test]
fn runtime_can_commit_base_ready_memory_and_query_it() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("career"), b"# Career\nRust")
        .unwrap();
    let query = MemoryQuery::builder()
        .spaces(vec![space])
        .text_cue("Rust")
        .build()
        .unwrap();
    let response = runtime.query(query).unwrap();
    assert_eq!(response.results.len(), 1);
    assert!(!response.execution.degraded);
}

#[test]
fn runtime_status_starts_at_authority_generation_zero() {
    let directory = tempdir().unwrap();
    let runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let status = runtime.status();
    assert_eq!(status.authority_generation.value(), 0);
    assert!(!status.closed);
}

#[test]
fn provider_work_is_queued_after_commit_and_advances_semantic_coverage() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let memory_id = runtime
        .create_memory(space, Some("career"), b"# Career\nRust")
        .unwrap();

    let before = runtime.status();
    assert_eq!(before.authority_generation.value(), 2);
    assert_eq!(before.semantic_coverage.value(), 0);

    let work = runtime.provider_poll_work().unwrap().unwrap();
    let work_id = match work {
        NeedWork::Embeddings(request) => {
            assert_eq!(request.items[0].key, memory_id.to_string());
            request.work_id
        }
        other => panic!("unexpected provider work: {other:?}"),
    };
    runtime
        .provider_submit_result(ProviderWorkResult {
            work_id,
            accepted: true,
            scores: Vec::new(),
            tags: Vec::new(),
        })
        .unwrap();

    assert_eq!(runtime.status().semantic_coverage.value(), 2);
}
