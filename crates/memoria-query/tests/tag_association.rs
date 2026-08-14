use memoria_derived::{TagDictionary, TagGraph, TagMembershipInput, TagProvenance};
use memoria_query::{
    TagSeedProvenance, TagVectorCandidate, readout_tag_candidates, select_tag_basis_candidates,
};
use memoria_types::{MemoryId, RevisionId, SpaceId};

fn ids(space: u8, memory: u8, revision: u8) -> (SpaceId, MemoryId, RevisionId) {
    (
        SpaceId::from_bytes([space; 16]),
        MemoryId::from_bytes([memory; 16]),
        RevisionId::from_bytes([revision; 32]),
    )
}

#[test]
fn exact_tag_seed_has_explicit_provenance() {
    assert_eq!(TagSeedProvenance::Explicit.weight(), 1.0);
    assert_eq!(TagSeedProvenance::ExactSupport.weight(), 0.95);
}

#[test]
fn semantic_tag_seed_requires_explicit_semantic_capability() {
    let semantic = TagVectorCandidate {
        tag_id: memoria_derived::TagId::from_bytes([1; 32]),
        vector: vec![1.0],
        provenance: TagSeedProvenance::Semantic,
        score: 1.0,
    };
    assert!(select_tag_basis_candidates([semantic.clone()], false, 24).is_empty());
    assert_eq!(select_tag_basis_candidates([semantic], true, 24).len(), 1);
}

#[test]
fn generated_and_inherited_membership_have_lower_seed_weight_than_explicit() {
    assert!(TagSeedProvenance::Generated.weight() < TagSeedProvenance::Explicit.weight());
    assert!(TagSeedProvenance::Inherited.weight() < TagSeedProvenance::Generated.weight());
}

#[test]
fn multi_space_composite_view_does_not_write_back() {
    let (space_a, memory_a, revision_a) = ids(1, 1, 1);
    let (space_b, memory_b, revision_b) = ids(2, 2, 2);
    let mut dictionary = TagDictionary::new();
    let mut graph = TagGraph::new();
    graph
        .insert_membership(
            &mut dictionary,
            TagMembershipInput::new(
                space_a,
                memory_a,
                revision_a,
                "rust",
                TagProvenance::Explicit,
            ),
        )
        .unwrap();
    graph
        .insert_membership(
            &mut dictionary,
            TagMembershipInput::new(
                space_b,
                memory_b,
                revision_b,
                "rust",
                TagProvenance::Explicit,
            ),
        )
        .unwrap();
    let before = graph.fingerprint();
    let view = memoria_query::CompositeTagView::from_scope(&graph, [space_a, space_b]);
    assert_eq!(view.spaces(), &[space_a, space_b]);
    assert_eq!(graph.fingerprint(), before);
}

#[test]
fn tag_readout_returns_revision_pinned_units_only_inside_scope() {
    let (space_a, memory_a, revision_a) = ids(1, 1, 1);
    let (space_b, memory_b, revision_b) = ids(2, 2, 2);
    let mut dictionary = TagDictionary::new();
    let mut graph = TagGraph::new();
    let tag_id = graph
        .insert_membership(
            &mut dictionary,
            TagMembershipInput::new(
                space_a,
                memory_a,
                revision_a,
                "rust",
                TagProvenance::Explicit,
            ),
        )
        .unwrap();
    graph
        .insert_membership(
            &mut dictionary,
            TagMembershipInput::new(
                space_b,
                memory_b,
                revision_b,
                "rust",
                TagProvenance::Explicit,
            ),
        )
        .unwrap();
    let candidates = readout_tag_candidates(&graph, &[tag_id], &[space_a]);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].target.memory_id, memory_a);
    assert_eq!(candidates[0].target.revision_id, revision_a);
}
