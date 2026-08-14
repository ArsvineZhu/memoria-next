mod support;

use memoria_derived::DerivedCatalog;
use memoria_query::{CapabilityPlanner, MemoryQuery, QueryQualityLevel};
use memoria_runtime::{
    EmbeddingVector, MemoriaRuntime, NeedWork, ProviderRouteConfig, ProviderWorkResult,
    SpaceProviderMode, SpaceProviderPolicy,
};
use memoria_types::MemoryId;
use tempfile::tempdir;

fn take_embedding_work(runtime: &mut MemoriaRuntime) -> memoria_runtime::EmbeddingBatchRequest {
    match runtime.provider_poll_work().unwrap().unwrap() {
        NeedWork::Embeddings(request) => request,
        other => panic!("expected embedding work, got {other:?}"),
    }
}

fn submit_embedding(runtime: &mut MemoriaRuntime, work: memoria_runtime::EmbeddingBatchRequest) {
    runtime
        .provider_submit_result(ProviderWorkResult::Embeddings {
            work_id: work.work_id,
            vectors: work
                .items
                .into_iter()
                .map(|item| EmbeddingVector {
                    key: item.key,
                    values: vec![1.0; 64],
                })
                .collect(),
        })
        .unwrap();
}

fn serving_manifest(runtime: &MemoriaRuntime) -> memoria_derived::DerivedManifest {
    DerivedCatalog::open(runtime.data_dir().join("derived/catalog.sqlite"))
        .unwrap()
        .serving_manifest()
        .unwrap()
        .unwrap()
}

fn semantic_membership_target(
    runtime: &MemoriaRuntime,
) -> (memoria_types::SpaceId, MemoryId, memoria_types::RevisionId) {
    let catalog = DerivedCatalog::open(runtime.data_dir().join("derived/catalog.sqlite")).unwrap();
    let manifest = catalog.serving_manifest().unwrap().unwrap();
    let artifact_id = manifest
        .artifacts()
        .find(|id| catalog.artifact(*id).unwrap().kind() == "semantic")
        .expect("semantic artifact should be serving");
    let membership = catalog
        .vector_memberships_for_artifact(artifact_id)
        .unwrap()
        .into_iter()
        .next()
        .expect("semantic membership should be serving");
    (
        membership.space_id,
        membership.memory_id,
        membership.revision_id,
    )
}

fn all_local_only_policy() -> SpaceProviderPolicy {
    SpaceProviderPolicy {
        embedding: SpaceProviderMode::LocalOnly,
        enrichment: SpaceProviderMode::LocalOnly,
        reranking: SpaceProviderMode::LocalOnly,
    }
}

fn drain_background_work(runtime: &mut MemoriaRuntime, generated_memory: MemoryId) {
    while let Some(work) = runtime.provider_poll_work().unwrap() {
        match work {
            NeedWork::Embeddings(request) => runtime
                .provider_submit_result(ProviderWorkResult::Embeddings {
                    work_id: request.work_id,
                    vectors: request
                        .items
                        .into_iter()
                        .map(|item| EmbeddingVector {
                            key: item.key,
                            values: vec![1.0; 64],
                        })
                        .collect(),
                })
                .unwrap(),
            NeedWork::Enrichment(request) => runtime
                .provider_submit_result(ProviderWorkResult::Enrichment {
                    work_id: request.work_id,
                    tags: if request.projection.memory_id() == generated_memory {
                        vec!["source".to_owned()]
                    } else {
                        Vec::new()
                    },
                })
                .unwrap(),
            NeedWork::Rerank(request) => panic!("unexpected background rerank {}", request.work_id),
        }
    }
}

#[test]
fn phase2_semantic_publication_rebases_late_g1_work_onto_g2() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let memory = runtime
        .create_memory(space, Some("note"), b"# Note\nstable semantic text\n")
        .unwrap();
    let old_work = take_embedding_work(&mut runtime);
    let head = runtime
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .text_cue("stable")
                .build()
                .unwrap(),
        )
        .unwrap()
        .results[0]
        .revision_id;

    let mutation = runtime
        .revise_memory(
            memory,
            head,
            b"# Note\nstable semantic text\n<Tag value=\"review\"/>\n",
        )
        .unwrap();
    submit_embedding(&mut runtime, old_work);

    let manifest = serving_manifest(&runtime);
    assert_eq!(manifest.authority_generation(), mutation.generation);
    assert!(manifest.capability("semantic").is_ready());
    let (published_space, published_memory, published_revision) =
        semantic_membership_target(&runtime);
    assert_eq!(published_space, space);
    assert_eq!(published_memory, memory);
    assert_eq!(published_revision, mutation.revision_id);
}

#[test]
fn phase2_explicit_seed_outranks_otherwise_equal_generated_seed_in_runtime() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let explicit_memory = runtime
        .create_memory(
            space,
            Some("explicit"),
            b"# Explicit\n<Tag value=\"source\"/>",
        )
        .unwrap();
    let generated_memory = runtime
        .create_memory(space, Some("generated"), b"# Generated")
        .unwrap();
    drain_background_work(&mut runtime, generated_memory);

    let response = runtime
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .cue_tag("source")
                .require_capability("associative")
                .max_results(2)
                .build()
                .unwrap(),
        )
        .unwrap();

    assert_eq!(response.results.len(), 2);
    assert_eq!(response.results[0].memory_id, explicit_memory);
    assert_eq!(response.results[1].memory_id, generated_memory);
}

#[test]
fn phase2_thorough_runtime_uses_larger_budget_and_diffusion() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(
            space,
            Some("first"),
            b"# First\n<Tag value=\"first\"/><Tag value=\"second\"/> shared cue",
        )
        .unwrap();
    runtime
        .create_memory(
            space,
            Some("second"),
            b"# Second\n<Tag value=\"second\"/><Tag value=\"third\"/> shared cue",
        )
        .unwrap();
    runtime
        .create_memory(
            space,
            Some("third"),
            b"# Third\n<Tag value=\"third\"/> shared cue",
        )
        .unwrap();

    let balanced_query = MemoryQuery::builder()
        .spaces(vec![space])
        .cue_tag("first")
        .text_cue("shared cue")
        .require_capability("associative")
        .quality(QueryQualityLevel::Balanced)
        .build()
        .unwrap();
    let thorough_query = MemoryQuery::builder()
        .spaces(vec![space])
        .cue_tag("first")
        .text_cue("shared cue")
        .require_capability("associative")
        .quality(QueryQualityLevel::Thorough)
        .build()
        .unwrap();

    let balanced_profile = CapabilityPlanner::retrieval_profile(&balanced_query);
    let thorough_profile = CapabilityPlanner::retrieval_profile(&thorough_query);
    assert!(
        thorough_profile.activation_budget.max_active_tags
            > balanced_profile.activation_budget.max_active_tags
    );
    assert!(
        thorough_profile.activation_budget.max_edge_visits
            > balanced_profile.activation_budget.max_edge_visits
    );
    assert!(thorough_profile.run_diffusion);

    let balanced = runtime.query(balanced_query).unwrap();
    let thorough = runtime.query(thorough_query).unwrap();
    assert!(!balanced.trace.channel_executed("diffusion"));
    assert!(thorough.trace.channel_executed("diffusion"));
    assert!(thorough.trace.diffusion_iterations > 0);
}

#[test]
fn phase2_rust_trust_gate_emits_no_external_work_for_local_only_space() {
    let directory = tempdir().unwrap();
    let mut runtime =
        MemoriaRuntime::open_with_provider_routes(directory.path(), ProviderRouteConfig::default())
            .unwrap();
    let space = runtime
        .create_space_with_policy("private", all_local_only_policy())
        .unwrap();
    runtime
        .create_memory(space, Some("private"), b"# Private\nprivate cue")
        .unwrap();

    assert!(runtime.provider_poll_work().unwrap().is_none());
}

#[test]
fn phase2_trace_candidate_counts_match_runtime_channel_work() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("one"), b"# One\ntrace shared cue")
        .unwrap();
    runtime
        .create_memory(space, Some("two"), b"# Two\ntrace shared cue")
        .unwrap();

    let response = runtime
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .text_cue("trace shared cue")
                .max_results(10)
                .build()
                .unwrap(),
        )
        .unwrap();
    let lexical_count = response
        .trace
        .candidate_counts
        .iter()
        .find(|(channel, _)| channel == "lexical")
        .map(|(_, count)| *count)
        .expect("lexical work must be traced");
    assert_eq!(lexical_count, 2);
    assert_eq!(response.results.len(), 2);
    assert_eq!(response.trace.channels_executed, vec!["lexical"]);
    assert_eq!(
        response.trace.candidate_counts,
        vec![("lexical".to_owned(), 2)]
    );
}
