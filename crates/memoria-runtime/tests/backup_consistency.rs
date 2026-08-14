use std::fs;

use memoria_authority::{AuthorityDb, SourceCas, StoreLayout};
use memoria_runtime::{MemoriaRuntime, restore_store_backup};
use tempfile::tempdir;

#[test]
fn backup_contains_one_pinned_authority_generation_and_reachable_source_blob() {
    let directory = tempdir().unwrap();
    let backup_dir = directory.path().join("backup");
    let mut runtime = MemoriaRuntime::open(directory.path().join("source")).unwrap();
    let space = runtime.create_space("backup").unwrap();
    runtime
        .create_memory(space, Some("document"), b"# Durable backup\n")
        .unwrap();
    let generation = runtime.status().authority_generation;

    let manifest = runtime.create_backup(Some(&backup_dir), true).unwrap();
    assert_eq!(manifest.authority_generation, generation);
    assert_eq!(manifest.source_objects.len(), 1);
    assert!(backup_dir.join("COMPLETE").is_file());
    assert!(backup_dir.join("adaptive/adaptive.sqlite").is_file());
    let source_hash = manifest.source_objects[0];
    assert_eq!(
        fs::read(
            backup_dir
                .join("authority/objects")
                .join(source_hash.to_string())
        )
        .unwrap(),
        b"# Durable backup\n"
    );
}

#[test]
fn corrupt_backup_is_rejected_without_creating_restore_target() {
    let directory = tempdir().unwrap();
    let source_dir = directory.path().join("source");
    let backup_dir = directory.path().join("backup");
    let target_dir = directory.path().join("restored");
    let mut runtime = MemoriaRuntime::open(&source_dir).unwrap();
    let space = runtime.create_space("backup").unwrap();
    runtime
        .create_memory(space, Some("document"), b"# Durable backup\n")
        .unwrap();
    runtime.create_backup(Some(&backup_dir), true).unwrap();
    fs::write(
        backup_dir
            .join("authority/objects")
            .read_dir()
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path(),
        b"corrupt",
    )
    .unwrap();

    assert!(restore_store_backup(&backup_dir, &target_dir).is_err());
    assert!(!target_dir.exists());
}

#[test]
fn restored_store_preserves_authority_identity_and_source_bytes() {
    let directory = tempdir().unwrap();
    let source_dir = directory.path().join("source");
    let backup_dir = directory.path().join("backup");
    let target_dir = directory.path().join("restored");
    let mut runtime = MemoriaRuntime::open(&source_dir).unwrap();
    let space = runtime.create_space("backup").unwrap();
    runtime
        .create_memory(space, Some("document"), b"# Durable backup\n")
        .unwrap();
    let source_layout = StoreLayout::open(&source_dir).unwrap();
    let source_store_id = source_layout.store_id();
    let manifest = runtime.create_backup(Some(&backup_dir), true).unwrap();
    drop(runtime);

    let restored = restore_store_backup(&backup_dir, &target_dir).unwrap();
    assert_eq!(restored.store_id, source_store_id);
    assert_eq!(restored.path, target_dir);
    let restored_layout = StoreLayout::open(&target_dir).unwrap();
    assert_eq!(restored_layout.store_id(), source_store_id);
    let restored_authority = AuthorityDb::open(restored_layout.authority_database()).unwrap();
    let restored_cas = SourceCas::new(&restored_layout);
    let hashes = restored_authority
        .source_blob_hashes_at(manifest.authority_generation)
        .unwrap();
    assert_eq!(hashes.len(), 1);
    assert_eq!(restored_cas.get(hashes[0]).unwrap(), b"# Durable backup\n");
}
