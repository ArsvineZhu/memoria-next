use memoria_adaptive::{
    AdaptiveEventLog, FeedbackEventInput, FeedbackOutcome, QueryAdaptiveSignature,
};
use memoria_types::{MemoryId, RevisionId, SpaceId, Timestamp};
use rusqlite::Connection;

#[test]
fn adaptive_feedback_batch_is_atomic_and_reader_friendly_under_wal() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("adaptive.sqlite");
    let mut log = AdaptiveEventLog::open(&database).unwrap();
    let reader = Connection::open(&database).unwrap();
    let journal_mode: String = reader
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
    reader
        .execute_batch("BEGIN; SELECT COUNT(*) FROM adaptive_events;")
        .unwrap();

    let space_id = SpaceId::from_bytes([1; 16]);
    let memory_id = MemoryId::from_bytes([2; 16]);
    let revision_id = RevisionId::from_bytes([3; 32]);
    let input = FeedbackEventInput {
        event_id: "event-concurrency".to_owned(),
        retrieval_id: "retrieval-concurrency".to_owned(),
        space_id,
        memory_id,
        revision_id,
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
        occurred_at: Timestamp::from_unix_seconds(1).unwrap(),
        idempotency_key: "idem-concurrency".to_owned(),
    };
    assert!(log.append_batch([input]).is_ok());
    reader.execute_batch("COMMIT").unwrap();
    assert_eq!(log.events().len(), 1);
}
