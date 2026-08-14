use memoria_types::{MemoriaError, MemoryId, SourceBlobHash};
use rusqlite::params;

use crate::{AuthorityDb, model::database_error};

impl AuthorityDb {
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
