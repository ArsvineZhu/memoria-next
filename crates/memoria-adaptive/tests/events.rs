use memoria_adaptive::{
    AdaptiveError, AdaptiveEventLog, FeedbackEventInput, FeedbackOutcome, QueryAdaptiveSignature,
};
use memoria_types::{AdaptiveGeneration, MemoryId, RevisionId, SpaceId, Timestamp};

fn fixture_feedback_batch() -> Vec<FeedbackEventInput> {
    let space_id = SpaceId::from_bytes([1; 16]);
    let signature = QueryAdaptiveSignature {
        scope: vec![space_id],
        entity_refs: vec!["person:alex".to_owned()],
        explicit_tags: vec!["career".to_owned()],
        query_class: Some("planning".to_owned()),
        text_projection_hash: Some("sha256:query".to_owned()),
        vector_identity: None,
    };

    (0..3)
        .map(|index| FeedbackEventInput {
            event_id: format!("event-{index}"),
            retrieval_id: "retrieval-1".to_owned(),
            space_id,
            memory_id: MemoryId::from_bytes([index as u8 + 2; 16]),
            revision_id: RevisionId::from_bytes([index as u8 + 3; 32]),
            semantic_node_id: None,
            query_signature: signature.clone(),
            outcome: if index == 0 {
                FeedbackOutcome::Used
            } else {
                FeedbackOutcome::CorrectForQuery
            },
            occurred_at: Timestamp::from_unix_seconds(100 + index).unwrap(),
            idempotency_key: format!("feedback-{index}"),
        })
        .collect()
}

#[test]
fn unused_is_not_a_feedback_outcome() {
    assert!("unused".parse::<FeedbackOutcome>().is_err());
}

#[test]
fn feedback_batch_advances_generation_once() {
    let mut log = AdaptiveEventLog::new();
    let batch = fixture_feedback_batch();

    let committed = log.append_batch(batch.clone()).unwrap();

    assert_eq!(committed.generation, AdaptiveGeneration::new(1));
    assert_eq!(committed.events.len(), 3);
    assert!(
        committed
            .events
            .iter()
            .all(|event| event.generation == AdaptiveGeneration::new(1))
    );
    assert_eq!(log.current_generation(), AdaptiveGeneration::new(1));

    let replayed = log.append_batch(batch).unwrap();
    assert_eq!(replayed.generation, AdaptiveGeneration::new(1));
    assert_eq!(replayed.events.len(), 3);
    assert_eq!(log.events().len(), 3);
}

#[test]
fn conflicting_idempotency_key_is_rejected() {
    let mut log = AdaptiveEventLog::new();
    let mut batch = fixture_feedback_batch();
    log.append_batch(batch.clone()).unwrap();
    batch[0].outcome = FeedbackOutcome::Rejected;

    assert!(matches!(
        log.append_batch(batch),
        Err(AdaptiveError::IdempotencyConflict { .. })
    ));
    assert_eq!(log.events().len(), 3);
}
