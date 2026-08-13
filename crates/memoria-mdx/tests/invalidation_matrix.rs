use memoria_mdx::{InvalidationCategory, SemanticDiff, compile_ir};

#[test]
fn valid_to_change_is_temporal_not_visible_text() {
    let old = compile_ir(r#"<State id="s" validFrom="2025">A</State>"#).unwrap();
    let new = compile_ir(r#"<State id="s" validFrom="2025" validTo="2026">A</State>"#).unwrap();
    let diff = SemanticDiff::between(&old, &new);
    assert!(diff.temporal_changed("s"));
    assert!(!diff.visible_text_changed("s"));
}

#[test]
fn formatting_only_change_has_no_invalidation_category() {
    let old = compile_ir("# X\n<State id=\"s\">A</State>\n").unwrap();
    let new = compile_ir("# X\r\n<State   id=\"s\">A</State>\r\n").unwrap();
    let diff = SemanticDiff::between(&old, &new);
    assert!(diff.is_empty());
    assert!(!diff.has_category(InvalidationCategory::Temporal));
}

#[test]
fn entity_binding_tag_and_relation_changes_use_structural_categories() {
    let old = compile_ir(
        r##"<Entity id="person" ref="person:old">Ada</Entity><Tag value="old"/><Section id="a"/><Section id="b"/><Relation id="r" from="#a" to="#b" kind="association"/>"##,
    )
    .unwrap();
    let new = compile_ir(
        r##"<Entity id="person" ref="person:new">Ada</Entity><Tag value="new"/><Section id="a"/><Section id="b"/><Relation id="r" from="#b" to="#a" kind="association"/>"##,
    )
    .unwrap();
    let diff = SemanticDiff::between(&old, &new);
    assert!(diff.category_changed("person", InvalidationCategory::EntityBinding));
    assert!(diff.has_category(InvalidationCategory::ExplicitTag));
    assert!(diff.category_changed("r", InvalidationCategory::Relation));
}
