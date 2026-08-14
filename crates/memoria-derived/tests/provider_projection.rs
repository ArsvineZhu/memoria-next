use memoria_derived::{
    ContextEmbeddingProjectionV1, LocalEmbeddingProjectionV1, QueryEmbeddingProjectionV1,
    RerankViewV1,
};
use memoria_mdx::compile_ir;

fn projection(source: &str) -> LocalEmbeddingProjectionV1 {
    let ir = compile_ir(source).unwrap();
    LocalEmbeddingProjectionV1::build(&ir, "embedding-v1").unwrap()
}

#[test]
fn embedding_projection_excludes_mdx_markup_refs_tags_and_source_locators() {
    let local = projection(
        r#"# Career
<Entity id="person" ref="person:7K4Q">Ada</Entity>
<Source id="src" ref="host-message:ABC"/>
<Tag value="private"/>
<State id="work">Rust systems work</State>
"#,
    );

    assert!(local.content().contains("Ada"));
    assert!(local.content().contains("Rust systems work"));
    assert!(!local.content().contains("<Entity"));
    assert!(!local.content().contains("person:7K4Q"));
    assert!(!local.content().contains("<Source"));
    assert!(!local.content().contains("host-message:ABC"));
    assert!(!local.content().contains("<Tag"));
    assert!(!local.content().contains("private"));
}

#[test]
fn tag_only_revision_keeps_content_embedding_projection_hash() {
    let before = projection(r#"<State id="s">career planning</State><Tag value="old"/>"#);
    let after = projection(r#"<State id="s">career planning</State><Tag value="new"/>"#);

    assert_eq!(before.input_hash(), after.input_hash());
    assert_eq!(before.content(), after.content());
}

#[test]
fn valid_to_only_revision_keeps_content_embedding_projection_hash() {
    let before = projection(r#"<State id="s" validFrom="2025">career planning</State>"#);
    let after =
        projection(r#"<State id="s" validFrom="2025" validTo="2026">career planning</State>"#);

    assert_eq!(before.input_hash(), after.input_hash());
}

#[test]
fn context_projection_includes_heading_and_display_surface_not_entity_ref() {
    let ir = compile_ir(
        r#"# Career
<Section id="systems"><Entity ref="person:7K4Q">Ada</Entity>Rust systems work</Section>
"#,
    )
    .unwrap();
    let context = ContextEmbeddingProjectionV1::build(&ir, "context-v1").unwrap();

    assert!(context.content().contains("Career"));
    assert!(context.content().contains("Ada"));
    assert!(context.content().contains("Rust systems work"));
    assert!(!context.content().contains("person:7K4Q"));
}

#[test]
fn rerank_view_hashes_only_bounded_provider_surfaces_and_handle() {
    let view = RerankViewV1::build(
        "SP:MEM:REV",
        "Career",
        "Systems",
        "Rust systems work",
        "bounded neighboring context",
        "rerank-v1",
    )
    .unwrap();

    assert_eq!(view.handle(), "SP:MEM:REV");
    assert!(view.content().contains("Career"));
    assert!(view.content().contains("Rust systems work"));
    assert!(!view.content().contains("<State"));
}

#[test]
fn query_embedding_projection_normalizes_unicode_whitespace_only() {
    let projection = QueryEmbeddingProjectionV1::build("  career\u{00a0}\n\tRust  ");

    assert_eq!(projection.content(), "career Rust");
}
