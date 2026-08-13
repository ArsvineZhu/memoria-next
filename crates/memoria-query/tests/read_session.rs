use std::time::Duration;

use memoria_derived::{AuthorityGeneration, DerivedCatalog};
use memoria_query::{MemoryQuery, QueryCompiler, ReadSession, SessionError};
use memoria_types::SpaceId;
use tempfile::tempdir;

fn query() -> MemoryQuery {
    MemoryQuery::builder()
        .spaces(vec![SpaceId::from_bytes([1; 16])])
        .text_cue("old")
        .build()
        .unwrap()
}

#[test]
fn session_keeps_old_logical_snapshot_after_new_write() {
    let dir = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(dir.path().join("derived.sqlite")).unwrap();
    let old_manifest = catalog
        .publish_empty_manifest(AuthorityGeneration::new(1))
        .unwrap();
    let old_compiled = QueryCompiler::new(AuthorityGeneration::new(1), Some(old_manifest))
        .compile(query())
        .unwrap();
    let session = ReadSession::open(&mut catalog, &old_compiled, Duration::from_secs(60)).unwrap();
    let continuation = session.continuation(3).unwrap();

    let new_manifest = catalog
        .publish_empty_manifest(AuthorityGeneration::new(2))
        .unwrap();
    let new_compiled = QueryCompiler::new(AuthorityGeneration::new(2), Some(new_manifest))
        .compile(query())
        .unwrap();

    assert_eq!(
        session.snapshot().authority_generation,
        AuthorityGeneration::new(1)
    );
    assert_ne!(
        session.snapshot().authority_generation,
        new_compiled.snapshot.authority_generation
    );
    assert_eq!(
        session
            .validate_continuation(&continuation, &query())
            .unwrap(),
        3
    );
}

#[test]
fn continuation_rejects_a_different_query_and_expiry() {
    let dir = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(dir.path().join("derived.sqlite")).unwrap();
    let manifest = catalog
        .publish_empty_manifest(AuthorityGeneration::new(1))
        .unwrap();
    let compiled = QueryCompiler::new(AuthorityGeneration::new(1), Some(manifest))
        .compile(query())
        .unwrap();
    let session = ReadSession::new(&compiled, Duration::from_millis(0)).unwrap();
    let continuation = session.continuation(0);
    assert!(matches!(continuation, Err(SessionError::Expired)));
}
