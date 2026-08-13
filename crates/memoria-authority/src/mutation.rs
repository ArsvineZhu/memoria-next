use memoria_types::{
    MemoriaError, MemoryId, RevisionId, RevisionSemanticIntent, SourceBlobHash, SpaceId, Timestamp,
};
use rusqlite::params;
use sha2::{Digest, Sha256};

use crate::cas::SourceCas;
use crate::db::AuthorityDb;
use crate::model::{
    AuthorityTransaction, AuthorityWriteResult, MemoryLifecycle, MemoryRecord, SpaceLifecycle,
    SpaceRecord, database_error,
};

const REVISION_FORMAT_VERSION: u32 = 1;

impl AuthorityDb {
    pub fn create_space(
        &self,
        space_key: impl AsRef<str>,
    ) -> Result<AuthorityWriteResult<SpaceRecord>, MemoriaError> {
        let space_key = space_key.as_ref().to_owned();
        self.write_memoria(|tx| {
            ensure_space_key_available(tx, &space_key)?;
            let space_id = tx.create_space_record(&space_key).map_err(database_error)?;
            Ok(SpaceRecord {
                space_id,
                space_key,
                lifecycle: SpaceLifecycle::Active,
                generation: tx.generation(),
            })
        })
    }

    pub fn create_memory(
        &self,
        cas: &SourceCas,
        space_id: SpaceId,
        document_key: Option<&str>,
        source: &[u8],
    ) -> Result<AuthorityWriteResult<MemoryRecord>, MemoriaError> {
        let source_blob_hash = cas.put(source)?;
        let memory_id = MemoryId::try_new()?;
        let revision_id = derive_revision_id(
            memory_id,
            &[],
            source_blob_hash,
            RevisionSemanticIntent::Edit,
        );
        let document_key = document_key.map(str::to_owned);
        let committed_at = Timestamp::now()?;

        self.write_memoria(|tx| {
            ensure_space_is_active(tx, space_id)?;
            ensure_document_key_available(tx, space_id, document_key.as_deref())?;
            let record = MemoryRecord {
                memory_id,
                space_id,
                document_key,
                head_revision_id: revision_id,
                lifecycle: MemoryLifecycle::Active,
                source_blob_hash,
                generation: tx.generation(),
            };
            tx.insert_memory_record(&record, RevisionSemanticIntent::Edit, committed_at)
                .map_err(database_error)?;
            Ok(record)
        })
    }

    pub fn revise_memory(
        &self,
        cas: &SourceCas,
        memory_id: MemoryId,
        expected_head: RevisionId,
        source: &[u8],
    ) -> Result<AuthorityWriteResult<MemoryRecord>, MemoriaError> {
        let source_blob_hash = cas.put(source)?;
        let revision_id = derive_revision_id(
            memory_id,
            &[expected_head],
            source_blob_hash,
            RevisionSemanticIntent::Edit,
        );
        let committed_at = Timestamp::now()?;

        self.write_memoria(|tx| {
            let state = current_memory_state(tx, memory_id)?;
            if state.head_revision_id != expected_head {
                return Err(MemoriaError::HeadConflict {
                    memory_id,
                    expected_head,
                    actual_head: state.head_revision_id,
                });
            }
            if state.lifecycle == MemoryLifecycle::Retired {
                return Err(MemoriaError::MemoryRetired { memory_id });
            }

            tx.insert_revision_record(
                memory_id,
                revision_id,
                source_blob_hash,
                RevisionSemanticIntent::Edit,
                committed_at,
                &[expected_head],
            )
            .map_err(database_error)?;
            let generation =
                crate::model::sqlite_generation(tx.generation()).map_err(database_error)?;
            tx.close_current_memory_state(memory_id, generation)
                .map_err(database_error)?;
            tx.insert_memory_state(
                memory_id,
                state.space_id,
                state.document_key.as_deref(),
                revision_id,
                state.lifecycle,
                generation,
            )
            .map_err(database_error)?;

            Ok(MemoryRecord {
                memory_id,
                space_id: state.space_id,
                document_key: state.document_key,
                head_revision_id: revision_id,
                lifecycle: state.lifecycle,
                source_blob_hash,
                generation: tx.generation(),
            })
        })
    }
}

#[derive(Debug)]
struct CurrentMemoryState {
    space_id: SpaceId,
    document_key: Option<String>,
    head_revision_id: RevisionId,
    lifecycle: MemoryLifecycle,
}

fn ensure_space_key_available(
    transaction: &AuthorityTransaction<'_>,
    space_key: &str,
) -> Result<(), MemoriaError> {
    let exists = transaction
        .transaction
        .query_row(
            "SELECT EXISTS (
                 SELECT 1
                 FROM space_state_history
                 WHERE space_key = ?1 AND valid_to_generation IS NULL
             )",
            params![space_key],
            |row| row.get::<_, bool>(0),
        )
        .map_err(database_error)?;
    if exists {
        return Err(MemoriaError::KeyConflict {
            scope: "space",
            key: space_key.to_owned(),
        });
    }
    Ok(())
}

fn ensure_space_is_active(
    transaction: &AuthorityTransaction<'_>,
    space_id: SpaceId,
) -> Result<(), MemoriaError> {
    let lifecycle = transaction.transaction.query_row(
        "SELECT lifecycle
         FROM space_state_history
         WHERE space_id = ?1 AND valid_to_generation IS NULL
         LIMIT 1",
        params![space_id.as_bytes().as_slice()],
        |row| row.get::<_, String>(0),
    );
    match lifecycle {
        Ok(value) if value == "active" => Ok(()),
        Ok(value) if value == "retired" => Err(MemoriaError::SpaceRetired { space_id }),
        Ok(_) => Err(MemoriaError::Database {
            message: "space has an unknown lifecycle".to_owned(),
        }),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(MemoriaError::NotFound {
            kind: "space",
            id: space_id.to_string(),
        }),
        Err(error) => Err(database_error(error)),
    }
}

fn ensure_document_key_available(
    transaction: &AuthorityTransaction<'_>,
    space_id: SpaceId,
    document_key: Option<&str>,
) -> Result<(), MemoriaError> {
    let Some(document_key) = document_key else {
        return Ok(());
    };
    let exists = transaction
        .transaction
        .query_row(
            "SELECT EXISTS (
                 SELECT 1
                 FROM memory_state_history
                 WHERE space_id = ?1
                   AND document_key = ?2
                   AND valid_to_generation IS NULL
             )",
            params![space_id.as_bytes().as_slice(), document_key],
            |row| row.get::<_, bool>(0),
        )
        .map_err(database_error)?;
    if exists {
        return Err(MemoriaError::KeyConflict {
            scope: "document",
            key: document_key.to_owned(),
        });
    }
    Ok(())
}

fn current_memory_state(
    transaction: &AuthorityTransaction<'_>,
    memory_id: MemoryId,
) -> Result<CurrentMemoryState, MemoriaError> {
    let result = transaction.transaction.query_row(
        "SELECT space_id, document_key, head_revision_id, lifecycle
         FROM memory_state_history
         WHERE memory_id = ?1 AND valid_to_generation IS NULL
         LIMIT 1",
        params![memory_id.as_bytes().as_slice()],
        |row| {
            let space_id =
                SpaceId::from_bytes(parse_fixed_bytes(row.get::<_, Vec<u8>>(0)?, "space id")?);
            let head_revision_id = RevisionId::from_bytes(parse_fixed_bytes(
                row.get::<_, Vec<u8>>(2)?,
                "revision id",
            )?);
            let lifecycle = match row.get::<_, String>(3)?.as_str() {
                "active" => MemoryLifecycle::Active,
                "retired" => MemoryLifecycle::Retired,
                _ => return Err(rusqlite::Error::InvalidQuery),
            };
            Ok(CurrentMemoryState {
                space_id,
                document_key: row.get(1)?,
                head_revision_id,
                lifecycle,
            })
        },
    );
    match result {
        Ok(state) => Ok(state),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(MemoriaError::NotFound {
            kind: "memory",
            id: memory_id.to_string(),
        }),
        Err(error) => Err(database_error(error)),
    }
}

fn derive_revision_id(
    memory_id: MemoryId,
    parents: &[RevisionId],
    source_blob_hash: SourceBlobHash,
    semantic_intent: RevisionSemanticIntent,
) -> RevisionId {
    let intent = semantic_intent.to_string();
    let mut hasher = Sha256::new();
    hasher.update(b"memoria.revision.commit\0");
    hasher.update(REVISION_FORMAT_VERSION.to_be_bytes());
    hasher.update(memory_id.as_bytes());
    hasher.update(
        u32::try_from(parents.len())
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    for parent in parents {
        hasher.update(parent.as_bytes());
    }
    hasher.update(source_blob_hash.as_bytes());
    hasher.update(
        u32::try_from(intent.len())
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    hasher.update(intent.as_bytes());
    RevisionId::from_bytes(hasher.finalize().into())
}

fn parse_fixed_bytes<const N: usize>(
    bytes: Vec<u8>,
    kind: &'static str,
) -> rusqlite::Result<[u8; N]> {
    bytes.try_into().map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Blob,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{kind} has an invalid byte length"),
            )),
        )
    })
}
