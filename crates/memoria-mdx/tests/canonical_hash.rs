use memoria_mdx::compile_ir;

#[test]
fn formatting_only_change_keeps_semantic_hash() {
    let a = compile_ir("# X\n<State id=\"s\">A</State>\n").unwrap();
    let b = compile_ir("# X\r\n<State   id=\"s\">A</State>\r\n").unwrap();
    assert_eq!(a.semantic_hash(), b.semantic_hash());
}

#[test]
fn valid_time_change_changes_semantic_hash() {
    let a = compile_ir(r#"<State id="s" validFrom="2025">A</State>"#).unwrap();
    let b = compile_ir(r#"<State id="s" validFrom="2026">A</State>"#).unwrap();
    assert_ne!(a.semantic_hash(), b.semantic_hash());
}
