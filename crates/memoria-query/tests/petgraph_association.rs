use memoria_derived::TagId;
use memoria_query::{
    AssociationEdge, AssociationError, AssociationGraphView, PropagationBudget, TagSeed,
    TagSeedProvenance, activation_propagate, diffusion_propagate,
};

fn edge(left: TagId, right: TagId, weight: f32) -> AssociationEdge {
    AssociationEdge {
        left,
        right,
        weight,
        static_weight: weight,
        adaptive_weight: 0.0,
    }
}

fn seed(tag_id: TagId, value: &str) -> TagSeed {
    TagSeed {
        tag_id,
        value: value.to_owned(),
        provenance: TagSeedProvenance::Explicit,
    }
}

#[test]
fn csr_neighbors_match_baseline_association_view() {
    let left = TagId::from_normalized("left");
    let middle = TagId::from_normalized("middle");
    let right = TagId::from_normalized("right");
    let graph = AssociationGraphView::from_nodes_and_edges(
        &[left, middle, right],
        &[edge(left, middle, 2.0), edge(middle, right, 3.0)],
    )
    .unwrap();

    assert_eq!(graph.edge_count(), 2);
    let left_middle = if left <= middle {
        edge(left, middle, 2.0)
    } else {
        edge(middle, left, 2.0)
    };
    let middle_right = if middle <= right {
        edge(middle, right, 3.0)
    } else {
        edge(right, middle, 3.0)
    };
    assert_eq!(graph.neighbors(left), vec![left_middle]);
    assert_eq!(graph.neighbors(middle).len(), 2);
    assert_eq!(graph.neighbors(right), vec![middle_right]);
}

#[test]
fn activation_output_matches_algorithm_contract() {
    let source = TagId::from_normalized("source");
    let target = TagId::from_normalized("target");
    let graph =
        AssociationGraphView::from_nodes_and_edges(&[source, target], &[edge(source, target, 1.0)])
            .unwrap();
    let budget = PropagationBudget {
        max_active_tags: 8,
        max_edge_visits: 32,
        max_hops: 2,
    };

    let trace = activation_propagate(&graph, &[seed(source, "source")], budget).unwrap();
    let propagated = trace
        .active_tags
        .iter()
        .find(|item| item.tag_id == target)
        .expect("activation fixture must reach target");
    assert!((propagated.score - 0.25).abs() < 1.0e-5);
}

#[test]
fn diffusion_output_matches_algorithm_contract() {
    let source = TagId::from_normalized("source");
    let target = TagId::from_normalized("target");
    let graph =
        AssociationGraphView::from_nodes_and_edges(&[source, target], &[edge(source, target, 1.0)])
            .unwrap();
    let budget = PropagationBudget {
        max_active_tags: 8,
        max_edge_visits: 32,
        max_hops: 2,
    };

    let trace = diffusion_propagate(&graph, &[seed(source, "source")], budget).unwrap();
    let total = trace.active_tags.iter().map(|item| item.score).sum::<f32>();
    assert!((total - 1.0).abs() < 1.0e-5);
    assert!(trace.active_tags.iter().any(|item| item.tag_id == target));
}

#[test]
fn graph_builder_rejects_duplicate_or_unknown_endpoints_deterministically() {
    let known = TagId::from_normalized("known");
    let unknown = TagId::from_normalized("unknown");

    assert_eq!(
        AssociationGraphView::from_nodes_and_edges(&[known, known], &[]),
        Err(AssociationError::DuplicateEndpoint { tag_id: known })
    );
    assert_eq!(
        AssociationGraphView::from_nodes_and_edges(&[known], &[edge(known, unknown, 1.0)]),
        Err(AssociationError::UnknownEndpoint { tag_id: unknown })
    );
}
