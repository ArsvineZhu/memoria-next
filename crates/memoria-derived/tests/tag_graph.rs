mod support;

use memoria_derived::TagProvenance;
use support::TagFixture;

#[test]
fn same_tag_identity_does_not_merge_space_evidence() {
    let mut fixture = TagFixture::new();
    let a = fixture.add_tag(1, "Rust", 1);
    let b = fixture.add_tag(2, " rust ", 2);

    assert_eq!(a.tag_id, b.tag_id);
    assert_ne!(
        fixture.graph(1).fingerprint(),
        fixture.graph(2).fingerprint()
    );
    assert_eq!(fixture.graph(1).membership_count(a.tag_id), 1);
    assert_eq!(fixture.graph(2).membership_count(b.tag_id), 1);
}

#[test]
fn cooccurrence_keeps_provenance_separate_from_final_weight() {
    let mut fixture = TagFixture::new();
    let rust = fixture.add_tag(1, "Rust", 1);
    let systems = fixture.add_tag(1, "Systems", 1);
    fixture.add_tag_with_provenance(1, "Rust", 2, TagProvenance::Generated);
    fixture.add_tag_with_provenance(1, "Systems", 2, TagProvenance::Generated);

    let edge = fixture.graph(1).edge(rust.tag_id, systems.tag_id).unwrap();
    assert_eq!(edge.explicit_evidence(), 1);
    assert_eq!(edge.generated_evidence(), 1);
    assert_eq!(edge.weight(), 1.5);
}
