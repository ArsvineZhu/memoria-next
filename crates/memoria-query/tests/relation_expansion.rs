use memoria_query::{
    CandidateEvidence, CandidateTarget, RelationExpansionBudget, RelationLink, expand_relations,
    relation_bonus,
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

#[test]
fn relation_expansion_uses_only_explicit_authority_relations() {
    let source = target(1, 1);
    let explicit_target = target(1, 2);
    let association_target = target(1, 3);
    let links = vec![
        RelationLink::new(
            source,
            explicit_target,
            "supports",
            evidence(explicit_target, "explicit"),
        ),
        RelationLink::new_association(
            source,
            association_target,
            "co-occurs",
            evidence(association_target, "association"),
        ),
    ];
    let results = expand_relations(
        vec![evidence(source, "source")],
        &links,
        &[SpaceId::from_bytes([1; 16])],
        RelationExpansionBudget {
            max_hops: 1,
            max_added: 16,
        },
    )
    .unwrap();
    assert!(
        results
            .iter()
            .any(|candidate| candidate.target == explicit_target)
    );
    assert!(
        !results
            .iter()
            .any(|candidate| candidate.target == association_target)
    );
}

#[test]
fn relation_expansion_never_crosses_unrequested_space_scope() {
    let source = target(1, 1);
    let outside = target(2, 2);
    let results = expand_relations(
        vec![evidence(source, "source")],
        &[RelationLink::new(
            source,
            outside,
            "supports",
            evidence(outside, "outside"),
        )],
        &[SpaceId::from_bytes([1; 16])],
        RelationExpansionBudget {
            max_hops: 1,
            max_added: 16,
        },
    )
    .unwrap();
    assert!(!results.iter().any(|candidate| candidate.target == outside));
}

#[test]
fn balanced_relation_depth_is_one_and_neighbor_cap_32() {
    let profile =
        memoria_query::RetrievalProfile::for_quality(memoria_query::QueryQualityLevel::Balanced);
    assert_eq!(profile.relation_budget.max_hops, 1);
    assert_eq!(profile.relation_budget.max_added, 32);
}

#[test]
fn relation_bonus_never_exceeds_0_05() {
    assert_eq!(relation_bonus(0), 0.0);
    assert!(relation_bonus(100) <= 0.05);
}

#[test]
fn association_edge_is_not_reported_as_causal_relation() {
    let source = target(1, 1);
    let target = target(1, 2);
    let results = expand_relations(
        vec![evidence(source, "source")],
        &[RelationLink::new_association(
            source,
            target,
            "causal",
            evidence(target, "association"),
        )],
        &[SpaceId::from_bytes([1; 16])],
        RelationExpansionBudget {
            max_hops: 1,
            max_added: 16,
        },
    )
    .unwrap();
    assert!(!results.iter().any(|candidate| candidate.target == target));
}
