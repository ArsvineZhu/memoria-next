use memoria_derived::DerivedCatalog;
use memoria_query::{ExactIndex, ExactRecord, MemoryQuery, QueryCompiler, execute_history};
use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};
use tempfile::tempdir;

fn compiler() -> QueryCompiler {
    let dir = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(dir.path().join("derived.sqlite")).unwrap();
    let manifest = catalog
        .publish_empty_manifest(AuthorityGeneration::new(2))
        .unwrap();
    QueryCompiler::new(AuthorityGeneration::new(2), Some(manifest))
}

fn query(text: &str) -> MemoryQuery {
    MemoryQuery::builder()
        .spaces(vec![SpaceId::from_bytes([1; 16])])
        .text_cue(text)
        .build()
        .unwrap()
}

#[test]
fn history_can_return_two_revisions_of_same_memory() {
    let memory = MemoryId::from_bytes([1; 16]);
    let old = ExactRecord::new(
        SpaceId::from_bytes([1; 16]),
        memory,
        RevisionId::from_bytes([1; 32]),
        "career transition",
    )
    .with_current(false)
    .with_authority_generation(AuthorityGeneration::new(1));
    let head = ExactRecord::new(
        SpaceId::from_bytes([1; 16]),
        memory,
        RevisionId::from_bytes([2; 32]),
        "career transition",
    )
    .with_authority_generation(AuthorityGeneration::new(2));
    let compiled = compiler().compile(query("career")).unwrap();
    let response = execute_history(&compiled, &ExactIndex::new(vec![old, head]));
    let revisions = response
        .results
        .iter()
        .map(|result| result.target.revision_id)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(revisions.len(), 2);
}

#[test]
fn current_query_does_not_search_removed_old_phrase() {
    let record = memoria_query::LexicalCandidate {
        target: memoria_query::CandidateTarget {
            space_id: SpaceId::from_bytes([1; 16]),
            memory_id: MemoryId::from_bytes([1; 16]),
            revision_id: RevisionId::from_bytes([2; 32]),
        },
        score: 1.0,
        text: "new phrase".to_owned(),
        entity_refs: Vec::new(),
        tags: Vec::new(),
        current: true,
        retired: false,
    };
    let compiled = compiler().compile(query("old unique phrase")).unwrap();
    let response = memoria_query::execute_lexical(
        &compiled,
        &memoria_query::LexicalCandidateIndex::new(vec![record]),
    );
    assert!(response.results.is_empty());
}
