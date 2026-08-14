use memoria_derived::{
    DerivedCatalog, EmbeddingNormalization, ProjectionInputHash, TagDictionary, TagGraph,
    TagMembershipInput, TagProvenance, VectorPayloadHash, VectorPayloadRecord,
};
use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};
use tempfile::tempdir;

use memoria_query::{
    AdaptiveSnapshotIdentity, AlgorithmChannelInputs, CandidateEvidence, CandidatePool,
    CandidateTarget, CompiledQuery, ExactEvidence, ExactIndex, ExactRecord, MemoryQuery,
    PhysicalChannel, QueryCompiler, RelationLink, SemanticChannel, SemanticEvidence,
    SemanticResidualOperator, SemanticResolution, TagSeed, TagSeedProvenance, TagVectorCandidate,
    execute_algorithm_channels,
};

fn compile_fixture(query: MemoryQuery, capabilities: &[&str]) -> CompiledQuery {
    let directory = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(directory.path().join("derived.sqlite")).unwrap();
    let generation = AuthorityGeneration::new(1);
    let space = query.scope.spaces[0];
    let mut artifact_ids = Vec::new();
    if capabilities.contains(&"semantic") {
        let artifact = catalog.stage_artifact("semantic", 1, generation).unwrap();
        catalog.validate_artifact(artifact.id()).unwrap();
        let payload_hash = VectorPayloadHash::from_bytes([0x31; 32]);
        catalog
            .register_vector_payload(&VectorPayloadRecord {
                payload_hash,
                dimension: 3,
                normalization: EmbeddingNormalization::L2,
                producer_signature: "fixture-semantic-v1".to_owned(),
                projection_input_hash: ProjectionInputHash::from_bytes([0x32; 32]),
                object_path: "objects/vector/31/payload.vec".to_owned(),
                byte_length: 99,
                checksum: [0x31; 32],
                created_at: 0,
            })
            .unwrap();
        catalog
            .insert_vector_membership(
                artifact.id(),
                space,
                MemoryId::from_bytes([8; 16]),
                RevisionId::from_bytes([9; 32]),
                "memory:fixture",
                None,
                "leaf",
                payload_hash,
                generation,
            )
            .unwrap();
        artifact_ids.push(artifact.id());
    }
    let manifest = catalog
        .publish_manifest_at_generation(
            artifact_ids,
            generation,
            capabilities.iter().map(|item| (*item).to_owned()).collect(),
        )
        .unwrap();
    QueryCompiler::new(generation, Some(manifest))
        .with_adaptive_snapshot(AdaptiveSnapshotIdentity::Disabled)
        .compile(query)
        .unwrap()
}

fn record(space: SpaceId, memory: u8, tags: &[&str]) -> ExactRecord {
    ExactRecord::new(
        space,
        MemoryId::from_bytes([memory; 16]),
        RevisionId::from_bytes([memory; 32]),
        format!("memory-{memory}"),
    )
    .with_tags(tags.iter().map(|tag| (*tag).to_owned()).collect())
}

fn evidence(target: CandidateTarget) -> CandidateEvidence {
    CandidateEvidence {
        target,
        exact: vec![ExactEvidence {
            field: "memory_id".to_owned(),
            value: target.memory_id.to_string(),
        }],
        lexical: Vec::new(),
        semantic: Vec::new(),
        tags: Vec::new(),
        propagation: Vec::new(),
        relations: Vec::new(),
        history: Vec::new(),
        text: "fixture".to_owned(),
        entity_refs: Vec::new(),
    }
}

struct ResidualFixture {
    evidence: Vec<CandidateEvidence>,
}

impl SemanticResidualOperator for ResidualFixture {
    fn search(
        &self,
        _query_vector: &[f32],
        _limit: usize,
    ) -> Result<Vec<CandidateEvidence>, memoria_query::QueryError> {
        Ok(self.evidence.clone())
    }
}

fn tag_inputs(space: SpaceId, memory: u8, with_vector: bool) -> (AlgorithmChannelInputs, TagSeed) {
    let memory_id = MemoryId::from_bytes([memory; 16]);
    let revision_id = RevisionId::from_bytes([memory; 32]);
    let mut dictionary = TagDictionary::new();
    let mut graph = TagGraph::new();
    let first = graph
        .insert_membership(
            &mut dictionary,
            TagMembershipInput::new(
                space,
                memory_id,
                revision_id,
                "rust",
                TagProvenance::Explicit,
            ),
        )
        .unwrap();
    let _second = graph
        .insert_membership(
            &mut dictionary,
            TagMembershipInput::new(
                space,
                memory_id,
                revision_id,
                "systems",
                TagProvenance::Explicit,
            ),
        )
        .unwrap();
    let tag_vectors = if with_vector {
        vec![TagVectorCandidate {
            tag_id: first,
            vector: vec![1.0, 0.0, 0.0],
            provenance: TagSeedProvenance::Explicit,
            score: 1.0,
        }]
    } else {
        Vec::new()
    };
    let seed = TagSeed {
        tag_id: first,
        value: "rust".to_owned(),
        provenance: TagSeedProvenance::Explicit,
    };
    (
        AlgorithmChannelInputs {
            tag_dictionary: dictionary,
            tag_graph: graph,
            tag_vectors,
            tag_seeds: vec![seed.clone()],
            relation_links: Vec::new(),
        },
        seed,
    )
}

#[test]
fn qualifying_query_trace_contains_tag_basis_and_residual_channel() {
    let space = SpaceId::from_bytes([1; 16]);
    let compiled = compile_fixture(
        MemoryQuery::builder()
            .spaces(vec![space])
            .text_cue("career")
            .require_capability("semantic")
            .build()
            .unwrap(),
        &["semantic"],
    );
    let target = CandidateTarget {
        space_id: space,
        memory_id: MemoryId::from_bytes([1; 16]),
        revision_id: RevisionId::from_bytes([1; 32]),
    };
    let exact = ExactIndex::new(vec![record(space, 1, &["rust"])]);
    let (inputs, _) = tag_inputs(space, 1, true);
    let residual = ResidualFixture {
        evidence: vec![CandidateEvidence {
            semantic: vec![SemanticEvidence {
                score: 0.7,
                resolution: SemanticResolution::Leaf,
                channel: SemanticChannel::Residual,
            }],
            ..evidence(target)
        }],
    };
    let mut pool = CandidatePool::new();
    let trace = execute_algorithm_channels(
        &compiled,
        &[1.0, 1.0, 0.0],
        &exact,
        &mut pool,
        &inputs,
        Some(&residual),
    )
    .unwrap();

    assert_eq!(trace.tag_basis_rank, Some(1));
    assert!(trace.channel_executed("semantic-residual"));
    assert_eq!(
        pool.keys_for_channel(PhysicalChannel::SemanticResidual)
            .len(),
        1
    );
}

#[test]
fn associative_query_trace_contains_activation() {
    let space = SpaceId::from_bytes([2; 16]);
    let compiled = compile_fixture(
        MemoryQuery::builder()
            .spaces(vec![space])
            .text_cue("career")
            .require_capability("semantic")
            .require_capability("associative")
            .build()
            .unwrap(),
        &["semantic", "associative"],
    );
    let exact = ExactIndex::new(vec![record(space, 2, &["rust", "systems"])]);
    let (inputs, _) = tag_inputs(space, 2, false);
    let mut pool = CandidatePool::new();
    let trace = execute_algorithm_channels(
        &compiled,
        &[1.0, 0.0, 0.0],
        &exact,
        &mut pool,
        &inputs,
        None,
    )
    .unwrap();

    assert!(trace.channel_executed("tag-readout"));
    assert!(trace.channel_executed("activation"));
    assert!(trace.activation_edge_visits > 0);
}

#[test]
fn thorough_associative_query_trace_contains_diffusion() {
    let space = SpaceId::from_bytes([3; 16]);
    let compiled = compile_fixture(
        MemoryQuery::builder()
            .spaces(vec![space])
            .text_cue("career")
            .require_capability("semantic")
            .require_capability("associative")
            .quality(memoria_query::QueryQualityLevel::Thorough)
            .build()
            .unwrap(),
        &["semantic", "associative"],
    );
    let exact = ExactIndex::new(vec![record(space, 3, &["rust", "systems"])]);
    let (inputs, _) = tag_inputs(space, 3, false);
    let mut pool = CandidatePool::new();
    let trace = execute_algorithm_channels(
        &compiled,
        &[1.0, 0.0, 0.0],
        &exact,
        &mut pool,
        &inputs,
        None,
    )
    .unwrap();

    assert!(trace.channel_executed("diffusion"));
    assert!(trace.diffusion_iterations > 0);
}

#[test]
fn relation_expansion_contributes_candidates_without_crossing_scope() {
    let space = SpaceId::from_bytes([4; 16]);
    let outside = SpaceId::from_bytes([5; 16]);
    let source = record(space, 4, &["rust"]);
    let target = record(space, 6, &["systems"]);
    let outside_record = record(outside, 7, &["outside"]);
    let compiled = compile_fixture(
        MemoryQuery::builder()
            .spaces(vec![space])
            .cue_tag("rust")
            .require_capability("associative")
            .build()
            .unwrap(),
        &["associative"],
    );
    let source_target = source.target;
    let exact = ExactIndex::new(vec![source.clone(), target.clone(), outside_record.clone()]);
    let (mut inputs, seed) = tag_inputs(space, 4, false);
    inputs.relation_links = vec![
        RelationLink::new(
            source_target,
            target.target,
            "supports",
            evidence(target.target),
        ),
        RelationLink::new(
            source_target,
            outside_record.target,
            "leaks",
            evidence(outside_record.target),
        ),
    ];
    let mut pool = CandidatePool::new();
    pool.insert_channel(PhysicalChannel::Exact, [evidence(source_target)]);
    let trace = execute_algorithm_channels(
        &compiled,
        &[],
        &exact,
        &mut pool,
        &AlgorithmChannelInputs {
            tag_seeds: vec![seed],
            ..inputs
        },
        None,
    )
    .unwrap();

    assert_eq!(trace.relation_expansions, 1);
    assert_eq!(pool.keys_for_channel(PhysicalChannel::Relation).len(), 1);
    assert_eq!(
        pool.keys_for_channel(PhysicalChannel::Relation)[0].space_id,
        space
    );
}

#[test]
fn direct_semantic_remains_when_tag_basis_is_used() {
    let space = SpaceId::from_bytes([6; 16]);
    let compiled = compile_fixture(
        MemoryQuery::builder()
            .spaces(vec![space])
            .text_cue("career")
            .require_capability("semantic")
            .build()
            .unwrap(),
        &["semantic"],
    );
    let exact = ExactIndex::new(vec![record(space, 8, &["rust"])]);
    let target = CandidateTarget {
        space_id: space,
        memory_id: MemoryId::from_bytes([8; 16]),
        revision_id: RevisionId::from_bytes([8; 32]),
    };
    let (inputs, _) = tag_inputs(space, 8, true);
    let mut pool = CandidatePool::new();
    pool.insert_channel(PhysicalChannel::SemanticDirect, [evidence(target)]);
    let trace = execute_algorithm_channels(
        &compiled,
        &[1.0, 0.0, 0.0],
        &exact,
        &mut pool,
        &inputs,
        None,
    )
    .unwrap();

    assert_eq!(
        pool.keys_for_channel(PhysicalChannel::SemanticDirect).len(),
        1
    );
    assert_eq!(trace.tag_basis_rank, Some(1));
}
