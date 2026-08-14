use memoria_mdx::{MdxError, SemanticKind, TemporalPrecision, parse_and_validate};

#[test]
fn semantic_node_ids_are_document_wide_unique() {
    let src = r#"<State id="x">A</State><Event id="x" occurredAt="2026">B</Event>"#;
    assert!(matches!(
        parse_and_validate(src),
        Err(MdxError::DuplicateSemanticNodeId { .. })
    ));
}

#[test]
fn temporal_precision_is_preserved() {
    let doc = parse_and_validate(r#"<Event id="x" occurredAt="2026-08">A</Event>"#).unwrap();
    assert_eq!(
        doc.event("x").unwrap().occurred_at().unwrap().precision(),
        TemporalPrecision::Month
    );
}

#[test]
fn entity_refs_use_a_generic_namespace_qualified_grammar() {
    assert!(parse_and_validate(r#"<Entity id="e" ref="person:7K4Q">A</Entity>"#).is_ok());
    assert!(matches!(
        parse_and_validate(r#"<Entity id="e" ref="not-qualified">A</Entity>"#),
        Err(MdxError::InvalidEntityRef { .. })
    ));
}

#[test]
fn relation_endpoints_must_resolve_to_declared_node_ids() {
    assert!(parse_and_validate(
        r##"<Section id="a"/><Section id="b"/><Relation id="r" from="#a" to="#b" kind="associated-with"/>"##
    )
    .is_ok());
    assert!(matches!(
        parse_and_validate(
            r##"<Relation id="r" from="#missing" to="#b" kind="associated-with"/>"##
        ),
        Err(MdxError::InvalidReference { .. })
    ));
}

#[test]
fn unknown_kind_is_rejected_but_opaque_class_is_valid() {
    assert!(matches!(
        parse_and_validate(r#"<Event id="e" occurredAt="2026" kind="made-up"/>"#),
        Err(MdxError::UnknownKind { .. })
    ));
    assert!(
        parse_and_validate(r#"<Event id="e" occurredAt="2026" class="caller.taxonomy"/>"#).is_ok()
    );
}

#[test]
fn core_kind_is_checked_against_the_element_schema() {
    assert!(matches!(
        parse_and_validate(
            r##"<Section id="a"/><Section id="b"/><Relation id="r" from="#a" to="#b" kind="person"/>"##
        ),
        Err(MdxError::UnknownKind { .. })
    ));
    assert!(matches!(
        parse_and_validate(r#"<Entity id="e" ref="person:ada" kind="causal">Ada</Entity>"#),
        Err(MdxError::UnknownKind { .. })
    ));
    let document = parse_and_validate(
        r#"<Entity id="e" ref="person:ada" kind="person" class="caller.taxonomy">Ada</Entity>"#,
    )
    .unwrap();
    assert_eq!(
        document.node(&"e".parse().unwrap()).unwrap().attributes()["class"],
        "caller.taxonomy"
    );
}

#[test]
fn extension_metadata_is_literal_and_versioned() {
    assert!(parse_and_validate(
        r#"<Extension id="x" namespace="com.example" type="decision-context" version="1">data</Extension>"#
    )
    .is_ok());
    assert!(matches!(
        parse_and_validate(
            r#"<Extension id="x" namespace="com.example" type="decision-context" version="v1"/>"#
        ),
        Err(MdxError::InvalidExtension { .. })
    ));
}

#[test]
fn source_and_quote_keep_typed_provenance_context() {
    let doc = parse_and_validate(
        r#"<Source id="src-career" ref="host-message:ABC" observedAt="2025-07-16"/><Quote speaker="person:7K4Q" source="src-career">我当时不想继续读研。</Quote>"#,
    )
    .unwrap();
    let source = doc
        .nodes()
        .find(|node| node.kind() == SemanticKind::Source)
        .unwrap();
    assert_eq!(
        source.observed_at().unwrap().precision(),
        TemporalPrecision::Day
    );
}
