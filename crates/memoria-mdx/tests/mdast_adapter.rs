use markdown::mdast::Node;
use memoria_mdx::{mdast_source_span, parse_mdast};

fn node_debug(node: &impl std::fmt::Debug) -> String {
    format!("{node:?}")
}

#[test]
fn mdast_parses_heading_prose_and_memoria_jsx_elements() {
    let source = "# Career\n<State id=\"current\">Software engineer</State>\n";
    let tree = parse_mdast(source).expect("MDX source should parse");
    let children = tree.children().expect("root should have children");

    let debug = children
        .iter()
        .map(node_debug)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        debug.contains("Heading"),
        "AST did not contain a heading: {debug}"
    );
    assert!(
        debug.contains("MdxJsx"),
        "AST did not contain a JSX element: {debug}"
    );
}

#[test]
fn mdast_positions_round_trip_to_source_byte_spans() {
    let source = "标题\n<State id=\"s\">正文</State>\n";
    let tree = parse_mdast(source).expect("MDX source should parse");
    let element = tree
        .children()
        .expect("root should have children")
        .iter()
        .find_map(find_jsx)
        .expect("semantic JSX element should be present");
    let span = mdast_source_span(element.position().expect("element should have a position"));

    assert_eq!(&source[span], "<State id=\"s\">正文</State>");
}

fn find_jsx(node: &Node) -> Option<&Node> {
    if node_debug(node).starts_with("MdxJsx") {
        return Some(node);
    }
    node.children()?.iter().find_map(find_jsx)
}

#[test]
fn mdast_exposes_mdx_expression_and_esm_nodes_for_rejection() {
    let source = "import X from \"x\"\n\n{compute()}\n";
    let tree = parse_mdast(source).expect("MDX syntax should be recognized");
    let children = tree.children().expect("root should have children");
    let debug = children
        .iter()
        .map(node_debug)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        debug.contains("MdxjsEsm"),
        "AST did not expose ESM: {debug}"
    );
    assert!(
        debug.contains("MdxFlowExpression"),
        "AST did not expose flow expression: {debug}"
    );
}

#[test]
fn fenced_code_with_angle_brackets_is_not_semantic_jsx() {
    let source = "```mdx\n<State id={compute()}>{not_code}</State>\n```\n";
    let tree = parse_mdast(source).expect("fenced code should parse as markdown");
    let children = tree.children().expect("root should have children");
    let debug = children
        .iter()
        .map(node_debug)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        debug.contains("Code {"),
        "AST did not contain a code node: {debug}"
    );
    assert!(!debug.contains("MdxJsx"), "fenced code became JSX: {debug}");
}
