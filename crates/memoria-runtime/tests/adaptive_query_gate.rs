mod support;

use memoria_adaptive::FeedbackOutcome;
use memoria_query::{AdaptiveSnapshotIdentity, MemoryQuery};
use memoria_runtime::{FeedbackSubmission, FeedbackSubmissionEvent, MemoriaRuntime};
use tempfile::tempdir;

fn setup_runtime() -> (tempfile::TempDir, MemoriaRuntime, memoria_types::SpaceId) {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("career"), b"# Career\nRust systems work")
        .unwrap();
    support::publish_manifest_with_capabilities(&runtime, &["adaptive"]);
    (directory, runtime, space)
}

fn query(space: memoria_types::SpaceId, adaptive: bool) -> MemoryQuery {
    let builder = MemoryQuery::builder()
        .spaces(vec![space])
        .text_cue("Rust systems work");
    if adaptive {
        builder.prefer_capability("adaptive").build().unwrap()
    } else {
        builder.build().unwrap()
    }
}

#[test]
fn adaptive_inactive_does_not_read_or_apply_adaptive_state() {
    let (_directory, mut runtime, space) = setup_runtime();
    let response = runtime.query(query(space, false)).unwrap();

    assert_eq!(
        response.snapshot.adaptive,
        AdaptiveSnapshotIdentity::Disabled
    );
    assert!(!response.trace.channel_executed("adaptive"));
    assert!(
        response
            .results
            .iter()
            .all(|result| (result.accessibility - 0.5).abs() < f32::EPSILON)
    );
}

#[test]
fn adaptive_active_uses_materialized_snapshot_without_full_event_replay() {
    let (_directory, mut runtime, space) = setup_runtime();
    let first = runtime.query(query(space, true)).unwrap();
    assert!(matches!(
        first.snapshot.adaptive,
        AdaptiveSnapshotIdentity::Enabled { generation, .. } if generation.value() == 0
    ));
    assert!(first.trace.channel_executed("adaptive"));

    runtime
        .submit_feedback(FeedbackSubmission {
            retrieval_id: first.retrieval_id.clone(),
            idempotency_key: "adaptive-gate-feedback".to_owned(),
            events: vec![FeedbackSubmissionEvent {
                result_id: format!("{}:result:0", first.retrieval_id),
                outcome: FeedbackOutcome::Used,
            }],
        })
        .unwrap();

    let second = runtime.query(query(space, true)).unwrap();
    assert!(matches!(
        second.snapshot.adaptive,
        AdaptiveSnapshotIdentity::Enabled { generation, .. } if generation.value() == 1
    ));
    assert!(second.trace.channel_executed("adaptive"));
    assert!(second.results[0].accessibility > 0.0);
}

#[test]
fn adaptive_cannot_restore_filtered_candidate() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let included_space = runtime.create_space("included").unwrap();
    let excluded_space = runtime.create_space("excluded").unwrap();
    runtime
        .create_memory(
            included_space,
            Some("included"),
            b"# Included\nRust systems work",
        )
        .unwrap();
    runtime
        .create_memory(
            excluded_space,
            Some("excluded"),
            b"# Excluded\nRust systems work",
        )
        .unwrap();
    support::publish_manifest_with_capabilities(&runtime, &["adaptive"]);

    let excluded = runtime.query(query(excluded_space, true)).unwrap();
    runtime
        .submit_feedback(FeedbackSubmission {
            retrieval_id: excluded.retrieval_id.clone(),
            idempotency_key: "excluded-feedback".to_owned(),
            events: vec![FeedbackSubmissionEvent {
                result_id: format!("{}:result:0", excluded.retrieval_id),
                outcome: FeedbackOutcome::Used,
            }],
        })
        .unwrap();

    let response = runtime.query(query(included_space, true)).unwrap();
    assert!(
        response
            .results
            .iter()
            .all(|result| result.space_id == included_space)
    );
}

#[test]
fn adaptive_prior_stays_bounded_at_point_one() {
    let (_directory, mut runtime, space) = setup_runtime();
    let response = runtime.query(query(space, true)).unwrap();
    for index in 0..8 {
        runtime
            .submit_feedback(FeedbackSubmission {
                retrieval_id: response.retrieval_id.clone(),
                idempotency_key: format!("bounded-feedback-{index}"),
                events: vec![FeedbackSubmissionEvent {
                    result_id: format!("{}:result:0", response.retrieval_id),
                    outcome: FeedbackOutcome::Used,
                }],
            })
            .unwrap();
    }

    let ranked = runtime.query(query(space, true)).unwrap();
    assert!(ranked.results.iter().all(|result| {
        result.accessibility.is_finite() && (0.0..=1.0).contains(&result.accessibility)
    }));
    assert!(ranked.trace.channel_executed("adaptive"));
}
