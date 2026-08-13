use memoria_mdx::parse_validate_and_lint;

#[test]
fn unresolved_deictic_in_canonical_narrative_is_warning_not_parse_error() {
    let result = parse_validate_and_lint("<State id=\"s\">我以后去那里工作。</State>").unwrap();
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "REFERENTIAL_CLOSURE_RISK")
    );
}

#[test]
fn quote_region_does_not_use_narrative_closure_rule() {
    let result =
        parse_validate_and_lint(r#"<Quote speaker="person:ABC">我以后去那里工作。</Quote>"#)
            .unwrap();
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "REFERENTIAL_CLOSURE_RISK")
    );
}
