use memoria_derived::{BuildJobState, DerivedCatalog};
use memoria_types::AuthorityGeneration;
use tempfile::tempdir;

#[test]
fn queued_embedding_job_survives_runtime_reopen() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("catalog.sqlite");
    let job_id;
    {
        let mut catalog = DerivedCatalog::open(&path).unwrap();
        job_id = catalog
            .enqueue_build_job(
                "embedding",
                "input",
                "provider:model:v1",
                AuthorityGeneration::new(4),
            )
            .unwrap()
            .job_id;
    }
    let catalog = DerivedCatalog::open(&path).unwrap();
    assert_eq!(
        catalog.build_job(&job_id).unwrap().state,
        BuildJobState::Queued
    );
}

#[test]
fn running_job_is_requeued_and_not_lost_after_reopen() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("catalog.sqlite");
    let job_id;
    {
        let mut catalog = DerivedCatalog::open(&path).unwrap();
        let job = catalog
            .enqueue_build_job(
                "embedding",
                "input",
                "provider:model:v1",
                AuthorityGeneration::new(4),
            )
            .unwrap();
        job_id = job.job_id.clone();
        catalog.mark_build_job_running(&job_id).unwrap();
    }
    let catalog = DerivedCatalog::open(&path).unwrap();
    let recovered = catalog.build_job(&job_id).unwrap();
    assert_eq!(recovered.state, BuildJobState::Queued);
    assert_eq!(recovered.attempt_count, 1);
}

#[test]
fn authority_ahead_of_manifest_reconstructs_missing_build_intent() {
    let directory = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(directory.path().join("catalog.sqlite")).unwrap();
    let job = catalog
        .enqueue_build_job(
            "embedding",
            "authority-generation-9",
            "provider:model:v1",
            AuthorityGeneration::new(9),
        )
        .unwrap();
    assert_eq!(job.state, BuildJobState::Queued);
    assert!(
        catalog
            .build_job_for("embedding", "authority-generation-9", "provider:model:v1")
            .unwrap()
            .is_some()
    );
}

#[test]
fn superseded_revision_work_is_cancelled_or_marked_superseded() {
    let directory = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(directory.path().join("catalog.sqlite")).unwrap();
    let job = catalog
        .enqueue_build_job(
            "embedding",
            "old-revision",
            "provider:model:v1",
            AuthorityGeneration::new(2),
        )
        .unwrap();
    let superseded = catalog.mark_build_job_superseded(&job.job_id).unwrap();
    assert_eq!(superseded.state, BuildJobState::Superseded);
}
