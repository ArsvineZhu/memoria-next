mod support;

use memoria_derived::{
    EmbeddingNormalization, EmbeddingSignature, EmbeddingVector, GeneratedTagArtifact,
    ProjectionKind, TagDictionary, TagProvenance,
};
use memoria_types::SourceBlobHash;
use support::enrichment_document;

#[test]
fn generated_tags_do_not_change_source_or_semantic_hash() {
    let source = "# Career\nRust systems work";
    let document = enrichment_document(source);
    let source_hash = SourceBlobHash::from_bytes(source.as_bytes());
    let semantic_hash = document.ir().semantic_hash().clone();
    let projection =
        memoria_derived::EnrichmentProjection::from_document(&document, "test-enrichment-v1")
            .unwrap();
    let mut dictionary = TagDictionary::new();

    let artifact = GeneratedTagArtifact::from_candidates(
        projection.clone(),
        &mut dictionary,
        ["systems".to_owned(), "career".to_owned()],
    )
    .unwrap();

    assert_eq!(SourceBlobHash::from_bytes(source.as_bytes()), source_hash);
    assert_eq!(document.ir().semantic_hash(), &semantic_hash);
    assert_eq!(artifact.projection_input_hash(), projection.input_hash());
    assert!(
        artifact
            .records()
            .iter()
            .all(|record| record.provenance == TagProvenance::Generated)
    );
}

#[test]
fn generated_tags_do_not_invalidate_content_embedding_payload() {
    let document = enrichment_document("# Career\nRust systems work");
    let signature = EmbeddingSignature::new(
        "test-provider",
        "test-model",
        "content",
        3,
        EmbeddingNormalization::L2,
        ProjectionKind::LocalEmbedding.version(),
    )
    .unwrap();
    let embedding = EmbeddingVector::new(signature, vec![1.0, 0.0, 0.0]).unwrap();
    let before = embedding.payload_hash();
    let projection =
        memoria_derived::EnrichmentProjection::from_document(&document, "test-enrichment-v1")
            .unwrap();
    let mut dictionary = TagDictionary::new();

    let _artifact =
        GeneratedTagArtifact::from_candidates(projection, &mut dictionary, ["career".to_owned()])
            .unwrap();

    assert_eq!(embedding.payload_hash(), before);
}
