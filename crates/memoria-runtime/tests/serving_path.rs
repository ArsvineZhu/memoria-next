use memoria_derived::DerivedCatalog;
use memoria_mdx::test_support::{ir_compile_calls, reset_ir_compile_calls};
use memoria_query::{EntityRef, MemoryQuery, QueryError};
use memoria_runtime::{MemoriaRuntime, RuntimeError};
use tempfile::tempdir;

#[test]
fn current_text_query_uses_lexical_artifact_without_source_cas_reads() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("one"), b"# One\nRust local index")
        .unwrap();
    runtime
        .create_memory(space, Some("two"), b"# Two\nRust second index")
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
    assert_eq!(ir_compile_calls(), 0);
}

#[test]
fn current_text_query_across_spaces_uses_one_global_lexical_artifact() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let first_space = runtime.create_space("first").unwrap();
    let second_space = runtime.create_space("second").unwrap();
    runtime
        .create_memory(first_space, Some("one"), b"# One\nshared cross-space cue")
        .unwrap();
    runtime
        .create_memory(second_space, Some("two"), b"# Two\nshared cross-space cue")
        .unwrap();

    let response = runtime
        .query(
            MemoryQuery::builder()
                .spaces(vec![first_space, second_space])
                .text_cue("shared cross-space cue")
                .build()
                .unwrap(),
        )
        .unwrap();

    assert_eq!(response.results.len(), 2);
    assert!(
        response
            .results
            .iter()
            .any(|result| result.space_id == first_space)
    );
    assert!(
        response
            .results
            .iter()
            .any(|result| result.space_id == second_space)
    );
}

#[test]
fn exact_entity_constraint_uses_published_local_index() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(
            space,
            Some("entity"),
            br#"# Alice
<Entity ref="person:alice">Alice</Entity>
"#,
        )
        .unwrap();
    runtime
        .create_memory(space, Some("other"), b"# Other\nNo matching entity")
        .unwrap();

    let response = runtime
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .require_entity(EntityRef::new("person:alice").unwrap())
                .build()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(response.results.len(), 1);
}

#[test]
fn deleted_derived_artifact_returns_not_ready_until_rebuild_instead_of_source_scan() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("one"), b"# One\nRust")
        .unwrap();
    let mut catalog =
        DerivedCatalog::open(runtime.data_dir().join("derived/catalog.sqlite")).unwrap();
    catalog.delete_all_derived().unwrap();

    let error = runtime
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .text_cue("Rust")
                .build()
                .unwrap(),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::Query(QueryError::CapabilityNotReady { .. })
    ));
}
