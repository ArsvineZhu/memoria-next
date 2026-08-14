use std::{thread, time::Duration};

use memoria_authority::{AuthorityDb, StoreLayout};
use rusqlite::Connection;

#[test]
fn authority_writer_waits_for_short_lock_contention() {
    let directory = tempfile::tempdir().unwrap();
    let layout = StoreLayout::create(directory.path()).unwrap();
    let db = AuthorityDb::open(layout.authority_database()).unwrap();
    let lock = Connection::open(layout.authority_database()).unwrap();
    lock.execute_batch("BEGIN IMMEDIATE").unwrap();

    let writer = thread::spawn(move || db.create_space("after-short-lock"));
    thread::sleep(Duration::from_millis(75));
    lock.execute_batch("COMMIT").unwrap();

    assert!(writer.join().unwrap().is_ok());
}

#[test]
fn concurrent_reader_does_not_block_normal_writer_commit_under_wal() {
    let directory = tempfile::tempdir().unwrap();
    let layout = StoreLayout::create(directory.path()).unwrap();
    let db = AuthorityDb::open(layout.authority_database()).unwrap();
    let reader = Connection::open(layout.authority_database()).unwrap();
    let journal_mode: String = reader
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
    reader
        .execute_batch("BEGIN; SELECT generation FROM authority_generation;")
        .unwrap();

    assert!(db.create_space("writer-under-reader").is_ok());
    reader.execute_batch("COMMIT").unwrap();
}
