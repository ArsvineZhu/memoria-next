use memoria_derived::DerivedCatalog;
use memoria_types::AuthorityGeneration;
use tempfile::tempdir;

use memoria_query::{MemoryQuery, QueryCompiler, QueryError};

fn fixture_space() -> memoria_types::SpaceId {
    memoria_types::SpaceId::from_bytes([7; 16])
}

#[test]
fn semantic_is_not_ready_without_compatible_manifest_artifact() {
    let directory = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(directory.path().join("derived.sqlite")).unwrap();
    let manifest = catalog
        .publish_manifest_at_generation(
            Vec::new(),
            AuthorityGeneration::new(5),
            vec!["semantic".to_owned()],
        )
        .unwrap();
    let query = MemoryQuery::builder()
        .spaces(vec![fixture_space()])
        .require_capability("semantic")
        .authority_at_least(AuthorityGeneration::new(5))
        .fail_if_not_ready()
        .build()
        .unwrap();

    assert!(matches!(
        QueryCompiler::new(AuthorityGeneration::new(5), Some(manifest)).compile(query),
        Err(QueryError::CapabilityNotReady { .. })
    ));
}

#[test]
fn semantic_is_ready_only_when_manifest_coverage_meets_authority_requirement() {
    let directory = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(directory.path().join("derived.sqlite")).unwrap();
    let artifact = catalog
        .stage_artifact("semantic", 1, AuthorityGeneration::new(5))
        .unwrap();
    catalog.validate_artifact(artifact.id()).unwrap();
    let manifest = catalog
        .publish_manifest_at_generation(
            vec![artifact.id()],
            AuthorityGeneration::new(5),
            Vec::new(),
        )
        .unwrap();
    let query = MemoryQuery::builder()
        .spaces(vec![fixture_space()])
        .require_capability("semantic")
        .authority_at_least(AuthorityGeneration::new(5))
        .fail_if_not_ready()
        .build()
        .unwrap();

    assert!(
        QueryCompiler::new(AuthorityGeneration::new(5), Some(manifest.clone()))
            .compile(query.clone())
            .is_ok()
    );
    assert!(matches!(
        QueryCompiler::new(AuthorityGeneration::new(6), Some(manifest)).compile(query),
        Err(QueryError::CapabilityNotReady { .. })
    ));
}

#[test]
fn provider_success_without_publication_does_not_change_serving_readiness() {
    let directory = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(directory.path().join("derived.sqlite")).unwrap();
    let manifest = catalog
        .publish_empty_manifest(AuthorityGeneration::new(5))
        .unwrap();
    let query = MemoryQuery::builder()
        .spaces(vec![fixture_space()])
        .prefer_capability("semantic")
        .build()
        .unwrap();

    let compiled = QueryCompiler::new(AuthorityGeneration::new(5), Some(manifest))
        .compile(query)
        .unwrap();
    assert!(compiled.execution.degraded);
    assert!(!compiled.execution.used("semantic"));
}
