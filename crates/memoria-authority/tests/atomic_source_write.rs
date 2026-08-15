use std::io::Write;

use atomic_write_file::AtomicWriteFile;
use memoria_authority::{SourceCas, StoreLayout};
use memoria_types::SourceBlobHash;
use tempfile::tempdir;

#[test]
fn put_get_preserves_raw_source_bytes() {
    let directory = tempdir().unwrap();
    let layout = StoreLayout::create(directory.path()).unwrap();
    let cas = SourceCas::new(&layout);
    let source = [0_u8, 0xff, b'\n', b'\r', 0x80];

    let hash = cas.put(&source).unwrap();

    assert_eq!(cas.get(hash).unwrap(), source);
}

#[test]
fn same_hash_is_idempotent_after_existing_object_verification() {
    let directory = tempdir().unwrap();
    let layout = StoreLayout::create(directory.path()).unwrap();
    let cas = SourceCas::new(&layout);
    let source = b"same source bytes";

    let first = cas.put(source).unwrap();
    let second = cas.put(source).unwrap();

    assert_eq!(first, second);
    assert_eq!(cas.get(first).unwrap(), source);
}

#[test]
fn corrupt_existing_object_is_detected_and_not_silently_reused() {
    let directory = tempdir().unwrap();
    let layout = StoreLayout::create(directory.path()).unwrap();
    let source = b"expected source";
    let hash = SourceBlobHash::from_bytes(source);
    std::fs::write(layout.objects_dir().join(hash.to_string()), b"tampered").unwrap();
    let cas = SourceCas::new(&layout);

    assert!(matches!(
        cas.put(source),
        Err(memoria_types::MemoriaError::Corruption { .. })
    ));
}

#[test]
fn interrupted_uncommitted_write_never_replaces_existing_object() {
    let directory = tempdir().unwrap();
    let layout = StoreLayout::create(directory.path()).unwrap();
    let cas = SourceCas::new(&layout);
    let original = b"durable original";
    let hash = cas.put(original).unwrap();
    let object_path = layout.objects_dir().join(hash.to_string());

    {
        let mut pending = AtomicWriteFile::open(&object_path).unwrap();
        pending.write_all(b"uncommitted replacement").unwrap();
    }

    assert_eq!(cas.get(hash).unwrap(), original);
}
