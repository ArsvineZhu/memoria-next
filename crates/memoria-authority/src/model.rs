use std::fmt;

use memoria_types::{AuthorityGeneration, SpaceId};
use rusqlite::{Transaction, params};

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
        self.transaction.execute(
            "INSERT INTO space_state_history (
                 space_id,
                 space_key,
                 lifecycle,
                 valid_from_generation
             ) VALUES (?1, ?2, 'active', ?3)",
            params![space_id.as_bytes().as_slice(), space_key, generation],
        )?;

        Ok(space_id)
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
