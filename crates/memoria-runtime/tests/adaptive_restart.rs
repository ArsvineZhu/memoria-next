use memoria_adaptive::FeedbackOutcome;
use memoria_query::MemoryQuery;
use memoria_runtime::{FeedbackSubmission, FeedbackSubmissionEvent, MemoriaRuntime};
use tempfile::tempdir;

#[test]
fn adaptive_generation_and_feedback_survive_runtime_reopen() {
    let directory = tempdir().unwrap();
    let space;
    {
        let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
        space = runtime.create_space("personal").unwrap();
        let _memory_id = runtime
            .create_memory(space, Some("career"), b"# Career\nRust")
            .unwrap();
        let response = runtime
            .query(
                MemoryQuery::builder()
                    .spaces(vec![space])
                    .text_cue("Rust")
                    .build()
                    .unwrap(),
            )
            .unwrap();
        let retrieval_id = response.retrieval_id.clone();
        let result_id = format!("{retrieval_id}:result:0");
        let commit = runtime
            .submit_feedback(FeedbackSubmission {
                retrieval_id: retrieval_id.clone(),
                idempotency_key: "feedback-1".to_owned(),
                events: vec![FeedbackSubmissionEvent {
                    result_id: result_id.clone(),
                    outcome: FeedbackOutcome::Used,
                }],
            })
            .unwrap();
        assert_eq!(commit.generation.value(), 1);
    }

    let mut reopened = MemoriaRuntime::open(directory.path()).unwrap();
    let response = reopened
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .text_cue("Rust")
                .build()
                .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        response.snapshot.adaptive,
        memoria_query::AdaptiveSnapshotIdentity::Enabled { generation, .. }
            if generation.value() == 1
    ));
}

#[test]
fn query_snapshot_records_reopened_adaptive_generation_without_unused_events() {
    let directory = tempdir().unwrap();
    let space;
    {
        let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
        space = runtime.create_space("personal").unwrap();
        runtime
            .create_memory(space, Some("career"), b"# Career\nRust")
            .unwrap();
        let response = runtime
            .query(
                MemoryQuery::builder()
                    .spaces(vec![space])
                    .text_cue("Rust")
                    .build()
                    .unwrap(),
            )
            .unwrap();
        assert!(matches!(
            response.snapshot.adaptive,
            memoria_query::AdaptiveSnapshotIdentity::Enabled { generation, .. }
                if generation.value() == 0
        ));
    }
    let mut reopened = MemoriaRuntime::open(directory.path()).unwrap();
    let response = reopened
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .text_cue("Rust")
                .build()
                .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        response.snapshot.adaptive,
        memoria_query::AdaptiveSnapshotIdentity::Enabled { generation, .. }
            if generation.value() == 0
    ));
}

#[test]
fn feedback_retry_after_process_restart_is_idempotent() {
    let directory = tempdir().unwrap();
    let (space, submission) = {
        let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
        let space = runtime.create_space("personal").unwrap();
        runtime
            .create_memory(space, Some("career"), b"# Career\nRust")
            .unwrap();
        let response = runtime
            .query(
                MemoryQuery::builder()
                    .spaces(vec![space])
                    .text_cue("Rust")
                    .build()
                    .unwrap(),
            )
            .unwrap();
        let submission = FeedbackSubmission {
            retrieval_id: response.retrieval_id.clone(),
            idempotency_key: "retryable-feedback".to_owned(),
            events: vec![FeedbackSubmissionEvent {
                result_id: format!("{}:result:0", response.retrieval_id),
                outcome: FeedbackOutcome::Used,
            }],
        };
        runtime.submit_feedback(submission.clone()).unwrap();
        (space, submission)
    };
    let mut reopened = MemoriaRuntime::open(directory.path()).unwrap();
    let commit = reopened.submit_feedback(submission).unwrap();
    assert_eq!(commit.generation.value(), 1);
    let response = reopened
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .text_cue("Rust")
                .build()
                .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        response.snapshot.adaptive,
        memoria_query::AdaptiveSnapshotIdentity::Enabled { generation, .. }
            if generation.value() == 1
    ));
}

#[test]
fn feedback_keeps_original_revision_after_head_changes() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let memory = runtime
        .create_memory(space, Some("career"), b"# Career\nRust")
        .unwrap();
    let response = runtime
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .text_cue("Rust")
                .build()
                .unwrap(),
        )
        .unwrap();
    let original_revision = response.results[0].revision_id;
    runtime
        .revise_memory(memory, original_revision, b"# Career\nGraph")
        .unwrap();
    let submission = FeedbackSubmission {
        retrieval_id: response.retrieval_id.clone(),
        idempotency_key: "original-revision".to_owned(),
        events: vec![FeedbackSubmissionEvent {
            result_id: format!("{}:result:0", response.retrieval_id),
            outcome: FeedbackOutcome::Used,
        }],
    };
    let commit = runtime.submit_feedback(submission).unwrap();
    assert_eq!(commit.events[0].revision_id, original_revision);
}
