use memoria_authority::{AuthorityDb, SourceCas, StoreLayout};
use memoria_types::{AuthorityGeneration, SourceBlobHash};
use sha2::{Digest, Sha256};

#[test]
fn source_blob_hash_contract_is_sha256_of_raw_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let layout = StoreLayout::create(directory.path()).unwrap();
    let cas = SourceCas::new(&layout);
    let source = b"authority contract source\0with raw bytes";

    let hash = cas.put(source).unwrap();
    let expected: [u8; 32] = Sha256::digest(source).into();
    assert_eq!(hash, SourceBlobHash::from_digest(expected));
    assert_eq!(cas.get(hash).unwrap(), source);
}

#[test]
fn revision_identity_and_snapshot_contract_survive_revise() {
    let directory = tempfile::tempdir().unwrap();
    let layout = StoreLayout::create(directory.path()).unwrap();
    let db = AuthorityDb::open(layout.authority_database()).unwrap();
    let cas = SourceCas::new(&layout);
    let space = db.create_space("contract").unwrap().into_value();
    let original = db
        .create_memory(&cas, space.id(), Some("contract"), b"original")
        .unwrap()
        .into_value();

    let revised = db
        .revise_memory(
            &cas,
            original.memory_id(),
            original.revision_id(),
            b"revised",
        )
        .unwrap()
        .into_value();

    assert_eq!(revised.memory_id(), original.memory_id());
    assert_ne!(revised.revision_id(), original.revision_id());
    assert_eq!(revised.generation(), AuthorityGeneration::new(3));
    assert_eq!(
        db.read_memory_at(&cas, original.memory_id(), original.generation())
            .unwrap()
            .source_bytes(),
        b"original"
    );
    assert_eq!(
        db.read_memory(&cas, original.memory_id())
            .unwrap()
            .source_bytes(),
        b"revised"
    );
}
