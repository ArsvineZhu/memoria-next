use memoria_derived::TagId;
use memoria_query::{
    BALANCED_TAG_BASIS_VECTORS, SemanticQueryChannels, TagBasisResult, TagSeedProvenance,
    TagVectorCandidate, build_semantic_query_channels, select_tag_basis_candidates,
};

fn candidate(id: u8, provenance: TagSeedProvenance, score: f32) -> TagVectorCandidate {
    TagVectorCandidate {
        tag_id: TagId::from_bytes([id; 32]),
        vector: vec![1.0, 0.0],
        provenance,
        score,
    }
}

#[test]
fn balanced_query_uses_at_most_twenty_four_basis_tags() {
    let selected = select_tag_basis_candidates(
        (0..40).map(|id| candidate(id, TagSeedProvenance::Explicit, 1.0)),
        true,
        BALANCED_TAG_BASIS_VECTORS,
    );
    assert_eq!(selected.len(), BALANCED_TAG_BASIS_VECTORS);
}

#[test]
fn residual_ann_recovers_candidate_not_explained_by_tag_subspace() {
    let channels =
        build_semantic_query_channels(&[1.0, 1.0, 0.0], &[vec![1.0, 0.0, 0.0]], 24).unwrap();
    assert_eq!(channels.direct, vec![1.0, 1.0, 0.0]);
    assert_eq!(channels.residual, Some(vec![0.0, 1.0, 0.0]));
    assert!(channels.tag_basis.as_ref().is_some_and(|basis| basis.used));
}

#[test]
fn ill_conditioned_basis_falls_back_to_original_query_vector() {
    let channels = build_semantic_query_channels(
        &[0.0, 1.0, 0.0],
        &[vec![1.0, 0.0, 0.0], vec![1.0, 0.000001, 0.0]],
        24,
    )
    .unwrap();
    let basis = channels.tag_basis.expect("basis diagnostics");
    assert!(!basis.used);
    assert_eq!(basis.residual, channels.direct);
    assert!(channels.residual.is_none());
}

#[test]
fn direct_semantic_channel_remains_even_when_basis_is_used() {
    let channels = build_semantic_query_channels(&[1.0, 0.0], &[vec![1.0, 0.0]], 24).unwrap();
    assert!(channels.direct.iter().any(|value| *value != 0.0));
    assert!(channels.residual.is_none());
    assert!(matches!(
        channels,
        SemanticQueryChannels {
            tag_basis: Some(TagBasisResult { used: true, .. }),
            ..
        }
    ));
}
