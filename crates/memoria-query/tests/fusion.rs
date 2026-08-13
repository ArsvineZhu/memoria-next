use memoria_derived::DerivedCatalog;
use memoria_query::{
    ExactIndex, ExactRecord, MemoryQuery, QueryCompiler, execute_exact, fuse_channels, rrf,
};
use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};
use tempfile::tempdir;

fn compiler() -> QueryCompiler {
    let dir = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(dir.path().join("derived.sqlite")).unwrap();
    let manifest = catalog
        .publish_empty_manifest(AuthorityGeneration::new(1))
        .unwrap();
    QueryCompiler::new(AuthorityGeneration::new(1), Some(manifest))
}

#[test]
fn rrf_fuses_channels_without_mixing_raw_score_scales() {
    let query = MemoryQuery::builder()
        .spaces(vec![SpaceId::from_bytes([1; 16])])
        .build()
        .unwrap();
    let compiled = compiler().compile(query).unwrap();
    let a = ExactRecord::new(
        SpaceId::from_bytes([1; 16]),
        MemoryId::from_bytes([1; 16]),
        RevisionId::from_bytes([1; 32]),
        "A",
    );
    let b = ExactRecord::new(
        SpaceId::from_bytes([1; 16]),
        MemoryId::from_bytes([2; 16]),
        RevisionId::from_bytes([2; 32]),
        "B",
    );
    let exact = execute_exact(&compiled, &ExactIndex::new(vec![a, b]));
    let fused = fuse_channels(
        vec![
            exact.results.clone(),
            exact.results.iter().rev().cloned().collect(),
        ],
        60.0,
        10,
    );
    assert_eq!(fused.len(), 2);
    assert_eq!(fused[0].score, rrf(0, 60.0) + rrf(1, 60.0));
    assert!(!fused[0].evidence.exact.is_empty());
}
