use memoria_adaptive::{AdaptiveEvent, AdaptiveStateV1, FeedbackOutcome, QueryAdaptiveSignature};
use memoria_derived::DerivedCatalog;
use memoria_query::{
    CandidateEvidence, CandidateTarget, ExactIndex, ExactRecord, MemoryQuery, QueryCompiler,
    execute_exact, fuse_channels, fuse_channels_scoped, fuse_channels_with_adaptive, rrf,
};
use memoria_types::{
    AdaptiveGeneration, AuthorityGeneration, MemoryId, RevisionId, SpaceId, Timestamp,
};
use tempfile::tempdir;

fn compiler() -> QueryCompiler {
    let dir = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(dir.path().join("derived.sqlite")).unwrap();
    let manifest = catalog
        .publish_empty_manifest(AuthorityGeneration::new(1))
        .unwrap();
    QueryCompiler::new(AuthorityGeneration::new(1), Some(manifest))
}

#[test]
fn rrf_fuses_channels_without_mixing_raw_score_scales() {
    let query = MemoryQuery::builder()
        .spaces(vec![SpaceId::from_bytes([1; 16])])
        .build()
        .unwrap();
    let compiled = compiler().compile(query).unwrap();
    let a = ExactRecord::new(
        SpaceId::from_bytes([1; 16]),
        MemoryId::from_bytes([1; 16]),
        RevisionId::from_bytes([1; 32]),
        "A",
    );
    let b = ExactRecord::new(
        SpaceId::from_bytes([1; 16]),
        MemoryId::from_bytes([2; 16]),
        RevisionId::from_bytes([2; 32]),
        "B",
    );
    let exact = execute_exact(&compiled, &ExactIndex::new(vec![a, b]));
    let fused = fuse_channels(
        vec![
            exact.results.clone(),
            exact.results.iter().rev().cloned().collect(),
        ],
        60.0,
        10,
    );
    assert_eq!(fused.len(), 2);
    assert_eq!(fused[0].score, rrf(0, 60.0) + rrf(1, 60.0));
    assert_eq!(fused[0].evidence.exact.len(), 2);
}

#[test]
fn scoped_fusion_applies_limit_after_scope_filtering() {
    let outside = CandidateEvidence {
        target: CandidateTarget {
            space_id: SpaceId::from_bytes([2; 16]),
            memory_id: MemoryId::from_bytes([1; 16]),
            revision_id: RevisionId::from_bytes([1; 32]),
        },
        exact: Vec::new(),
        lexical: Vec::new(),
        semantic: Vec::new(),
        tags: Vec::new(),
        propagation: Vec::new(),
        relations: Vec::new(),
        history: Vec::new(),
        text: "outside".to_owned(),
        entity_refs: Vec::new(),
    };
    let inside = CandidateEvidence {
        target: CandidateTarget {
            space_id: SpaceId::from_bytes([1; 16]),
            memory_id: MemoryId::from_bytes([2; 16]),
            revision_id: RevisionId::from_bytes([2; 32]),
        },
        exact: Vec::new(),
        lexical: Vec::new(),
        semantic: Vec::new(),
        tags: Vec::new(),
        propagation: Vec::new(),
        relations: Vec::new(),
        history: Vec::new(),
        text: "inside".to_owned(),
        entity_refs: Vec::new(),
    };

    let scoped = fuse_channels_scoped(
        vec![vec![outside, inside]],
        60.0,
        1,
        &[SpaceId::from_bytes([1; 16])],
    );

    assert_eq!(scoped.len(), 1);
    assert_eq!(
        scoped[0].evidence.target.space_id,
        SpaceId::from_bytes([1; 16])
    );
}

#[test]
fn adaptive_fusion_only_breaks_equal_base_score_within_scope() {
    let space = SpaceId::from_bytes([1; 16]);
    let a = CandidateEvidence {
        target: CandidateTarget {
            space_id: space,
            memory_id: MemoryId::from_bytes([1; 16]),
            revision_id: RevisionId::from_bytes([1; 32]),
        },
        exact: Vec::new(),
        lexical: Vec::new(),
        semantic: Vec::new(),
        tags: Vec::new(),
        propagation: Vec::new(),
        relations: Vec::new(),
        history: Vec::new(),
        text: "a".to_owned(),
        entity_refs: Vec::new(),
    };
    let b = CandidateEvidence {
        target: CandidateTarget {
            space_id: space,
            memory_id: MemoryId::from_bytes([2; 16]),
            revision_id: RevisionId::from_bytes([2; 32]),
        },
        exact: Vec::new(),
        lexical: Vec::new(),
        semantic: Vec::new(),
        tags: Vec::new(),
        propagation: Vec::new(),
        relations: Vec::new(),
        history: Vec::new(),
        text: "b".to_owned(),
        entity_refs: Vec::new(),
    };
    let state = AdaptiveStateV1::replay(&[AdaptiveEvent {
        event_id: "event-b".to_owned(),
        generation: AdaptiveGeneration::new(1),
        retrieval_id: "retrieval".to_owned(),
        space_id: space,
        memory_id: MemoryId::from_bytes([2; 16]),
        revision_id: RevisionId::from_bytes([2; 32]),
        semantic_node_id: None,
        query_signature: QueryAdaptiveSignature {
            scope: vec![space],
            entity_refs: Vec::new(),
            explicit_tags: vec!["career".to_owned()],
            query_class: None,
            text_projection_hash: None,
            vector_identity: None,
        },
        outcome: FeedbackOutcome::Used,
        occurred_at: Timestamp::from_unix_seconds(1).unwrap(),
        idempotency_key: "key-b".to_owned(),
    }]);
    let signature = QueryAdaptiveSignature {
        scope: vec![space],
        entity_refs: Vec::new(),
        explicit_tags: vec!["career".to_owned()],
        query_class: None,
        text_projection_hash: None,
        vector_identity: None,
    };

    let fused = fuse_channels_with_adaptive(
        vec![vec![a.clone(), b.clone()], vec![b, a]],
        60.0,
        2,
        &[space],
        &state,
        &signature,
        Timestamp::from_unix_seconds(2).unwrap(),
    );

    assert_eq!(
        fused[0].evidence.target.memory_id,
        MemoryId::from_bytes([2; 16])
    );
}
