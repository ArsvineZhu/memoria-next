mod support;

use std::time::Duration;

use memoria_query::MemoryQuery;
use memoria_runtime::{MemoriaRuntime, QueryStep, QueryWork};
use tempfile::tempdir;

fn semantic_query(space: memoria_types::SpaceId, timeout: Duration) -> MemoryQuery {
    MemoryQuery::builder()
        .spaces(vec![space])
        .text_cue("career")
        .require_capability("semantic")
        .wait_for(timeout)
        .build()
        .unwrap()
}

#[test]
fn required_fail_returns_capability_not_ready() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("career"), b"# Career\nRust systems work")
        .unwrap();
    let mut query = semantic_query(space, Duration::from_secs(1));
    query.consistency.readiness = memoria_query::ReadinessBehavior::Fail;

    let error = runtime.query_start(query).unwrap_err();
    assert!(matches!(
        error,
        memoria_runtime::RuntimeError::Query(memoria_query::QueryError::CapabilityNotReady {
            capability,
            ..
        }) if capability == "semantic"
    ));
}

#[test]
fn required_wait_returns_readiness_pending_not_degraded_query() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("career"), b"# Career\nRust systems work")
        .unwrap();

    let step = runtime
        .query_start(semantic_query(space, Duration::from_secs(1)))
        .unwrap();
    assert!(matches!(step, QueryStep::ReadinessPending { .. }));
}

#[test]
fn readiness_continue_completes_after_new_manifest_is_published() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("career"), b"# Career\nRust systems work")
        .unwrap();

    let operation_id = match runtime
        .query_start(semantic_query(space, Duration::from_secs(1)))
        .unwrap()
    {
        QueryStep::ReadinessPending { operation_id, .. } => operation_id,
        other => panic!("expected readiness pending, got {other:?}"),
    };
    support::drain_background_work(&mut runtime, |_| vec![1.0; 64]);

    let continued = runtime.query_continue(&operation_id).unwrap();
    assert!(matches!(
        continued,
        QueryStep::ProviderPending {
            work: QueryWork::Embedding(_),
            ..
        }
    ));
}

#[test]
fn readiness_continue_times_out_as_capability_not_ready() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("career"), b"# Career\nRust systems work")
        .unwrap();

    let operation_id = match runtime
        .query_start(semantic_query(space, Duration::ZERO))
        .unwrap()
    {
        QueryStep::ReadinessPending { operation_id, .. } => operation_id,
        other => panic!("expected readiness pending, got {other:?}"),
    };
    let error = runtime.query_continue(&operation_id).unwrap_err();
    assert!(matches!(
        error,
        memoria_runtime::RuntimeError::Query(memoria_query::QueryError::CapabilityNotReady {
            capability,
            ..
        }) if capability == "semantic"
    ));
}
