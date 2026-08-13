use memoria_types::{
    AuthorityGeneration, MemoriaError, MemoryId, RevisionId, SourceBlobHash, SpaceId,
};
use rusqlite::{Connection, Row, params};

use crate::cas::SourceCas;
use crate::db::AuthorityDb;
use crate::model::{
    MemoryLifecycle, MemoryRecord, RevisionRecord, SpaceLifecycle, SpaceRecord,
    authority_generation, database_error, sqlite_generation,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryRead {
    pub memory: MemoryRecord,
    pub revision: RevisionRecord,
    pub source: Vec<u8>,
}

impl MemoryRead {
    #[must_use]
    pub fn memory_id(&self) -> MemoryId {
        self.memory.memory_id
    }

    #[must_use]
    pub fn revision_id(&self) -> RevisionId {
        self.revision.revision_id
    }

    #[must_use]
    pub fn source_bytes(&self) -> &[u8] {
        &self.source
    }
}

impl AuthorityDb {
    pub fn get_space(&self, space_id: SpaceId) -> Result<SpaceRecord, MemoriaError> {
        self.read(|connection| {
            let generation = current_generation(connection)?;
            read_space_at(connection, space_id, generation)
        })
        .map_err(database_error)
        .and_then(|record| record)
    }

    pub fn get_space_at(
        &self,
        space_id: SpaceId,
        generation: AuthorityGeneration,
    ) -> Result<SpaceRecord, MemoriaError> {
        self.read(|connection| read_space_at(connection, space_id, generation))
            .map_err(database_error)
            .and_then(|record| record)
    }

    pub fn get_memory(&self, memory_id: MemoryId) -> Result<MemoryRecord, MemoriaError> {
        self.read(|connection| {
            let generation = current_generation(connection)?;
            read_memory_at(connection, memory_id, generation)
        })
        .map_err(database_error)
        .and_then(|record| record)
    }

    pub fn get_memory_at(
        &self,
        memory_id: MemoryId,
        generation: AuthorityGeneration,
    ) -> Result<MemoryRecord, MemoriaError> {
        self.read(|connection| read_memory_at(connection, memory_id, generation))
            .map_err(database_error)
            .and_then(|record| record)
    }

    pub fn get_revision(&self, revision_id: RevisionId) -> Result<RevisionRecord, MemoriaError> {
        self.read(|connection| read_revision(connection, revision_id))
            .map_err(database_error)
            .and_then(|record| record)
    }

    pub fn read_memory(
        &self,
        cas: &SourceCas,
        memory_id: MemoryId,
    ) -> Result<MemoryRead, MemoriaError> {
        let memory = self.get_memory(memory_id)?;
        let revision = self.get_revision(memory.head_revision_id)?;
        let source = cas.get(memory.source_blob_hash)?;
        Ok(MemoryRead {
            memory,
            revision,
            source,
        })
    }

    pub fn read_memory_at(
        &self,
        cas: &SourceCas,
        memory_id: MemoryId,
        generation: AuthorityGeneration,
    ) -> Result<MemoryRead, MemoriaError> {
        let memory = self.get_memory_at(memory_id, generation)?;
        let revision = self.get_revision(memory.head_revision_id)?;
        let source = cas.get(memory.source_blob_hash)?;
        Ok(MemoryRead {
            memory,
            revision,
            source,
        })
    }

    pub fn list_memories_at(
        &self,
        cas: &SourceCas,
        space_id: SpaceId,
        generation: AuthorityGeneration,
    ) -> Result<Vec<MemoryRead>, MemoriaError> {
        self.read(|connection| {
            let generation_value = sqlite_generation(generation)?;
            let mut statement = connection.prepare(
                "SELECT memory_state_history.space_id,
                        memory_state_history.document_key,
                        memory_state_history.head_revision_id,
                        memory_state_history.lifecycle,
                        revisions.source_blob_hash,
                        memory_state_history.memory_id
                 FROM memory_state_history
                 JOIN revisions
                   ON revisions.revision_id = memory_state_history.head_revision_id
                  AND revisions.memory_id = memory_state_history.memory_id
                 WHERE memory_state_history.space_id = ?1
                   AND memory_state_history.lifecycle = 'active'
                   AND memory_state_history.valid_from_generation <= ?2
                   AND (memory_state_history.valid_to_generation IS NULL
                        OR ?2 < memory_state_history.valid_to_generation)
                 ORDER BY memory_state_history.memory_id",
            )?;
            let rows = statement.query_map(
                params![space_id.as_bytes().as_slice(), generation_value],
                |row| {
                    let memory_id = MemoryId::from_bytes(parse_fixed_bytes(
                        row.get::<_, Vec<u8>>(5)?,
                        "memory id",
                    )?);
                    memory_record_from_row(row, memory_id, generation_value)
                },
            )?;
            rows.map(|row| {
                let memory = row?;
                let revision = match read_revision(connection, memory.head_revision_id)? {
                    Ok(revision) => revision,
                    Err(error) => {
                        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(error)));
                    }
                };
                let source = cas
                    .get(memory.source_blob_hash)
                    .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
                Ok(MemoryRead {
                    memory,
                    revision,
                    source,
                })
            })
            .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(database_error)
    }
}

fn current_generation(connection: &Connection) -> rusqlite::Result<AuthorityGeneration> {
    connection
        .query_row(
            "SELECT generation FROM authority_generation WHERE id = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .and_then(authority_generation)
}

fn read_space_at(
    connection: &Connection,
    space_id: SpaceId,
    generation: AuthorityGeneration,
) -> rusqlite::Result<Result<SpaceRecord, MemoriaError>> {
    let generation = sqlite_generation(generation)?;
    let result = connection.query_row(
        "SELECT space_key, lifecycle, valid_from_generation
         FROM space_state_history
         WHERE space_id = ?1
           AND valid_from_generation <= ?2
           AND (valid_to_generation IS NULL OR ?2 < valid_to_generation)
         ORDER BY valid_from_generation DESC
         LIMIT 1",
        params![space_id.as_bytes().as_slice(), generation],
        |row| {
            let lifecycle = parse_space_lifecycle(row.get::<_, String>(1)?)?;
            let valid_from_generation = row.get::<_, i64>(2)?;
            Ok(SpaceRecord {
                space_id,
                space_key: row.get(0)?,
                lifecycle,
                generation: authority_generation(valid_from_generation)?,
            })
        },
    );

    match result {
        Ok(mut record) => {
            record.generation =
                AuthorityGeneration::new(u64::try_from(generation).map_err(|_| {
                    rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "authority generation is negative",
                    )))
                })?);
            Ok(Ok(record))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(Err(MemoriaError::NotFound {
            kind: "space",
            id: space_id.to_string(),
        })),
        Err(error) => Err(error),
    }
}

fn read_memory_at(
    connection: &Connection,
    memory_id: MemoryId,
    generation: AuthorityGeneration,
) -> rusqlite::Result<Result<MemoryRecord, MemoriaError>> {
    let generation = sqlite_generation(generation)?;
    let result = connection.query_row(
        "SELECT space_id,
                document_key,
                head_revision_id,
                lifecycle,
                source_blob_hash
         FROM memory_state_history
         JOIN revisions
           ON revisions.revision_id = memory_state_history.head_revision_id
          AND revisions.memory_id = memory_state_history.memory_id
         WHERE memory_state_history.memory_id = ?1
           AND valid_from_generation <= ?2
           AND (valid_to_generation IS NULL OR ?2 < valid_to_generation)
         ORDER BY valid_from_generation DESC
         LIMIT 1",
        params![memory_id.as_bytes().as_slice(), generation],
        |row| memory_record_from_row(row, memory_id, generation),
    );

    match result {
        Ok(record) => Ok(Ok(record)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(Err(MemoriaError::NotFound {
            kind: "memory",
            id: memory_id.to_string(),
        })),
        Err(error) => Err(error),
    }
}

fn memory_record_from_row(
    row: &Row<'_>,
    memory_id: MemoryId,
    generation: i64,
) -> rusqlite::Result<MemoryRecord> {
    let space_id = SpaceId::from_bytes(parse_fixed_bytes(row.get::<_, Vec<u8>>(0)?, "space id")?);
    let head_revision_id =
        RevisionId::from_bytes(parse_fixed_bytes(row.get::<_, Vec<u8>>(2)?, "revision id")?);
    let lifecycle = parse_memory_lifecycle(row.get::<_, String>(3)?)?;
    let source_blob_hash = row
        .get::<_, String>(4)?
        .parse::<SourceBlobHash>()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    let generation = u64::try_from(generation).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Integer,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "authority generation is negative",
            )),
        )
    })?;
    Ok(MemoryRecord {
        memory_id,
        space_id,
        document_key: row.get(1)?,
        head_revision_id,
        lifecycle,
        source_blob_hash,
        generation: AuthorityGeneration::new(generation),
    })
}

fn read_revision(
    connection: &Connection,
    revision_id: RevisionId,
) -> rusqlite::Result<Result<RevisionRecord, MemoriaError>> {
    let result = connection.query_row(
        "SELECT memory_id,
                source_blob_hash,
                semantic_intent,
                committed_generation,
                committed_at_unix_seconds,
                committed_at_subsec_nanos
         FROM revisions
         WHERE revision_id = ?1",
        params![revision_id.as_bytes().as_slice()],
        |row| {
            let memory_id =
                MemoryId::from_bytes(parse_fixed_bytes(row.get::<_, Vec<u8>>(0)?, "memory id")?);
            let source_blob_hash =
                row.get::<_, String>(1)?
                    .parse::<SourceBlobHash>()
                    .map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
            let semantic_intent =
                row.get::<_, String>(2)?
                    .parse()
                    .map_err(|error: MemoriaError| {
                        rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
            let committed_generation = authority_generation(row.get(3)?)?;
            let committed_at = memoria_types::Timestamp::from_parts(row.get(4)?, row.get(5)?)
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        5,
                        rusqlite::types::Type::Integer,
                        Box::new(error),
                    )
                })?;
            Ok(RevisionRecord {
                revision_id,
                memory_id,
                source_blob_hash,
                semantic_intent,
                parents: Vec::new(),
                committed_generation,
                committed_at,
            })
        },
    );

    let mut revision = match result {
        Ok(revision) => revision,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Ok(Err(MemoriaError::NotFound {
                kind: "revision",
                id: revision_id.to_string(),
            }));
        }
        Err(error) => return Err(error),
    };

    let mut parents = connection.prepare(
        "SELECT parent_revision_id
         FROM revision_parents
         WHERE revision_id = ?1
         ORDER BY parent_order",
    )?;
    revision.parents = parents
        .query_map(params![revision_id.as_bytes().as_slice()], |row| {
            Ok(RevisionId::from_bytes(parse_fixed_bytes(
                row.get::<_, Vec<u8>>(0)?,
                "revision id",
            )?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Ok(revision))
}

fn parse_space_lifecycle(value: String) -> rusqlite::Result<SpaceLifecycle> {
    match value.as_str() {
        "active" => Ok(SpaceLifecycle::Active),
        "retired" => Ok(SpaceLifecycle::Retired),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

fn parse_memory_lifecycle(value: String) -> rusqlite::Result<MemoryLifecycle> {
    match value.as_str() {
        "active" => Ok(MemoryLifecycle::Active),
        "retired" => Ok(MemoryLifecycle::Retired),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
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
