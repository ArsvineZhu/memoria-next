use memoria_query::{ExactIndex, ExactRecord, MemoryReference, ReferenceStatus};
use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};

fn memory() -> MemoryId {
    MemoryId::from_bytes([1; 16])
}

fn revision(value: u8) -> RevisionId {
    RevisionId::from_bytes([value; 32])
}

fn record(revision_id: RevisionId, generation: u64) -> ExactRecord {
    ExactRecord::new(SpaceId::from_bytes([2; 16]), memory(), revision_id, "text")
        .with_authority_generation(AuthorityGeneration::new(generation))
        .with_node_ids(vec!["head-node".to_owned()])
}

#[test]
fn logical_memory_ref_resolves_head_at_query_snapshot() {
    let index = ExactIndex::new(vec![
        record(revision(1), 1).with_current(false),
        record(revision(2), 2),
    ]);
    let resolved = index.resolve(
        &MemoryReference::logical(memory()),
        AuthorityGeneration::new(2),
    );
    assert_eq!(resolved.target.unwrap().revision_id, revision(2));
    assert_eq!(resolved.status, ReferenceStatus::Resolved);
}

#[test]
fn missing_current_node_is_unresolved_not_historical_guess() {
    let index = ExactIndex::new(vec![
        record(revision(1), 1)
            .with_current(false)
            .with_node_ids(vec!["old-node".to_owned()]),
        record(revision(2), 2),
    ]);
    let resolved = index.resolve(
        &MemoryReference::logical(memory()).node("old-node"),
        AuthorityGeneration::new(2),
    );
    assert_eq!(resolved.status, ReferenceStatus::Unresolved);
    assert!(resolved.target.is_none());
}

#[test]
fn retired_memory_is_excluded_from_current_but_pinned_history_remains_addressable() {
    let retired = record(revision(1), 1)
        .with_retired(true)
        .with_current(false);
    let index = ExactIndex::new(vec![retired]);
    let logical = index.resolve(
        &MemoryReference::logical(memory()),
        AuthorityGeneration::new(1),
    );
    assert_eq!(logical.status, ReferenceStatus::Unresolved);
    let pinned = index.resolve(
        &MemoryReference::revision(memory(), revision(1)),
        AuthorityGeneration::new(1),
    );
    assert_eq!(pinned.status, ReferenceStatus::Retired);
    assert_eq!(pinned.target.unwrap().revision_id, revision(1));
}
