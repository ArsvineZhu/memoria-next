use std::path::{Path, PathBuf};

use memoria_types::AuthorityGeneration;
use rusqlite::{Connection, TransactionBehavior, params};

use crate::model::{
    AuthorityTransaction, AuthorityWriteResult, authority_generation, sqlite_conversion_error,
    sqlite_generation,
};
use crate::schema::SCHEMA_V1;

#[derive(Clone, Debug)]
pub struct AuthorityDb {
    path: PathBuf,
}

impl AuthorityDb {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let connection = Connection::open(&path)?;
        configure_connection(&connection)?;
        connection.execute_batch(SCHEMA_V1)?;
        Ok(Self { path })
    }

    pub fn current_generation(&self) -> rusqlite::Result<AuthorityGeneration> {
        let connection = self.open_connection()?;
        connection
            .query_row(
                "SELECT generation FROM authority_generation WHERE id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .and_then(authority_generation)
    }

    pub fn write<F, T>(&self, operation: F) -> rusqlite::Result<AuthorityWriteResult<T>>
    where
        F: for<'tx> FnOnce(&mut AuthorityTransaction<'tx>) -> rusqlite::Result<T>,
    {
        let mut connection = self.open_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let base_generation = transaction
            .query_row(
                "SELECT generation FROM authority_generation WHERE id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .and_then(authority_generation)?;
        let next_generation = base_generation.checked_next().ok_or_else(|| {
            sqlite_conversion_error(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "authority generation exhausted",
            ))
        })?;
        let mut transaction =
            AuthorityTransaction::new(transaction, base_generation, next_generation);

        let value = operation(&mut transaction)?;
        let base_generation_value = sqlite_generation(base_generation)?;
        let next_generation_value = sqlite_generation(next_generation)?;
        let updated = transaction.transaction.execute(
            "UPDATE authority_generation
             SET generation = ?1
             WHERE id = 1 AND generation = ?2",
            params![next_generation_value, base_generation_value],
        )?;
        if updated != 1 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        transaction.transaction.commit()?;

        Ok(AuthorityWriteResult::new(value, next_generation))
    }

    fn open_connection(&self) -> rusqlite::Result<Connection> {
        let connection = Connection::open(&self.path)?;
        configure_connection(&connection)?;
        Ok(connection)
    }
}

fn configure_connection(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch("PRAGMA foreign_keys = ON;")
}
