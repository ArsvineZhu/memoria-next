use memoria_derived::{AuthorityGeneration, DerivedCatalog, DerivedCompiler, LexicalDocument};
use memoria_mdx::compile_ir;
use memoria_types::{MemoryId, RevisionId, SpaceId};
use tempfile::{TempDir, tempdir};

struct DerivedFixture {
    _dir: TempDir,
    catalog: DerivedCatalog,
}

impl DerivedFixture {
    fn new() -> Self {
        let dir = tempdir().unwrap();
        let catalog = DerivedCatalog::open(dir.path().join("derived.sqlite")).unwrap();
        Self { _dir: dir, catalog }
    }

    fn document(source: &str) -> LexicalDocument {
        LexicalDocument::new(
            SpaceId::from_bytes([1; 16]),
            MemoryId::from_bytes([2; 16]),
            RevisionId::from_bytes([3; 32]),
            compile_ir(source).unwrap(),
        )
    }
}

#[test]
fn base_ready_requires_zero_provider_work() {
    let mut fixture = DerivedFixture::new();
    let generation = AuthorityGeneration::new(4);
    let report = DerivedCompiler::default()
        .compile_base(
            &mut fixture.catalog,
            generation,
            [DerivedFixture::document("# Career\nRust")],
        )
        .unwrap();

    assert_eq!(report.provider_work_items, 0);
    assert_eq!(report.coverage, generation);
    assert!(report.manifest.capability("base-search").is_ready());
    assert_eq!(
        fixture.catalog.serving_manifest().unwrap().unwrap().id(),
        report.manifest.id()
    );
}

#[test]
fn failed_manifest_publication_keeps_previous_manifest_serving() {
    let mut fixture = DerivedFixture::new();
    let previous = fixture
        .catalog
        .publish_empty_manifest(AuthorityGeneration::initial())
        .unwrap();
    let artifact = fixture
        .catalog
        .stage_artifact("lexical", 1, AuthorityGeneration::new(1))
        .unwrap();
    fixture.catalog.validate_artifact(artifact.id()).unwrap();

    assert!(
        fixture
            .catalog
            .publish_manifest_at_generation(
                vec![artifact.id()],
                AuthorityGeneration::new(2),
                vec!["lexical".to_owned()],
            )
            .is_err()
    );
    assert_eq!(
        fixture.catalog.serving_manifest().unwrap().unwrap().id(),
        previous.id()
    );
}
