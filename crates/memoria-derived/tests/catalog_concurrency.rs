use memoria_derived::{AuthorityGeneration, DerivedCatalog};
use rusqlite::Connection;

#[test]
fn concurrent_reader_does_not_block_catalog_commit_under_wal() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("derived.sqlite");
    let mut catalog = DerivedCatalog::open(&database).unwrap();
    let reader = Connection::open(&database).unwrap();
    let journal_mode: String = reader
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
    reader
        .execute_batch("BEGIN; SELECT COUNT(*) FROM artifacts;")
        .unwrap();

    assert!(
        catalog
            .publish_empty_manifest(AuthorityGeneration::initial())
            .is_ok()
    );
    reader.execute_batch("COMMIT").unwrap();
}
