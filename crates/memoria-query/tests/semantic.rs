use memoria_derived::{
    DerivedCatalog, EmbeddingNormalization, ProjectionInputHash, VectorPayloadHash,
    VectorPayloadRecord,
};
use memoria_query::{
    ExactRecord, MemoryQuery, QueryCompiler, SemanticCandidate, SemanticCandidateIndex,
    SemanticResolution, execute_semantic,
};
use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};
use tempfile::tempdir;

fn target(value: u8) -> memoria_query::CandidateTarget {
    memoria_query::CandidateTarget {
        space_id: SpaceId::from_bytes([1; 16]),
        memory_id: MemoryId::from_bytes([value; 16]),
        revision_id: RevisionId::from_bytes([value; 32]),
    }
}

fn compiler() -> QueryCompiler {
    let dir = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(dir.path().join("derived.sqlite")).unwrap();
    let artifact = catalog
        .stage_artifact("semantic", 1, AuthorityGeneration::new(1))
        .unwrap();
    catalog.validate_artifact(artifact.id()).unwrap();
    let payload_hash = VectorPayloadHash::from_bytes([0x11; 32]);
    catalog
        .register_vector_payload(&VectorPayloadRecord {
            payload_hash,
            dimension: 3,
            normalization: EmbeddingNormalization::L2,
            producer_signature: "test:semantic:v1".to_owned(),
            projection_input_hash: ProjectionInputHash::from_bytes([0x22; 32]),
            object_path: "objects/vector/11/test.vec".to_owned(),
            byte_length: 99,
            checksum: [0x11; 32],
            created_at: 0,
        })
        .unwrap();
    catalog
        .insert_vector_membership(
            artifact.id(),
            SpaceId::from_bytes([1; 16]),
            MemoryId::from_bytes([1; 16]),
            RevisionId::from_bytes([1; 32]),
            "memory:1",
            None,
            "leaf",
            payload_hash,
            AuthorityGeneration::new(1),
        )
        .unwrap();
    let manifest = catalog
        .publish_manifest_at_generation(
            vec![artifact.id()],
            AuthorityGeneration::new(1),
            Vec::new(),
        )
        .unwrap();
    QueryCompiler::new(AuthorityGeneration::new(1), Some(manifest))
}

fn candidate(value: u8, score: f32, resolution: SemanticResolution) -> SemanticCandidate {
    let record = ExactRecord::new(
        target(value).space_id,
        target(value).memory_id,
        target(value).revision_id,
        format!("memory-{value}"),
    );
    SemanticCandidate::from_record(&record, score, resolution)
}

#[test]
fn semantic_candidates_keep_resolution_provenance_and_suppress_correlated_duplicates() {
    let query = MemoryQuery::builder()
        .spaces(vec![SpaceId::from_bytes([1; 16])])
        .text_cue("career")
        .require_capability("semantic")
        .build()
        .unwrap();
    let compiled = compiler().compile(query).unwrap();
    let response = execute_semantic(
        &compiled,
        &SemanticCandidateIndex::new(vec![
            candidate(1, 0.80, SemanticResolution::Leaf),
            candidate(1, 0.70, SemanticResolution::Node),
            candidate(1, 0.60, SemanticResolution::Section),
            candidate(1, 0.10, SemanticResolution::Node),
            candidate(2, 0.90, SemanticResolution::Section),
        ]),
    );
    assert_eq!(response.results.len(), 2);
    assert_eq!(
        response.results[0].target.memory_id,
        MemoryId::from_bytes([2; 16])
    );
    assert_eq!(response.results[1].semantic.len(), 3);
    assert!(
        response.results[1]
            .semantic
            .iter()
            .any(|evidence| evidence.resolution == SemanticResolution::Leaf)
    );
    assert!(
        response.results[1]
            .semantic
            .iter()
            .any(|evidence| evidence.resolution == SemanticResolution::Node)
    );
}

#[test]
fn semantic_without_manifest_artifact_degrades_preferred_queries() {
    let dir = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(dir.path().join("derived.sqlite")).unwrap();
    let manifest = catalog
        .publish_empty_manifest(AuthorityGeneration::new(1))
        .unwrap();
    let query = MemoryQuery::builder()
        .spaces(vec![SpaceId::from_bytes([1; 16])])
        .text_cue("career")
        .prefer_capability("semantic")
        .build()
        .unwrap();
    let compiled = QueryCompiler::new(AuthorityGeneration::new(1), Some(manifest))
        .compile(query)
        .unwrap();
    assert!(compiled.execution.degraded);
    assert!(!compiled.execution.used("semantic"));
}

#[test]
fn semantic_constraints_remain_hard() {
    let query = MemoryQuery::builder()
        .spaces(vec![SpaceId::from_bytes([1; 16])])
        .text_cue("career")
        .require_memory(MemoryId::from_bytes([1; 16]))
        .require_capability("semantic")
        .build()
        .unwrap();
    let compiled = compiler().compile(query).unwrap();
    let response = execute_semantic(
        &compiled,
        &SemanticCandidateIndex::new(vec![
            candidate(1, 0.8, SemanticResolution::Leaf),
            candidate(2, 0.99, SemanticResolution::Leaf),
        ]),
    );
    assert_eq!(response.results.len(), 1);
    assert_eq!(
        response.results[0].target.memory_id,
        MemoryId::from_bytes([1; 16])
    );
}
