use memoria_derived::TagDictionary;
use memoria_query::{
    AssociationGraph, CapabilityPlanner, MemoryQuery, PropagationBudget, QueryQualityLevel,
    TagSeed, TagSeedProvenance, activation_propagate, diffusion_propagate,
};
use memoria_types::SpaceId;

fn graph_and_seeds() -> (AssociationGraph, Vec<TagSeed>) {
    let mut dictionary = TagDictionary::new();
    let first = dictionary.intern("first").unwrap();
    let second = dictionary.intern("second").unwrap();
    let third = dictionary.intern("third").unwrap();
    let fourth = dictionary.intern("fourth").unwrap();
    let mut graph = AssociationGraph::new();
    graph.add_edge(first, second, 1.0).unwrap();
    graph.add_edge(first, third, 0.5).unwrap();
    graph.add_edge(second, fourth, 0.25).unwrap();
    (
        graph,
        vec![TagSeed {
            tag_id: first,
            value: "first".to_owned(),
            provenance: TagSeedProvenance::Explicit,
        }],
    )
}

fn budget() -> PropagationBudget {
    PropagationBudget {
        max_active_tags: 96,
        max_edge_visits: 1024,
        max_hops: 3,
    }
}

#[test]
fn diffusion_is_not_equal_to_activation_on_branching_graph() {
    let (graph, seeds) = graph_and_seeds();
    let activation = activation_propagate(&graph, &seeds, budget()).unwrap();
    let diffusion = diffusion_propagate(&graph, &seeds, budget()).unwrap();
    let activation_scores = activation
        .active_tags
        .iter()
        .map(|tag| tag.score)
        .collect::<Vec<_>>();
    let diffusion_scores = diffusion
        .active_tags
        .iter()
        .map(|tag| tag.score)
        .collect::<Vec<_>>();
    assert_ne!(activation_scores, diffusion_scores);
}

#[test]
fn diffusion_conserves_normalized_nonnegative_activation() {
    let (graph, seeds) = graph_and_seeds();
    let diffusion = diffusion_propagate(&graph, &seeds, budget()).unwrap();
    let total = diffusion
        .active_tags
        .iter()
        .map(|tag| tag.score)
        .sum::<f32>();
    assert!(diffusion.active_tags.iter().all(|tag| tag.score >= 0.0));
    assert!((total - 1.0).abs() <= 1.0e-5);
    assert!(
        diffusion
            .active_tags
            .iter()
            .any(|tag| !tag.support_seed_ids.is_empty())
    );
}

#[test]
fn diffusion_stops_at_eight_iterations_or_l1_tolerance() {
    let (graph, seeds) = graph_and_seeds();
    let diffusion = diffusion_propagate(&graph, &seeds, budget()).unwrap();
    assert!(diffusion.iterations <= 8);
    assert!(diffusion.converged || diffusion.convergence_delta > 1.0e-5);
}

#[test]
fn balanced_profile_does_not_run_diffusion_by_default() {
    let query = MemoryQuery::builder()
        .spaces(vec![SpaceId::from_bytes([1; 16])])
        .quality(QueryQualityLevel::Balanced)
        .build()
        .unwrap();
    assert!(!CapabilityPlanner::retrieval_profile(&query).run_diffusion);
}

#[test]
fn thorough_profile_runs_diffusion() {
    let query = MemoryQuery::builder()
        .spaces(vec![SpaceId::from_bytes([1; 16])])
        .quality(QueryQualityLevel::Thorough)
        .build()
        .unwrap();
    assert!(CapabilityPlanner::retrieval_profile(&query).run_diffusion);
}
