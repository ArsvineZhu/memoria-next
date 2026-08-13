use memoria_derived::DerivedCatalog;
use memoria_query::{
    EntityRef, ExactIndex, ExactRecord, LexicalCandidate, LexicalCandidateIndex, MemoryQuery,
    QueryCompiler, execute_exact, execute_lexical,
};
use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};
use tempfile::tempdir;

fn space(value: u8) -> SpaceId {
    SpaceId::from_bytes([value; 16])
}

fn memory(value: u8) -> MemoryId {
    MemoryId::from_bytes([value; 16])
}

fn revision(value: u8) -> RevisionId {
    RevisionId::from_bytes([value; 32])
}

fn compiler() -> QueryCompiler {
    let dir = tempdir().unwrap();
    let mut catalog = DerivedCatalog::open(dir.path().join("derived.sqlite")).unwrap();
    let manifest = catalog
        .publish_empty_manifest(AuthorityGeneration::new(1))
        .unwrap();
    QueryCompiler::new(AuthorityGeneration::new(1), Some(manifest))
}

#[test]
fn exact_memory_lookup_needs_no_semantic_capability() {
    let target = ExactRecord::new(space(1), memory(1), revision(1), "career");
    let query = MemoryQuery::builder()
        .spaces(vec![space(1)])
        .require_memory(memory(1))
        .build()
        .unwrap();
    let compiled = compiler().compile(query).unwrap();
    let response = execute_exact(&compiled, &ExactIndex::new(vec![target]));
    assert_eq!(response.results.len(), 1);
    assert!(!response.execution.used("semantic"));
}

#[test]
fn relation_does_not_expand_outside_scope() {
    let person_a = EntityRef::new("person:A").unwrap();
    let in_scope = ExactRecord::new(space(1), memory(1), revision(1), "Rust")
        .with_entities(vec![person_a.clone()])
        .with_relations(vec!["refers-to:space-2".to_owned()]);
    let outside =
        ExactRecord::new(space(2), memory(2), revision(2), "Rust").with_entities(vec![person_a]);
    let query = MemoryQuery::builder()
        .spaces(vec![space(1)])
        .text_cue("Rust")
        .require_entity(EntityRef::new("person:A").unwrap())
        .build()
        .unwrap();
    let response = execute_exact(
        &compiler().compile(query).unwrap(),
        &ExactIndex::new(vec![in_scope, outside]),
    );
    assert!(
        response
            .results
            .iter()
            .all(|item| item.target.space_id == space(1))
    );
}

#[test]
fn lexical_hits_still_obey_entity_constraint() {
    let entity_a = EntityRef::new("person:A").unwrap();
    let entity_b = EntityRef::new("person:B").unwrap();
    let query = MemoryQuery::builder()
        .spaces(vec![space(1)])
        .text_cue("Rust")
        .require_entity(entity_a.clone())
        .build()
        .unwrap();
    let compiled = compiler().compile(query).unwrap();
    let index = LexicalCandidateIndex::new(vec![
        LexicalCandidate {
            target: memoria_query::CandidateTarget {
                space_id: space(1),
                memory_id: memory(1),
                revision_id: revision(1),
            },
            score: 2.0,
            text: "Rust A".to_owned(),
            entity_refs: vec![entity_a.clone()],
            tags: Vec::new(),
            current: true,
            retired: false,
        },
        LexicalCandidate {
            target: memoria_query::CandidateTarget {
                space_id: space(1),
                memory_id: memory(2),
                revision_id: revision(2),
            },
            score: 3.0,
            text: "Rust B".to_owned(),
            entity_refs: vec![entity_b],
            tags: Vec::new(),
            current: true,
            retired: false,
        },
    ]);
    let response = execute_lexical(&compiled, &index);
    assert_eq!(response.results.len(), 1);
    assert!(response.results[0].contains_entity(&entity_a));
    assert_eq!(response.results[0].target.memory_id, memory(1));
}
