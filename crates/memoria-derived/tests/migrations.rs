use std::path::Path;

use memoria_derived::DerivedCatalog;
use rusqlite::Connection;
use tempfile::tempdir;

fn user_version(path: &Path) -> i64 {
    Connection::open(path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn fresh_database_migrates_to_latest() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("derived.sqlite");

    DerivedCatalog::open(&database).unwrap();

    assert_eq!(user_version(&database), 1);
}

#[test]
fn previous_schema_fixture_migrates_atomically() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("derived.sqlite");
    Connection::open(&database)
        .unwrap()
        .execute_batch("PRAGMA user_version = 0;")
        .unwrap();

    DerivedCatalog::open(&database).unwrap();

    let connection = Connection::open(database).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT manifest_id FROM serving_pointer WHERE singleton = 1",
                [],
                |row| row.get::<_, Option<i64>>(0),
            )
            .unwrap(),
        None
    );
}

#[test]
fn database_ahead_of_binary_is_rejected() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("derived.sqlite");
    Connection::open(&database)
        .unwrap()
        .execute_batch("PRAGMA user_version = 2;")
        .unwrap();

    assert!(DerivedCatalog::open(&database).is_err());
    assert_eq!(user_version(&database), 2);
}
