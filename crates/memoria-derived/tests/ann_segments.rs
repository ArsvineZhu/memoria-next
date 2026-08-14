use std::collections::BTreeSet;

use memoria_derived::{
    AnnSegmentEntry, AnnSegmentV1, VectorFilter, VectorMembership, should_schedule_ann_compaction,
};
use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};
use tempfile::tempdir;

fn membership(seed: u8, generation: u64) -> VectorMembership {
    VectorMembership::from_parts(
        SpaceId::from_bytes([1; 16]),
        MemoryId::from_bytes([seed; 16]),
        RevisionId::from_bytes([seed; 32]),
        AuthorityGeneration::new(generation),
        [seed; 32],
    )
}

fn entry(target: &str, seed: u8, generation: u64, values: Vec<f32>) -> AnnSegmentEntry {
    AnnSegmentEntry::new(target, membership(seed, generation), values).unwrap()
}

#[test]
fn query_across_segments_deduplicates_same_target_to_newest_membership() {
    let older = AnnSegmentV1::build(
        "provider:model:v1",
        vec![entry("memory:one", 1, 1, vec![1.0, 0.0])],
    )
    .unwrap();
    let newer = AnnSegmentV1::build(
        "provider:model:v1",
        vec![
            entry("memory:one", 2, 2, vec![0.0, 1.0]),
            entry("memory:two", 3, 2, vec![1.0, 0.0]),
        ],
    )
    .unwrap();

    let hits = AnnSegmentV1::merge_search(
        [&newer, &older],
        &BTreeSet::new(),
        &[0.0, 1.0],
        10,
        &VectorFilter::any(),
    )
    .unwrap();

    assert_eq!(hits.len(), 2);
    assert_eq!(
        hits[0].membership().memory_id(),
        MemoryId::from_bytes([2; 16])
    );
    assert_eq!(
        hits[0].membership().authority_generation(),
        AuthorityGeneration::new(2)
    );
}

#[test]
fn tombstoned_target_is_never_returned_from_older_segment() {
    let older = AnnSegmentV1::build(
        "provider:model:v1",
        vec![entry("memory:removed", 1, 1, vec![1.0, 0.0])],
    )
    .unwrap();
    let mut tombstones = BTreeSet::new();
    tombstones.insert("memory:removed".to_owned());

    let hits =
        AnnSegmentV1::merge_search([&older], &tombstones, &[1.0, 0.0], 10, &VectorFilter::any())
            .unwrap();

    assert!(hits.is_empty());
}

#[test]
fn ninth_delta_segment_schedules_compaction() {
    assert!(!should_schedule_ann_compaction(8, 0, 100));
    assert!(should_schedule_ann_compaction(9, 0, 100));
}

#[test]
fn tombstone_ratio_above_twenty_percent_schedules_compaction() {
    assert!(!should_schedule_ann_compaction(1, 20, 80));
    assert!(should_schedule_ann_compaction(1, 21, 79));
}

#[test]
fn compaction_manifest_swap_preserves_query_results() {
    let older = AnnSegmentV1::build(
        "provider:model:v1",
        vec![
            entry("memory:one", 1, 1, vec![1.0, 0.0]),
            entry("memory:two", 2, 1, vec![0.0, 1.0]),
        ],
    )
    .unwrap();
    let newer = AnnSegmentV1::build(
        "provider:model:v1",
        vec![entry("memory:one", 3, 2, vec![0.8, 0.2])],
    )
    .unwrap();
    let tombstones = BTreeSet::new();
    let before = AnnSegmentV1::merge_search(
        [&newer, &older],
        &tombstones,
        &[1.0, 0.0],
        10,
        &VectorFilter::any(),
    )
    .unwrap();

    let compacted_entries = [&newer, &older]
        .into_iter()
        .flat_map(|segment| segment.entries())
        .filter(|entry| {
            !matches!(
                entry.target_key(),
                "memory:one" if entry.membership().authority_generation() == AuthorityGeneration::new(1)
            )
        })
        .cloned()
        .collect();
    let compacted = AnnSegmentV1::build("provider:model:v1", compacted_entries).unwrap();
    let after = AnnSegmentV1::merge_search(
        [&compacted],
        &tombstones,
        &[1.0, 0.0],
        10,
        &VectorFilter::any(),
    )
    .unwrap();

    assert_eq!(before.len(), after.len());
    assert_eq!(
        before
            .iter()
            .map(|hit| hit.membership().memory_id())
            .collect::<Vec<_>>(),
        after
            .iter()
            .map(|hit| hit.membership().memory_id())
            .collect::<Vec<_>>()
    );

    let directory = tempdir().unwrap();
    let hash = compacted.put(directory.path()).unwrap();
    let restored = AnnSegmentV1::get(directory.path(), hash).unwrap();
    assert_eq!(restored.vector_count(), compacted.vector_count());
}
