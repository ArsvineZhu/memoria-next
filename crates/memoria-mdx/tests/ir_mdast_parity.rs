use memoria_mdx::compile_ir;

#[test]
fn mdast_ir_parity_preserves_existing_parser_contract_fixture() {
    let source = "# Career\n<State id=\"current\" validFrom=\"2024\">Software engineer</State>\n<Event id=\"joined\" occurredAt=\"2024-01-02\">Joined team</Event>\n";
    let ir = compile_ir(source).expect("contract fixture must compile through mdast");

    assert_eq!(
        ir.semantic_hash().to_hex(),
        "6b87f246b9de0735f670cfd48a8cde9ffe64284805b2761cdccfefdd32690a3a"
    );
    assert_eq!(ir.text_hierarchy(), "# Career");
    assert_eq!(
        ir.nodes()
            .map(|node| (
                node.kind().as_str(),
                node.id().map(|id| id.as_str()),
                node.text()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("State", Some("current"), "Software engineer"),
            ("Event", Some("joined"), "Joined team"),
        ]
    );
    assert_eq!(
        ir.source_mappings()
            .map(|mapping| &source[mapping.span()])
            .collect::<Vec<_>>(),
        vec![
            "<State id=\"current\" validFrom=\"2024\">Software engineer</State>",
            "<Event id=\"joined\" occurredAt=\"2024-01-02\">Joined team</Event>",
        ]
    );
}

#[test]
fn mdast_ir_parity_preserves_nested_hierarchy_and_temporal_values() {
    let source =
        "# Root\n<Section id=\"root\"><State id=\"s\" validFrom=\"2025\">A</State></Section>\n";
    let ir = compile_ir(source).expect("nested contract fixture must compile through mdast");

    assert_eq!(ir.text_hierarchy(), "# Root");
    assert_eq!(
        ir.nodes()
            .map(|node| (
                node.kind().as_str(),
                node.id().map(|id| id.as_str()),
                node.text()
            ))
            .collect::<Vec<_>>(),
        vec![("Section", Some("root"), "A"), ("State", Some("s"), "A"),]
    );
    assert_eq!(
        ir.nodes().nth(1).unwrap().valid_from().unwrap().raw(),
        "2025"
    );
    assert_eq!(
        ir.source_mappings()
            .map(|mapping| &source[mapping.span()])
            .collect::<Vec<_>>(),
        vec![
            "<Section id=\"root\"><State id=\"s\" validFrom=\"2025\">A</State></Section>",
            "<State id=\"s\" validFrom=\"2025\">A</State>",
        ]
    );
}

#[test]
fn mdast_ir_parity_excludes_html_comments_inside_semantic_text() {
    let plain = compile_ir(r#"<State id="s">AB</State>"#).unwrap();
    let commented = compile_ir(r#"<State id="s">A<!-- keep -->B</State>"#).unwrap();

    assert_eq!(plain.semantic_hash(), commented.semantic_hash());
    assert_eq!(commented.nodes().next().unwrap().text(), "AB");
}
