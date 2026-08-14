use memoria_derived::DerivedCatalog;
use memoria_mdx::test_support::{ir_compile_calls, reset_ir_compile_calls};
use memoria_query::{MemoryQuery, QueryCompiler, QueryError};
use memoria_runtime::{MemoriaRuntime, NeedWork};
use memoria_types::AuthorityGeneration;
use tempfile::tempdir;

fn compiler_with_empty_manifest(authority: u64, coverage: u64) -> QueryCompiler {
    let dir = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(dir.path().join("derived.sqlite")).unwrap();
    let manifest = catalog
        .publish_empty_manifest(AuthorityGeneration::new(coverage))
        .unwrap();
    QueryCompiler::new(AuthorityGeneration::new(authority), Some(manifest))
}

#[test]
fn semantic_readiness_requires_manifest_artifact() {
    let query = MemoryQuery::builder()
        .spaces(vec![memoria_types::SpaceId::from_bytes([7; 16])])
        .require_capability("semantic")
        .authority_at_least(AuthorityGeneration::new(5))
        .fail_if_not_ready()
        .build()
        .unwrap();
    let compiler = compiler_with_empty_manifest(5, 5);
    assert!(matches!(
        compiler.compile(query),
        Err(QueryError::CapabilityNotReady { .. })
    ));
}

#[test]
fn raw_mdx_is_not_an_embedding_work_item() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(
            space,
            Some("career"),
            br#"# Career
<Tag value="private"/>
Rust systems work
"#,
        )
        .unwrap();

    let work = runtime.provider_poll_work().unwrap().unwrap();
    match work {
        NeedWork::Embeddings(request) => {
            assert_eq!(request.items.len(), 1);
            assert!(!request.items[0].text.contains("<Tag"));
            assert!(!request.items[0].text.contains(r#"value="private""#));
            assert!(request.items[0].text.contains("Rust systems work"));
        }
        other => panic!("expected embedding work, got {other:?}"),
    }
}

#[test]
fn normal_current_query_does_not_reparse_every_authority_source() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("career"), b"# Career\nRust systems work")
        .unwrap();
    runtime
        .create_memory(space, Some("graph"), b"# Graph\nRust retrieval notes")
        .unwrap();

    reset_ir_compile_calls();
    let response = runtime
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .text_cue("Rust")
                .build()
                .unwrap(),
        )
        .unwrap();

    assert_eq!(response.results.len(), 2);
    assert_eq!(
        ir_compile_calls(),
        0,
        "normal current query reparsed Authority sources",
    );
}
