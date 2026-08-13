use memoria_query::{EntityRef, MemoryQuery, QueryError};
use memoria_types::SpaceId;

fn fixture_space() -> SpaceId {
    SpaceId::from_bytes([7; 16])
}

#[test]
fn scope_is_required() {
    let query = MemoryQuery::builder().text_cue("career").build_unchecked();
    assert!(matches!(query.validate(), Err(QueryError::ScopeRequired)));
}

#[test]
fn cue_entity_is_not_a_constraint() {
    let query = MemoryQuery::builder()
        .spaces(vec![fixture_space()])
        .cue_entity("person:ABC".parse::<EntityRef>().unwrap())
        .build()
        .unwrap();
    assert_eq!(query.cue.entities.len(), 1);
    assert!(query.constraints.entities.is_empty());
}
