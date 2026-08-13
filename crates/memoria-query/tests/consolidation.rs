use memoria_query::{
    CandidateEvidence, CandidateTarget, ConsolidationCandidate, ExactEvidence, QueryBudget,
    consolidate,
};
use memoria_types::{MemoryId, RevisionId, SpaceId};
use std::time::Duration;

fn target(memory: u8) -> CandidateTarget {
    CandidateTarget {
        space_id: SpaceId::from_bytes([1; 16]),
        memory_id: MemoryId::from_bytes([memory; 16]),
        revision_id: RevisionId::from_bytes([memory; 32]),
    }
}

fn evidence(memory: u8, field: &str) -> CandidateEvidence {
    CandidateEvidence {
        target: target(memory),
        exact: vec![ExactEvidence {
            field: field.to_owned(),
            value: "value".to_owned(),
        }],
        lexical: Vec::new(),
        semantic: Vec::new(),
        tags: Vec::new(),
        propagation: Vec::new(),
        relations: Vec::new(),
        history: Vec::new(),
        text: "text".to_owned(),
        entity_refs: Vec::new(),
    }
}

#[test]
fn overlapping_leaf_and_node_hits_become_one_match() {
    let results = consolidate(
        vec![
            ConsolidationCandidate::new(evidence(1, "leaf"), 0.8),
            ConsolidationCandidate::new(evidence(1, "node"), 0.6),
        ],
        QueryBudget::new(10, 3, Duration::from_millis(1_500)),
    )
    .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].matches.len(), 1);
    assert_eq!(results[0].matches[0].evidence.exact.len(), 2);
}

#[test]
fn many_weak_matches_do_not_linearly_dominate_one_strong_result() {
    let mut candidates = vec![ConsolidationCandidate::new(evidence(1, "strong"), 0.9)];
    candidates.extend(
        (2_u8..22).map(|memory| ConsolidationCandidate::new(evidence(memory, "weak"), 0.1)),
    );
    let results = consolidate(
        candidates,
        QueryBudget::new(10, 30, Duration::from_millis(1_500)),
    )
    .unwrap();
    assert_eq!(results[0].memory_id, MemoryId::from_bytes([1; 16]));
}

#[test]
fn response_metrics_are_deterministic() {
    let results = consolidate(
        vec![evidence(1, "exact")],
        QueryBudget::new(10, 3, Duration::from_millis(1_500)),
    )
    .unwrap();
    let assessment = memoria_query::assess(&results);
    assert_eq!(assessment.result_count, 1);
    assert!(assessment.top_relevance.is_some());
    assert!((0.0..=1.0).contains(&assessment.channel_coverage));
}
