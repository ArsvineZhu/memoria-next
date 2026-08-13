use memoria_derived::{
    EntityObservationBuilder, ExplicitTagBuilder, LexicalDocument, TagProvenance, build_lexical,
    lexical_projection_hash,
};
use memoria_mdx::{MemoryIr, compile_ir};
use memoria_types::{MemoryId, RevisionId, SpaceId};

fn fixture_ir(source: &str) -> MemoryIr {
    compile_ir(source).unwrap()
}

#[test]
fn entity_observation_keeps_ref_and_surface() {
    let ir = fixture_ir(r#"<Entity ref="person:ABC">Alex</Entity>"#);
    let artifact = EntityObservationBuilder::build(&ir).unwrap();
    assert_eq!(artifact.observations()[0].entity_ref.as_str(), "person:ABC");
    assert_eq!(artifact.observations()[0].surface, "Alex");
}

#[test]
fn explicit_tag_records_scope_and_provenance() {
    let ir = fixture_ir(r#"<State id="s"><Tag value="work"/>A</State>"#);
    let artifact = ExplicitTagBuilder::build(&ir).unwrap();
    assert_eq!(artifact.memberships()[0].node_id.as_deref(), Some("s"));
    assert_eq!(
        artifact.memberships()[0].provenance,
        TagProvenance::Explicit
    );
}

fn fixture_memory_with_text(text: &str) -> LexicalDocument {
    LexicalDocument::new(
        SpaceId::from_bytes([1; 16]),
        MemoryId::from_bytes([2; 16]),
        RevisionId::from_bytes([3; 32]),
        compile_ir(text).unwrap(),
    )
}

#[test]
fn lexical_hit_is_revision_pinned() {
    let fixture = fixture_memory_with_text("# Career\nRust planning");
    let index = build_lexical(&fixture).unwrap();
    let hits = index.search("Rust", 10).unwrap();
    assert_eq!(hits[0].memory_id, fixture.memory_id());
    assert_eq!(hits[0].revision_id, fixture.revision_id());
}

#[test]
fn tag_only_change_keeps_lexical_projection_hash() {
    let a = fixture_memory_with_text(r#"<State id="s">career planning</State><Tag value="old"/>"#);
    let b = fixture_memory_with_text(r#"<State id="s">career planning</State><Tag value="new"/>"#);
    assert_eq!(lexical_projection_hash(&a), lexical_projection_hash(&b));
}
