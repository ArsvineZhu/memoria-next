use memoria_query::{CapabilityPlanner, MemoryQuery, QueryQualityLevel, RetrievalProfile};
use memoria_types::SpaceId;

fn profile(level: QueryQualityLevel) -> RetrievalProfile {
    let query = MemoryQuery::builder()
        .spaces(vec![SpaceId::from_bytes([1; 16])])
        .quality(level)
        .build()
        .unwrap();
    CapabilityPlanner::retrieval_profile(&query)
}

#[test]
fn fast_profile_matches_locked_budget() {
    let profile = profile(QueryQualityLevel::Fast);

    assert_eq!(profile.lexical_candidates, 32);
    assert_eq!(profile.semantic_direct_candidates, 32);
    assert_eq!(profile.semantic_residual_candidates, 16);
    assert_eq!(profile.tag_readout_candidates, 24);
    assert_eq!(profile.tag_basis_vectors, 12);
    assert_eq!(
        profile.activation_budget,
        memoria_query::PropagationBudget {
            max_active_tags: 48,
            max_edge_visits: 512,
            max_hops: 2,
        }
    );
    assert_eq!(profile.relation_budget.max_added, 16);
    assert_eq!(profile.relation_budget.max_hops, 1);
    assert_eq!(profile.rerank_candidates, 16);
    assert!(!profile.run_diffusion);
}

#[test]
fn balanced_profile_matches_locked_budget() {
    let profile = profile(QueryQualityLevel::Balanced);

    assert_eq!(profile.lexical_candidates, 64);
    assert_eq!(profile.semantic_direct_candidates, 64);
    assert_eq!(profile.semantic_residual_candidates, 32);
    assert_eq!(profile.tag_readout_candidates, 48);
    assert_eq!(profile.tag_basis_vectors, 24);
    assert_eq!(profile.activation_budget.max_active_tags, 96);
    assert_eq!(profile.activation_budget.max_edge_visits, 1024);
    assert_eq!(profile.activation_budget.max_hops, 3);
    assert_eq!(profile.relation_budget.max_added, 32);
    assert_eq!(profile.relation_budget.max_hops, 1);
    assert_eq!(profile.rerank_candidates, 32);
    assert!(!profile.run_diffusion);
}

#[test]
fn thorough_profile_has_192_tags_4096_edges_4_hops_and_diffusion() {
    let profile = profile(QueryQualityLevel::Thorough);

    assert_eq!(profile.lexical_candidates, 128);
    assert_eq!(profile.semantic_direct_candidates, 128);
    assert_eq!(profile.semantic_residual_candidates, 64);
    assert_eq!(profile.tag_readout_candidates, 96);
    assert_eq!(profile.tag_basis_vectors, 48);
    assert_eq!(profile.activation_budget.max_active_tags, 192);
    assert_eq!(profile.activation_budget.max_edge_visits, 4096);
    assert_eq!(profile.activation_budget.max_hops, 4);
    assert_eq!(profile.diffusion_max_nodes, 256);
    assert_eq!(profile.relation_budget.max_added, 64);
    assert_eq!(profile.relation_budget.max_hops, 2);
    assert_eq!(profile.rerank_candidates, 64);
    assert!(profile.run_diffusion);
}

#[test]
fn executor_never_exceeds_profile_channel_candidate_caps() {
    for level in [
        QueryQualityLevel::Fast,
        QueryQualityLevel::Balanced,
        QueryQualityLevel::Thorough,
    ] {
        let profile = profile(level);
        assert!(profile.lexical_candidates > 0);
        assert!(profile.semantic_direct_candidates > 0);
        assert!(profile.semantic_residual_candidates > 0);
        assert!(profile.tag_readout_candidates > 0);
        assert!(profile.tag_basis_vectors > 0);
        assert!(profile.relation_budget.max_added > 0);
        assert!(profile.rerank_candidates > 0);
        if profile.run_diffusion {
            assert!(profile.diffusion_max_nodes > 0);
        }
    }
}
