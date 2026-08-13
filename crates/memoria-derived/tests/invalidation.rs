use memoria_derived::{InvalidationPlan, ProjectionKind};
use memoria_mdx::{SemanticDiff, compile_ir};

fn fixture_semantic_diff(case_name: &str) -> SemanticDiff {
    let (old_source, new_source) = match case_name {
        "state-valid-to" => (
            r#"<State id="s" validFrom="2025">A</State>"#,
            r#"<State id="s" validFrom="2025" validTo="2026">A</State>"#,
        ),
        "explicit-tag" => (
            r#"<State id="s">A</State><Tag value="old"/>"#,
            r#"<State id="s">A</State><Tag value="new"/>"#,
        ),
        _ => panic!("unknown fixture: {case_name}"),
    };
    let old = compile_ir(old_source).unwrap();
    let new = compile_ir(new_source).unwrap();
    SemanticDiff::between(&old, &new)
}

#[test]
fn valid_to_change_invalidates_temporal_not_lexical() {
    let plan = InvalidationPlan::from_diff(&fixture_semantic_diff("state-valid-to"));
    assert!(plan.rebuilds(ProjectionKind::Temporal));
    assert!(!plan.rebuilds(ProjectionKind::Lexical));
}

#[test]
fn explicit_tag_change_does_not_rebuild_default_text_embedding() {
    let plan = InvalidationPlan::from_diff(&fixture_semantic_diff("explicit-tag"));
    assert!(plan.rebuilds(ProjectionKind::ExplicitTags));
    assert!(!plan.rebuilds(ProjectionKind::LocalEmbedding));
}
