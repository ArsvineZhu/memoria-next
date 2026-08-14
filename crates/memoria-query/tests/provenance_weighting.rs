use memoria_derived::TagId;
use memoria_query::{
    AssociationGraph, PropagationBudget, TagSeed, TagSeedProvenance, activation_propagate,
    diffusion_propagate,
};

fn seed(value: &str, provenance: TagSeedProvenance) -> TagSeed {
    TagSeed {
        tag_id: TagId::from_normalized(value),
        value: value.to_owned(),
        provenance,
    }
}

fn budget() -> PropagationBudget {
    PropagationBudget {
        max_active_tags: 16,
        max_edge_visits: 64,
        max_hops: 2,
    }
}

fn score(trace: &memoria_query::PropagationTrace, value: &str) -> f32 {
    let tag_id = TagId::from_normalized(value);
    trace
        .active_tags
        .iter()
        .find(|tag| tag.tag_id == tag_id)
        .expect("tag should be active")
        .score
}

fn diffusion_score(trace: &memoria_query::DiffusionTrace, value: &str) -> f32 {
    let tag_id = TagId::from_normalized(value);
    trace
        .active_tags
        .iter()
        .find(|tag| tag.tag_id == tag_id)
        .expect("tag should be active")
        .score
}

#[test]
fn activation_explicit_seed_starts_at_one() {
    let trace = activation_propagate(
        &AssociationGraph::new(),
        &[seed("explicit", TagSeedProvenance::Explicit)],
        budget(),
    )
    .unwrap();

    assert_eq!(score(&trace, "explicit"), 1.0);
}

#[test]
fn activation_generated_seed_starts_at_point_five_five() {
    let trace = activation_propagate(
        &AssociationGraph::new(),
        &[seed("generated", TagSeedProvenance::Generated)],
        budget(),
    )
    .unwrap();

    assert_eq!(score(&trace, "generated"), 0.55);
}

#[test]
fn otherwise_equal_explicit_path_outranks_generated_path() {
    let explicit_source = TagId::from_normalized("explicit-source");
    let generated_source = TagId::from_normalized("generated-source");
    let target = TagId::from_normalized("target");

    let mut explicit_graph = AssociationGraph::new();
    explicit_graph
        .add_edge(explicit_source, target, 1.0)
        .unwrap();
    let explicit_trace = activation_propagate(
        &explicit_graph,
        &[seed("explicit-source", TagSeedProvenance::Explicit)],
        budget(),
    )
    .unwrap();

    let mut generated_graph = AssociationGraph::new();
    generated_graph
        .add_edge(generated_source, target, 1.0)
        .unwrap();
    let generated_trace = activation_propagate(
        &generated_graph,
        &[seed("generated-source", TagSeedProvenance::Generated)],
        budget(),
    )
    .unwrap();

    assert!(score(&explicit_trace, "target") > score(&generated_trace, "target"));
}

#[test]
fn diffusion_a0_is_proportional_to_seed_provenance_weights() {
    let trace = diffusion_propagate(
        &AssociationGraph::new(),
        &[
            seed("explicit", TagSeedProvenance::Explicit),
            seed("generated", TagSeedProvenance::Generated),
        ],
        budget(),
    )
    .unwrap();

    let explicit = diffusion_score(&trace, "explicit");
    let generated = diffusion_score(&trace, "generated");
    assert!((explicit / generated - (1.0 / 0.55)).abs() < 1.0e-4);
}

#[test]
fn diffusion_two_equal_provenance_seeds_still_normalize_equally() {
    let trace = diffusion_propagate(
        &AssociationGraph::new(),
        &[
            seed("left", TagSeedProvenance::Explicit),
            seed("right", TagSeedProvenance::Explicit),
        ],
        budget(),
    )
    .unwrap();

    assert!((diffusion_score(&trace, "left") - diffusion_score(&trace, "right")).abs() < 1.0e-6);
}
