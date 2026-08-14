use memoria_adaptive::{AdaptiveEvent, AdaptiveStateV1, FeedbackOutcome, QueryAdaptiveSignature};
use memoria_query::{
    CandidateEvidence, CandidateTarget, MemoryMatch, MemoryResult, rank_with_adaptive,
};
use memoria_types::{AdaptiveGeneration, MemoryId, RevisionId, SpaceId, Timestamp};

fn space() -> SpaceId {
    SpaceId::from_bytes([1; 16])
}

fn signature() -> QueryAdaptiveSignature {
    QueryAdaptiveSignature {
        scope: vec![space()],
        entity_refs: Vec::new(),
        explicit_tags: vec!["career".to_owned()],
        query_class: Some("planning".to_owned()),
        text_projection_hash: None,
        vector_identity: None,
    }
}

fn familiar_state(memory_id: MemoryId) -> AdaptiveStateV1 {
    AdaptiveStateV1::replay(&[AdaptiveEvent {
        event_id: "event-1".to_owned(),
        generation: AdaptiveGeneration::new(1),
        retrieval_id: "retrieval-1".to_owned(),
        space_id: space(),
        memory_id,
        revision_id: RevisionId::from_bytes([memory_id.as_bytes()[0]; 32]),
        semantic_node_id: None,
        query_signature: signature(),
        outcome: FeedbackOutcome::Used,
        occurred_at: Timestamp::from_unix_seconds(1).unwrap(),
        idempotency_key: "key-1".to_owned(),
    }])
}

fn result(memory: u8, relevance: f32, accessibility: f32) -> MemoryResult {
    let target = CandidateTarget {
        space_id: space(),
        memory_id: MemoryId::from_bytes([memory; 16]),
        revision_id: RevisionId::from_bytes([memory; 32]),
    };
    MemoryResult {
        space_id: target.space_id,
        memory_id: target.memory_id,
        revision_id: target.revision_id,
        matches: vec![MemoryMatch {
            evidence: CandidateEvidence {
                target,
                exact: Vec::new(),
                lexical: Vec::new(),
                semantic: Vec::new(),
                tags: Vec::new(),
                propagation: Vec::new(),
                relations: Vec::new(),
                history: Vec::new(),
                text: "text".to_owned(),
                entity_refs: Vec::new(),
            },
            score: relevance,
        }],
        relevance,
        confidence: 1.0,
        accessibility,
        effort: 1.0,
    }
}

#[test]
fn high_familiarity_cannot_make_irrelevant_candidate_win() {
    let familiar = MemoryId::from_bytes([2; 16]);
    let ranked = rank_with_adaptive(
        vec![result(1, 0.9, 0.0), result(2, 0.1, 0.0)],
        &familiar_state(familiar),
        &signature(),
        Timestamp::from_unix_seconds(2).unwrap(),
    );

    assert_eq!(ranked[0].memory_id, MemoryId::from_bytes([1; 16]));
    assert!(ranked[1].accessibility > 0.0);
}

#[test]
fn exact_lookup_survives_zero_accessibility() {
    let result = rank_with_adaptive(
        vec![result(3, 1.0, 0.0)],
        &AdaptiveStateV1::default(),
        &signature(),
        Timestamp::from_unix_seconds(2).unwrap(),
    );

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].memory_id, MemoryId::from_bytes([3; 16]));
}

#[test]
fn assessment_keeps_accessibility_separate_from_confidence() {
    let result = result(4, 0.8, 0.0);
    let assessment = memoria_query::assess(&[result]);

    assert_eq!(assessment.mean_confidence, 1.0);
    assert_eq!(assessment.mean_accessibility, 0.0);
}
