use memoria_derived::TagDictionary;
use memoria_query::{
    AssociationGraph, PropagationBudget, TagSeed, TagSeedProvenance, activation_propagate,
};

fn seed(tag_id: memoria_derived::TagId, value: &str) -> TagSeed {
    TagSeed {
        tag_id,
        value: value.to_owned(),
        provenance: TagSeedProvenance::Explicit,
    }
}

#[test]
fn balanced_activation_uses_hop_decay_point_five() {
    let mut dictionary = TagDictionary::new();
    let first = dictionary.intern("first").unwrap();
    let second = dictionary.intern("second").unwrap();
    let mut graph = AssociationGraph::new();
    graph.add_edge(first, second, 1.0).unwrap();
    let trace = activation_propagate(
        &graph,
        &[seed(first, "first")],
        PropagationBudget {
            max_active_tags: 96,
            max_edge_visits: 1024,
            max_hops: 3,
        },
    )
    .unwrap();
    let propagated = trace
        .active_tags
        .iter()
        .find(|tag| tag.tag_id == second)
        .unwrap();
    assert!((propagated.score - 0.25).abs() < 1.0e-6);
}

#[test]
fn balanced_activation_never_exceeds_96_tags_1024_edges_3_hops() {
    let mut dictionary = TagDictionary::new();
    let seed_id = dictionary.intern("seed").unwrap();
    let mut graph = AssociationGraph::new();
    for index in 0..150_u16 {
        let tag_id = dictionary.intern(format!("tag-{index}")).unwrap();
        graph.add_edge(seed_id, tag_id, 1.0).unwrap();
    }
    let trace = activation_propagate(
        &graph,
        &[seed(seed_id, "seed")],
        PropagationBudget {
            max_active_tags: 96,
            max_edge_visits: 1024,
            max_hops: 3,
        },
    )
    .unwrap();
    assert!(trace.active_tags.len() <= 96);
    assert!(trace.edge_visits <= 1024);
    assert!(trace.max_hops_reached <= 3);
    assert!(trace.truncated);
}

#[test]
fn activation_support_tracks_independent_seed_origins() {
    let mut dictionary = TagDictionary::new();
    let left = dictionary.intern("left").unwrap();
    let right = dictionary.intern("right").unwrap();
    let target = dictionary.intern("target").unwrap();
    let mut graph = AssociationGraph::new();
    graph.add_edge(left, target, 1.0).unwrap();
    graph.add_edge(right, target, 1.0).unwrap();
    let trace = activation_propagate(
        &graph,
        &[seed(left, "left"), seed(right, "right")],
        PropagationBudget {
            max_active_tags: 96,
            max_edge_visits: 1024,
            max_hops: 1,
        },
    )
    .unwrap();
    let propagated = trace
        .active_tags
        .iter()
        .find(|tag| tag.tag_id == target)
        .unwrap();
    assert_eq!(propagated.independent_seed_count, 2);
    assert_eq!(propagated.support_seed_ids.len(), 2);
}

#[test]
fn propagation_budget_truncation_is_reported_not_hidden() {
    let mut dictionary = TagDictionary::new();
    let first = dictionary.intern("first").unwrap();
    let second = dictionary.intern("second").unwrap();
    let third = dictionary.intern("third").unwrap();
    let mut graph = AssociationGraph::new();
    graph.add_edge(first, second, 1.0).unwrap();
    graph.add_edge(first, third, 1.0).unwrap();
    let trace = activation_propagate(
        &graph,
        &[seed(first, "first")],
        PropagationBudget {
            max_active_tags: 96,
            max_edge_visits: 1,
            max_hops: 3,
        },
    )
    .unwrap();
    assert!(trace.truncated);
    assert_eq!(trace.edge_visits, 1);
}
