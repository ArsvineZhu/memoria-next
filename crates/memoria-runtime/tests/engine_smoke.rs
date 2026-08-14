use memoria_query::MemoryQuery;
use memoria_runtime::{
    EmbeddingVector, MemoriaRuntime, NeedWork, ProviderCapability, ProviderEgressPolicy,
    ProviderWorkResult, RuntimeError,
};
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
fn provider_work_is_queued_after_commit_and_advances_semantic_build_coverage() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let memory_id = runtime
        .create_memory(space, Some("career"), b"# Career\nRust")
        .unwrap();

    let before = runtime.status();
    assert_eq!(before.authority_generation.value(), 2);
    assert_eq!(before.semantic_coverage.value(), 0);
    assert_eq!(before.semantic_build_coverage.value(), 0);

    let work = runtime.provider_poll_work().unwrap().unwrap();
    let work_id = match work {
        NeedWork::Embeddings(request) => {
            assert_eq!(request.items[0].key, memory_id.to_string());
            request.work_id
        }
        other => panic!("unexpected provider work: {other:?}"),
    };
    runtime
        .provider_submit_result(ProviderWorkResult::Embeddings {
            work_id,
            vectors: vec![EmbeddingVector {
                key: memory_id.to_string(),
                values: vec![0.0; 64],
            }],
        })
        .unwrap();

    let after = runtime.status();
    assert_eq!(after.semantic_coverage.value(), 2);
    assert_eq!(after.semantic_build_coverage.value(), 2);
}

#[test]
fn projection_specific_revision_changes_do_not_enqueue_content_embedding() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let memory_id = runtime
        .create_memory(
            space,
            Some("career"),
            br#"# Career
<State id="s" validFrom="2025">Rust systems work</State>
<Tag value="work"/>
"#,
        )
        .unwrap();

    let embedding = runtime.provider_poll_work().unwrap().unwrap();
    let embedding_work_id = match embedding {
        NeedWork::Embeddings(request) => request.work_id,
        other => panic!("expected initial embedding work, got {other:?}"),
    };
    runtime
        .provider_submit_result(ProviderWorkResult::Embeddings {
            work_id: embedding_work_id,
            vectors: vec![EmbeddingVector {
                key: memory_id.to_string(),
                values: vec![0.0; 64],
            }],
        })
        .unwrap();
    while let Some(work) = runtime.provider_poll_work().unwrap() {
        let work_id = match &work {
            NeedWork::Embeddings(request) => request.work_id.clone(),
            NeedWork::Rerank(request) => request.work_id.clone(),
            NeedWork::Enrichment(request) => request.work_id.clone(),
        };
        let result = match &work {
            NeedWork::Embeddings(request) => ProviderWorkResult::Embeddings {
                work_id,
                vectors: request
                    .items
                    .iter()
                    .map(|item| EmbeddingVector {
                        key: item.key.clone(),
                        values: vec![0.0; 64],
                    })
                    .collect(),
            },
            NeedWork::Rerank(request) => ProviderWorkResult::Rerank {
                work_id,
                scores: request
                    .candidates
                    .iter()
                    .map(|handle| memoria_runtime::RerankScore {
                        handle: handle.clone(),
                        score: 0.0,
                    })
                    .collect(),
            },
            NeedWork::Enrichment(_) => ProviderWorkResult::Enrichment {
                work_id,
                tags: vec!["derived".to_owned()],
            },
        };
        runtime.provider_submit_result(result).unwrap();
    }

    let query = MemoryQuery::builder()
        .spaces(vec![space])
        .text_cue("Rust")
        .build()
        .unwrap();
    let revision_id = runtime.query(query).unwrap().results[0].revision_id;
    let mutation = runtime
        .revise_memory(
            memory_id,
            revision_id,
            br#"# Career
<State id="s" validFrom="2025" validTo="2026">Rust systems work</State>
<Tag value="work"/>
"#,
        )
        .unwrap();
    let work = runtime.provider_poll_work().unwrap().unwrap();
    assert!(matches!(work, NeedWork::Enrichment(_)));
    assert_eq!(mutation.memory_id, memory_id);
    assert_eq!(
        runtime.status().semantic_coverage,
        mutation.generation,
        "tag-only revisions rebase the immutable semantic payload instead of dropping readiness",
    );
    assert_eq!(
        runtime.status().semantic_build_coverage,
        mutation.generation,
    );
}

#[test]
fn denied_provider_egress_is_blocked_before_work_is_emitted() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open_with_provider_egress_policy(
        directory.path(),
        ProviderEgressPolicy {
            allow_embedding: false,
            allow_rerank: true,
            allow_enrichment: true,
        },
    )
    .unwrap();
    let space = runtime.create_space("local-only").unwrap();
    runtime
        .create_memory(space, Some("private"), b"# Private\nLocal")
        .unwrap();

    let error = runtime.provider_poll_work().unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::ProviderEgressDenied {
            capability: ProviderCapability::Embedding
        }
    ));
}

#[test]
fn completed_purge_removes_authority_visibility_and_managed_source_blob() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let memory_id = runtime
        .create_memory(space, Some("private"), b"# Private\nPurge me")
        .unwrap();
    let object_count_before = std::fs::read_dir(directory.path().join("authority/objects"))
        .unwrap()
        .count();
    assert_eq!(object_count_before, 1);

    let plan = runtime.plan_purge(memory_id).unwrap();
    let completed = runtime.execute_purge(&plan.id).unwrap();
    assert_eq!(completed.state, memoria_runtime::PurgeState::Completed);
    assert_eq!(
        std::fs::read_dir(directory.path().join("authority/objects"))
            .unwrap()
            .count(),
        0
    );

    let query = MemoryQuery::builder()
        .spaces(vec![space])
        .text_cue("Purge")
        .build()
        .unwrap();
    assert!(runtime.query(query).unwrap().results.is_empty());
}
