use memoria_derived::TagId;
use memoria_query::{
    AssociationGraph, PropagationBudget, TagSeed, TagSeedProvenance, activation_propagate,
};

fn dense_test_graph(edge_count: usize) -> AssociationGraph {
    let seed = TagId::from_normalized("seed");
    let mut graph = AssociationGraph::new();
    for index in 0..edge_count {
        let target = TagId::from_normalized(&format!("tag-{index}"));
        graph.add_edge(seed, target, 1.0).unwrap();
    }
    graph
}

fn seed_tag() -> TagSeed {
    TagSeed {
        tag_id: TagId::from_normalized("seed"),
        value: "seed".to_owned(),
        provenance: TagSeedProvenance::Explicit,
    }
}

fn seed_with(value: &str, provenance: TagSeedProvenance) -> TagSeed {
    TagSeed {
        tag_id: TagId::from_normalized(value),
        value: value.to_owned(),
        provenance,
    }
}

#[test]
fn propagation_respects_edge_visit_budget() {
    let graph = dense_test_graph(10_000);
    let budget = PropagationBudget {
        max_active_tags: 64,
        max_edge_visits: 512,
        max_hops: 3,
    };
    let trace = activation_propagate(&graph, &[seed_tag()], budget).unwrap();

    assert!(trace.edge_visits <= 512);
    assert!(trace.active_tags.len() <= 64);
}

#[test]
fn propagation_keeps_support_provenance_and_components() {
    let seed = TagId::from_normalized("seed");
    let target = TagId::from_normalized("target");
    let mut graph = AssociationGraph::new();
    graph
        .add_edge_with_components(seed, target, 1.0, 0.25)
        .unwrap();

    let trace = activation_propagate(
        &graph,
        &[seed_with("seed", TagSeedProvenance::Explicit)],
        PropagationBudget {
            max_active_tags: 8,
            max_edge_visits: 8,
            max_hops: 1,
        },
    )
    .unwrap();
    let propagated = trace
        .active_tags
        .iter()
        .find(|item| item.tag_id == target)
        .expect("target should be activated");

    assert_eq!(propagated.hops, 1);
    assert_eq!(propagated.seed_origins, vec![TagSeedProvenance::Explicit]);
    assert_eq!(propagated.independent_seed_count, 1);
    assert!(propagated.static_contribution > 0.0);
    assert!(propagated.adaptive_contribution > 0.0);
}

#[test]
fn propagation_merges_independent_seed_support() {
    let left = TagId::from_normalized("left");
    let right = TagId::from_normalized("right");
    let target = TagId::from_normalized("target");
    let mut graph = AssociationGraph::new();
    graph.add_edge(left, target, 1.0).unwrap();
    graph.add_edge(right, target, 1.0).unwrap();

    let trace = activation_propagate(
        &graph,
        &[
            seed_with("left", TagSeedProvenance::Explicit),
            seed_with("right", TagSeedProvenance::Semantic),
        ],
        PropagationBudget {
            max_active_tags: 8,
            max_edge_visits: 8,
            max_hops: 1,
        },
    )
    .unwrap();
    let propagated = trace
        .active_tags
        .iter()
        .find(|item| item.tag_id == target)
        .expect("target should be activated");

    assert_eq!(propagated.independent_seed_count, 2);
    assert_eq!(
        propagated.seed_origins,
        vec![TagSeedProvenance::Explicit, TagSeedProvenance::Semantic]
    );
}
