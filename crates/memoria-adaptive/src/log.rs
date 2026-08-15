use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use memoria_types::{AdaptiveGeneration, MemoryId, SpaceId};
use rusqlite::{
    Connection, ErrorCode, OptionalExtension, Transaction, TransactionBehavior, params,
};
use rusqlite_migration::{M, Migrations};
use sha2::{Digest, Sha256};
use std::{thread, time::Duration};

use crate::{
    AdaptiveCheckpoint, AdaptiveError, AdaptiveEvent, AdaptiveReadSnapshot, AdaptiveStateV1,
    FeedbackEventInput,
};

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS adaptive_meta(
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS adaptive_events(
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE,
    generation INTEGER NOT NULL,
    retrieval_id TEXT NOT NULL,
    space_id BLOB NOT NULL CHECK(length(space_id) = 16),
    memory_id BLOB NOT NULL CHECK(length(memory_id) = 16),
    revision_id BLOB NOT NULL CHECK(length(revision_id) = 32),
    semantic_node_id TEXT,
    query_signature TEXT NOT NULL,
    outcome TEXT NOT NULL,
    occurred_at INTEGER NOT NULL,
    occurred_at_nanos INTEGER NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,
    fingerprint TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS adaptive_familiarity(
    space_id BLOB NOT NULL,
    memory_id BLOB NOT NULL,
    success_count INTEGER NOT NULL,
    negative_count INTEGER NOT NULL,
    positive_weight REAL NOT NULL,
    last_success_at INTEGER,
    last_feedback_generation INTEGER NOT NULL,
    PRIMARY KEY(space_id, memory_id)
);
CREATE TABLE IF NOT EXISTS adaptive_tag_affinity(
    space_id BLOB NOT NULL,
    tag_id TEXT NOT NULL,
    memory_id BLOB NOT NULL,
    positive_count INTEGER NOT NULL,
    negative_count INTEGER NOT NULL,
    positive_weight REAL NOT NULL,
    negative_weight REAL NOT NULL,
    PRIMARY KEY(space_id, tag_id, memory_id)
);
CREATE TABLE IF NOT EXISTS adaptive_query_class_affinity(
    space_id BLOB NOT NULL,
    query_class TEXT NOT NULL,
    memory_id BLOB NOT NULL,
    positive_count INTEGER NOT NULL,
    negative_count INTEGER NOT NULL,
    positive_weight REAL NOT NULL,
    negative_weight REAL NOT NULL,
    PRIMARY KEY(space_id, query_class, memory_id)
);
"#;

const MIGRATION_LIST: &[M<'_>] = &[M::up(SCHEMA_V1)];
const MIGRATIONS: Migrations<'_> = Migrations::from_slice(MIGRATION_LIST);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdaptiveLogCommit {
    pub generation: AdaptiveGeneration,
    pub events: Vec<AdaptiveEvent>,
}

#[derive(Debug)]
pub struct AdaptiveEventLog {
    generation: AdaptiveGeneration,
    events: Vec<AdaptiveEvent>,
    by_idempotency_key: BTreeMap<String, usize>,
    by_event_id: BTreeMap<String, usize>,
    materialized_state: AdaptiveStateV1,
    connection: Option<Connection>,
    database_path: Option<PathBuf>,
}

impl AdaptiveEventLog {
    #[must_use]
    pub fn new() -> Self {
        Self {
            generation: AdaptiveGeneration::initial(),
            events: Vec::new(),
            by_idempotency_key: BTreeMap::new(),
            by_event_id: BTreeMap::new(),
            materialized_state: AdaptiveStateV1::default(),
            connection: None,
            database_path: None,
        }
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, AdaptiveError> {
        let database_path = path.as_ref().to_path_buf();
        if let Some(parent) = database_path.parent() {
            std::fs::create_dir_all(parent).map_err(storage_error)?;
        }
        let mut connection = Connection::open(&database_path).map_err(storage_error)?;
        configure_connection(&connection)?;
        initialize_schema(&mut connection)?;
        let events = load_events(&connection)?;
        let mut log = Self::from_events(events)?;
        log.generation = log.generation.max(load_generation(&connection)?);
        log.materialized_state.set_generation(log.generation);
        log.connection = Some(connection);
        log.database_path = Some(database_path);
        log.restore_or_materialize_state()?;
        Ok(log)
    }

    #[must_use]
    pub const fn current_generation(&self) -> AdaptiveGeneration {
        self.generation
    }

    #[must_use]
    pub fn events(&self) -> &[AdaptiveEvent] {
        &self.events
    }

    #[must_use]
    pub fn state(&self) -> &AdaptiveStateV1 {
        &self.materialized_state
    }

    pub fn snapshot_at(
        &self,
        generation: memoria_types::AdaptiveGeneration,
    ) -> Result<AdaptiveReadSnapshot, AdaptiveError> {
        if generation != self.generation {
            return Err(AdaptiveError::SnapshotUnavailable {
                requested: generation,
                current: self.generation,
            });
        }
        Ok(AdaptiveReadSnapshot::new(
            generation,
            self.materialized_state.clone(),
        ))
    }

    #[must_use]
    pub fn database_path(&self) -> Option<&Path> {
        self.database_path.as_deref()
    }

    #[must_use]
    pub fn event_for_idempotency_key(&self, key: &str) -> Option<AdaptiveEvent> {
        self.by_idempotency_key
            .get(key)
            .and_then(|index| self.events.get(*index))
            .cloned()
    }

    #[must_use]
    pub fn input_for_idempotency_key(&self, key: &str) -> Option<FeedbackEventInput> {
        self.event_for_idempotency_key(key)
            .map(|event| event.as_input())
    }

    pub fn from_events<I>(events: I) -> Result<Self, AdaptiveError>
    where
        I: IntoIterator<Item = AdaptiveEvent>,
    {
        let mut log = Self::new();
        log.events = events.into_iter().collect();
        log.rebuild_indexes()?;
        log.materialized_state = AdaptiveStateV1::replay(&log.events);
        log.materialized_state.set_generation(log.generation);
        Ok(log)
    }

    pub fn rewrite_without_memory(&mut self, memory_id: MemoryId) -> Result<(), AdaptiveError> {
        let events = crate::purge::rewrite_without_memory(&self.events, memory_id);
        self.persist_rewrite(&events)?;
        self.events = events;
        self.rebuild_indexes()
    }

    pub fn reset_space(&mut self, space_id: SpaceId) -> Result<(), AdaptiveError> {
        let events = crate::reset::reset_space(&self.events, space_id);
        self.persist_rewrite(&events)?;
        self.events = events;
        self.rebuild_indexes()
    }

    pub fn reset_store(&mut self) -> Result<(), AdaptiveError> {
        let events = crate::reset::reset_store(&self.events);
        self.persist_rewrite(&events)?;
        self.events = events;
        self.rebuild_indexes()
    }

    pub fn append_batch<I>(&mut self, inputs: I) -> Result<AdaptiveLogCommit, AdaptiveError>
    where
        I: IntoIterator<Item = FeedbackEventInput>,
    {
        let inputs: Vec<_> = inputs.into_iter().collect();
        for input in &inputs {
            input.validate()?;
        }

        let mut staged_by_key = BTreeMap::<String, FeedbackEventInput>::new();
        let mut staged_by_event_id = BTreeMap::<String, FeedbackEventInput>::new();

        for input in &inputs {
            if let Some(index) = self.by_idempotency_key.get(&input.idempotency_key) {
                if self.events[*index].as_input() != *input {
                    return Err(AdaptiveError::IdempotencyConflict {
                        key: input.idempotency_key.clone(),
                    });
                }
                continue;
            }
            if let Some(existing) = staged_by_key.get(&input.idempotency_key) {
                if existing != input {
                    return Err(AdaptiveError::IdempotencyConflict {
                        key: input.idempotency_key.clone(),
                    });
                }
                continue;
            }
            if self.by_event_id.contains_key(&input.event_id)
                || staged_by_event_id.contains_key(&input.event_id)
            {
                return Err(AdaptiveError::EventIdConflict {
                    event_id: input.event_id.clone(),
                });
            }
            staged_by_key.insert(input.idempotency_key.clone(), input.clone());
            staged_by_event_id.insert(input.event_id.clone(), input.clone());
        }

        if !staged_by_key.is_empty() {
            let generation = self
                .generation
                .checked_next()
                .ok_or(AdaptiveError::GenerationExhausted)?;
            let staged_events = staged_by_key
                .values()
                .cloned()
                .map(|input| AdaptiveEvent::from_input(input, generation))
                .collect::<Vec<_>>();
            let mut candidate_events = self.events.clone();
            candidate_events.extend(staged_events.iter().cloned());
            let mut candidate_state = AdaptiveStateV1::replay(&candidate_events);
            candidate_state.set_generation(generation);
            if let Some(connection) = self.connection.as_mut() {
                persist_append(
                    connection,
                    &staged_events,
                    &candidate_state,
                    generation,
                    candidate_events.len(),
                )?;
            }
            for event in staged_events {
                let index = self.events.len();
                self.by_idempotency_key
                    .insert(event.idempotency_key.clone(), index);
                self.by_event_id.insert(event.event_id.clone(), index);
                self.events.push(event);
            }
            self.generation = generation;
            self.materialized_state = candidate_state;
        }

        let events = inputs
            .iter()
            .filter_map(|input| {
                self.by_idempotency_key
                    .get(&input.idempotency_key)
                    .map(|index| self.events[*index].clone())
            })
            .collect();

        Ok(AdaptiveLogCommit {
            generation: self.generation,
            events,
        })
    }

    fn rebuild_indexes(&mut self) -> Result<(), AdaptiveError> {
        self.by_idempotency_key.clear();
        self.by_event_id.clear();
        for (index, event) in self.events.iter().enumerate() {
            event.as_input().validate()?;
            if self
                .by_event_id
                .insert(event.event_id.clone(), index)
                .is_some()
            {
                return Err(AdaptiveError::EventIdConflict {
                    event_id: event.event_id.clone(),
                });
            }
            if self
                .by_idempotency_key
                .insert(event.idempotency_key.clone(), index)
                .is_some()
            {
                return Err(AdaptiveError::IdempotencyConflict {
                    key: event.idempotency_key.clone(),
                });
            }
            self.generation = self.generation.max(event.generation);
        }
        self.materialized_state = AdaptiveStateV1::replay(&self.events);
        self.materialized_state.set_generation(self.generation);
        Ok(())
    }

    fn persist_rewrite(&mut self, events: &[AdaptiveEvent]) -> Result<(), AdaptiveError> {
        let Some(connection) = self.connection.as_mut() else {
            return Ok(());
        };
        let mut state = AdaptiveStateV1::replay(events);
        state.set_generation(self.generation);
        persist_snapshot(connection, events, &state, self.generation)
    }

    fn restore_or_materialize_state(&mut self) -> Result<(), AdaptiveError> {
        let (state_json, reduced_count) = {
            let connection = self
                .connection
                .as_ref()
                .ok_or_else(|| storage_error("adaptive database is not open"))?;
            let state_json = connection
                .query_row(
                    "SELECT value FROM adaptive_meta WHERE key = 'materialized_state'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(storage_error)?;
            let reduced_count = connection
                .query_row(
                    "SELECT value FROM adaptive_meta WHERE key = 'last_reduced_sequence'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(storage_error)?
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            (state_json, reduced_count)
        };
        if let Some(state_json) = state_json
            && let Ok(state) = serde_json::from_str::<AdaptiveStateV1>(&state_json)
        {
            let checkpoint = AdaptiveCheckpoint {
                event_count: reduced_count.min(self.events.len()),
                generation: self.generation,
                state,
            };
            self.materialized_state = AdaptiveStateV1::from_checkpoint_and_tail(
                checkpoint,
                self.events[reduced_count.min(self.events.len())..]
                    .iter()
                    .cloned(),
            );
            self.materialized_state.set_generation(self.generation);
            return Ok(());
        }
        self.materialized_state = AdaptiveStateV1::replay(&self.events);
        self.materialized_state.set_generation(self.generation);
        Ok(())
    }
}

impl Default for AdaptiveEventLog {
    fn default() -> Self {
        Self::new()
    }
}

fn configure_connection(connection: &Connection) -> Result<(), AdaptiveError> {
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;",
        )
        .map_err(storage_error)
}

fn initialize_schema(connection: &mut Connection) -> Result<(), AdaptiveError> {
    MIGRATIONS
        .to_latest(connection)
        .map_err(|error| storage_error(format!("adaptive schema migration failed: {error}")))
}

fn persist_append(
    connection: &mut Connection,
    events: &[AdaptiveEvent],
    state: &AdaptiveStateV1,
    generation: AdaptiveGeneration,
    event_count: usize,
) -> Result<(), AdaptiveError> {
    let transaction = begin_immediate_with_retry(connection).map_err(storage_error)?;
    for event in events {
        insert_event(&transaction, event)?;
    }
    persist_materialized(&transaction, state, generation, event_count)?;
    transaction.commit().map_err(storage_error)
}

fn persist_snapshot(
    connection: &mut Connection,
    events: &[AdaptiveEvent],
    state: &AdaptiveStateV1,
    generation: AdaptiveGeneration,
) -> Result<(), AdaptiveError> {
    let transaction = begin_immediate_with_retry(connection).map_err(storage_error)?;
    transaction
        .execute("DELETE FROM adaptive_events", [])
        .map_err(storage_error)?;
    for event in events {
        insert_event(&transaction, event)?;
    }
    transaction
        .execute("DELETE FROM adaptive_familiarity", [])
        .map_err(storage_error)?;
    transaction
        .execute("DELETE FROM adaptive_tag_affinity", [])
        .map_err(storage_error)?;
    transaction
        .execute("DELETE FROM adaptive_query_class_affinity", [])
        .map_err(storage_error)?;
    persist_materialized(&transaction, state, generation, events.len())?;
    transaction.commit().map_err(storage_error)
}

fn insert_event(
    transaction: &rusqlite::Transaction<'_>,
    event: &AdaptiveEvent,
) -> Result<(), AdaptiveError> {
    let query_signature = serde_json::to_string(&event.query_signature).map_err(storage_error)?;
    let fingerprint = event_fingerprint(event)?;
    let generation = sql_u64(event.generation.value())?;
    transaction
        .execute(
            "INSERT INTO adaptive_events(
                 event_id, generation, retrieval_id, space_id, memory_id, revision_id,
                 semantic_node_id, query_signature, outcome, occurred_at,
                 occurred_at_nanos, idempotency_key, fingerprint
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                event.event_id,
                generation,
                event.retrieval_id,
                event.space_id.as_bytes().as_slice(),
                event.memory_id.as_bytes().as_slice(),
                event.revision_id.as_bytes().as_slice(),
                event.semantic_node_id,
                query_signature,
                event.outcome.to_string(),
                event.occurred_at.unix_seconds(),
                i64::from(event.occurred_at.subsec_nanos()),
                event.idempotency_key,
                fingerprint,
            ],
        )
        .map_err(storage_error)?;
    Ok(())
}

fn begin_immediate_with_retry(connection: &Connection) -> rusqlite::Result<Transaction<'_>> {
    begin_immediate_with_retry_count(connection, 0)
}

fn begin_immediate_with_retry_count(
    connection: &Connection,
    retry: usize,
) -> rusqlite::Result<Transaction<'_>> {
    const RETRY_DELAYS_MS: [u64; 3] = [10, 50, 200];
    match Transaction::new_unchecked(connection, TransactionBehavior::Immediate) {
        Ok(transaction) => Ok(transaction),
        Err(error) if is_busy_or_locked(&error) && retry < RETRY_DELAYS_MS.len() => {
            thread::sleep(Duration::from_millis(RETRY_DELAYS_MS[retry]));
            begin_immediate_with_retry_count(connection, retry + 1)
        }
        Err(error) => Err(error),
    }
}

fn is_busy_or_locked(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if matches!(failure.code, ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
    )
}

fn persist_materialized(
    transaction: &rusqlite::Transaction<'_>,
    state: &AdaptiveStateV1,
    generation: AdaptiveGeneration,
    event_count: usize,
) -> Result<(), AdaptiveError> {
    // The materialized tables are the durable projection. AdaptiveStateV1's
    // tuple-keyed maps are intentionally not encoded as JSON; the event log
    // remains the canonical replay source on open.
    let state_json = "replay-from-events-v1";
    put_meta(
        transaction,
        "adaptive_generation",
        &generation.value().to_string(),
    )?;
    put_meta(
        transaction,
        "last_reduced_sequence",
        &event_count.to_string(),
    )?;
    put_meta(transaction, "materialized_state", state_json)?;
    transaction
        .execute("DELETE FROM adaptive_familiarity", [])
        .map_err(storage_error)?;
    for ((space_id, memory_id), familiarity) in state.target_rows() {
        let success_count = sql_u64(familiarity.success_count)?;
        let negative_count = sql_u64(familiarity.negative_count)?;
        let last_feedback_generation = sql_u64(familiarity.last_feedback_generation.value())?;
        transaction
            .execute(
                "INSERT INTO adaptive_familiarity(
                     space_id, memory_id, success_count, negative_count, positive_weight,
                     last_success_at, last_feedback_generation
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    space_id.as_bytes().as_slice(),
                    memory_id.as_bytes().as_slice(),
                    success_count,
                    negative_count,
                    familiarity.positive_weight,
                    familiarity.last_success_at.map(|time| time.unix_seconds()),
                    last_feedback_generation,
                ],
            )
            .map_err(storage_error)?;
    }
    transaction
        .execute("DELETE FROM adaptive_tag_affinity", [])
        .map_err(storage_error)?;
    for ((space_id, memory_id, tag), affinity) in state.tag_affinity_rows() {
        let positive_count = sql_u64(affinity.positive_count)?;
        let negative_count = sql_u64(affinity.negative_count)?;
        transaction
            .execute(
                "INSERT INTO adaptive_tag_affinity(
                     space_id, tag_id, memory_id, positive_count, negative_count,
                     positive_weight, negative_weight
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    space_id.as_bytes().as_slice(),
                    tag,
                    memory_id.as_bytes().as_slice(),
                    positive_count,
                    negative_count,
                    affinity.positive_weight,
                    affinity.negative_weight,
                ],
            )
            .map_err(storage_error)?;
    }
    transaction
        .execute("DELETE FROM adaptive_query_class_affinity", [])
        .map_err(storage_error)?;
    for ((space_id, memory_id, query_class), affinity) in state.query_class_affinity_rows() {
        let positive_count = sql_u64(affinity.positive_count)?;
        let negative_count = sql_u64(affinity.negative_count)?;
        transaction
            .execute(
                "INSERT INTO adaptive_query_class_affinity(
                     space_id, query_class, memory_id, positive_count, negative_count,
                     positive_weight, negative_weight
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    space_id.as_bytes().as_slice(),
                    query_class,
                    memory_id.as_bytes().as_slice(),
                    positive_count,
                    negative_count,
                    affinity.positive_weight,
                    affinity.negative_weight,
                ],
            )
            .map_err(storage_error)?;
    }
    Ok(())
}

fn put_meta(
    transaction: &rusqlite::Transaction<'_>,
    key: &str,
    value: &str,
) -> Result<(), AdaptiveError> {
    transaction
        .execute(
            "INSERT INTO adaptive_meta(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(storage_error)?;
    Ok(())
}

fn load_events(connection: &Connection) -> Result<Vec<AdaptiveEvent>, AdaptiveError> {
    let mut statement = connection
        .prepare(
            "SELECT event_id, generation, retrieval_id, space_id, memory_id, revision_id,
                    semantic_node_id, query_signature, outcome, occurred_at,
                    occurred_at_nanos, idempotency_key
             FROM adaptive_events ORDER BY sequence",
        )
        .map_err(storage_error)?;
    let rows = statement
        .query_map([], decode_event)
        .map_err(storage_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(storage_error)
}

fn decode_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<AdaptiveEvent> {
    let generation = row.get::<_, i64>(1)?.try_into().map_err(|_| {
        rusqlite::Error::InvalidColumnType(
            1,
            "generation".to_owned(),
            rusqlite::types::Type::Integer,
        )
    })?;
    let occurred_at = memoria_types::Timestamp::from_parts(row.get(9)?, row.get(10)?)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let query_signature = serde_json::from_str(&row.get::<_, String>(7)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let outcome = row
        .get::<_, String>(8)?
        .parse()
        .map_err(|error: AdaptiveError| {
            rusqlite::Error::FromSqlConversionFailure(
                8,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    Ok(AdaptiveEvent {
        event_id: row.get(0)?,
        generation: AdaptiveGeneration::new(generation),
        retrieval_id: row.get(2)?,
        space_id: fixed_id(row.get(3)?, 16)?,
        memory_id: fixed_id(row.get(4)?, 16)?,
        revision_id: fixed_id(row.get(5)?, 32)?,
        semantic_node_id: row.get(6)?,
        query_signature,
        outcome,
        occurred_at,
        idempotency_key: row.get(11)?,
    })
}

fn fixed_id<T>(bytes: Vec<u8>, length: usize) -> rusqlite::Result<T>
where
    T: FromFixedBytes,
{
    if bytes.len() != length {
        return Err(rusqlite::Error::InvalidColumnType(
            0,
            "fixed identity".to_owned(),
            rusqlite::types::Type::Blob,
        ));
    }
    T::from_fixed_bytes(&bytes).ok_or_else(|| {
        rusqlite::Error::InvalidColumnType(
            0,
            "fixed identity".to_owned(),
            rusqlite::types::Type::Blob,
        )
    })
}

trait FromFixedBytes: Sized {
    fn from_fixed_bytes(bytes: &[u8]) -> Option<Self>;
}

impl FromFixedBytes for SpaceId {
    fn from_fixed_bytes(bytes: &[u8]) -> Option<Self> {
        Some(Self::from_bytes(bytes.try_into().ok()?))
    }
}

impl FromFixedBytes for MemoryId {
    fn from_fixed_bytes(bytes: &[u8]) -> Option<Self> {
        Some(Self::from_bytes(bytes.try_into().ok()?))
    }
}

impl FromFixedBytes for memoria_types::RevisionId {
    fn from_fixed_bytes(bytes: &[u8]) -> Option<Self> {
        Some(Self::from_bytes(bytes.try_into().ok()?))
    }
}

fn load_generation(connection: &Connection) -> Result<AdaptiveGeneration, AdaptiveError> {
    let value = connection
        .query_row(
            "SELECT value FROM adaptive_meta WHERE key = 'adaptive_generation'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(storage_error)?;
    value
        .map(|value| {
            value
                .parse::<u64>()
                .map(AdaptiveGeneration::new)
                .map_err(|_| storage_error("invalid adaptive generation"))
        })
        .transpose()
        .map(|value| value.unwrap_or_else(AdaptiveGeneration::initial))
}

fn event_fingerprint(event: &AdaptiveEvent) -> Result<String, AdaptiveError> {
    let bytes = serde_json::to_vec(event).map_err(storage_error)?;
    Ok(hex_lower(&Sha256::digest(bytes)))
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn storage_error(error: impl ToString) -> AdaptiveError {
    AdaptiveError::Storage {
        message: error.to_string(),
    }
}

fn sql_u64(value: u64) -> Result<i64, AdaptiveError> {
    i64::try_from(value).map_err(|_| storage_error("adaptive integer exceeds SQLite range"))
}
