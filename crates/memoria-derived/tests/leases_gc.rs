use std::time::Duration;

use memoria_derived::{
    ArtifactState, AuthorityGeneration, DerivedCatalog, DerivedCompiler, LexicalDocument,
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

fn publish_empty(catalog: &mut DerivedCatalog, generation: u64) -> memoria_derived::ManifestId {
    catalog
        .publish_empty_manifest(AuthorityGeneration::new(generation))
        .unwrap()
        .id()
}

#[test]
fn active_lease_keeps_old_manifest() {
    let dir = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(dir.path().join("derived.sqlite")).unwrap();
    let old = publish_empty(&mut catalog, 1);
    let lease = catalog.acquire_lease(old, Duration::from_secs(60)).unwrap();
    let _new = publish_empty(&mut catalog, 2);

    let report = catalog.gc().collect().unwrap();
    assert!(!report.deleted_manifests.contains(&old));
    drop(lease);

    let report = catalog.gc().collect().unwrap();
    assert!(report.deleted_manifests.contains(&old));
}

#[test]
fn deleting_derived_is_recoverable() {
    let dir = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(dir.path().join("derived.sqlite")).unwrap();
    let compiler = DerivedCompiler::default();
    let generation = AuthorityGeneration::new(7);
    let first = compiler
        .compile_base(&mut catalog, generation, [document()])
        .unwrap();
    let first_fingerprint = manifest_fingerprint(&catalog, first.manifest().id());

    catalog.delete_all_derived().unwrap();
    assert!(catalog.serving_manifest().unwrap().is_none());

    let second = compiler
        .compile_base(&mut catalog, generation, [document()])
        .unwrap();
    assert_eq!(
        first_fingerprint,
        manifest_fingerprint(&catalog, second.manifest().id())
    );
}

fn manifest_fingerprint(catalog: &DerivedCatalog, id: memoria_derived::ManifestId) -> Vec<String> {
    let manifest = catalog.manifest(id).unwrap();
    let mut fingerprint = manifest
        .capabilities()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    fingerprint.extend(manifest.artifacts().map(|artifact_id| {
        let artifact = catalog.artifact(artifact_id).unwrap();
        format!(
            "{}:{}:{:?}:{}",
            artifact.kind(),
            artifact.version(),
            artifact.state(),
            artifact.authority_generation()
        )
    }));
    fingerprint
}

#[test]
fn collected_artifact_is_not_reported_as_published() {
    let dir = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(dir.path().join("derived.sqlite")).unwrap();
    let artifact = catalog
        .stage_artifact("lexical", 1, AuthorityGeneration::new(1))
        .unwrap();
    catalog.validate_artifact(artifact.id()).unwrap();
    let old = catalog.publish_manifest(vec![artifact.id()]).unwrap().id();
    let _new = publish_empty(&mut catalog, 2);
    let _ = catalog.gc().collect().unwrap();
    assert_eq!(
        catalog.artifact(artifact.id()).unwrap().state(),
        ArtifactState::Collected
    );
    assert!(catalog.manifest(old).is_err());
}
