use memoria_derived::{AuthorityGeneration, DerivedCatalog, DerivedError};
use tempfile::{TempDir, tempdir};

struct TestCatalog {
    _dir: TempDir,
    catalog: DerivedCatalog,
}

impl TestCatalog {
    fn new() -> Self {
        let dir = tempdir().unwrap();
        let catalog = DerivedCatalog::open(dir.path().join("derived.sqlite")).unwrap();
        Self { _dir: dir, catalog }
    }
}

#[test]
fn unvalidated_artifact_cannot_be_published() {
    let mut fixture = TestCatalog::new();
    let artifact = fixture
        .catalog
        .stage_artifact("lexical", 1, AuthorityGeneration::new(4))
        .unwrap();
    assert!(matches!(
        fixture.catalog.publish_manifest(vec![artifact.id()]),
        Err(DerivedError::ArtifactNotValidated { .. })
    ));
}

#[test]
fn published_manifest_is_immutable() {
    let mut fixture = TestCatalog::new();
    let manifest = fixture
        .catalog
        .publish_empty_manifest(AuthorityGeneration::new(0))
        .unwrap();
    assert!(matches!(
        fixture
            .catalog
            .replace_manifest_artifacts(manifest.id(), Vec::new()),
        Err(DerivedError::ManifestImmutable { .. })
    ));
}

#[test]
fn validated_artifacts_publish_through_the_serving_pointer() {
    let mut fixture = TestCatalog::new();
    let artifact = fixture
        .catalog
        .stage_artifact("structural", 1, AuthorityGeneration::new(7))
        .unwrap();
    fixture.catalog.validate_artifact(artifact.id()).unwrap();
    let manifest = fixture
        .catalog
        .publish_manifest(vec![artifact.id()])
        .unwrap();
    let serving = fixture.catalog.serving_manifest().unwrap().unwrap();
    assert_eq!(serving.id(), manifest.id());
    assert_eq!(serving.authority_generation(), AuthorityGeneration::new(7));
    assert_eq!(serving.artifacts().collect::<Vec<_>>(), vec![artifact.id()]);
}
