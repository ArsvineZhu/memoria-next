use memoria_adaptive::{AdaptiveEvent, AdaptiveStateV1, FeedbackOutcome, QueryAdaptiveSignature};
use memoria_types::{AdaptiveGeneration, MemoryId, RevisionId, SpaceId, Timestamp};

fn event(space_id: SpaceId, generation: u64) -> AdaptiveEvent {
    AdaptiveEvent {
        event_id: format!("event-{generation}"),
        generation: AdaptiveGeneration::new(generation),
        retrieval_id: "retrieval-1".to_owned(),
        space_id,
        memory_id: MemoryId::from_bytes([7; 16]),
        revision_id: RevisionId::from_bytes([8; 32]),
        semantic_node_id: None,
        query_signature: QueryAdaptiveSignature {
            scope: vec![space_id],
            entity_refs: Vec::new(),
            explicit_tags: Vec::new(),
            query_class: None,
            text_projection_hash: None,
            vector_identity: None,
        },
        outcome: FeedbackOutcome::Used,
        occurred_at: Timestamp::from_unix_seconds(generation as i64).unwrap(),
        idempotency_key: format!("key-{generation}"),
    }
}

#[test]
fn move_does_not_migrate_old_space_familiarity() {
    let space_a = SpaceId::from_bytes([1; 16]);
    let space_b = SpaceId::from_bytes([2; 16]);
    let memory = MemoryId::from_bytes([7; 16]);
    let state = AdaptiveStateV1::replay(&[event(space_a, 1)]);

    assert!(state.familiarity(space_a, memory).is_some());
    assert!(state.familiarity(space_b, memory).is_none());
}

#[test]
fn same_memory_in_two_spaces_keeps_independent_counts() {
    let space_a = SpaceId::from_bytes([1; 16]);
    let space_b = SpaceId::from_bytes([2; 16]);
    let memory = MemoryId::from_bytes([7; 16]);
    let state = AdaptiveStateV1::replay(&[event(space_a, 1), event(space_b, 2)]);

    assert_eq!(state.familiarity(space_a, memory).unwrap().success_count, 1);
    assert_eq!(state.familiarity(space_b, memory).unwrap().success_count, 1);
}
