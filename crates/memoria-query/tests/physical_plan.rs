use memoria_derived::{
    DerivedCatalog, EmbeddingNormalization, ProjectionInputHash, VectorPayloadHash,
    VectorPayloadRecord,
};
use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};
use tempfile::tempdir;

use memoria_query::{
    AdaptiveSnapshotIdentity, CapabilityExecution, CompiledQuery, MemoryQuery, PhysicalChannel,
    PhysicalQueryPlanner, QueryCompiler, QueryQualityLevel,
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

#[test]
fn text_plus_semantic_plans_lexical_and_semantic_direct() {
    let compiled = compile_fixture(
        MemoryQuery::builder()
            .spaces(vec![SpaceId::from_bytes([1; 16])])
            .text_cue("career")
            .prefer_capability("semantic")
            .build()
            .unwrap(),
        &["semantic"],
    );

    let plan = PhysicalQueryPlanner::plan(&compiled);
    assert!(plan.channels.contains(&PhysicalChannel::Lexical));
    assert!(plan.channels.contains(&PhysicalChannel::SemanticDirect));
}

#[test]
fn thorough_associative_plan_includes_diffusion() {
    let compiled = compile_fixture(
        MemoryQuery::builder()
            .spaces(vec![SpaceId::from_bytes([2; 16])])
            .text_cue("career")
            .prefer_capability("semantic")
            .prefer_capability("associative")
            .quality(QueryQualityLevel::Thorough)
            .build()
            .unwrap(),
        &["semantic", "associative"],
    );

    let plan = PhysicalQueryPlanner::plan(&compiled);
    assert!(plan.channels.contains(&PhysicalChannel::Activation));
    assert!(plan.channels.contains(&PhysicalChannel::Diffusion));
}

#[test]
fn preferred_missing_capability_is_not_planned() {
    let compiled = compile_fixture(
        MemoryQuery::builder()
            .spaces(vec![SpaceId::from_bytes([3; 16])])
            .text_cue("career")
            .prefer_capability("semantic")
            .build()
            .unwrap(),
        &[],
    );

    assert_eq!(
        compiled.execution,
        CapabilityExecution {
            degraded: true,
            used_capabilities: Vec::new(),
            degraded_capabilities: vec!["semantic".to_owned()],
        }
    );
    let plan = PhysicalQueryPlanner::plan(&compiled);
    assert!(plan.channels.contains(&PhysicalChannel::Lexical));
    assert!(!plan.channels.contains(&PhysicalChannel::SemanticDirect));
}
