mod support;

use memoria_derived::{DerivedCatalog, TagId};
use memoria_query::{
    AssociationGraph, CapabilityPlanner, MemoryQuery, PropagationBudget, QueryQualityLevel,
    TagSeed, TagSeedProvenance, activation_propagate,
};
use memoria_runtime::{
    EmbeddingVector, MemoriaRuntime, NeedWork, ProviderRouteConfig, ProviderTrust,
    ProviderWorkResult, SpaceProviderMode, SpaceProviderPolicy,
};
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
                    values: vec![1.0, 0.0, 0.0],
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

fn local_only_embedding_policy() -> SpaceProviderPolicy {
    SpaceProviderPolicy {
        embedding: SpaceProviderMode::LocalOnly,
        ..SpaceProviderPolicy::default()
    }
}

fn seed(value: &str, provenance: TagSeedProvenance) -> TagSeed {
    TagSeed {
        tag_id: TagId::from_normalized(value),
        value: value.to_owned(),
        provenance,
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
}

#[test]
fn phase2_explicit_seed_outranks_otherwise_equal_generated_seed() {
    let source = TagId::from_normalized("source");
    let target = TagId::from_normalized("target");
    let mut explicit_graph = AssociationGraph::new();
    explicit_graph.add_edge(source, target, 1.0).unwrap();
    let mut generated_graph = AssociationGraph::new();
    generated_graph.add_edge(source, target, 1.0).unwrap();

    let budget = PropagationBudget {
        max_active_tags: 16,
        max_edge_visits: 64,
        max_hops: 2,
    };
    let explicit = activation_propagate(
        &explicit_graph,
        &[seed("source", TagSeedProvenance::Explicit)],
        budget,
    )
    .unwrap();
    let generated = activation_propagate(
        &generated_graph,
        &[seed("source", TagSeedProvenance::Generated)],
        budget,
    )
    .unwrap();

    let explicit_score = explicit
        .active_tags
        .iter()
        .find(|tag| tag.tag_id == target)
        .unwrap()
        .score;
    let generated_score = generated
        .active_tags
        .iter()
        .find(|tag| tag.tag_id == target)
        .unwrap()
        .score;
    assert!(explicit_score > generated_score);
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
        .create_space_with_policy("private", local_only_embedding_policy())
        .unwrap();
    runtime
        .create_memory(space, Some("private"), b"# Private\nprivate cue")
        .unwrap();

    while let Some(work) = runtime.provider_poll_work().unwrap() {
        match work {
            NeedWork::Embeddings(request) => {
                assert_eq!(request.route.trust, ProviderTrust::Local);
                panic!("external embedding work must be rejected before emission");
            }
            NeedWork::Enrichment(request) => runtime
                .provider_submit_result(memoria_runtime::ProviderWorkResult::Enrichment {
                    work_id: request.work_id,
                    tags: Vec::new(),
                })
                .unwrap(),
            NeedWork::Rerank(request) => panic!("unexpected rerank work {}", request.work_id),
        }
    }
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
    assert_eq!(
        response
            .trace
            .candidate_counts
            .iter()
            .map(|(_, count)| *count)
            .sum::<usize>(),
        response.trace.candidate_counts.len().saturating_mul(2),
        "trace contains one bounded count per executed channel"
    );
}
