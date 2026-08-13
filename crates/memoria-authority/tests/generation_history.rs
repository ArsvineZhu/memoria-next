use memoria_authority::{AuthorityDb, StoreLayout};
use memoria_types::{AuthorityGeneration, RevisionSemanticIntent};
use rusqlite::{Connection, params};

struct TestStore {
    _directory: tempfile::TempDir,
    layout: StoreLayout,
}

impl TestStore {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let layout = StoreLayout::create(directory.path()).unwrap();
        Self {
            _directory: directory,
            layout,
        }
    }

    fn layout(&self) -> &StoreLayout {
        &self.layout
    }
}

#[test]
fn new_store_starts_at_generation_zero() {
    let fixture = TestStore::new();
    let db = AuthorityDb::open(fixture.layout().authority_database()).unwrap();
    assert_eq!(
        db.current_generation().unwrap(),
        AuthorityGeneration::new(0)
    );
}

#[test]
fn one_atomic_write_advances_once() {
    let fixture = TestStore::new();
    let db = AuthorityDb::open(fixture.layout().authority_database()).unwrap();
    let result = db
        .write(|tx| {
            tx.create_space_record("personal")?;
            tx.create_space_record("project")?;
            Ok(())
        })
        .unwrap();
    assert_eq!(result.generation(), AuthorityGeneration::new(1));
    assert_eq!(
        db.current_generation().unwrap(),
        AuthorityGeneration::new(1)
    );
}

#[test]
fn schema_v1_creates_all_authority_tables() {
    let fixture = TestStore::new();
    let database = fixture.layout().authority_database();
    let db = AuthorityDb::open(database).unwrap();
    assert_eq!(
        db.current_generation().unwrap(),
        AuthorityGeneration::new(0)
    );

    let connection = Connection::open(database).unwrap();
    let mut statement = connection
        .prepare(
            "SELECT name
             FROM sqlite_master
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
             ORDER BY name",
        )
        .unwrap();
    let tables = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();

    assert_eq!(
        tables,
        vec![
            "authority_generation",
            "idempotency_records",
            "import_records",
            "memories",
            "memory_state_history",
            "revision_parents",
            "revisions",
            "space_state_history",
            "spaces",
            "store_meta",
        ]
    );
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT meta_value FROM store_meta WHERE meta_key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "1"
    );
}

#[test]
fn first_schema_initialization_persists_version_across_reopen() {
    let fixture = TestStore::new();
    let database = fixture.layout().authority_database();
    let db = AuthorityDb::open(database).unwrap();
    drop(db);

    let connection = Connection::open(database).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(connection);

    let reopened = AuthorityDb::open(database).unwrap();
    assert_eq!(
        reopened.current_generation().unwrap(),
        AuthorityGeneration::new(0)
    );
}

#[test]
fn future_schema_version_is_rejected_without_resetting_version() {
    let fixture = TestStore::new();
    let database = fixture.layout().authority_database();
    let connection = Connection::open(database).unwrap();
    connection
        .execute_batch("PRAGMA user_version = 2;")
        .unwrap();
    drop(connection);

    let result = AuthorityDb::open(database);
    assert!(result.is_err(), "future schema versions must be rejected");

    let connection = Connection::open(database).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn incompatible_existing_schema_is_rejected_without_bootstrap_mutation() {
    let fixture = TestStore::new();
    let database = fixture.layout().authority_database();
    let connection = Connection::open(database).unwrap();
    connection
        .execute("CREATE TABLE legacy_marker (value TEXT NOT NULL)", [])
        .unwrap();
    drop(connection);

    let result = AuthorityDb::open(database);
    assert!(
        result.is_err(),
        "unversioned existing schemas must be rejected"
    );

    let connection = Connection::open(database).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'authority_generation'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
}

#[test]
fn space_history_rejects_overlapping_intervals_and_duplicate_current_rows() {
    let fixture = TestStore::new();
    let database = fixture.layout().authority_database();
    let db = AuthorityDb::open(database).unwrap();
    let connection = Connection::open(database).unwrap();
    connection
        .execute(
            "INSERT INTO spaces (space_id, created_generation) VALUES (?1, 1)",
            params![&[3_u8; 16][..]],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO space_state_history (
                 space_id, space_key, lifecycle, valid_from_generation, valid_to_generation
             ) VALUES (?1, 'first', 'active', 1, 4)",
            params![&[3_u8; 16][..]],
        )
        .unwrap();

    let overlapping = connection.execute(
        "INSERT INTO space_state_history (
             space_id, space_key, lifecycle, valid_from_generation, valid_to_generation
         ) VALUES (?1, 'overlap', 'active', 3, 5)",
        params![&[3_u8; 16][..]],
    );
    assert!(
        overlapping.is_err(),
        "overlapping space history must be rejected"
    );

    connection
        .execute(
            "INSERT INTO space_state_history (
                 space_id, space_key, lifecycle, valid_from_generation
             ) VALUES (?1, 'current-a', 'active', 4)",
            params![&[3_u8; 16][..]],
        )
        .unwrap();
    let duplicate_current = connection.execute(
        "INSERT INTO space_state_history (
             space_id, space_key, lifecycle, valid_from_generation
         ) VALUES (?1, 'current-b', 'active', 4)",
        params![&[3_u8; 16][..]],
    );
    assert!(
        duplicate_current.is_err(),
        "a space must have at most one open current row"
    );
    drop(db);
}

#[test]
fn memory_history_rejects_overlapping_intervals_and_duplicate_current_rows() {
    let fixture = TestStore::new();
    let database = fixture.layout().authority_database();
    let db = AuthorityDb::open(database).unwrap();
    let connection = Connection::open(database).unwrap();
    connection
        .execute(
            "INSERT INTO spaces (space_id, created_generation) VALUES (?1, 1)",
            params![&[4_u8; 16][..]],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO memories (memory_id, created_generation) VALUES (?1, 1)",
            params![&[5_u8; 16][..]],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO memory_state_history (
                 memory_id, space_id, lifecycle, valid_from_generation, valid_to_generation
             ) VALUES (?1, ?2, 'active', 1, 4)",
            params![&[5_u8; 16][..], &[4_u8; 16][..]],
        )
        .unwrap();

    let overlapping = connection.execute(
        "INSERT INTO memory_state_history (
             memory_id, space_id, lifecycle, valid_from_generation, valid_to_generation
         ) VALUES (?1, ?2, 'active', 3, 5)",
        params![&[5_u8; 16][..], &[4_u8; 16][..]],
    );
    assert!(
        overlapping.is_err(),
        "overlapping memory history must be rejected"
    );

    connection
        .execute(
            "INSERT INTO memory_state_history (
                 memory_id, space_id, lifecycle, valid_from_generation
             ) VALUES (?1, ?2, 'active', 4)",
            params![&[5_u8; 16][..], &[4_u8; 16][..]],
        )
        .unwrap();
    let duplicate_current = connection.execute(
        "INSERT INTO memory_state_history (
             memory_id, space_id, lifecycle, valid_from_generation
         ) VALUES (?1, ?2, 'retired', 4)",
        params![&[5_u8; 16][..], &[4_u8; 16][..]],
    );
    assert!(
        duplicate_current.is_err(),
        "a memory must have at most one open current row"
    );
    drop(db);
}

#[test]
fn space_history_update_rejects_cross_row_overlap() {
    let fixture = TestStore::new();
    let database = fixture.layout().authority_database();
    let db = AuthorityDb::open(database).unwrap();
    let connection = Connection::open(database).unwrap();
    connection
        .execute(
            "INSERT INTO spaces (space_id, created_generation) VALUES (?1, 1)",
            params![&[6_u8; 16][..]],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO space_state_history (
                 space_id, space_key, lifecycle, valid_from_generation, valid_to_generation
             ) VALUES (?1, 'first', 'active', 1, 4)",
            params![&[6_u8; 16][..]],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO space_state_history (
                 space_id, space_key, lifecycle, valid_from_generation, valid_to_generation
             ) VALUES (?1, 'second', 'active', 4, 8)",
            params![&[6_u8; 16][..]],
        )
        .unwrap();

    let result = connection.execute(
        "UPDATE space_state_history
         SET valid_from_generation = 3, valid_to_generation = 5
         WHERE space_key = 'second'",
        [],
    );
    assert!(
        result.is_err(),
        "space history updates must preserve disjoint intervals"
    );
    drop(db);
}

#[test]
fn memory_history_update_rejects_cross_row_overlap() {
    let fixture = TestStore::new();
    let database = fixture.layout().authority_database();
    let db = AuthorityDb::open(database).unwrap();
    let connection = Connection::open(database).unwrap();
    connection
        .execute(
            "INSERT INTO spaces (space_id, created_generation) VALUES (?1, 1)",
            params![&[7_u8; 16][..]],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO memories (memory_id, created_generation) VALUES (?1, 1)",
            params![&[8_u8; 16][..]],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO memory_state_history (
                 memory_id, space_id, lifecycle, valid_from_generation, valid_to_generation
             ) VALUES (?1, ?2, 'active', 1, 4)",
            params![&[8_u8; 16][..], &[7_u8; 16][..]],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO memory_state_history (
                 memory_id, space_id, lifecycle, valid_from_generation, valid_to_generation
             ) VALUES (?1, ?2, 'active', 4, 8)",
            params![&[8_u8; 16][..], &[7_u8; 16][..]],
        )
        .unwrap();

    let result = connection.execute(
        "UPDATE memory_state_history
         SET valid_from_generation = 3, valid_to_generation = 5
         WHERE valid_from_generation = 4",
        [],
    );
    assert!(
        result.is_err(),
        "memory history updates must preserve disjoint intervals"
    );
    drop(db);
}

#[test]
fn new_space_history_starts_at_committed_generation() {
    let fixture = TestStore::new();
    let database = fixture.layout().authority_database();
    let db = AuthorityDb::open(database).unwrap();

    db.write(|tx| {
        tx.create_space_record("personal")?;
        tx.create_space_record("project")?;
        Ok(())
    })
    .unwrap();

    let connection = Connection::open(database).unwrap();
    let mut statement = connection
        .prepare(
            "SELECT space_key, valid_from_generation, valid_to_generation
             FROM space_state_history
             ORDER BY space_key",
        )
        .unwrap();
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();

    assert_eq!(
        rows,
        vec![
            ("personal".to_owned(), 1, None),
            ("project".to_owned(), 1, None),
        ]
    );
}

#[test]
fn failed_atomic_write_rolls_back_rows_and_generation() {
    let fixture = TestStore::new();
    let database = fixture.layout().authority_database();
    let db = AuthorityDb::open(database).unwrap();

    let result = db.write(|tx| -> rusqlite::Result<()> {
        tx.create_space_record("temporary")?;
        Err(rusqlite::Error::InvalidQuery)
    });

    assert!(result.is_err());
    assert_eq!(
        db.current_generation().unwrap(),
        AuthorityGeneration::new(0)
    );
    let connection = Connection::open(database).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM spaces", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn revision_semantic_intent_constraint_accepts_only_v1_values() {
    let fixture = TestStore::new();
    let database = fixture.layout().authority_database();
    let db = AuthorityDb::open(database).unwrap();
    let connection = Connection::open(database).unwrap();
    connection
        .execute(
            "INSERT INTO memories (memory_id, created_generation) VALUES (?1, 0)",
            params![&[0_u8; 16][..],],
        )
        .unwrap();

    let intents = [
        RevisionSemanticIntent::Edit,
        RevisionSemanticIntent::Transition,
        RevisionSemanticIntent::Correction,
        RevisionSemanticIntent::Supersession,
        RevisionSemanticIntent::Merge,
    ];
    for (index, intent) in intents.iter().enumerate() {
        let revision_id = [u8::try_from(index + 1).unwrap(); 16];
        connection
            .execute(
                "INSERT INTO revisions (
                     revision_id,
                     memory_id,
                     source_blob_hash,
                     semantic_intent,
                     committed_generation,
                     committed_at_unix_seconds,
                     committed_at_subsec_nanos
                 ) VALUES (?1, ?2, ?3, ?4, 0, 0, 0)",
                params![&revision_id[..], &[0_u8; 16][..], "00", intent.to_string(),],
            )
            .unwrap();
    }

    let stored = connection
        .prepare("SELECT semantic_intent FROM revisions ORDER BY rowid")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(
        stored,
        vec!["edit", "transition", "correction", "supersession", "merge"]
    );

    let invalid = connection.execute(
        "INSERT INTO revisions (
             revision_id,
             memory_id,
             source_blob_hash,
             semantic_intent,
             committed_generation,
             committed_at_unix_seconds,
             committed_at_subsec_nanos
         ) VALUES (?1, ?2, '00', 'unknown', 0, 0, 0)",
        params![&[99_u8; 16][..], &[0_u8; 16][..]],
    );
    assert!(invalid.is_err());
    drop(db);
}

#[test]
fn revision_timestamp_rejects_negative_nanoseconds() {
    let fixture = TestStore::new();
    let database = fixture.layout().authority_database();
    let db = AuthorityDb::open(database).unwrap();
    let connection = Connection::open(database).unwrap();
    connection
        .execute(
            "INSERT INTO memories (memory_id, created_generation) VALUES (?1, 0)",
            params![&[1_u8; 16][..]],
        )
        .unwrap();

    let result = connection.execute(
        "INSERT INTO revisions (
             revision_id,
             memory_id,
             source_blob_hash,
             semantic_intent,
             committed_generation,
             committed_at_unix_seconds,
             committed_at_subsec_nanos
         ) VALUES (?1, ?2, '00', 'edit', 0, 0, -1)",
        params![&[2_u8; 16][..], &[1_u8; 16][..]],
    );

    assert!(result.is_err(), "negative nanoseconds must be rejected");
    drop(db);
}
