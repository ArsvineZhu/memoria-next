use memoria_adaptive::{
    AdaptiveEvent, AdaptiveStateV1, FeedbackOutcome, QueryAdaptiveSignature, reduce,
};
use memoria_types::{AdaptiveGeneration, MemoryId, RevisionId, SpaceId, Timestamp};

fn fixture_event(index: u8, outcome: FeedbackOutcome) -> AdaptiveEvent {
    AdaptiveEvent {
        event_id: format!("event-{index}"),
        generation: AdaptiveGeneration::new(u64::from(index)),
        retrieval_id: "retrieval-1".to_owned(),
        space_id: SpaceId::from_bytes([1; 16]),
        memory_id: MemoryId::from_bytes([2; 16]),
        revision_id: RevisionId::from_bytes([3; 32]),
        semantic_node_id: None,
        query_signature: QueryAdaptiveSignature {
            scope: vec![SpaceId::from_bytes([1; 16])],
            entity_refs: Vec::new(),
            explicit_tags: vec!["career".to_owned()],
            query_class: Some("planning".to_owned()),
            text_projection_hash: None,
            vector_identity: None,
        },
        outcome,
        occurred_at: Timestamp::from_unix_seconds(i64::from(index)).unwrap(),
        idempotency_key: format!("key-{index}"),
    }
}

fn fixture_successes(count: u8) -> Vec<AdaptiveEvent> {
    (1..=count)
        .map(|index| fixture_event(index, FeedbackOutcome::Used))
        .collect()
}

#[test]
fn repeated_success_has_diminishing_returns() {
    let one = reduce(fixture_successes(1)).target_strength();
    let ten = reduce(fixture_successes(10)).target_strength();
    let hundred = reduce(
        (1..=100)
            .map(|index| fixture_event(index, FeedbackOutcome::Used))
            .collect::<Vec<_>>(),
    )
    .target_strength();

    assert!(ten > one);
    assert!(hundred > ten);
    assert!((hundred - ten) < (ten - one));
}

#[test]
fn negative_feedback_does_not_erase_target_familiarity() {
    let mut events = fixture_successes(3);
    events.extend((4..=7).map(|index| fixture_event(index, FeedbackOutcome::IncorrectForQuery)));
    let state = AdaptiveStateV1::replay(&events);
    let familiarity = state
        .familiarity(SpaceId::from_bytes([1; 16]), MemoryId::from_bytes([2; 16]))
        .unwrap();

    assert_eq!(familiarity.success_count, 3);
    assert!(familiarity.strength() > 0.0);
    assert!(
        state
            .query_class_affinity(
                SpaceId::from_bytes([1; 16]),
                "planning",
                MemoryId::from_bytes([2; 16]),
            )
            .unwrap()
            .score()
            < 0.0
    );
}

#[test]
fn accessibility_forgets_at_read_time_without_changing_replay_state() {
    let state = reduce(fixture_successes(1));
    let familiarity = state
        .familiarity(SpaceId::from_bytes([1; 16]), MemoryId::from_bytes([2; 16]))
        .unwrap();
    let at_feedback = familiarity.accessibility_at(Timestamp::from_unix_seconds(1).unwrap());
    let after_thirty_days =
        familiarity.accessibility_at(Timestamp::from_unix_seconds(30 * 24 * 60 * 60).unwrap());

    assert!(at_feedback > after_thirty_days);
    assert_eq!(familiarity.success_count, 1);
}
