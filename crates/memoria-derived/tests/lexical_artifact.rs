use memoria_derived::{
    AuthorityGeneration, DerivedCatalog, DerivedCompiler, LexicalArtifactHandle, LexicalDocument,
};
use memoria_mdx::compile_ir;
use memoria_types::{MemoryId, RevisionId, SpaceId};
use tempfile::{TempDir, tempdir};

struct Fixture {
    dir: TempDir,
    catalog: DerivedCatalog,
    space: SpaceId,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempdir().unwrap();
        let catalog = DerivedCatalog::open(dir.path().join("derived/catalog.sqlite")).unwrap();
        Self {
            dir,
            catalog,
            space: SpaceId::from_bytes([1; 16]),
        }
    }

    fn document(&self, memory: u8, source: &str) -> LexicalDocument {
        LexicalDocument::new(
            self.space,
            MemoryId::from_bytes([memory; 16]),
            RevisionId::from_bytes([memory; 32]),
            compile_ir(source).unwrap(),
        )
    }

    fn artifact_path(&self, object_path: &str) -> std::path::PathBuf {
        self.dir.path().join("derived").join(object_path)
    }
}

#[test]
fn lexical_artifact_survives_catalog_reopen() {
    let mut fixture = Fixture::new();
    let document = fixture.document(2, "# Career\nRust systems");
    let report = DerivedCompiler::default()
        .compile_base(
            &mut fixture.catalog,
            AuthorityGeneration::new(1),
            [document],
        )
        .unwrap();
    let record = fixture
        .catalog
        .lexical_artifact_for_manifest(report.manifest().id())
        .unwrap();
    let handle = LexicalArtifactHandle::open(fixture.artifact_path(&record.object_path)).unwrap();
    let first = handle.search("career", &[fixture.space], 10).unwrap();
    assert_eq!(first.len(), 1);

    let root = fixture.dir.path().to_path_buf();
    let space = fixture.space;
    drop(fixture.catalog);
    let catalog = DerivedCatalog::open(root.join("derived/catalog.sqlite")).unwrap();
    let reopened_record = catalog
        .lexical_artifact_for_manifest(report.manifest().id())
        .unwrap();
    let reopened =
        LexicalArtifactHandle::open(root.join("derived").join(&reopened_record.object_path))
            .unwrap();
    assert_eq!(reopened.search("career", &[space], 10).unwrap(), first);
}

#[test]
fn lexical_search_does_not_rebuild_index_on_each_query() {
    let mut fixture = Fixture::new();
    let document = fixture.document(2, "# Career\nRust systems");
    let report = DerivedCompiler::default()
        .compile_base(
            &mut fixture.catalog,
            AuthorityGeneration::new(1),
            [document],
        )
        .unwrap();
    let record = fixture
        .catalog
        .lexical_artifact_for_manifest(report.manifest().id())
        .unwrap();
    let handle = LexicalArtifactHandle::open(fixture.artifact_path(&record.object_path)).unwrap();

    let first = handle.search("career", &[fixture.space], 10).unwrap();
    let second = handle.search("systems", &[fixture.space], 10).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_eq!(handle.search_count(), 2);
}

#[test]
fn manifest_pins_one_immutable_lexical_artifact() {
    let mut fixture = Fixture::new();
    let compiler = DerivedCompiler::default();
    let first_document = fixture.document(2, "# Career\nRust systems");
    let first = compiler
        .compile_base(
            &mut fixture.catalog,
            AuthorityGeneration::new(1),
            [first_document],
        )
        .unwrap();
    let first_record = fixture
        .catalog
        .lexical_artifact_for_manifest(first.manifest().id())
        .unwrap();

    let second_document = fixture.document(2, "# Finance\nBudget systems");
    let second = compiler
        .compile_base(
            &mut fixture.catalog,
            AuthorityGeneration::new(2),
            [second_document],
        )
        .unwrap();
    let second_record = fixture
        .catalog
        .lexical_artifact_for_manifest(second.manifest().id())
        .unwrap();

    assert_ne!(first.manifest().id(), second.manifest().id());
    assert_ne!(first_record.object_path, second_record.object_path);
    assert_eq!(
        fixture
            .catalog
            .lexical_artifact_for_manifest(first.manifest().id())
            .unwrap(),
        first_record
    );
}

#[test]
fn old_manifest_can_still_open_old_lexical_artifact_while_leased() {
    let mut fixture = Fixture::new();
    let compiler = DerivedCompiler::default();
    let first_document = fixture.document(2, "# Career\nRust systems");
    let first = compiler
        .compile_base(
            &mut fixture.catalog,
            AuthorityGeneration::new(1),
            [first_document],
        )
        .unwrap();
    let first_record = fixture
        .catalog
        .lexical_artifact_for_manifest(first.manifest().id())
        .unwrap();
    let _lease = fixture
        .catalog
        .acquire_lease(first.manifest().id(), std::time::Duration::from_secs(60))
        .unwrap();
    let old_handle =
        LexicalArtifactHandle::open(fixture.artifact_path(&first_record.object_path)).unwrap();

    let second_document = fixture.document(3, "# Finance\nBudget systems");
    compiler
        .compile_base(
            &mut fixture.catalog,
            AuthorityGeneration::new(2),
            [second_document],
        )
        .unwrap();

    assert_eq!(
        old_handle
            .search("career", &[fixture.space], 10)
            .unwrap()
            .len(),
        1
    );
}
