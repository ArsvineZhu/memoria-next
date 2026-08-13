use memoria_derived::{TagDictionary, TagGraph, TagMembershipInput, TagProvenance};
use memoria_query::{CompositeTagView, TagSeedProvenance, resolve_explicit_tag_seeds};
use memoria_types::{MemoryId, RevisionId, SpaceId};

fn ids(space: u8, memory: u8) -> (SpaceId, MemoryId, RevisionId) {
    (
        SpaceId::from_bytes([space; 16]),
        MemoryId::from_bytes([memory; 16]),
        RevisionId::from_bytes([memory; 32]),
    )
}

#[test]
fn composite_tag_view_is_scoped_and_query_local() {
    let (space_a, memory_a, revision_a) = ids(1, 1);
    let (space_b, memory_b, revision_b) = ids(2, 2);
    let mut dictionary = TagDictionary::new();
    let mut graph = TagGraph::new();
    let rust = graph
        .insert_membership(
            &mut dictionary,
            TagMembershipInput::new(
                space_a,
                memory_a,
                revision_a,
                "Rust",
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
                "Rust",
                TagProvenance::Explicit,
            ),
        )
        .unwrap();
    let before = graph.fingerprint();

    let view = CompositeTagView::from_scope(&graph, [space_a, space_b]);
    assert_eq!(view.membership_count(rust), 2);
    assert_eq!(view.spaces(), &[space_a, space_b]);
    assert_eq!(graph.fingerprint(), before);
}

#[test]
fn explicit_tag_seeds_keep_deterministic_provenance() {
    let mut dictionary = TagDictionary::new();
    let seeds = resolve_explicit_tag_seeds(&mut dictionary, ["Rust", " rust "]).unwrap();
    assert_eq!(seeds.len(), 1);
    assert_eq!(seeds[0].provenance, TagSeedProvenance::Explicit);
    assert_eq!(seeds[0].value, "rust");
}
