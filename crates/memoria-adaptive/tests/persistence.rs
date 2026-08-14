use memoria_adaptive::{
    AdaptiveEventLog, AdaptiveStateV1, FeedbackEventInput, FeedbackOutcome, QueryAdaptiveSignature,
    checkpoint_after,
};
use memoria_types::{MemoryId, RevisionId, SpaceId, Timestamp};
use tempfile::tempdir;

fn input(index: u8, space: u8, memory: u8) -> FeedbackEventInput {
    let space_id = SpaceId::from_bytes([space; 16]);
    FeedbackEventInput {
        event_id: format!("event-{index}"),
        retrieval_id: "retrieval".to_owned(),
        space_id,
        memory_id: MemoryId::from_bytes([memory; 16]),
        revision_id: RevisionId::from_bytes([index; 32]),
        semantic_node_id: None,
        query_signature: QueryAdaptiveSignature {
            scope: vec![space_id],
            entity_refs: Vec::new(),
            explicit_tags: vec!["career".to_owned()],
            query_class: Some("current".to_owned()),
            text_projection_hash: None,
            vector_identity: None,
        },
        outcome: FeedbackOutcome::Used,
        occurred_at: Timestamp::from_unix_seconds(i64::from(index) + 1).unwrap(),
        idempotency_key: format!("key-{index}"),
    }
}

#[test]
fn feedback_event_survives_adaptive_reopen() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("adaptive.sqlite");
    let mut log = AdaptiveEventLog::open(&path).unwrap();
    log.append_batch([input(1, 1, 2)]).unwrap();
    let state_before = log.state().clone();
    drop(log);

    let reopened = AdaptiveEventLog::open(&path).unwrap();
    assert_eq!(reopened.events().len(), 1);
    assert_eq!(reopened.state(), &state_before);
}

#[test]
fn adaptive_generation_never_resets_on_reopen() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("adaptive.sqlite");
    let mut log = AdaptiveEventLog::open(&path).unwrap();
    log.append_batch([input(1, 1, 2)]).unwrap();
    let generation = log.current_generation();
    drop(log);
    assert_eq!(
        AdaptiveEventLog::open(&path).unwrap().current_generation(),
        generation
    );
}

#[test]
fn materialized_state_plus_tail_equals_full_event_replay() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("adaptive.sqlite");
    let mut log = AdaptiveEventLog::open(&path).unwrap();
    log.append_batch([
        input(1, 1, 2),
        input(2, 1, 3),
        input(3, 2, 4),
        input(4, 2, 5),
    ])
    .unwrap();
    let events = log.events().to_vec();
    let (checkpoint, tail) = checkpoint_after(&events, 2);
    assert_eq!(
        AdaptiveStateV1::from_checkpoint_and_tail(checkpoint, tail),
        *log.state()
    );
}

#[test]
fn space_at_event_is_preserved_after_memory_move() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("adaptive.sqlite");
    let mut log = AdaptiveEventLog::open(&path).unwrap();
    log.append_batch([input(1, 1, 2)]).unwrap();
    log.append_batch([input(2, 2, 2)]).unwrap();
    assert!(
        log.state()
            .familiarity(SpaceId::from_bytes([1; 16]), MemoryId::from_bytes([2; 16]))
            .is_some()
    );
    assert!(
        log.state()
            .familiarity(SpaceId::from_bytes([2; 16]), MemoryId::from_bytes([2; 16]))
            .is_some()
    );
    assert_eq!(log.events()[0].space_id, SpaceId::from_bytes([1; 16]));
}
