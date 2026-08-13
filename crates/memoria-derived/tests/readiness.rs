use std::time::Duration;

use memoria_derived::{
    AuthorityGeneration, DerivedCatalog, DerivedCompiler, DerivedScheduler, LexicalDocument,
};
use memoria_mdx::compile_ir;
use memoria_types::{MemoryId, RevisionId, SpaceId};
use tempfile::tempdir;

fn document() -> LexicalDocument {
    LexicalDocument::new(
        SpaceId::from_bytes([1; 16]),
        MemoryId::from_bytes([2; 16]),
        RevisionId::from_bytes([3; 32]),
        compile_ir("# Career\nRust").unwrap(),
    )
}

#[test]
fn authority_commit_is_visible_before_derived_catches_up() {
    let dir = tempdir().unwrap();
    let catalog = DerivedCatalog::open(dir.path().join("derived.sqlite")).unwrap();
    let scheduler = DerivedScheduler::new(
        catalog,
        DerivedCompiler::default(),
        AuthorityGeneration::initial(),
    );
    scheduler.pause();

    let generation = AuthorityGeneration::new(1);
    scheduler.enqueue_base(generation, [document()]).unwrap();
    let status = scheduler.status();
    assert_eq!(status.authority_generation, generation);
    assert!(status.base_coverage < generation);

    scheduler.resume();
    let manifest = scheduler
        .wait_for_capabilities(generation, "base-search", Duration::from_secs(2))
        .unwrap();
    assert_eq!(manifest.authority_generation(), generation);
    assert!(scheduler.status().base_coverage >= generation);
}

#[test]
fn newer_generation_coalesces_queued_work() {
    let dir = tempdir().unwrap();
    let catalog = DerivedCatalog::open(dir.path().join("derived.sqlite")).unwrap();
    let scheduler = DerivedScheduler::new(
        catalog,
        DerivedCompiler::default(),
        AuthorityGeneration::initial(),
    );
    scheduler.pause();
    scheduler
        .enqueue_base(AuthorityGeneration::new(1), [document()])
        .unwrap();
    scheduler
        .enqueue_base(AuthorityGeneration::new(2), [document()])
        .unwrap();
    assert_eq!(
        scheduler.status().queued_generation,
        Some(AuthorityGeneration::new(2))
    );
}
