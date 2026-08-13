use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use memoria_types::{AuthorityGeneration, MemoriaError};
use rusqlite::{Connection, TransactionBehavior, params};

use crate::model::{
    AuthorityTransaction, AuthorityWriteResult, authority_generation, database_error, schema_error,
    sqlite_conversion_error, sqlite_generation,
};
use crate::schema::{
    SCHEMA_V1, SCHEMA_V1_COLUMNS, SCHEMA_V1_TABLES, SCHEMA_V1_TRIGGERS, SCHEMA_V1_VERSION,
};

#[derive(Clone, Debug)]
pub struct AuthorityDb {
    path: PathBuf,
}

impl AuthorityDb {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut connection = Connection::open(&path)?;
        configure_connection(&connection)?;
        initialize_schema(&mut connection)?;
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

    pub(crate) fn write_memoria<F, T>(
        &self,
        operation: F,
    ) -> Result<AuthorityWriteResult<T>, MemoriaError>
    where
        F: for<'tx> FnOnce(&mut AuthorityTransaction<'tx>) -> Result<T, MemoriaError>,
    {
        let mut connection = self.open_connection().map_err(database_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(database_error)?;
        let base_generation = transaction
            .query_row(
                "SELECT generation FROM authority_generation WHERE id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .and_then(authority_generation)
            .map_err(database_error)?;
        let next_generation =
            base_generation
                .checked_next()
                .ok_or_else(|| MemoriaError::Database {
                    message: "authority generation exhausted".to_owned(),
                })?;
        let mut transaction =
            AuthorityTransaction::new(transaction, base_generation, next_generation);

        let value = operation(&mut transaction)?;
        let base_generation_value = sqlite_generation(base_generation).map_err(database_error)?;
        let next_generation_value = sqlite_generation(next_generation).map_err(database_error)?;
        let updated = transaction
            .transaction
            .execute(
                "UPDATE authority_generation
                 SET generation = ?1
                 WHERE id = 1 AND generation = ?2",
                params![next_generation_value, base_generation_value],
            )
            .map_err(database_error)?;
        if updated != 1 {
            return Err(MemoriaError::Database {
                message: "authority generation compare-and-swap failed".to_owned(),
            });
        }
        transaction.transaction.commit().map_err(database_error)?;

        Ok(AuthorityWriteResult::new(value, next_generation))
    }

    pub(crate) fn read<F, T>(&self, operation: F) -> rusqlite::Result<T>
    where
        F: FnOnce(&Connection) -> rusqlite::Result<T>,
    {
        let connection = self.open_connection()?;
        operation(&connection)
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

fn initialize_schema(connection: &mut Connection) -> rusqlite::Result<()> {
    match read_user_version(connection)? {
        0 if !has_user_objects(connection)? => {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch(SCHEMA_V1)?;
            transaction.pragma_update(None, "user_version", SCHEMA_V1_VERSION)?;
            validate_schema_v1(&transaction)?;
            transaction.commit()
        }
        0 => Err(schema_error(
            "cannot initialize Authority schema over an existing unversioned database",
        )),
        SCHEMA_V1_VERSION => validate_schema_v1(connection),
        version => Err(schema_error(format!(
            "unsupported Authority schema user_version {version}; expected {SCHEMA_V1_VERSION}"
        ))),
    }
}

fn read_user_version(connection: &Connection) -> rusqlite::Result<i64> {
    connection.query_row("PRAGMA user_version", [], |row| row.get(0))
}

fn has_user_objects(connection: &Connection) -> rusqlite::Result<bool> {
    connection.query_row(
        "SELECT EXISTS (
             SELECT 1
             FROM sqlite_master
             WHERE type IN ('table', 'index', 'trigger', 'view')
               AND name NOT LIKE 'sqlite_%'
         )",
        [],
        |row| row.get(0),
    )
}

fn validate_schema_v1(connection: &Connection) -> rusqlite::Result<()> {
    let actual_tables = schema_object_names(connection, "table")?;
    let expected_tables = SCHEMA_V1_TABLES
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<BTreeSet<_>>();
    if actual_tables != expected_tables {
        return Err(schema_error(format!(
            "Authority schema tables do not match V1: expected {expected_tables:?}, found {actual_tables:?}"
        )));
    }

    for (table, expected_columns) in SCHEMA_V1_COLUMNS {
        let sql = format!("PRAGMA table_info(\"{table}\")");
        let mut statement = connection.prepare(&sql)?;
        let actual_columns = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let expected_columns = expected_columns
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>();
        if actual_columns != expected_columns {
            return Err(schema_error(format!(
                "Authority table {table} columns do not match V1: expected {expected_columns:?}, found {actual_columns:?}"
            )));
        }
    }

    let actual_triggers = schema_object_names(connection, "trigger")?;
    let expected_triggers = SCHEMA_V1_TRIGGERS
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<BTreeSet<_>>();
    if actual_triggers != expected_triggers {
        return Err(schema_error(format!(
            "Authority schema triggers do not match V1: expected {expected_triggers:?}, found {actual_triggers:?}"
        )));
    }

    let expected_schema = expected_schema_catalog()?;
    let actual_schema = schema_catalog(connection)?;
    if actual_schema != expected_schema {
        return Err(schema_error(format!(
            "Authority schema definitions do not match V1: expected {expected_schema:?}, found {actual_schema:?}"
        )));
    }

    let schema_version = connection.query_row(
        "SELECT meta_value FROM store_meta WHERE meta_key = 'schema_version'",
        [],
        |row| row.get::<_, String>(0),
    )?;
    if schema_version != SCHEMA_V1_VERSION.to_string() {
        return Err(schema_error(format!(
            "store_meta schema_version is {schema_version:?}, expected {:?}",
            SCHEMA_V1_VERSION.to_string()
        )));
    }

    let generation = connection.query_row(
        "SELECT generation FROM authority_generation WHERE id = 1",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    if generation < 0 {
        return Err(schema_error(
            "authority_generation contains a negative generation",
        ));
    }

    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct SchemaObject {
    object_type: String,
    name: String,
    table_name: String,
    sql: Option<String>,
}

fn expected_schema_catalog() -> rusqlite::Result<Vec<SchemaObject>> {
    let connection = Connection::open_in_memory()?;
    configure_connection(&connection)?;
    connection.execute_batch(SCHEMA_V1)?;
    schema_catalog(&connection)
}

fn schema_catalog(connection: &Connection) -> rusqlite::Result<Vec<SchemaObject>> {
    let mut statement = connection.prepare(
        "SELECT type, name, tbl_name, sql
         FROM sqlite_master
         WHERE name NOT LIKE 'sqlite_%'
         ORDER BY type, name",
    )?;
    statement
        .query_map([], |row| {
            Ok(SchemaObject {
                object_type: row.get(0)?,
                name: row.get(1)?,
                table_name: row.get(2)?,
                sql: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
}

fn schema_object_names(
    connection: &Connection,
    object_type: &str,
) -> rusqlite::Result<BTreeSet<String>> {
    let mut statement = connection.prepare(
        "SELECT name
         FROM sqlite_master
         WHERE type = ?1 AND name NOT LIKE 'sqlite_%'
         ORDER BY name",
    )?;
    statement
        .query_map([object_type], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<BTreeSet<_>>>()
}
