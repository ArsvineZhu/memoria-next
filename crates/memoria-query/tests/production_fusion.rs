use memoria_query::{
    CandidateEvidence, CandidateKey, CandidatePool, CandidateTarget, ExactEvidence,
    LexicalEvidence, PhysicalChannel, PropagationEvidence, RRF_K, RelationEvidence,
    SemanticChannel, SemanticEvidence, SemanticResolution, fuse_candidate_pool, rrf,
};
use memoria_types::{MemoryId, RevisionId, SpaceId};

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
fn rrf_k_is_sixty_for_production_channels() {
    assert_eq!(RRF_K, 60.0);
    assert_eq!(rrf(0, RRF_K), 1.0 / 60.0);
}

#[test]
fn lexical_and_semantic_scores_are_not_raw_summed() {
    let mut pool = CandidatePool::new();
    let mut lexical = evidence(1);
    lexical.lexical.push(LexicalEvidence {
        rank: 0,
        score: 100.0,
    });
    let mut semantic = evidence(1);
    semantic.semantic.push(SemanticEvidence {
        score: 0.01,
        resolution: SemanticResolution::Leaf,
        channel: SemanticChannel::Direct,
    });
    pool.insert_channel(PhysicalChannel::Lexical, [lexical]);
    pool.insert_channel(PhysicalChannel::SemanticDirect, [semantic]);

    let fused = fuse_candidate_pool(&pool, 10, &[target(1).space_id]);
    assert_eq!(fused.len(), 1);
    assert_eq!(fused[0].score, rrf(0, RRF_K) + rrf(0, RRF_K));
    assert!(fused[0].score < 1.0);
}

#[test]
fn parent_child_resolution_hits_do_not_count_as_independent_support() {
    let mut pool = CandidatePool::new();
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
    pool.insert_channel(PhysicalChannel::SemanticDirect, [candidate]);

    let fused = fuse_candidate_pool(&pool, 10, &[target(1).space_id]);
    assert_eq!(fused[0].independent_support_count, 1);
    assert_eq!(fused[0].correlation_suppressed_evidence, 2);
}

#[test]
fn support_structure_relation_bonuses_respect_caps() {
    let mut pool = CandidatePool::new();
    let mut candidate = evidence(1);
    candidate.exact.push(ExactEvidence {
        field: "fixture".to_owned(),
        value: "one".to_owned(),
    });
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
    candidate.propagation = (0..32)
        .map(|_| PropagationEvidence { score: 1.0 })
        .collect();
    candidate.relations = (0..32)
        .map(|index| RelationEvidence {
            relation: format!("relation-{index}"),
        })
        .collect();
    pool.insert_channel(PhysicalChannel::Exact, [candidate]);

    let fused = fuse_candidate_pool(&pool, 10, &[target(1).space_id]);
    let base = rrf(0, RRF_K);
    assert!(fused[0].score <= base + 0.12 + 0.08 + 0.05 + 1.0e-6);
}

#[test]
fn many_overlapping_units_do_not_dominate_one_strong_memory() {
    let mut pool = CandidatePool::new();
    let strong = target(1);
    pool.insert(
        PhysicalChannel::SemanticDirect,
        CandidateKey::new(
            strong.space_id,
            strong.memory_id,
            strong.revision_id,
            Some("strong".to_owned()),
        ),
        evidence(1),
    );
    for index in 0..32 {
        pool.insert(
            PhysicalChannel::SemanticDirect,
            CandidateKey::new(
                strong.space_id,
                strong.memory_id,
                strong.revision_id,
                Some(format!("overlap-{index}")),
            ),
            evidence(1),
        );
    }
    pool.insert_channel(PhysicalChannel::SemanticDirect, [evidence(2)]);

    let fused = fuse_candidate_pool(&pool, 10, &[strong.space_id]);
    assert_eq!(fused.len(), 2);
    assert_eq!(fused[0].evidence.target.memory_id, strong.memory_id);
    assert_eq!(fused[0].score, rrf(0, RRF_K));
    assert_eq!(fused[1].score, rrf(1, RRF_K));
}
