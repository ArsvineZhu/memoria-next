mod support;

use std::time::Duration;

use memoria_query::{AdaptiveSnapshotIdentity, MemoryQuery};
use memoria_runtime::{MemoriaRuntime, ProviderWorkResult, QueryStep, QueryWork, RerankScore};
use tempfile::tempdir;

#[test]
fn semantic_does_not_replace_lexical_channel() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let memory_id = runtime
        .create_memory(space, Some("lexical"), b"# Lexical\nunique-lexical-token")
        .unwrap();
    support::drain_background_work(&mut runtime, |_| vec![1.0, 0.0, 0.0]);

    let query = MemoryQuery::builder()
        .spaces(vec![space])
        .text_cue("unique-lexical-token")
        .require_capability("semantic")
        .build()
        .unwrap();
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
                vectors: vec![memoria_runtime::EmbeddingVector {
                    key: "query".to_owned(),
                    values: vec![1.0, 0.0, 0.0],
                }],
            },
        )
        .unwrap()
    else {
        panic!("expected query completion");
    };

    let result = response
        .results
        .iter()
        .find(|result| result.memory_id == memory_id)
        .expect("lexical memory should remain a result");
    assert!(
        !result.matches[0].evidence.lexical.is_empty(),
        "semantic execution must retain lexical evidence"
    );
}

#[test]
fn required_semantic_wait_never_degrades_to_lexical() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("career"), b"# Career\nRust systems work")
        .unwrap();

    let query = MemoryQuery::builder()
        .spaces(vec![space])
        .text_cue("career")
        .require_capability("semantic")
        .wait_for(Duration::from_secs(1))
        .build()
        .unwrap();
    let first = runtime.query_start(query).unwrap();
    match first {
        QueryStep::Complete(response) => assert!(
            !response.execution.degraded,
            "required semantic wait must not complete with a degraded fallback"
        ),
        QueryStep::ProviderPending {
            operation_id,
            work: QueryWork::Embedding(request),
        } => {
            let resumed = runtime
                .query_resume(
                    &operation_id,
                    ProviderWorkResult::Embeddings {
                        work_id: request.work_id,
                        vectors: vec![memoria_runtime::EmbeddingVector {
                            key: "query".to_owned(),
                            values: vec![1.0, 0.0, 0.0],
                        }],
                    },
                )
                .unwrap();
            if let QueryStep::Complete(response) = resumed {
                assert!(
                    !response.execution.degraded,
                    "required semantic wait must not complete with a degraded fallback"
                );
            }
        }
        _ => {}
    }
}

#[test]
fn rerank_scores_change_result_order() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let first_memory = runtime
        .create_memory(space, Some("first"), b"# First\nshared retrieval cue")
        .unwrap();
    let second_memory = runtime
        .create_memory(space, Some("second"), b"# Second\nshared retrieval cue")
        .unwrap();
    let baseline = runtime
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .text_cue("shared retrieval cue")
                .build()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(baseline.results.len(), 2);
    let desired = if baseline.results[0].memory_id == first_memory {
        second_memory
    } else {
        first_memory
    };

    support::publish_manifest_with_capabilities(&runtime, &["reranking"]);
    let query = MemoryQuery::builder()
        .spaces(vec![space])
        .text_cue("shared retrieval cue")
        .cue_memory(first_memory)
        .cue_memory(second_memory)
        .prefer_capability("reranking")
        .build()
        .unwrap();
    let QueryStep::ProviderPending {
        operation_id,
        work: QueryWork::Rerank(request),
    } = runtime.query_start(query).unwrap()
    else {
        panic!("expected rerank provider work");
    };
    let scores = request
        .candidates
        .iter()
        .map(|handle| RerankScore {
            handle: handle.clone(),
            score: f32::from(handle.contains(&desired.to_string())),
        })
        .collect();
    let QueryStep::Complete(response) = runtime
        .query_resume(
            &operation_id,
            ProviderWorkResult::Rerank {
                work_id: request.work_id,
                scores,
            },
        )
        .unwrap()
    else {
        panic!("expected rerank query completion");
    };
    assert_eq!(response.results[0].memory_id, desired);
}

#[test]
fn adaptive_is_not_used_without_adaptive_capability() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("career"), b"# Career\nRust systems work")
        .unwrap();

    let response = runtime
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .text_cue("career")
                .build()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        response.snapshot.adaptive,
        AdaptiveSnapshotIdentity::Disabled
    );
}
