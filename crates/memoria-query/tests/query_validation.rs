use memoria_derived::DerivedCatalog;
use memoria_query::{AuthorityConsistency, EntityRef, MemoryQuery, QueryCompiler, QueryError};
use memoria_types::AuthorityGeneration;
use memoria_types::SpaceId;
use tempfile::tempdir;

fn fixture_space() -> SpaceId {
    SpaceId::from_bytes([7; 16])
}

#[test]
fn scope_is_required() {
    let query = MemoryQuery::builder().text_cue("career").build_unchecked();
    assert!(matches!(query.validate(), Err(QueryError::ScopeRequired)));
}

#[test]
fn cue_entity_is_not_a_constraint() {
    let query = MemoryQuery::builder()
        .spaces(vec![fixture_space()])
        .cue_entity("person:ABC".parse::<EntityRef>().unwrap())
        .build()
        .unwrap();
    assert_eq!(query.cue.entities.len(), 1);
    assert!(query.constraints.entities.is_empty());
}

#[test]
fn unsupported_capability_is_rejected_by_domain_validation() {
    let error = MemoryQuery::builder()
        .spaces(vec![fixture_space()])
        .require_capability("unknown")
        .build()
        .unwrap_err();
    assert!(matches!(
        error,
        QueryError::UnsupportedCapability { capability } if capability == "unknown"
    ));
}

fn compiler_with_semantic_manifest(authority: u64, coverage: u64) -> QueryCompiler {
    let dir = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(dir.path().join("derived.sqlite")).unwrap();
    let artifact = catalog
        .stage_artifact("semantic", 1, AuthorityGeneration::new(coverage))
        .unwrap();
    catalog.validate_artifact(artifact.id()).unwrap();
    let manifest = catalog
        .publish_manifest_at_generation(
            vec![artifact.id()],
            AuthorityGeneration::new(coverage),
            Vec::new(),
        )
        .unwrap();
    QueryCompiler::new(AuthorityGeneration::new(authority), Some(manifest))
}

#[test]
fn required_semantic_cannot_use_stale_manifest_coverage() {
    let query = MemoryQuery::builder()
        .spaces(vec![fixture_space()])
        .require_capability("semantic")
        .authority_at_least(AuthorityGeneration::new(12))
        .fail_if_not_ready()
        .build()
        .unwrap();
    let compiler = compiler_with_semantic_manifest(12, 10);
    assert!(matches!(
        compiler.compile(query),
        Err(QueryError::CapabilityNotReady { .. })
    ));
}

#[test]
fn preferred_semantic_degrades_when_manifest_is_stale() {
    let query = MemoryQuery::builder()
        .spaces(vec![fixture_space()])
        .prefer_capability("semantic")
        .build()
        .unwrap();
    let compiler = compiler_with_semantic_manifest(12, 10);
    let compiled = compiler.compile(query).unwrap();
    assert!(compiled.execution.degraded);
    assert!(matches!(
        compiled.query.consistency.authority,
        AuthorityConsistency::Latest
    ));
}
