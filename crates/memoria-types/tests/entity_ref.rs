use memoria_types::EntityRef;

#[test]
fn entity_ref_requires_nonempty_namespace_and_opaque_id() {
    assert!(EntityRef::new("person:alice").is_ok());
    for value in ["", ":alice", "Person:alice", "person_1:alice", "person:"] {
        assert!(EntityRef::new(value).is_err(), "accepted {value:?}");
    }
}

#[test]
fn entity_ref_rejects_control_whitespace_and_colons_in_opaque_id() {
    for value in ["person:alice bob", "person:alice\n", "person:alice:child"] {
        assert!(EntityRef::new(value).is_err(), "accepted {value:?}");
    }
}

#[test]
fn entity_ref_accepts_utf8_opaque_ids_within_byte_limits() {
    assert!(EntityRef::new("place:北京/故宫").is_ok());
    assert!(EntityRef::new(format!("person:{}", "x".repeat(384))).is_ok());
    assert!(EntityRef::new(format!("person:{}", "x".repeat(385))).is_err());
}
