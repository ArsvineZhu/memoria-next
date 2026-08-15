use memoria_mdx::compile_ir;

#[test]
fn parser_contract_keeps_canonical_ir_hash_and_source_spans() {
    let source = "# Career\n<State id=\"current\" validFrom=\"2024\">Software engineer</State>\n<Event id=\"joined\" occurredAt=\"2024-01-02\">Joined team</Event>\n";
    let ir = compile_ir(source).expect("characterization fixture must compile");

    assert_eq!(ir.version(), 1);
    assert_eq!(
        ir.semantic_hash().to_hex(),
        "6b87f246b9de0735f670cfd48a8cde9ffe64284805b2761cdccfefdd32690a3a"
    );

    let nodes = ir
        .nodes()
        .map(|node| {
            (
                node.kind().as_str(),
                node.id().map(|id| id.as_str()),
                node.text(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        nodes,
        vec![
            ("State", Some("current"), "Software engineer"),
            ("Event", Some("joined"), "Joined team"),
        ]
    );

    let mapped_source = ir
        .source_mappings()
        .map(|mapping| source[mapping.span()].to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        mapped_source,
        vec![
            "<State id=\"current\" validFrom=\"2024\">Software engineer</State>",
            "<Event id=\"joined\" occurredAt=\"2024-01-02\">Joined team</Event>",
        ]
    );
}

#[test]
fn formatting_only_change_keeps_parser_contract_hash() {
    let canonical = compile_ir("<State id=\"current\">Software engineer</State>").unwrap();
    let formatted = compile_ir("\n<State id=\"current\">Software engineer</State>\n").unwrap();

    assert_eq!(canonical.semantic_hash(), formatted.semantic_hash());
    assert_eq!(canonical.nodes().count(), formatted.nodes().count());
}
