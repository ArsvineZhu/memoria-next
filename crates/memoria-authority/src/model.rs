use std::fmt;

use memoria_types::{
    AuthorityGeneration, MemoriaError, MemoryId, RevisionId, RevisionSemanticIntent,
    SourceBlobHash, SpaceId, Timestamp,
};
use rusqlite::{OptionalExtension, Transaction, params};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpaceLifecycle {
    Active,
    Retired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryLifecycle {
    Active,
    Retired,
}

impl MemoryLifecycle {
    pub(crate) const fn as_sql(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Retired => "retired",
        }
    }
}

impl SpaceLifecycle {
    pub(crate) const fn as_sql(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Retired => "retired",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpaceRecord {
    pub space_id: SpaceId,
    pub space_key: String,
    pub lifecycle: SpaceLifecycle,
    pub generation: AuthorityGeneration,
}

impl SpaceRecord {
    #[must_use]
    pub fn id(&self) -> SpaceId {
        self.space_id
    }

    #[must_use]
    pub fn generation(&self) -> AuthorityGeneration {
        self.generation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryRecord {
    pub memory_id: MemoryId,
    pub space_id: SpaceId,
    pub document_key: Option<String>,
    pub head_revision_id: RevisionId,
    pub lifecycle: MemoryLifecycle,
    pub source_blob_hash: SourceBlobHash,
    pub generation: AuthorityGeneration,
}

impl MemoryRecord {
    #[must_use]
    pub fn memory_id(&self) -> MemoryId {
        self.memory_id
    }

    #[must_use]
    pub fn revision_id(&self) -> RevisionId {
        self.head_revision_id
    }

    #[must_use]
    pub fn head_revision_id(&self) -> RevisionId {
        self.head_revision_id
    }

    #[must_use]
    pub fn source_blob_hash(&self) -> SourceBlobHash {
        self.source_blob_hash
    }

    #[must_use]
    pub fn generation(&self) -> AuthorityGeneration {
        self.generation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevisionRecord {
    pub revision_id: RevisionId,
    pub memory_id: MemoryId,
    pub source_blob_hash: SourceBlobHash,
    pub semantic_intent: RevisionSemanticIntent,
    pub parents: Vec<RevisionId>,
    pub committed_generation: AuthorityGeneration,
    pub committed_at: Timestamp,
}

impl RevisionRecord {
    #[must_use]
    pub fn revision_id(&self) -> RevisionId {
        self.revision_id
    }

    #[must_use]
    pub fn parent_revision_ids(&self) -> &[RevisionId] {
        &self.parents
    }
}

#[derive(Debug)]
pub struct AuthorityWriteResult<T> {
    value: T,
    generation: AuthorityGeneration,
}

impl<T> AuthorityWriteResult<T> {
    pub(crate) fn new(value: T, generation: AuthorityGeneration) -> Self {
        Self { value, generation }
    }

    #[must_use]
    pub fn generation(&self) -> AuthorityGeneration {
        self.generation
    }

    #[must_use]
    pub fn value(&self) -> &T {
        &self.value
    }

    #[must_use]
    pub fn into_value(self) -> T {
        self.value
    }
}

pub(crate) enum AuthorityWriteAction<T> {
    Commit(T),
    Noop(AuthorityWriteResult<T>),
}

pub struct AuthorityTransaction<'tx> {
    pub(crate) transaction: Transaction<'tx>,
    base_generation: AuthorityGeneration,
    generation: AuthorityGeneration,
}

impl<'tx> AuthorityTransaction<'tx> {
    pub(crate) fn new(
        transaction: Transaction<'tx>,
        base_generation: AuthorityGeneration,
        generation: AuthorityGeneration,
    ) -> Self {
        Self {
            transaction,
            base_generation,
            generation,
        }
    }

    #[must_use]
    pub fn base_generation(&self) -> AuthorityGeneration {
        self.base_generation
    }

    #[must_use]
    pub fn generation(&self) -> AuthorityGeneration {
        self.generation
    }

    pub fn create_space_record(&mut self, space_key: impl AsRef<str>) -> rusqlite::Result<SpaceId> {
        let space_id = SpaceId::try_new().map_err(sqlite_conversion_error)?;
        let space_key = space_key.as_ref();
        let generation = sqlite_generation(self.generation)?;

        self.transaction.execute(
            "INSERT INTO spaces (space_id, created_generation) VALUES (?1, ?2)",
            params![space_id.as_bytes().as_slice(), generation],
        )?;
        self.insert_space_state(
            space_id,
            space_key,
            None,
            None,
            SpaceLifecycle::Active,
            generation,
        )?;

        Ok(space_id)
    }

    pub(crate) fn insert_space_state(
        &mut self,
        space_id: SpaceId,
        space_key: &str,
        display_name: Option<&str>,
        description: Option<&str>,
        lifecycle: SpaceLifecycle,
        valid_from_generation: i64,
    ) -> rusqlite::Result<()> {
        self.transaction.execute(
            "INSERT INTO space_state_history (
                 space_id,
                 space_key,
                 display_name,
                 description,
                 lifecycle,
                 valid_from_generation
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                space_id.as_bytes().as_slice(),
                space_key,
                display_name,
                description,
                lifecycle.as_sql(),
                valid_from_generation,
            ],
        )?;
        Ok(())
    }

    pub(crate) fn close_current_space_state(
        &mut self,
        space_id: SpaceId,
        valid_to_generation: i64,
    ) -> rusqlite::Result<()> {
        let updated = self.transaction.execute(
            "UPDATE space_state_history
             SET valid_to_generation = ?1
             WHERE space_id = ?2 AND valid_to_generation IS NULL",
            params![valid_to_generation, space_id.as_bytes().as_slice()],
        )?;
        if updated != 1 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(())
    }

    pub(crate) fn replace_space_state(
        &mut self,
        space_id: SpaceId,
        space_key: &str,
        display_name: Option<&str>,
        description: Option<&str>,
        lifecycle: SpaceLifecycle,
        valid_from_generation: i64,
    ) -> rusqlite::Result<()> {
        let current_generation = self
            .transaction
            .query_row(
                "SELECT valid_from_generation
                 FROM space_state_history
                 WHERE space_id = ?1 AND valid_to_generation IS NULL
                 LIMIT 1",
                params![space_id.as_bytes().as_slice()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;

        if current_generation == Some(valid_from_generation) {
            let updated = self.transaction.execute(
                "UPDATE space_state_history
                 SET space_key = ?1,
                     display_name = ?2,
                     description = ?3,
                     lifecycle = ?4
                 WHERE space_id = ?5 AND valid_from_generation = ?6
                   AND valid_to_generation IS NULL",
                params![
                    space_key,
                    display_name,
                    description,
                    lifecycle.as_sql(),
                    space_id.as_bytes().as_slice(),
                    valid_from_generation,
                ],
            )?;
            if updated != 1 {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            return Ok(());
        }

        self.close_current_space_state(space_id, valid_from_generation)?;
        self.insert_space_state(
            space_id,
            space_key,
            display_name,
            description,
            lifecycle,
            valid_from_generation,
        )
    }

    pub(crate) fn insert_memory_record(
        &mut self,
        record: &MemoryRecord,
        semantic_intent: RevisionSemanticIntent,
        committed_at: Timestamp,
    ) -> rusqlite::Result<()> {
        let generation = sqlite_generation(self.generation)?;
        self.transaction.execute(
            "INSERT INTO memories (memory_id, created_generation) VALUES (?1, ?2)",
            params![record.memory_id.as_bytes().as_slice(), generation],
        )?;
        self.insert_revision_record(
            record.memory_id,
            record.head_revision_id,
            record.source_blob_hash,
            semantic_intent,
            committed_at,
            &[],
        )?;
        self.insert_memory_state(
            record.memory_id,
            record.space_id,
            record.document_key.as_deref(),
            record.head_revision_id,
            MemoryLifecycle::Active,
            generation,
        )
    }

    pub(crate) fn insert_revision_record(
        &mut self,
        memory_id: MemoryId,
        revision_id: RevisionId,
        source_blob_hash: SourceBlobHash,
        semantic_intent: RevisionSemanticIntent,
        committed_at: Timestamp,
        parents: &[RevisionId],
    ) -> rusqlite::Result<()> {
        let generation = sqlite_generation(self.generation)?;
        self.transaction.execute(
            "INSERT INTO revisions (
                 revision_id,
                 memory_id,
                 source_blob_hash,
                 semantic_intent,
                 committed_generation,
                 committed_at_unix_seconds,
                 committed_at_subsec_nanos
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                revision_id.as_bytes().as_slice(),
                memory_id.as_bytes().as_slice(),
                source_blob_hash.to_string(),
                semantic_intent.to_string(),
                generation,
                committed_at.unix_seconds(),
                i64::from(committed_at.subsec_nanos()),
            ],
        )?;

        for (parent_order, parent_revision_id) in parents.iter().enumerate() {
            self.transaction.execute(
                "INSERT INTO revision_parents (
                     revision_id,
                     parent_revision_id,
                     parent_order
                 ) VALUES (?1, ?2, ?3)",
                params![
                    revision_id.as_bytes().as_slice(),
                    parent_revision_id.as_bytes().as_slice(),
                    i64::try_from(parent_order).map_err(sqlite_conversion_error)?,
                ],
            )?;
        }

        Ok(())
    }

    pub(crate) fn insert_memory_state(
        &mut self,
        memory_id: MemoryId,
        space_id: SpaceId,
        document_key: Option<&str>,
        head_revision_id: RevisionId,
        lifecycle: MemoryLifecycle,
        valid_from_generation: i64,
    ) -> rusqlite::Result<()> {
        self.transaction.execute(
            "INSERT INTO memory_state_history (
                 memory_id,
                 space_id,
                 document_key,
                 head_revision_id,
                 lifecycle,
                 valid_from_generation
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                memory_id.as_bytes().as_slice(),
                space_id.as_bytes().as_slice(),
                document_key,
                head_revision_id.as_bytes().as_slice(),
                lifecycle.as_sql(),
                valid_from_generation,
            ],
        )?;
        Ok(())
    }

    pub(crate) fn close_current_memory_state(
        &mut self,
        memory_id: MemoryId,
        valid_to_generation: i64,
    ) -> rusqlite::Result<()> {
        let updated = self.transaction.execute(
            "UPDATE memory_state_history
             SET valid_to_generation = ?1
             WHERE memory_id = ?2 AND valid_to_generation IS NULL",
            params![valid_to_generation, memory_id.as_bytes().as_slice()],
        )?;
        if updated != 1 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(())
    }

    pub(crate) fn replace_memory_state(
        &mut self,
        memory_id: MemoryId,
        space_id: SpaceId,
        document_key: Option<&str>,
        head_revision_id: RevisionId,
        lifecycle: MemoryLifecycle,
        valid_from_generation: i64,
    ) -> rusqlite::Result<()> {
        let current_generation = self
            .transaction
            .query_row(
                "SELECT valid_from_generation
                 FROM memory_state_history
                 WHERE memory_id = ?1 AND valid_to_generation IS NULL
                 LIMIT 1",
                params![memory_id.as_bytes().as_slice()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;

        if current_generation == Some(valid_from_generation) {
            let updated = self.transaction.execute(
                "UPDATE memory_state_history
                 SET space_id = ?1,
                     document_key = ?2,
                     head_revision_id = ?3,
                     lifecycle = ?4
                 WHERE memory_id = ?5 AND valid_from_generation = ?6
                   AND valid_to_generation IS NULL",
                params![
                    space_id.as_bytes().as_slice(),
                    document_key,
                    head_revision_id.as_bytes().as_slice(),
                    lifecycle.as_sql(),
                    memory_id.as_bytes().as_slice(),
                    valid_from_generation,
                ],
            )?;
            if updated != 1 {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            return Ok(());
        }

        self.close_current_memory_state(memory_id, valid_from_generation)?;
        self.insert_memory_state(
            memory_id,
            space_id,
            document_key,
            head_revision_id,
            lifecycle,
            valid_from_generation,
        )
    }

    pub(crate) fn insert_idempotency_record(
        &mut self,
        idempotency_key: &str,
        request_fingerprint: &str,
        result_kind: &str,
        result_id: &[u8],
    ) -> rusqlite::Result<()> {
        self.transaction.execute(
            "INSERT INTO idempotency_records (
                 idempotency_key,
                 request_fingerprint,
                 result_kind,
                 result_id,
                 committed_generation
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                idempotency_key,
                request_fingerprint,
                result_kind,
                result_id,
                sqlite_generation(self.generation)?,
            ],
        )?;
        Ok(())
    }
}

pub(crate) fn sqlite_generation(generation: AuthorityGeneration) -> rusqlite::Result<i64> {
    i64::try_from(generation.value()).map_err(|_| {
        sqlite_conversion_error(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "authority generation exceeds SQLite INTEGER range",
        ))
    })
}

pub(crate) fn database_error(error: rusqlite::Error) -> MemoriaError {
    MemoriaError::Database {
        message: error.to_string(),
    }
}

pub(crate) fn authority_generation(value: i64) -> rusqlite::Result<AuthorityGeneration> {
    u64::try_from(value)
        .map(AuthorityGeneration::new)
        .map_err(|_| {
            sqlite_conversion_error(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "authority generation is negative",
            ))
        })
}

pub(crate) fn sqlite_conversion_error(
    error: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
}

#[derive(Debug)]
struct SchemaError(String);

impl fmt::Display for SchemaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SchemaError {}

pub(crate) fn schema_error(message: impl Into<String>) -> rusqlite::Error {
    sqlite_conversion_error(SchemaError(message.into()))
}
