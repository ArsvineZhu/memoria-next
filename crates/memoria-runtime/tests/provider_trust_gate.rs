mod support;

use memoria_query::MemoryQuery;
use memoria_runtime::{
    MemoriaRuntime, NeedWork, ProviderCapability, ProviderRouteConfig, ProviderTrust, QueryStep,
    RuntimeError,
};
use tempfile::tempdir;

fn local_only_embedding_policy() -> memoria_runtime::SpaceProviderPolicy {
    memoria_runtime::SpaceProviderPolicy {
        embedding: memoria_runtime::SpaceProviderMode::LocalOnly,
        ..memoria_runtime::SpaceProviderPolicy::default()
    }
}

fn semantic_query(space: memoria_types::SpaceId, required: bool) -> MemoryQuery {
    let mut builder = MemoryQuery::builder()
        .spaces(vec![space])
        .text_cue("private retrieval cue");
    builder = if required {
        builder.require_capability("semantic")
    } else {
        builder.prefer_capability("semantic")
    };
    builder.build().unwrap()
}

fn runtime_with_ready_local_only_semantic()
-> (tempfile::TempDir, MemoriaRuntime, memoria_types::SpaceId) {
    let directory = tempdir().unwrap();
    let mut routes = ProviderRouteConfig::default();
    routes.embedding.trust = ProviderTrust::Local;
    let mut runtime = MemoriaRuntime::open_with_provider_routes(directory.path(), routes).unwrap();
    let space = runtime
        .create_space_with_policy("private", local_only_embedding_policy())
        .unwrap();
    runtime
        .create_memory(space, Some("private"), b"# Private\nlocal retrieval cue")
        .unwrap();
    support::drain_background_work(&mut runtime, |_| vec![1.0; 64]);
    drop(runtime);
    let runtime = MemoriaRuntime::open(directory.path()).unwrap();
    (directory, runtime, space)
}

#[test]
fn rust_does_not_emit_external_embedding_work_for_local_only_space() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime
        .create_space_with_policy("private", local_only_embedding_policy())
        .unwrap();
    runtime
        .create_memory(space, Some("private"), b"# Private\nlocal source")
        .unwrap();

    assert_no_external_embedding_work(&mut runtime);
}

#[test]
fn rust_emits_local_embedding_work_for_local_only_space() {
    let directory = tempdir().unwrap();
    let mut routes = ProviderRouteConfig::default();
    routes.embedding.trust = ProviderTrust::Local;
    let mut runtime = MemoriaRuntime::open_with_provider_routes(directory.path(), routes).unwrap();
    let space = runtime
        .create_space_with_policy("private", local_only_embedding_policy())
        .unwrap();
    runtime
        .create_memory(space, Some("private"), b"# Private\nlocal source")
        .unwrap();

    let Some(NeedWork::Embeddings(work)) = runtime.provider_poll_work().unwrap() else {
        panic!("expected local embedding work");
    };
    assert_eq!(work.route.capability, ProviderCapability::Embedding);
    assert_eq!(work.route.trust, ProviderTrust::Local);
}

#[test]
fn required_external_denial_returns_provider_policy_denied_before_napi_work() {
    let (_directory, mut runtime, space) = runtime_with_ready_local_only_semantic();

    let error = runtime
        .query_start(semantic_query(space, true))
        .unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::ProviderPolicyDenied {
            capability: ProviderCapability::Embedding
        }
    ));
    assert_no_external_embedding_work(&mut runtime);
}

#[test]
fn preferred_external_denial_degrades_before_napi_work() {
    let (_directory, mut runtime, space) = runtime_with_ready_local_only_semantic();

    let QueryStep::Complete(response) = runtime.query_start(semantic_query(space, false)).unwrap()
    else {
        panic!("preferred denied semantic capability should degrade locally");
    };
    assert!(response.execution.degraded);
    assert!(
        response
            .execution
            .degraded_capabilities
            .iter()
            .any(|capability| capability == "semantic")
    );
    assert_no_external_embedding_work(&mut runtime);
}

fn assert_no_external_embedding_work(runtime: &mut MemoriaRuntime) {
    while let Some(work) = runtime.provider_poll_work().unwrap() {
        match work {
            NeedWork::Embeddings(request) => {
                assert_ne!(request.route.trust, ProviderTrust::External);
                panic!("unexpected embedding work for local-only test");
            }
            NeedWork::Enrichment(request) => runtime
                .provider_submit_result(memoria_runtime::ProviderWorkResult::Enrichment {
                    work_id: request.work_id,
                    tags: Vec::new(),
                })
                .unwrap(),
            NeedWork::Rerank(request) => {
                panic!("unexpected background rerank work: {}", request.work_id)
            }
        }
    }
}
