mod support;

use std::time::Duration;

use memoria_query::{EntityRef, MemoryQuery, QueryQualityLevel, RetrievalResponse};
use memoria_runtime::{
    EmbeddingVector, MemoriaRuntime, ProviderWorkResult, QueryStep, QueryWork, RerankScore,
};
use memoria_types::{MemoryId, SpaceId};
use tempfile::tempdir;

fn complete_with_embedding(
    runtime: &mut MemoriaRuntime,
    query: MemoryQuery,
    values: Vec<f32>,
) -> RetrievalResponse {
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

fn complete_with_rerank(
    runtime: &mut MemoriaRuntime,
    query: MemoryQuery,
    desired: MemoryId,
) -> RetrievalResponse {
    let QueryStep::ProviderPending {
        operation_id,
        work: QueryWork::Rerank(request),
    } = runtime.query_start(query).unwrap()
    else {
        panic!("expected rerank provider work");
    };
    assert!(request.candidates.len() >= 2);
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
    response
}

fn add_tagged_memory(
    runtime: &mut MemoriaRuntime,
    space: SpaceId,
    key: &str,
    tag: &str,
    body: &str,
) -> MemoryId {
    runtime
        .create_memory(
            space,
            Some(key),
            format!("# {key}\n<Tag value=\"{tag}\"/>\n{body}").as_bytes(),
        )
        .unwrap()
}

#[test]
fn physical_executor_serves_lexical_only_without_semantic() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("lexical"), b"# Lexical\nunique lexical cue")
        .unwrap();

    let response = runtime
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .text_cue("unique lexical cue")
                .build()
                .unwrap(),
        )
        .unwrap();

    assert!(response.trace.channel_executed("lexical"));
    assert!(!response.trace.channel_executed("semantic-direct"));
    assert!(!response.trace.channel_executed("semantic-residual"));
}

#[test]
fn physical_executor_keeps_lexical_with_semantic_direct() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("semantic"), b"# Semantic\nshared retrieval cue")
        .unwrap();
    support::drain_background_work(&mut runtime, |_| vec![1.0, 0.0, 0.0]);

    let response = complete_with_embedding(
        &mut runtime,
        MemoryQuery::builder()
            .spaces(vec![space])
            .text_cue("shared retrieval cue")
            .require_capability("semantic")
            .build()
            .unwrap(),
        vec![1.0, 0.0, 0.0],
    );

    assert!(response.trace.channel_executed("lexical"));
    assert!(response.trace.channel_executed("semantic-direct"));
    assert!(response.results.iter().any(|result| {
        !result.matches[0].evidence.lexical.is_empty()
            && !result.matches[0].evidence.semantic.is_empty()
    }));
}

#[test]
fn physical_executor_serves_tag_basis_and_semantic_residual() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let first = add_tagged_memory(&mut runtime, space, "first", "systems", "systems cue");
    support::drain_background_work(&mut runtime, |key| {
        if key.contains(&first.to_string()) {
            vec![1.0, 0.0, 0.0]
        } else {
            vec![0.0, 1.0, 0.0]
        }
    });
    let second = add_tagged_memory(&mut runtime, space, "second", "career", "career cue");
    support::drain_background_work(&mut runtime, |key| {
        if key.contains(&second.to_string()) {
            vec![0.0, 1.0, 0.0]
        } else {
            vec![1.0, 0.0, 0.0]
        }
    });

    let response = complete_with_embedding(
        &mut runtime,
        MemoryQuery::builder()
            .spaces(vec![space])
            .text_cue("systems career")
            .require_capability("semantic")
            .build()
            .unwrap(),
        vec![1.0, 1.0, 1.0],
    );

    assert!(response.trace.channel_executed("semantic-direct"));
    assert!(
        response.trace.channel_executed("semantic-residual"),
        "trace={:?}",
        response.trace
    );
    assert!(response.trace.tag_basis_rank.is_some());
}

#[test]
fn physical_executor_serves_activation_and_thorough_diffusion() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    add_tagged_memory(
        &mut runtime,
        space,
        "career",
        "systems",
        "Rust systems work",
    );
    add_tagged_memory(
        &mut runtime,
        space,
        "project",
        "systems",
        "project systems work",
    );
    support::publish_manifest_with_capabilities(&runtime, &["associative"]);

    let response = runtime
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .cue_tag("systems")
                .require_capability("associative")
                .quality(QueryQualityLevel::Thorough)
                .build()
                .unwrap(),
        )
        .unwrap();

    assert!(response.trace.channel_executed("tag-readout"));
    assert!(response.trace.channel_executed("activation"));
    assert!(response.trace.channel_executed("diffusion"));
    assert!(response.trace.diffusion_iterations > 0);
}

#[test]
fn physical_executor_serves_explicit_relation_expansion() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let target = runtime
        .create_memory(space, Some("target"), b"# Target\nrelated target")
        .unwrap();
    runtime
        .create_memory(
            space,
            Some("relations"),
            format!(
                "# Relations\n<MemoryRef memoryId=\"{target}\"/>\n<Tag value=\"systems\"/>\nrelation cue"
            )
            .as_bytes(),
        )
        .unwrap();
    support::publish_manifest_with_capabilities(&runtime, &["associative"]);

    let response = runtime
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .cue_tag("systems")
                .require_capability("associative")
                .build()
                .unwrap(),
        )
        .unwrap();

    assert!(
        response.trace.channel_executed("relation"),
        "trace={:?}",
        response.trace
    );
    assert!(response.trace.relation_expansions > 0);
    assert!(
        response
            .results
            .iter()
            .any(|result| result.memory_id == target)
    );
    assert!(
        response
            .results
            .iter()
            .flat_map(|result| result.matches.iter())
            .any(|result| !result.evidence.relations.is_empty())
    );
}

#[test]
fn physical_executor_applies_real_rerank_after_fusion() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let first = runtime
        .create_memory(space, Some("first"), b"# First\nshared rerank cue")
        .unwrap();
    let second = runtime
        .create_memory(space, Some("second"), b"# Second\nshared rerank cue")
        .unwrap();
    support::publish_manifest_with_capabilities(&runtime, &["reranking"]);

    let response = complete_with_rerank(
        &mut runtime,
        MemoryQuery::builder()
            .spaces(vec![space])
            .text_cue("shared rerank cue")
            .prefer_capability("reranking")
            .build()
            .unwrap(),
        second,
    );

    assert_eq!(response.results[0].memory_id, second);
    assert_ne!(response.results[0].memory_id, first);
    assert!(response.trace.rerank_requested);
    assert!(response.trace.rerank_applied);
}

#[test]
fn physical_executor_gates_adaptive_active_and_inactive_queries() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("career"), b"# Career\nRust systems work")
        .unwrap();
    support::publish_manifest_with_capabilities(&runtime, &["adaptive"]);

    let inactive = runtime
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .text_cue("Rust systems work")
                .build()
                .unwrap(),
        )
        .unwrap();
    let active = runtime
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .text_cue("Rust systems work")
                .prefer_capability("adaptive")
                .build()
                .unwrap(),
        )
        .unwrap();

    assert!(!inactive.trace.channel_executed("adaptive"));
    assert!(active.trace.channel_executed("adaptive"));
}

#[test]
fn physical_executor_waits_for_required_semantic_readiness() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("pending"), b"# Pending\nsemantic pending cue")
        .unwrap();

    let step = runtime
        .query_start(
            MemoryQuery::builder()
                .spaces(vec![space])
                .text_cue("semantic pending cue")
                .require_capability("semantic")
                .wait_for(Duration::from_secs(1))
                .build()
                .unwrap(),
        )
        .unwrap();
    let QueryStep::ReadinessPending { operation_id, .. } = step else {
        panic!("required semantic wait must remain a readiness operation");
    };
    assert!(matches!(
        runtime.query_continue(&operation_id).unwrap(),
        QueryStep::ReadinessPending { .. }
    ));
}

#[test]
fn physical_executor_applies_entity_and_space_constraints_before_all_channels() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let included_space = runtime.create_space("included").unwrap();
    let excluded_space = runtime.create_space("excluded").unwrap();
    let included = runtime
        .create_memory(
            included_space,
            Some("included"),
            br##"# Included
<Entity ref="person:alice">Alice</Entity>
<Section id="a"/><Section id="b"/>
<Relation id="supports" from="#a" to="#b" kind="supports"/>
<Tag value="systems"/>
shared constrained cue
"##,
        )
        .unwrap();
    let excluded = add_tagged_memory(
        &mut runtime,
        excluded_space,
        "excluded",
        "systems",
        "shared constrained cue",
    );
    support::drain_background_work(&mut runtime, |key| {
        if key.contains(&included.to_string()) {
            vec![1.0, 0.0, 0.0]
        } else if key.contains(&excluded.to_string()) {
            vec![0.0, 1.0, 0.0]
        } else {
            vec![1.0, 0.0, 0.0]
        }
    });
    support::publish_manifest_with_capabilities(&runtime, &["associative"]);

    let response = complete_with_embedding(
        &mut runtime,
        MemoryQuery::builder()
            .spaces(vec![included_space])
            .text_cue("shared constrained cue")
            .cue_tag("systems")
            .require_entity(EntityRef::new("person:alice").unwrap())
            .require_capability("semantic")
            .require_capability("associative")
            .quality(QueryQualityLevel::Thorough)
            .build()
            .unwrap(),
        vec![1.0, 0.0, 1.0],
    );

    assert!(response.results.iter().all(|result| {
        result.space_id == included_space
            && result.matches.iter().all(|item| {
                item.evidence
                    .contains_entity(&EntityRef::new("person:alice").unwrap())
            })
    }));
    assert!(response.trace.channel_executed("lexical"));
    assert!(response.trace.channel_executed("semantic-direct"));
    assert!(response.trace.channel_executed("tag-readout"));
    assert!(response.trace.channel_executed("activation"));
    assert!(response.trace.channel_executed("diffusion"));
    assert!(
        response.trace.channel_executed("relation"),
        "trace={:?}",
        response.trace
    );
}
