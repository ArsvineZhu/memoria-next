use std::path::{Path, PathBuf};
use std::{collections::BTreeSet, fs, time::Duration};

use memoria_types::AuthorityGeneration;
use rusqlite::{Connection, OptionalExtension, params};

use crate::artifact::{ArtifactDescriptor, ArtifactId, ArtifactState};
use crate::gc::DerivedGc;
use crate::lease::{ManifestLease, unix_now};
use crate::manifest::{DerivedManifest, ManifestId};
use crate::{DerivedError, generation_to_sql};

pub struct DerivedCatalog {
    connection: Connection,
    database_path: PathBuf,
}

impl DerivedCatalog {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DerivedError> {
        let database_path = path.as_ref().to_path_buf();
        if let Some(parent) = database_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(&database_path)?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS artifacts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL,
                version INTEGER NOT NULL,
                authority_generation INTEGER NOT NULL,
                state TEXT NOT NULL CHECK (state IN (
                    'planned', 'building', 'staged', 'validated', 'published',
                    'failed', 'superseded', 'collected'
                ))
            );
            CREATE TABLE IF NOT EXISTS artifact_dependencies (
                artifact_id INTEGER NOT NULL REFERENCES artifacts(id),
                dependency_artifact_id INTEGER NOT NULL REFERENCES artifacts(id),
                PRIMARY KEY (artifact_id, dependency_artifact_id)
            );
            CREATE TABLE IF NOT EXISTS manifests (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                authority_generation INTEGER NOT NULL,
                immutable INTEGER NOT NULL DEFAULT 1 CHECK (immutable = 1)
            );
            CREATE TABLE IF NOT EXISTS manifest_artifacts (
                manifest_id INTEGER NOT NULL REFERENCES manifests(id),
                artifact_id INTEGER NOT NULL REFERENCES artifacts(id),
                PRIMARY KEY (manifest_id, artifact_id)
            );
            CREATE TABLE IF NOT EXISTS manifest_capabilities (
                manifest_id INTEGER NOT NULL REFERENCES manifests(id),
                capability TEXT NOT NULL,
                PRIMARY KEY (manifest_id, capability)
            );
            CREATE TABLE IF NOT EXISTS build_jobs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                artifact_id INTEGER REFERENCES artifacts(id),
                state TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS leases (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                manifest_id INTEGER NOT NULL REFERENCES manifests(id),
                expires_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS serving_pointer (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                manifest_id INTEGER REFERENCES manifests(id)
            );
            INSERT OR IGNORE INTO serving_pointer(singleton, manifest_id)
                VALUES (1, NULL);
            ",
        )?;
        Ok(Self {
            connection,
            database_path,
        })
    }

    pub fn stage_artifact(
        &mut self,
        kind: impl AsRef<str>,
        version: u32,
        authority_generation: AuthorityGeneration,
    ) -> Result<ArtifactDescriptor, DerivedError> {
        self.connection.execute(
            "INSERT INTO artifacts(kind, version, authority_generation, state)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                kind.as_ref(),
                i64::from(version),
                generation_to_sql(authority_generation)?,
                ArtifactState::Staged.as_str(),
            ],
        )?;
        self.artifact(ArtifactId::from_raw(self.connection.last_insert_rowid()))
    }

    pub fn validate_artifact(
        &mut self,
        id: ArtifactId,
    ) -> Result<ArtifactDescriptor, DerivedError> {
        let changed = self.connection.execute(
            "UPDATE artifacts SET state = ?1 WHERE id = ?2 AND state IN ('staged', 'building')",
            params![ArtifactState::Validated.as_str(), id.value()],
        )?;
        if changed == 0 {
            let artifact = self.artifact(id)?;
            if artifact.state() == ArtifactState::Validated {
                return Ok(artifact);
            }
            return Err(DerivedError::InvalidArtifactTransition {
                id,
                state: artifact.state(),
            });
        }
        self.artifact(id)
    }

    pub fn artifact(&self, id: ArtifactId) -> Result<ArtifactDescriptor, DerivedError> {
        self.connection
            .query_row(
                "SELECT id, kind, version, authority_generation, state
                 FROM artifacts WHERE id = ?1",
                params![id.value()],
                |row| {
                    let version = row.get::<_, i64>(2)?;
                    let version = u32::try_from(version).map_err(|_| {
                        rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Integer,
                            Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "artifact version is out of range",
                            )),
                        )
                    })?;
                    let generation = row.get::<_, i64>(3)?;
                    let generation = u64::try_from(generation).map_err(|_| {
                        rusqlite::Error::FromSqlConversionFailure(
                            3,
                            rusqlite::types::Type::Integer,
                            Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "authority generation is out of range",
                            )),
                        )
                    })?;
                    let state =
                        ArtifactState::parse(&row.get::<_, String>(4)?).map_err(|error| {
                            rusqlite::Error::ToSqlConversionFailure(Box::new(error))
                        })?;
                    Ok(ArtifactDescriptor::new(
                        ArtifactId::from_raw(row.get(0)?),
                        row.get(1)?,
                        version,
                        AuthorityGeneration::new(generation),
                        state,
                    ))
                },
            )
            .optional()?
            .ok_or(DerivedError::ArtifactNotFound { id })
    }

    pub fn publish_manifest(
        &mut self,
        artifact_ids: Vec<ArtifactId>,
    ) -> Result<DerivedManifest, DerivedError> {
        let generation = if let Some(id) = artifact_ids.first() {
            self.artifact(*id)?.authority_generation()
        } else {
            AuthorityGeneration::initial()
        };
        self.publish_manifest_at_generation(artifact_ids, generation, Vec::new())
    }

    pub fn publish_manifest_at_generation(
        &mut self,
        artifact_ids: Vec<ArtifactId>,
        generation: AuthorityGeneration,
        capabilities: Vec<String>,
    ) -> Result<DerivedManifest, DerivedError> {
        let transaction = self.connection.transaction()?;
        let mut kinds = BTreeSet::new();
        for id in &artifact_ids {
            let (state, artifact_generation, kind) = transaction
                .query_row(
                    "SELECT state, authority_generation, kind FROM artifacts WHERE id = ?1",
                    params![id.value()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()?
                .ok_or(DerivedError::ArtifactNotFound { id: *id })?;
            let state = ArtifactState::parse(&state)?;
            if !state.can_publish() {
                return Err(DerivedError::ArtifactNotValidated { id: *id, state });
            }
            let artifact_generation = u64::try_from(artifact_generation).map_err(|_| {
                DerivedError::InvalidGeneration {
                    value: artifact_generation,
                }
            })?;
            let artifact_generation = AuthorityGeneration::new(artifact_generation);
            if artifact_generation != generation {
                return Err(DerivedError::ArtifactGenerationMismatch {
                    id: *id,
                    expected: generation,
                    actual: artifact_generation,
                });
            }
            kinds.insert(kind);
        }
        let capabilities = if capabilities.is_empty() {
            capabilities_for_kinds(&kinds)
        } else {
            capabilities
        };
        transaction.execute(
            "INSERT INTO manifests(authority_generation) VALUES (?1)",
            params![generation_to_sql(generation)?],
        )?;
        let manifest_id = ManifestId::from_raw(transaction.last_insert_rowid());
        for id in &artifact_ids {
            transaction.execute(
                "INSERT INTO manifest_artifacts(manifest_id, artifact_id) VALUES (?1, ?2)",
                params![manifest_id.value(), id.value()],
            )?;
            transaction.execute(
                "UPDATE artifacts SET state = ?1 WHERE id = ?2",
                params![ArtifactState::Published.as_str(), id.value()],
            )?;
        }
        for capability in &capabilities {
            transaction.execute(
                "INSERT INTO manifest_capabilities(manifest_id, capability)
                 VALUES (?1, ?2)",
                params![manifest_id.value(), capability],
            )?;
        }
        transaction.execute(
            "UPDATE serving_pointer SET manifest_id = ?1 WHERE singleton = 1",
            params![manifest_id.value()],
        )?;
        transaction.commit()?;
        self.manifest(manifest_id)
    }

    pub fn publish_empty_manifest(
        &mut self,
        generation: AuthorityGeneration,
    ) -> Result<DerivedManifest, DerivedError> {
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO manifests(authority_generation) VALUES (?1)",
            params![generation_to_sql(generation)?],
        )?;
        let id = ManifestId::from_raw(transaction.last_insert_rowid());
        transaction.execute(
            "UPDATE serving_pointer SET manifest_id = ?1 WHERE singleton = 1",
            params![id.value()],
        )?;
        transaction.commit()?;
        self.manifest(id)
    }

    pub fn replace_manifest_artifacts(
        &mut self,
        id: ManifestId,
        _artifact_ids: Vec<ArtifactId>,
    ) -> Result<(), DerivedError> {
        let exists = self
            .connection
            .query_row(
                "SELECT 1 FROM manifests WHERE id = ?1",
                params![id.value()],
                |_| Ok(()),
            )
            .optional()?;
        if exists.is_none() {
            return Err(DerivedError::ManifestNotFound { id });
        }
        Err(DerivedError::ManifestImmutable { id })
    }

    pub fn manifest(&self, id: ManifestId) -> Result<DerivedManifest, DerivedError> {
        let generation = self
            .connection
            .query_row(
                "SELECT authority_generation FROM manifests WHERE id = ?1",
                params![id.value()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .ok_or(DerivedError::ManifestNotFound { id })?;
        let generation = u64::try_from(generation)
            .map_err(|_| DerivedError::InvalidGeneration { value: generation })?;
        let mut statement = self.connection.prepare(
            "SELECT artifact_id FROM manifest_artifacts
             WHERE manifest_id = ?1 ORDER BY artifact_id",
        )?;
        let artifacts = statement
            .query_map(params![id.value()], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(ArtifactId::from_raw)
            .collect();
        let mut statement = self.connection.prepare(
            "SELECT capability FROM manifest_capabilities
             WHERE manifest_id = ?1 ORDER BY capability",
        )?;
        let capabilities = statement
            .query_map(params![id.value()], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DerivedManifest::new(
            id,
            AuthorityGeneration::new(generation),
            artifacts,
            capabilities,
        ))
    }

    pub fn serving_manifest(&self) -> Result<Option<DerivedManifest>, DerivedError> {
        let id = self.connection.query_row(
            "SELECT manifest_id FROM serving_pointer WHERE singleton = 1",
            [],
            |row| row.get::<_, Option<i64>>(0),
        )?;
        id.map(|id| self.manifest(ManifestId::from_raw(id)))
            .transpose()
    }

    pub fn acquire_lease(
        &mut self,
        manifest_id: ManifestId,
        ttl: Duration,
    ) -> Result<ManifestLease, DerivedError> {
        self.manifest(manifest_id)?;
        let ttl_seconds =
            i64::try_from(ttl.as_secs()).map_err(|_| DerivedError::LeaseDuration {
                seconds: ttl.as_secs(),
            })?;
        let expires_at =
            unix_now()
                .checked_add(ttl_seconds)
                .ok_or(DerivedError::LeaseDuration {
                    seconds: ttl.as_secs(),
                })?;
        self.connection.execute(
            "INSERT INTO leases(manifest_id, expires_at) VALUES (?1, ?2)",
            params![manifest_id.value(), expires_at],
        )?;
        let id = self.connection.last_insert_rowid();
        Ok(ManifestLease::new(
            id,
            manifest_id,
            self.database_path.clone(),
            expires_at,
        ))
    }

    pub fn gc(&mut self) -> DerivedGc<'_> {
        DerivedGc::new(self)
    }

    pub fn delete_all_derived(&mut self) -> Result<(), DerivedError> {
        let transaction = self.connection.transaction()?;
        transaction.execute_batch(
            "
            DELETE FROM manifest_capabilities;
            DELETE FROM manifest_artifacts;
            DELETE FROM leases;
            UPDATE serving_pointer SET manifest_id = NULL WHERE singleton = 1;
            DELETE FROM manifests;
            DELETE FROM artifact_dependencies;
            DELETE FROM build_jobs;
            DELETE FROM artifacts;
            ",
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn connection_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }
}

fn capabilities_for_kinds(kinds: &BTreeSet<String>) -> Vec<String> {
    let mut capabilities = Vec::new();
    if kinds.contains("lexical") {
        capabilities.push("lexical".to_owned());
    }
    if kinds.contains("structural")
        && kinds.contains("temporal")
        && kinds.contains("relations")
        && kinds.contains("entity_observations")
        && kinds.contains("explicit_tags")
    {
        capabilities.push("structured".to_owned());
    }
    if [
        "ir",
        "structural",
        "temporal",
        "relations",
        "entity_observations",
        "explicit_tags",
        "lexical",
    ]
    .iter()
    .all(|kind| kinds.contains(*kind))
    {
        capabilities.push("base-search".to_owned());
    }
    capabilities
}
