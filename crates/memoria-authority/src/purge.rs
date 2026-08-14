use std::time::{SystemTime, UNIX_EPOCH};

use memoria_types::{AuthorityGeneration, MemoriaError, MemoryId, SourceBlobHash};
use rusqlite::{OptionalExtension, params};

use crate::{
    AuthorityDb,
    model::{AuthorityWriteAction, AuthorityWriteResult, database_error},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PurgeOperationRecord {
    pub purge_id: String,
    pub memory_id: MemoryId,
    pub state: String,
    pub planned_at: i64,
    pub committed_generation: Option<AuthorityGeneration>,
    pub updated_at: i64,
}

impl AuthorityDb {
    pub fn create_purge_operation(
        &self,
        purge_id: &str,
        memory_id: MemoryId,
    ) -> Result<PurgeOperationRecord, MemoriaError> {
        let purge_id = purge_id.to_owned();
        self.write_memoria_action(|transaction| {
            let existing = transaction
                .transaction
                .query_row(
                    "SELECT purge_id, memory_id, state, planned_at,
                            committed_generation, updated_at
                     FROM purge_operations
                     WHERE purge_id = ?1",
                    params![purge_id],
                    decode_purge_operation,
                )
                .optional()
                .map_err(database_error)?;
            if let Some(existing) = existing {
                if existing.memory_id != memory_id {
                    return Err(MemoriaError::IdempotencyConflict {
                        idempotency_key: purge_id,
                    });
                }
                return Ok(AuthorityWriteAction::Noop(AuthorityWriteResult::new(
                    existing,
                    transaction.base_generation(),
                )));
            }
            transaction
                .transaction
                .query_row(
                    "SELECT 1 FROM memories WHERE memory_id = ?1",
                    params![memory_id.as_bytes().as_slice()],
                    |_| Ok(()),
                )
                .optional()
                .map_err(database_error)?
                .ok_or_else(|| MemoriaError::NotFound {
                    kind: "memory",
                    id: memory_id.to_string(),
                })?;
            let now = unix_now();
            transaction
                .transaction
                .execute(
                    "INSERT INTO purge_operations(
                         purge_id, memory_id, state, planned_at,
                         committed_generation, updated_at,
                         last_error_code, last_error_message
                     ) VALUES (?1, ?2, 'planned', ?3, NULL, ?3, NULL, NULL)",
                    params![purge_id, memory_id.as_bytes().as_slice(), now],
                )
                .map_err(database_error)?;
            Ok(AuthorityWriteAction::Commit(PurgeOperationRecord {
                purge_id,
                memory_id,
                state: "planned".to_owned(),
                planned_at: now,
                committed_generation: None,
                updated_at: now,
            }))
        })
        .map(|result| result.into_value())
    }

    pub fn list_purge_operations(&self) -> Result<Vec<PurgeOperationRecord>, MemoriaError> {
        self.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT purge_id, memory_id, state, planned_at,
                        committed_generation, updated_at
                 FROM purge_operations
                 WHERE state <> 'completed'
                 ORDER BY planned_at, purge_id",
            )?;
            statement
                .query_map([], decode_purge_operation)?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(database_error)
    }

    pub fn transition_purge_operation(
        &self,
        purge_id: &str,
        expected_state: &str,
        next_state: &str,
    ) -> Result<PurgeOperationRecord, MemoriaError> {
        let purge_id = purge_id.to_owned();
        let expected_state = expected_state.to_owned();
        let next_state = next_state.to_owned();
        self.write_memoria_action(|transaction| {
            let current = transaction
                .transaction
                .query_row(
                    "SELECT purge_id, memory_id, state, planned_at,
                            committed_generation, updated_at
                     FROM purge_operations WHERE purge_id = ?1",
                    params![purge_id],
                    decode_purge_operation,
                )
                .optional()
                .map_err(database_error)?
                .ok_or_else(|| MemoriaError::NotFound {
                    kind: "purge operation",
                    id: purge_id.clone(),
                })?;
            if current.state != expected_state {
                return Err(MemoriaError::InvalidMutationBatch {
                    message: format!(
                        "purge operation {} expected state {}, found {}",
                        purge_id, expected_state, current.state
                    ),
                });
            }
            let committed_generation = if next_state == "committed" {
                Some(transaction.generation())
            } else {
                current.committed_generation
            };
            let now = unix_now();
            transaction
                .transaction
                .execute(
                    "UPDATE purge_operations
                     SET state = ?1,
                         committed_generation = ?2,
                         updated_at = ?3,
                         last_error_code = NULL,
                         last_error_message = NULL
                     WHERE purge_id = ?4",
                    params![
                        next_state,
                        committed_generation
                            .map(|value| { i64::try_from(value.value()).unwrap_or(i64::MAX) }),
                        now,
                        purge_id,
                    ],
                )
                .map_err(database_error)?;
            Ok(AuthorityWriteAction::Commit(PurgeOperationRecord {
                purge_id,
                memory_id: current.memory_id,
                state: next_state,
                planned_at: current.planned_at,
                committed_generation,
                updated_at: now,
            }))
        })
        .map(|result| result.into_value())
    }

    /// Physically remove one Memory and all of its revision/history rows in a
    /// single Authority transaction. The returned hashes are candidates for
    /// CAS collection after reachability is checked.
    pub fn purge_memory(&self, memory_id: MemoryId) -> Result<Vec<SourceBlobHash>, MemoriaError> {
        let result = self.write_memoria(|transaction| {
            let mut revisions = transaction.transaction.prepare(
                "SELECT source_blob_hash
                 FROM revisions
                 WHERE memory_id = ?1
                 ORDER BY revision_id",
            )
            .map_err(database_error)?;
            let hashes = revisions
                .query_map(params![memory_id.as_bytes().as_slice()], |row| {
                    row.get::<_, String>(0)
                        .and_then(|value| value.parse().map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        }))
                })
                .map_err(database_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(database_error)?;
            if hashes.is_empty() {
                return Err(MemoriaError::NotFound {
                    kind: "memory",
                    id: memory_id.to_string(),
                });
            }
            drop(revisions);

            transaction.transaction.execute(
                "DELETE FROM revision_parents
                 WHERE revision_id IN (SELECT revision_id FROM revisions WHERE memory_id = ?1)
                    OR parent_revision_id IN (SELECT revision_id FROM revisions WHERE memory_id = ?1)",
                params![memory_id.as_bytes().as_slice()],
            )
            .map_err(database_error)?;
            transaction.transaction.execute(
                "DELETE FROM memory_state_history WHERE memory_id = ?1",
                params![memory_id.as_bytes().as_slice()],
            )
            .map_err(database_error)?;
            transaction.transaction.execute(
                "DELETE FROM revisions WHERE memory_id = ?1",
                params![memory_id.as_bytes().as_slice()],
            )
            .map_err(database_error)?;
            transaction.transaction.execute(
                "DELETE FROM idempotency_records WHERE result_id = ?1",
                params![memory_id.as_bytes().as_slice()],
            )
            .map_err(database_error)?;
            let deleted = transaction.transaction.execute(
                "DELETE FROM memories WHERE memory_id = ?1",
                params![memory_id.as_bytes().as_slice()],
            )
            .map_err(database_error)?;
            if deleted != 1 {
                return Err(MemoriaError::NotFound {
                    kind: "memory",
                    id: memory_id.to_string(),
                });
            }
            Ok(hashes)
        })?;
        Ok(result.into_value())
    }

    pub fn source_blob_is_referenced(&self, hash: SourceBlobHash) -> Result<bool, MemoriaError> {
        self.read(|connection| {
            connection.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM revisions WHERE source_blob_hash = ?1
                )",
                params![hash.to_string()],
                |row| row.get(0),
            )
        })
        .map_err(|error| MemoriaError::Database {
            message: error.to_string(),
        })
    }
}

fn decode_purge_operation(row: &rusqlite::Row<'_>) -> rusqlite::Result<PurgeOperationRecord> {
    let memory_id: [u8; 16] = row.get::<_, Vec<u8>>(1)?.try_into().map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Blob,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "purge memory id has invalid length",
            )),
        )
    })?;
    let committed_generation = row
        .get::<_, Option<i64>>(4)?
        .map(|value| {
            u64::try_from(value)
                .map(AuthorityGeneration::new)
                .map_err(|_| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Integer,
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "purge committed generation is negative",
                        )),
                    )
                })
        })
        .transpose()?;
    Ok(PurgeOperationRecord {
        purge_id: row.get(0)?,
        memory_id: MemoryId::from_bytes(memory_id),
        state: row.get(2)?,
        planned_at: row.get(3)?,
        committed_generation,
        updated_at: row.get(5)?,
    })
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
        })
}
