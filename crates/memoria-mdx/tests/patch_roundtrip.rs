use memoria_mdx::{
    CorrectionPatch, PatchOp, SemanticKind, SupersessionPatch, TemporalValue, TransitionState,
    apply_patch, apply_transition_state,
};

#[test]
fn time_patch_preserves_unrelated_comment() {
    let src = "# X\n<!-- keep -->\n<State id=\"s\" validFrom=\"2025\">A</State>\n";
    let result = apply_patch(
        src,
        &[PatchOp::SetStateValidTo {
            node_id: "s".parse().unwrap(),
            value: TemporalValue::month(2026, 8).unwrap(),
        }],
    );
    let out = result.unwrap();

    assert!(out.source().contains("<!-- keep -->"));
    assert!(out.source().contains("validTo=\"2026-08\""));
}

#[test]
fn existing_attributes_and_text_are_patched_without_reformatting() {
    let src = "<State id=\"s\" validFrom=\"2025\" validTo=\"2026\">old</State>\n<Event id=\"e\" occurredAt=\"2025\">event</Event>\n";
    let result = apply_patch(
        src,
        &[
            PatchOp::SetStateValidFrom {
                node_id: "s".parse().unwrap(),
                value: TemporalValue::year(2024).unwrap(),
            },
            PatchOp::ReplaceSemanticNodeText {
                node_id: "s".parse().unwrap(),
                text: "new".to_owned(),
            },
            PatchOp::SetEventOccurredAt {
                node_id: "e".parse().unwrap(),
                value: TemporalValue::month(2025, 7).unwrap(),
            },
        ],
    );
    let out = result.unwrap();

    assert!(out.source().contains("validFrom=\"2024\""));
    assert!(out.source().contains(">new</State>"));
    assert!(out.source().contains("occurredAt=\"2025-07\""));
}

#[test]
fn tags_and_relations_round_trip_through_validation() {
    let src = "<Section id=\"a\"/><Section id=\"b\"/>\n";
    let out = apply_patch(
        src,
        &[
            PatchOp::AddExplicitTag {
                value: "career".to_owned(),
            },
            PatchOp::AddRelation {
                node_id: "r".parse().unwrap(),
                from: "a".parse().unwrap(),
                to: "b".parse().unwrap(),
                kind: "associated-with".to_owned(),
            },
        ],
    )
    .unwrap();
    assert!(out.nodes().any(|node| node.kind() == SemanticKind::Tag));
    assert!(
        out.nodes()
            .any(|node| node.kind() == SemanticKind::Relation)
    );

    let out = apply_patch(
        out.source(),
        &[
            PatchOp::RemoveExplicitTag {
                value: "career".to_owned(),
            },
            PatchOp::RemoveSemanticNode {
                node_id: "r".parse().unwrap(),
            },
        ],
    )
    .unwrap();
    assert!(!out.source().contains("career"));
    assert!(!out.source().contains("<Relation"));
}

#[test]
fn transition_closes_old_state_and_adds_new_state_atomically() {
    let src = "<State id=\"old\" validFrom=\"2025\">A</State>\n";
    let transition = TransitionState::new(
        "old".parse().unwrap(),
        "new".parse().unwrap(),
        TemporalValue::month(2026, 8).unwrap(),
        "B",
    );
    let out = apply_transition_state(src, &transition).unwrap();
    assert!(out.source().contains("validTo=\"2026-08\""));
    assert!(
        out.source()
            .contains("<State id=\"new\" validFrom=\"2026-08\">B</State>")
    );
}

#[test]
fn correction_and_supersession_keep_explicit_revision_intents() {
    let correction = CorrectionPatch::new(Vec::new());
    let supersession = SupersessionPatch::new(Vec::new());
    assert_eq!(correction.semantic_intent().to_string(), "correction");
    assert_eq!(supersession.semantic_intent().to_string(), "supersession");
}

#[test]
fn self_closing_node_can_receive_time_and_text_edits() {
    let src = "<State id=\"s\"/>\n";
    let out = apply_patch(
        src,
        &[
            PatchOp::SetStateValidTo {
                node_id: "s".parse().unwrap(),
                value: TemporalValue::year(2026).unwrap(),
            },
            PatchOp::ReplaceSemanticNodeText {
                node_id: "s".parse().unwrap(),
                text: "A".to_owned(),
            },
        ],
    )
    .unwrap();
    assert_eq!(out.source(), "<State id=\"s\" validTo=\"2026\">A</State>\n");
}
