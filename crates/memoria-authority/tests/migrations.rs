use std::path::Path;

use memoria_authority::AuthorityDb;
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
    let database = directory.path().join("authority.sqlite");

    AuthorityDb::open(&database).unwrap();

    assert_eq!(user_version(&database), 1);
    let connection = Connection::open(database).unwrap();
    let table_count = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    assert_eq!(table_count, 11);
}

#[test]
fn previous_schema_fixture_migrates_atomically() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("authority.sqlite");
    Connection::open(&database)
        .unwrap()
        .execute_batch("PRAGMA user_version = 0;")
        .unwrap();

    AuthorityDb::open(&database).unwrap();

    assert_eq!(user_version(&database), 1);
    assert_eq!(
        Connection::open(database)
            .unwrap()
            .query_row(
                "SELECT generation FROM authority_generation WHERE id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
}

#[test]
fn database_ahead_of_binary_is_rejected() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("authority.sqlite");
    Connection::open(&database)
        .unwrap()
        .execute_batch("PRAGMA user_version = 2;")
        .unwrap();

    assert!(AuthorityDb::open(&database).is_err());
    assert_eq!(user_version(&database), 2);
}
