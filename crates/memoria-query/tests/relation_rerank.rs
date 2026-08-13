use memoria_query::{
    CandidateEvidence, CandidateTarget, MemoryMatch, MemoryResult, RelationExpansionBudget,
    RelationLink, RerankScore, apply_rerank, build_rerank_batch, expand_relations,
};
use memoria_types::{MemoryId, RevisionId, SpaceId};

fn target(space: u8, memory: u8) -> CandidateTarget {
    CandidateTarget {
        space_id: SpaceId::from_bytes([space; 16]),
        memory_id: MemoryId::from_bytes([memory; 16]),
        revision_id: RevisionId::from_bytes([memory; 32]),
    }
}

fn evidence(target: CandidateTarget, text: &str) -> CandidateEvidence {
    CandidateEvidence {
        target,
        exact: Vec::new(),
        lexical: Vec::new(),
        semantic: Vec::new(),
        tags: Vec::new(),
        propagation: Vec::new(),
        relations: Vec::new(),
        history: Vec::new(),
        text: text.to_owned(),
        entity_refs: Vec::new(),
    }
}

fn result(target: CandidateTarget, relevance: f32, text: &str) -> MemoryResult {
    MemoryResult {
        space_id: target.space_id,
        memory_id: target.memory_id,
        revision_id: target.revision_id,
        matches: vec![MemoryMatch {
            evidence: evidence(target, text),
            score: relevance,
        }],
        relevance,
        confidence: 0.5,
        accessibility: 0.5,
        effort: 0.5,
    }
}

#[test]
fn relation_expansion_keeps_targets_inside_scope() {
    let source = target(1, 1);
    let in_scope = target(1, 2);
    let outside = target(2, 3);
    let expanded = expand_relations(
        vec![evidence(source, "source")],
        &[
            RelationLink::new(source, in_scope, "supports", evidence(in_scope, "in scope")),
            RelationLink::new(source, outside, "leaks", evidence(outside, "outside")),
        ],
        &[source.space_id],
        RelationExpansionBudget {
            max_hops: 2,
            max_added: 8,
        },
    )
    .unwrap();

    assert!(
        expanded
            .iter()
            .all(|item| item.target.space_id == source.space_id)
    );
    assert!(expanded.iter().any(|item| item.target == in_scope));
    assert!(!expanded.iter().any(|item| item.target == outside));
    assert!(
        expanded
            .iter()
            .find(|item| item.target == in_scope)
            .is_some_and(|item| item.relations.iter().any(|r| r.relation == "supports"))
    );
}

#[test]
fn rerank_batch_and_application_are_scope_safe() {
    let in_scope = result(target(1, 1), 0.2, "in scope");
    let outside = result(target(2, 2), 0.9, "outside");
    let batch = build_rerank_batch(
        "career",
        &[in_scope.clone(), outside.clone()],
        &[in_scope.space_id],
        8,
    )
    .unwrap();
    assert_eq!(batch.views.len(), 1);
    let reranked = apply_rerank(
        vec![in_scope, outside],
        &batch,
        &[RerankScore {
            handle: batch.views[0].handle.clone(),
            score: 1.0,
        }],
        &[SpaceId::from_bytes([1; 16])],
    )
    .unwrap();

    assert_eq!(reranked.len(), 1);
    assert_eq!(reranked[0].space_id, SpaceId::from_bytes([1; 16]));
}
