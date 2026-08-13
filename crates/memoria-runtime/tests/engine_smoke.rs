use memoria_query::MemoryQuery;
use memoria_runtime::MemoriaRuntime;
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
