mod support;

use memoria_query::{EntityRef, MemoryQuery};
use memoria_runtime::{EmbeddingVector, MemoriaRuntime, ProviderWorkResult, QueryStep, QueryWork};
use tempfile::tempdir;

fn query_embedding_step(
    runtime: &mut MemoriaRuntime,
    query: MemoryQuery,
    values: Vec<f32>,
) -> memoria_query::RetrievalResponse {
    let QueryStep::ProviderPending {
        operation_id,
        work: QueryWork::Embedding(request),
    } = runtime.query_start(query).unwrap()
    else {
        panic!("expected query embedding work");
    };
    let QueryStep::Complete(response) = runtime
        .query_resume(
            &operation_id,
            ProviderWorkResult::Embeddings {
                work_id: request.work_id,
                vectors: vec![EmbeddingVector {
                    key: "query".to_owned(),
                    values,
                }],
            },
        )
        .unwrap()
    else {
        panic!("expected query completion");
    };
    response
}

#[test]
fn semantic_query_also_executes_lexical() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(
            space,
            Some("career"),
            b"# Career\nunique lexical and semantic token",
        )
        .unwrap();
    support::drain_background_work(&mut runtime, |_| vec![1.0, 0.0, 0.0]);

    let response = query_embedding_step(
        &mut runtime,
        MemoryQuery::builder()
            .spaces(vec![space])
            .text_cue("unique lexical and semantic token")
            .require_capability("semantic")
            .build()
            .unwrap(),
        vec![1.0, 0.0, 0.0],
    );

    let evidence = &response.results[0].matches[0].evidence;
    assert!(!evidence.lexical.is_empty());
    assert!(!evidence.semantic.is_empty());
}

#[test]
fn same_target_from_lexical_and_semantic_merges_evidence_not_target_count() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let memory_id = runtime
        .create_memory(space, Some("one"), b"# One\nshared retrieval cue")
        .unwrap();
    support::drain_background_work(&mut runtime, |_| vec![1.0, 0.0, 0.0]);

    let response = query_embedding_step(
        &mut runtime,
        MemoryQuery::builder()
            .spaces(vec![space])
            .text_cue("shared retrieval cue")
            .require_capability("semantic")
            .build()
            .unwrap(),
        vec![1.0, 0.0, 0.0],
    );

    assert_eq!(
        response
            .results
            .iter()
            .filter(|result| result.memory_id == memory_id)
            .count(),
        1
    );
    let result = response
        .results
        .iter()
        .find(|result| result.memory_id == memory_id)
        .unwrap();
    let evidence = &result.matches[0].evidence;
    assert!(!evidence.lexical.is_empty());
    assert!(!evidence.semantic.is_empty());
}

#[test]
fn hard_entity_constraint_filters_all_channels() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(
            space,
            Some("alice"),
            b"# Alice\n<Entity ref=\"person:alice\">Alice</Entity> shared cue",
        )
        .unwrap();
    runtime
        .create_memory(space, Some("other"), b"# Other\nshared cue")
        .unwrap();
    support::drain_background_work(&mut runtime, |_| vec![1.0, 0.0, 0.0]);

    let response = query_embedding_step(
        &mut runtime,
        MemoryQuery::builder()
            .spaces(vec![space])
            .text_cue("shared cue")
            .require_capability("semantic")
            .require_entity(EntityRef::new("person:alice").unwrap())
            .build()
            .unwrap(),
        vec![1.0, 0.0, 0.0],
    );

    assert_eq!(response.results.len(), 1);
    assert!(response.results[0].matches.iter().all(|result| {
        result
            .evidence
            .contains_entity(&EntityRef::new("person:alice").unwrap())
    }));
}

#[test]
fn exact_candidate_survives_even_if_semantic_rank_is_low() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let exact_memory = runtime
        .create_memory(space, Some("exact"), b"# Exact\nshared retrieval cue")
        .unwrap();
    runtime
        .create_memory(space, Some("semantic"), b"# Semantic\nshared retrieval cue")
        .unwrap();
    support::drain_background_work(&mut runtime, |_| vec![0.0, 1.0, 0.0]);

    let response = query_embedding_step(
        &mut runtime,
        MemoryQuery::builder()
            .spaces(vec![space])
            .text_cue("shared retrieval cue")
            .cue_memory(exact_memory)
            .require_capability("semantic")
            .build()
            .unwrap(),
        vec![1.0, 0.0, 0.0],
    );

    assert!(
        response
            .results
            .iter()
            .any(|result| result.memory_id == exact_memory)
    );
}
