use memoria_authority::{
    AuthorityDb, MemoryRecord, SourceCas, SpaceRecord, StoreLayout, StoreWriterLock,
};
use memoria_types::{
    AuthorityGeneration, MemoriaError, MemoryId, RevisionId, SourceBlobHash, SpaceId,
};

struct TestAuthority {
    _directory: tempfile::TempDir,
    _layout: StoreLayout,
    db: AuthorityDb,
    cas: SourceCas,
}

impl TestAuthority {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let layout = StoreLayout::create(directory.path()).unwrap();
        let db = AuthorityDb::open(layout.authority_database()).unwrap();
        let cas = SourceCas::new(&layout);
        Self {
            _directory: directory,
            _layout: layout,
            db,
            cas,
        }
    }

    fn create_space(&self, space_key: &str) -> Result<SpaceRecord, MemoriaError> {
        self.db
            .create_space(space_key)
            .map(|result| result.into_value())
    }

    fn create_memory(
        &self,
        space_id: SpaceId,
        document_key: Option<&str>,
        source: &[u8],
    ) -> Result<MemoryRecord, MemoriaError> {
        self.db
            .create_memory(&self.cas, space_id, document_key, source)
            .map(|result| result.into_value())
    }

    fn revise_memory(
        &self,
        memory_id: MemoryId,
        expected_head: RevisionId,
        source: &[u8],
    ) -> Result<MemoryRecord, MemoriaError> {
        self.db
            .revise_memory(&self.cas, memory_id, expected_head, source)
            .map(|result| result.into_value())
    }

    fn get_memory(&self, memory_id: MemoryId) -> Result<MemoryRecord, MemoriaError> {
        self.db.get_memory(memory_id)
    }

    fn get_memory_at(
        &self,
        memory_id: MemoryId,
        generation: AuthorityGeneration,
    ) -> Result<MemoryRecord, MemoriaError> {
        self.db.get_memory_at(memory_id, generation)
    }

    fn read_memory(
        &self,
        memory_id: MemoryId,
    ) -> Result<memoria_authority::MemoryRead, MemoriaError> {
        self.db.read_memory(&self.cas, memory_id)
    }

    fn read_memory_at(
        &self,
        memory_id: MemoryId,
        generation: AuthorityGeneration,
    ) -> Result<memoria_authority::MemoryRead, MemoriaError> {
        self.db.read_memory_at(&self.cas, memory_id, generation)
    }
}

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

#[test]
fn wrong_expected_head_never_overwrites() {
    let store = TestAuthority::new();
    let space = store.create_space("personal").unwrap();
    let created = store
        .create_memory(space.id(), Some("career"), b"# Career\n")
        .unwrap();

    let err = store
        .revise_memory(
            created.memory_id(),
            RevisionId::from_bytes([7; 32]),
            b"# Career\nchanged\n",
        )
        .unwrap_err();

    assert_eq!(err.code(), "HEAD_CONFLICT");
    assert!(matches!(err, MemoriaError::HeadConflict { .. }));
    assert_eq!(
        store
            .get_memory(created.memory_id())
            .unwrap()
            .head_revision_id,
        created.revision_id()
    );
    assert!(
        store
            .cas
            .contains(SourceBlobHash::from_bytes(b"# Career\nchanged\n"))
            .unwrap(),
        "CAS publication precedes the transactional HEAD check"
    );
}

#[test]
fn revise_preserves_historical_head_and_reads_current_source() {
    let store = TestAuthority::new();
    let space = store.create_space("personal").unwrap();
    let created = store
        .create_memory(space.id(), Some("career"), b"# Career\n")
        .unwrap();

    let revised = store
        .revise_memory(
            created.memory_id(),
            created.revision_id(),
            b"# Career\nchanged\n",
        )
        .unwrap();

    assert_eq!(revised.memory_id(), created.memory_id());
    assert_ne!(revised.revision_id(), created.revision_id());
    assert_eq!(revised.generation(), AuthorityGeneration::new(3));
    assert_eq!(store.get_memory(created.memory_id()).unwrap(), revised);
    assert_eq!(
        store
            .get_memory_at(created.memory_id(), created.generation())
            .unwrap()
            .head_revision_id,
        created.revision_id()
    );
    assert_eq!(
        store
            .read_memory(created.memory_id())
            .unwrap()
            .source_bytes(),
        b"# Career\nchanged\n"
    );
    assert_eq!(
        store
            .read_memory_at(created.memory_id(), created.generation())
            .unwrap()
            .source_bytes(),
        b"# Career\n"
    );
    let revision = store.db.get_revision(revised.revision_id()).unwrap();
    assert_eq!(revision.parent_revision_ids(), &[created.revision_id()]);
}

#[test]
fn duplicate_document_key_is_rejected_without_advancing_generation() {
    let store = TestAuthority::new();
    let space = store.create_space("personal").unwrap();
    store
        .create_memory(space.id(), Some("career"), b"# Career\n")
        .unwrap();
    let error = store
        .create_memory(space.id(), Some("career"), b"# Career\nother\n")
        .unwrap_err();

    assert!(matches!(
        error,
        MemoriaError::KeyConflict {
            scope: "document",
            ..
        }
    ));
    assert_eq!(
        store.db.current_generation().unwrap(),
        AuthorityGeneration::new(2)
    );
}
