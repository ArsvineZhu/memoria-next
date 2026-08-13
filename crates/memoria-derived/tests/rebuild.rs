use memoria_derived::{EntityObservationBuilder, ExplicitTagBuilder, TagProvenance};
use memoria_mdx::{MemoryIr, compile_ir};

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
