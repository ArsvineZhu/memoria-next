use memoria_query::{
    CandidateEvidence, CandidateTarget, ConsolidationCandidate, ExactEvidence, RRF_K,
    SemanticChannel, SemanticEvidence, SemanticResolution, consolidate, fuse_channels, rrf,
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

fn evidence(memory: u8) -> CandidateEvidence {
    CandidateEvidence {
        target: target(memory),
        exact: Vec::new(),
        lexical: Vec::new(),
        semantic: Vec::new(),
        tags: Vec::new(),
        propagation: Vec::new(),
        relations: Vec::new(),
        history: Vec::new(),
        text: format!("memory-{memory}"),
        entity_refs: Vec::new(),
    }
}

#[test]
fn raw_bm25_and_cosine_scores_are_not_directly_summed() {
    let mut lexical = evidence(1);
    lexical.exact.push(ExactEvidence {
        field: "lexical".to_owned(),
        value: "bm25=100".to_owned(),
    });
    let mut semantic = evidence(1);
    semantic.semantic.push(SemanticEvidence {
        score: 0.99,
        resolution: SemanticResolution::Leaf,
        channel: SemanticChannel::Direct,
    });
    let fused = fuse_channels(vec![vec![lexical], vec![semantic]], RRF_K, 10);
    assert_eq!(fused.len(), 1);
    assert_eq!(fused[0].score, rrf(0, RRF_K) + rrf(0, RRF_K));
    assert!(fused[0].score < 1.0);
}

#[test]
fn rrf_uses_k_60() {
    assert_eq!(RRF_K, 60.0);
    assert_eq!(rrf(0, RRF_K), 1.0 / 60.0);
}

#[test]
fn direct_and_residual_semantic_hits_keep_two_channel_provenance_but_one_target() {
    let mut direct = evidence(1);
    direct.semantic.push(SemanticEvidence {
        score: 0.8,
        resolution: SemanticResolution::Leaf,
        channel: SemanticChannel::Direct,
    });
    let mut residual = evidence(1);
    residual.semantic.push(SemanticEvidence {
        score: 0.7,
        resolution: SemanticResolution::Leaf,
        channel: SemanticChannel::Residual,
    });
    let fused = fuse_channels(vec![vec![direct], vec![residual]], RRF_K, 10);
    assert_eq!(fused.len(), 1);
    assert_eq!(fused[0].evidence.semantic.len(), 2);
    assert!(
        fused[0]
            .evidence
            .semantic
            .iter()
            .any(|item| item.channel == SemanticChannel::Direct)
    );
    assert!(
        fused[0]
            .evidence
            .semantic
            .iter()
            .any(|item| item.channel == SemanticChannel::Residual)
    );
}

#[test]
fn leaf_node_section_same_span_do_not_count_as_three_independent_supports() {
    let mut candidate = evidence(1);
    candidate.semantic = vec![
        SemanticEvidence {
            score: 0.9,
            resolution: SemanticResolution::Leaf,
            channel: SemanticChannel::Direct,
        },
        SemanticEvidence {
            score: 0.8,
            resolution: SemanticResolution::Node,
            channel: SemanticChannel::Direct,
        },
        SemanticEvidence {
            score: 0.7,
            resolution: SemanticResolution::Section,
            channel: SemanticChannel::Direct,
        },
    ];
    let results = consolidate(
        [ConsolidationCandidate::new(candidate, 0.9)],
        memoria_query::QueryBudget::new(10, 10, Duration::from_secs(1)),
    )
    .unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].relevance < 0.5);
}

#[test]
fn many_weak_overlapping_units_do_not_dominate_one_strong_result() {
    let mut strong = evidence(1);
    strong.exact.push(ExactEvidence {
        field: "id".to_owned(),
        value: "strong".to_owned(),
    });
    let mut candidates = vec![ConsolidationCandidate::new(strong, 1.0)];
    candidates.extend((2_u8..32).map(|memory| {
        let mut weak = evidence(memory);
        weak.semantic.push(SemanticEvidence {
            score: 0.01,
            resolution: SemanticResolution::Node,
            channel: SemanticChannel::Direct,
        });
        ConsolidationCandidate::new(weak, 0.01)
    }));
    let results = consolidate(
        candidates,
        memoria_query::QueryBudget::new(10, 50, Duration::from_secs(1)),
    )
    .unwrap();
    assert_eq!(results[0].memory_id, MemoryId::from_bytes([1; 16]));
}
