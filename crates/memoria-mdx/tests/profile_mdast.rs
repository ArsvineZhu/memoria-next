use memoria_mdx::{MdxError, SemanticKind, parse_and_validate};

#[test]
fn known_memoria_element_with_literal_attributes_is_valid() {
    let document = parse_and_validate(r#"<State id="s" validFrom="2025">A</State>"#)
        .expect("literal semantic JSX should be accepted");

    let node = document.state("s").expect("state should be present");
    assert_eq!(node.kind(), SemanticKind::State);
    assert_eq!(node.attributes()["validFrom"], "2025");
}

#[test]
fn mdx_text_expression_is_rejected() {
    assert!(matches!(
        parse_and_validate("Narrative {compute()}"),
        Err(MdxError::ExecutableSyntax { .. })
    ));
}

#[test]
fn mdx_flow_expression_is_rejected() {
    assert!(matches!(
        parse_and_validate("{compute()}"),
        Err(MdxError::ExecutableSyntax { .. })
    ));
}

#[test]
fn mdx_esm_is_rejected() {
    assert!(matches!(
        parse_and_validate("import X from \"x\""),
        Err(MdxError::ForbiddenDirective { .. })
    ));
}

#[test]
fn jsx_expression_attribute_is_rejected() {
    assert!(matches!(
        parse_and_validate("<State id={compute()}>A</State>"),
        Err(MdxError::ExecutableSyntax { .. })
    ));
}

#[test]
fn raw_html_node_is_rejected() {
    assert!(matches!(
        parse_and_validate("<div>unsafe</div>"),
        Err(MdxError::RawHtml { .. })
    ));
}

#[test]
fn unknown_namespaced_extension_is_preserved_as_extension() {
    let document = parse_and_validate(
        r#"<vendor:Widget id="x" namespace="com.example" type="widget" version="1">data</vendor:Widget>"#,
    )
    .expect("unknown namespaced JSX should enter the extension profile");

    let node = document
        .node(&"x".parse().unwrap())
        .expect("extension should be present");
    assert_eq!(node.kind(), SemanticKind::Extension);
    assert_eq!(node.attributes()["namespace"], "com.example");
}
