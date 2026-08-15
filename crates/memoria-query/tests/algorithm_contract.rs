use memoria_derived::TagId;
use memoria_query::{
    AssociationGraph, PropagationBudget, TagSeed, TagSeedProvenance, activation_propagate,
    diffusion_propagate, l2_norm, project_tag_basis,
};

fn seed(value: &str, provenance: TagSeedProvenance) -> TagSeed {
    TagSeed {
        tag_id: TagId::from_normalized(value),
        value: value.to_owned(),
        provenance,
    }
}

#[test]
fn tag_basis_contract_preserves_numeric_fixture() {
    let result = project_tag_basis(
        &[1.0_f32, 2.0],
        &[vec![1.0, 0.0], vec![0.0, 1.0]],
    )
    .unwrap();

    assert_eq!(result.rank, 2);
    assert!(result.used);
    assert!(l2_norm(&result.residual) < 1.0e-5);
    assert!((result.explained_energy - 1.0).abs() < 1.0e-5);
}

#[test]
fn activation_and_diffusion_contracts_preserve_distinct_outputs() {
    let source = TagId::from_normalized("source");
    let target = TagId::from_normalized("target");
    let mut graph = AssociationGraph::new();
    graph.add_edge(source, target, 1.0).unwrap();
    let seeds = vec![seed("source", TagSeedProvenance::Explicit)];
    let budget = PropagationBudget {
        max_active_tags: 8,
        max_edge_visits: 32,
        max_hops: 2,
    };

    let activation = activation_propagate(&graph, &seeds, budget).unwrap();
    let activated_target = activation
        .active_tags
        .iter()
        .find(|item| item.tag_id == target)
        .expect("activation fixture must reach target");
    assert!((activated_target.score - 0.25).abs() < 1.0e-5);

    let diffusion = diffusion_propagate(&graph, &seeds, budget).unwrap();
    let total = diffusion
        .active_tags
        .iter()
        .map(|item| item.score)
        .sum::<f32>();
    assert!((total - 1.0).abs() < 1.0e-5);
    assert_ne!(
        activation
            .active_tags
            .iter()
            .map(|item| item.score)
            .collect::<Vec<_>>(),
        diffusion
            .active_tags
            .iter()
            .map(|item| item.score)
            .collect::<Vec<_>>()
    );
}
