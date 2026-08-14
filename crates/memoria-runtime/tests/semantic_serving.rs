use memoria_query::{EntityRef, MemoryQuery};
use memoria_runtime::{EmbeddingVector, MemoriaRuntime, NeedWork, ProviderWorkResult, QueryStep};
use tempfile::tempdir;

fn submit_content_embedding(runtime: &mut MemoriaRuntime, values: Vec<f32>) {
    let request = loop {
        let work = runtime.provider_poll_work().unwrap().unwrap();
        match work {
            NeedWork::Embeddings(request) => break request,
            NeedWork::Enrichment(request) => runtime
                .provider_submit_result(ProviderWorkResult::Enrichment {
                    work_id: request.work_id,
                    tags: Vec::new(),
                })
                .unwrap(),
            NeedWork::Rerank(_) => panic!("unexpected rerank work"),
        }
    };
    runtime
        .provider_submit_result(ProviderWorkResult::Embeddings {
            work_id: request.work_id,
            vectors: request
                .items
                .into_iter()
                .map(|item| EmbeddingVector {
                    key: item.key,
                    values: values.clone(),
                })
                .collect(),
        })
        .unwrap();
}

fn query_embedding_work_id(step: QueryStep) -> (String, String) {
    let QueryStep::ProviderPending {
        operation_id,
        work: memoria_runtime::QueryWork::Embedding(request),
    } = step
    else {
        panic!("expected pending query embedding");
    };
    (operation_id, request.work_id)
}

#[test]
fn explicit_semantic_query_uses_query_vector_and_ann_candidates() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("one"), b"# One\nRust systems")
        .unwrap();
    submit_content_embedding(&mut runtime, vec![1.0, 0.0, 0.0]);

    let (operation_id, work_id) = query_embedding_work_id(
        runtime
            .query_start(
                MemoryQuery::builder()
                    .spaces(vec![space])
                    .text_cue("systems")
                    .require_capability("semantic")
                    .build()
                    .unwrap(),
            )
            .unwrap(),
    );
    let step = runtime
        .query_resume(
            &operation_id,
            ProviderWorkResult::Embeddings {
                work_id,
                vectors: vec![EmbeddingVector {
                    key: "query".to_owned(),
                    values: vec![1.0, 0.0, 0.0],
                }],
            },
        )
        .unwrap();
    let QueryStep::Complete(response) = step else {
        panic!("expected semantic query completion");
    };
    assert_eq!(response.results.len(), 1);
    assert!(!response.results[0].matches[0].evidence.semantic.is_empty());
    assert!(!response.execution.degraded);
}

#[test]
fn text_only_query_does_not_open_semantic_index() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("one"), b"# One\nRust")
        .unwrap();
    let step = runtime
        .query_start(
            MemoryQuery::builder()
                .spaces(vec![space])
                .text_cue("Rust")
                .build()
                .unwrap(),
        )
        .unwrap();
    assert!(matches!(step, QueryStep::Complete(_)));
}

#[test]
fn semantic_candidate_is_revision_pinned_and_space_filtered() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let first_space = runtime.create_space("first").unwrap();
    let second_space = runtime.create_space("second").unwrap();
    runtime
        .create_memory(first_space, Some("one"), b"# One\nfirst")
        .unwrap();
    submit_content_embedding(&mut runtime, vec![1.0, 0.0, 0.0]);
    runtime
        .create_memory(second_space, Some("two"), b"# Two\nsecond")
        .unwrap();
    submit_content_embedding(&mut runtime, vec![1.0, 0.0, 0.0]);

    let (operation_id, work_id) = query_embedding_work_id(
        runtime
            .query_start(
                MemoryQuery::builder()
                    .spaces(vec![first_space])
                    .require_capability("semantic")
                    .build()
                    .unwrap(),
            )
            .unwrap(),
    );
    let QueryStep::Complete(response) = runtime
        .query_resume(
            &operation_id,
            ProviderWorkResult::Embeddings {
                work_id,
                vectors: vec![EmbeddingVector {
                    key: "query".to_owned(),
                    values: vec![1.0, 0.0, 0.0],
                }],
            },
        )
        .unwrap()
    else {
        panic!("expected query completion");
    };
    assert!(
        response
            .results
            .iter()
            .all(|result| result.space_id == first_space)
    );
}

#[test]
fn hard_entity_constraint_filters_ann_candidates_before_public_response() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(
            space,
            Some("one"),
            br#"# One
<Entity ref="person:alice">Alice</Entity>
"#,
        )
        .unwrap();
    submit_content_embedding(&mut runtime, vec![1.0, 0.0, 0.0]);
    let (operation_id, work_id) = query_embedding_work_id(
        runtime
            .query_start(
                MemoryQuery::builder()
                    .spaces(vec![space])
                    .require_capability("semantic")
                    .require_entity(EntityRef::new("person:nobody").unwrap())
                    .build()
                    .unwrap(),
            )
            .unwrap(),
    );
    let QueryStep::Complete(response) = runtime
        .query_resume(
            &operation_id,
            ProviderWorkResult::Embeddings {
                work_id,
                vectors: vec![EmbeddingVector {
                    key: "query".to_owned(),
                    values: vec![1.0, 0.0, 0.0],
                }],
            },
        )
        .unwrap()
    else {
        panic!("expected query completion");
    };
    assert!(response.results.is_empty());
}
