use memoria_authority::{StoreLayout, StoreWriterLock};
use memoria_types::MemoriaError;

#[test]
fn store_layout_creates_declared_roots() {
    let dir = tempfile::tempdir().unwrap();
    let layout = StoreLayout::create(dir.path()).unwrap();
    assert!(layout.authority_dir().is_dir());
    assert!(layout.objects_dir().is_dir());
    assert!(layout.derived_dir().is_dir());
    assert!(layout.adaptive_dir().is_dir());
    assert!(layout.cache_dir().is_dir());
    assert!(layout.runtime_dir().is_dir());
}

#[test]
fn store_layout_opens_existing_store() {
    let dir = tempfile::tempdir().unwrap();
    StoreLayout::create(dir.path()).unwrap();
    let writer = StoreWriterLock::acquire(dir.path()).unwrap();

    StoreLayout::open(dir.path()).unwrap();
    drop(writer);
}

#[test]
fn second_writer_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    StoreLayout::create(dir.path()).unwrap();
    let first = StoreWriterLock::acquire(dir.path()).unwrap();
    let second = StoreWriterLock::acquire(dir.path());
    assert!(
        matches!(&second, Err(MemoriaError::StoreLocked { .. })),
        "second writer result: {second:?}"
    );
    drop(first);
    StoreWriterLock::acquire(dir.path()).unwrap();
}
