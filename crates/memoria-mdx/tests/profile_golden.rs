use memoria_mdx::{MdxError, parse_and_validate, parse_source};

#[test]
fn semantic_element_keeps_exact_span() {
    let src = "# X\n<State id=\"s\">A</State>\n";
    let parsed = parse_source(src).unwrap();
    let node = parsed.semantic_elements().next().unwrap();
    assert_eq!(&src[node.span()], "<State id=\"s\">A</State>");
}

#[test]
fn expression_attribute_is_rejected() {
    assert!(matches!(
        parse_source("<State id={compute()}>A</State>"),
        Err(MdxError::ExecutableSyntax { .. })
    ));
}

#[test]
fn literal_attributes_are_retained_and_self_closing_elements_have_exact_span() {
    let src = "<State id=\"s\" validFrom=\"2025\" />";
    let parsed = parse_source(src).unwrap();
    let node = parsed.semantic_elements().next().unwrap();
    assert_eq!(&src[node.span()], src);
    assert!(node.self_closing());
    assert_eq!(node.attributes()[0].name(), "id");
    assert_eq!(node.attributes()[0].value(), "s");
}

#[test]
fn literal_attribute_spans_point_at_values() {
    let src = "<State id=\"s\" validFrom=\"2025\"/>";
    let parsed = parse_source(src).unwrap();
    let node = parsed.semantic_elements().next().unwrap();
    assert_eq!(&src[node.attributes()[0].span()], "s");
    assert_eq!(&src[node.attributes()[1].span()], "2025");
}

#[test]
fn runtime_components_and_directives_are_rejected() {
    assert!(matches!(
        parse_source("<Button>Run</Button>"),
        Err(MdxError::UnsupportedRuntimeComponent { .. })
    ));
    assert!(matches!(
        parse_source("import X from 'x'"),
        Err(MdxError::ForbiddenDirective { .. })
    ));
}

#[test]
fn markdown_code_does_not_execute_the_restricted_lexer() {
    let src = "```mdx\n<State id={compute()}>{not_code}</State>\n```\n";
    let parsed = parse_source(src).unwrap();
    assert_eq!(parsed.semantic_elements().count(), 0);
}

#[test]
fn unicode_before_tag_does_not_break_byte_scanning() {
    let src = "中文\n<State id=\"s\">A</State>\n";
    let parsed = parse_source(src).unwrap();
    assert_eq!(parsed.semantic_elements().next().unwrap().name(), "State");
}

#[test]
fn pathological_nesting_hits_resource_limit() {
    let mut src = String::new();
    for _ in 0..300 {
        src.push_str("<Section id=\"x\">");
    }
    src.push('x');
    for _ in 0..300 {
        src.push_str("</Section>");
    }
    assert!(matches!(
        parse_and_validate(&src),
        Err(MdxError::ResourceLimit { .. })
    ));
}
