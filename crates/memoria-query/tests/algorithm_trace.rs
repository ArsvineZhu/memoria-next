use memoria_derived::TagDictionary;
use memoria_query::{
    AssociationGraph, PropagationBudget, QueryOperatorTrace, TagSeed, TagSeedProvenance,
    activation_propagate, diffusion_propagate,
};

#[test]
fn trace_records_tag_basis_use_and_residual_energy() {
    let mut trace = QueryOperatorTrace::default();
    trace.record_tag_basis(2, 12.0, 0.75);
    assert_eq!(trace.tag_basis_rank, Some(2));
    assert_eq!(trace.tag_basis_explained_energy, Some(0.75));
}

#[test]
fn trace_records_activation_edge_visits_and_seed_origins() {
    let mut dictionary = TagDictionary::new();
    let first = dictionary.intern("first").unwrap();
    let second = dictionary.intern("second").unwrap();
    let mut graph = AssociationGraph::new();
    graph.add_edge(first, second, 1.0).unwrap();
    let trace = activation_propagate(
        &graph,
        &[TagSeed {
            tag_id: first,
            value: "first".to_owned(),
            provenance: TagSeedProvenance::Explicit,
        }],
        PropagationBudget {
            max_active_tags: 96,
            max_edge_visits: 1024,
            max_hops: 3,
        },
    )
    .unwrap();
    assert_eq!(trace.edge_visits, 2);
    assert_eq!(
        trace.active_tags[0].seed_origins,
        vec![TagSeedProvenance::Explicit]
    );
}

#[test]
fn trace_distinguishes_activation_from_diffusion() {
    let mut dictionary = TagDictionary::new();
    let first = dictionary.intern("first").unwrap();
    let second = dictionary.intern("second").unwrap();
    let third = dictionary.intern("third").unwrap();
    let mut graph = AssociationGraph::new();
    graph.add_edge(first, second, 1.0).unwrap();
    graph.add_edge(first, third, 0.5).unwrap();
    let seeds = [TagSeed {
        tag_id: first,
        value: "first".to_owned(),
        provenance: TagSeedProvenance::Explicit,
    }];
    let budget = PropagationBudget {
        max_active_tags: 96,
        max_edge_visits: 1024,
        max_hops: 3,
    };
    let activation = activation_propagate(&graph, &seeds, budget).unwrap();
    let diffusion = diffusion_propagate(&graph, &seeds, budget).unwrap();
    assert_ne!(
        activation
            .active_tags
            .iter()
            .map(|tag| tag.score)
            .collect::<Vec<_>>(),
        diffusion
            .active_tags
            .iter()
            .map(|tag| tag.score)
            .collect::<Vec<_>>()
    );
    assert!(diffusion.iterations <= 8);
}

#[test]
fn trace_records_relation_support_and_correlation_suppression() {
    let trace = QueryOperatorTrace {
        independent_support_count: 2,
        correlation_suppressed_evidence: 3,
        relation_expansions: 1,
        ..QueryOperatorTrace::default()
    };
    assert_eq!(trace.independent_support_count, 2);
    assert_eq!(trace.correlation_suppressed_evidence, 3);
    assert_eq!(trace.relation_expansions, 1);
}
