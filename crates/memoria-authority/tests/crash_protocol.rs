use memoria_authority::{SourceCas, StoreLayout};
use memoria_types::SourceBlobHash;

struct TestStore {
    _directory: tempfile::TempDir,
    layout: StoreLayout,
}

impl TestStore {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let layout = StoreLayout::create(directory.path()).unwrap();
        Self {
            _directory: directory,
            layout,
        }
    }

    fn source_cas(&self) -> SourceCas {
        SourceCas::new(&self.layout)
    }
}

#[test]
fn identical_raw_bytes_reuse_object() {
    let fixture = TestStore::new();
    let cas = fixture.source_cas();
    let a = cas.put(b"# X\n").unwrap();
    let b = cas.put(b"# X\n").unwrap();
    assert_eq!(a, b);
    assert_eq!(cas.get(a).unwrap(), b"# X\n");
}

#[test]
fn raw_line_endings_change_source_hash() {
    let fixture = TestStore::new();
    let cas = fixture.source_cas();
    assert_ne!(cas.put(b"# X\n").unwrap(), cas.put(b"# X\r\n").unwrap());
}

#[test]
fn publication_leaves_only_the_content_addressed_object() {
    let fixture = TestStore::new();
    let cas = fixture.source_cas();
    let bytes = b"# durable source\n";
    let hash = cas.put(bytes).unwrap();

    assert!(cas.contains(hash).unwrap());
    assert_eq!(cas.get(hash).unwrap(), bytes);
    assert!(
        fixture
            .layout
            .objects_dir()
            .join(hash.to_string())
            .is_file()
    );
    assert_eq!(fixture.layout.runtime_dir().read_dir().unwrap().count(), 0);
}

#[test]
fn missing_object_is_not_contained_and_cannot_be_read() {
    let fixture = TestStore::new();
    let cas = fixture.source_cas();
    let hash = SourceBlobHash::from_bytes(b"missing");

    assert!(!cas.contains(hash).unwrap());
    assert!(cas.get(hash).is_err());
}
