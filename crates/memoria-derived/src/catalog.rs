use std::path::{Path, PathBuf};
use std::{collections::BTreeSet, fs, time::Duration};

use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};
use rusqlite::{Connection, OptionalExtension, params};

use crate::artifact::{ArtifactDescriptor, ArtifactId, ArtifactState, BuildJob, BuildJobState};
use crate::gc::DerivedGc;
use crate::lease::{ManifestLease, unix_now};
use crate::manifest::{DerivedManifest, ManifestId};
use crate::{
    DerivedError, EmbeddingNormalization, ProjectionInputHash, VectorPayloadHash, generation_to_sql,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VectorPayloadRecord {
    pub payload_hash: VectorPayloadHash,
    pub dimension: u32,
    pub normalization: EmbeddingNormalization,
    pub producer_signature: String,
    pub projection_input_hash: ProjectionInputHash,
    pub object_path: String,
    pub byte_length: u64,
    pub checksum: [u8; 32],
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VectorMembershipRecord {
    pub membership_id: i64,
    pub artifact_id: ArtifactId,
    pub space_id: SpaceId,
    pub memory_id: MemoryId,
    pub revision_id: RevisionId,
    pub derived_unit_id: String,
    pub semantic_node_id: Option<String>,
    pub resolution: String,
    pub payload_hash: VectorPayloadHash,
    pub live_from_artifact_generation: AuthorityGeneration,
    pub live_to_artifact_generation: Option<AuthorityGeneration>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnSegmentRecord {
    pub segment_id: i64,
    pub artifact_id: ArtifactId,
    pub object_hash: [u8; 32],
    pub vector_count: u64,
    pub dimension: u32,
    pub producer_signature: String,
    pub created_at: i64,
}

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
            CREATE TABLE IF NOT EXISTS vector_payloads (
                payload_hash BLOB PRIMARY KEY CHECK (length(payload_hash) = 32),
                dimension INTEGER NOT NULL CHECK (dimension > 0),
                normalization INTEGER NOT NULL CHECK (normalization IN (0, 1)),
                producer_signature TEXT NOT NULL,
                projection_input_hash BLOB NOT NULL CHECK (length(projection_input_hash) = 32),
                object_path TEXT NOT NULL,
                byte_length INTEGER NOT NULL CHECK (byte_length > 0),
                checksum BLOB NOT NULL CHECK (length(checksum) = 32),
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS vector_memberships (
                membership_id INTEGER PRIMARY KEY AUTOINCREMENT,
                artifact_id INTEGER NOT NULL REFERENCES artifacts(id),
                space_id BLOB NOT NULL CHECK (length(space_id) = 16),
                memory_id BLOB NOT NULL CHECK (length(memory_id) = 16),
                revision_id BLOB NOT NULL CHECK (length(revision_id) = 32),
                derived_unit_id TEXT NOT NULL,
                semantic_node_id TEXT,
                resolution TEXT NOT NULL,
                payload_hash BLOB NOT NULL REFERENCES vector_payloads(payload_hash),
                live_from_artifact_generation INTEGER NOT NULL,
                live_to_artifact_generation INTEGER,
                UNIQUE (artifact_id, space_id, memory_id, revision_id, derived_unit_id, resolution)
            );
            CREATE TABLE IF NOT EXISTS ann_segments (
                segment_id INTEGER PRIMARY KEY AUTOINCREMENT,
                artifact_id INTEGER NOT NULL REFERENCES artifacts(id),
                object_hash BLOB NOT NULL CHECK (length(object_hash) = 32),
                vector_count INTEGER NOT NULL CHECK (vector_count >= 0),
                dimension INTEGER NOT NULL CHECK (dimension > 0),
                producer_signature TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS ann_tombstones (
                artifact_id INTEGER NOT NULL REFERENCES artifacts(id),
                target_key TEXT NOT NULL,
                PRIMARY KEY (artifact_id, target_key)
            );
            CREATE TABLE IF NOT EXISTS build_jobs (
                job_id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                input_hash TEXT NOT NULL,
                producer_signature TEXT NOT NULL,
                authority_generation INTEGER NOT NULL,
                state TEXT NOT NULL CHECK (state IN (
                    'queued', 'running', 'succeeded', 'failed', 'superseded'
                )),
                attempt_count INTEGER NOT NULL CHECK (attempt_count >= 0),
                next_attempt_at INTEGER NOT NULL,
                last_error_code TEXT,
                last_error_message TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                UNIQUE (kind, input_hash, producer_signature)
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
        connection.execute(
            "UPDATE build_jobs SET state = 'queued', updated_at = ?1 WHERE state = 'running'",
            params![unix_now()],
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

    pub fn register_vector_payload(
        &mut self,
        record: &VectorPayloadRecord,
    ) -> Result<VectorPayloadRecord, DerivedError> {
        let dimension = i64::from(record.dimension);
        let byte_length = i64::try_from(record.byte_length)?;
        self.connection.execute(
            "INSERT OR IGNORE INTO vector_payloads(
                payload_hash, dimension, normalization, producer_signature,
                projection_input_hash, object_path, byte_length, checksum, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.payload_hash.as_bytes().as_slice(),
                dimension,
                normalization_to_sql(record.normalization),
                record.producer_signature,
                record.projection_input_hash.as_bytes().as_slice(),
                record.object_path,
                byte_length,
                record.checksum.as_slice(),
                record.created_at,
            ],
        )?;
        let stored = self.vector_payload(record.payload_hash)?;
        if stored != *record {
            return Err(DerivedError::InvalidProjectionValue {
                value: "vector payload identity already contains different metadata".to_owned(),
            });
        }
        Ok(stored)
    }

    pub fn vector_payload(
        &self,
        payload_hash: VectorPayloadHash,
    ) -> Result<VectorPayloadRecord, DerivedError> {
        self.connection
            .query_row(
                "SELECT payload_hash, dimension, normalization, producer_signature,
                        projection_input_hash, object_path, byte_length, checksum, created_at
                 FROM vector_payloads WHERE payload_hash = ?1",
                params![payload_hash.as_bytes().as_slice()],
                decode_vector_payload_record,
            )
            .optional()?
            .ok_or_else(|| DerivedError::InvalidProjectionValue {
                value: format!("vector payload {payload_hash} was not found"),
            })
    }

    pub fn vector_payload_count(&self) -> Result<usize, DerivedError> {
        let count =
            self.connection
                .query_row("SELECT COUNT(*) FROM vector_payloads", [], |row| {
                    row.get::<_, i64>(0)
                })?;
        usize::try_from(count).map_err(DerivedError::from)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_vector_membership(
        &mut self,
        artifact_id: ArtifactId,
        space_id: SpaceId,
        memory_id: MemoryId,
        revision_id: RevisionId,
        derived_unit_id: impl AsRef<str>,
        semantic_node_id: Option<&str>,
        resolution: impl AsRef<str>,
        payload_hash: VectorPayloadHash,
        live_from_artifact_generation: AuthorityGeneration,
    ) -> Result<VectorMembershipRecord, DerivedError> {
        self.vector_payload(payload_hash)?;
        self.connection.execute(
            "INSERT OR IGNORE INTO vector_memberships(
                artifact_id, space_id, memory_id, revision_id, derived_unit_id,
                semantic_node_id, resolution, payload_hash, live_from_artifact_generation
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                artifact_id.value(),
                space_id.as_bytes().as_slice(),
                memory_id.as_bytes().as_slice(),
                revision_id.as_bytes().as_slice(),
                derived_unit_id.as_ref(),
                semantic_node_id,
                resolution.as_ref(),
                payload_hash.as_bytes().as_slice(),
                generation_to_sql(live_from_artifact_generation)?,
            ],
        )?;
        self.connection
            .query_row(
                "SELECT membership_id, artifact_id, space_id, memory_id, revision_id,
                        derived_unit_id, semantic_node_id, resolution, payload_hash,
                        live_from_artifact_generation, live_to_artifact_generation
                 FROM vector_memberships
                 WHERE artifact_id = ?1 AND space_id = ?2 AND memory_id = ?3
                   AND revision_id = ?4 AND derived_unit_id = ?5 AND resolution = ?6",
                params![
                    artifact_id.value(),
                    space_id.as_bytes().as_slice(),
                    memory_id.as_bytes().as_slice(),
                    revision_id.as_bytes().as_slice(),
                    derived_unit_id.as_ref(),
                    resolution.as_ref(),
                ],
                decode_vector_membership_record,
            )
            .map_err(DerivedError::from)
    }

    pub fn vector_memberships_for_artifact(
        &self,
        artifact_id: ArtifactId,
    ) -> Result<Vec<VectorMembershipRecord>, DerivedError> {
        let mut statement = self.connection.prepare(
            "SELECT membership_id, artifact_id, space_id, memory_id, revision_id,
                    derived_unit_id, semantic_node_id, resolution, payload_hash,
                    live_from_artifact_generation, live_to_artifact_generation
             FROM vector_memberships WHERE artifact_id = ?1 ORDER BY membership_id",
        )?;
        let rows = statement.query_map(
            params![artifact_id.value()],
            decode_vector_membership_record,
        )?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(DerivedError::from)
    }

    pub fn register_ann_segment(
        &mut self,
        artifact_id: ArtifactId,
        object_hash: [u8; 32],
        vector_count: u64,
        dimension: u32,
        producer_signature: impl AsRef<str>,
    ) -> Result<AnnSegmentRecord, DerivedError> {
        let created_at = unix_now();
        self.connection.execute(
            "INSERT INTO ann_segments(
                artifact_id, object_hash, vector_count, dimension, producer_signature, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                artifact_id.value(),
                object_hash.as_slice(),
                i64::try_from(vector_count)?,
                i64::from(dimension),
                producer_signature.as_ref(),
                created_at,
            ],
        )?;
        let segment_id = self.connection.last_insert_rowid();
        self.ann_segment(segment_id)
    }

    pub fn ann_segment(&self, segment_id: i64) -> Result<AnnSegmentRecord, DerivedError> {
        self.connection
            .query_row(
                "SELECT segment_id, artifact_id, object_hash, vector_count, dimension,
                        producer_signature, created_at
                 FROM ann_segments WHERE segment_id = ?1",
                params![segment_id],
                decode_ann_segment_record,
            )
            .map_err(DerivedError::from)
    }

    pub fn add_ann_tombstone(
        &mut self,
        artifact_id: ArtifactId,
        target_key: impl AsRef<str>,
    ) -> Result<(), DerivedError> {
        self.connection.execute(
            "INSERT OR IGNORE INTO ann_tombstones(artifact_id, target_key)
             VALUES (?1, ?2)",
            params![artifact_id.value(), target_key.as_ref()],
        )?;
        Ok(())
    }

    pub fn ann_tombstones(&self, artifact_id: ArtifactId) -> Result<Vec<String>, DerivedError> {
        let mut statement = self.connection.prepare(
            "SELECT target_key FROM ann_tombstones
             WHERE artifact_id = ?1 ORDER BY target_key",
        )?;
        let rows = statement.query_map(params![artifact_id.value()], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(DerivedError::from)
    }

    pub fn enqueue_build_job(
        &mut self,
        kind: impl AsRef<str>,
        input_hash: impl AsRef<str>,
        producer_signature: impl AsRef<str>,
        authority_generation: AuthorityGeneration,
    ) -> Result<BuildJob, DerivedError> {
        let kind = kind.as_ref();
        let input_hash = input_hash.as_ref();
        let producer_signature = producer_signature.as_ref();
        let job_id = format!("BJ:{kind}:{input_hash}:{producer_signature}");
        let now = unix_now();
        self.connection.execute(
            "INSERT OR IGNORE INTO build_jobs(
                job_id, kind, input_hash, producer_signature, authority_generation,
                state, attempt_count, next_attempt_at, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, 'queued', 0, ?6, ?6, ?6)",
            params![
                job_id,
                kind,
                input_hash,
                producer_signature,
                generation_to_sql(authority_generation)?,
                now,
            ],
        )?;
        self.build_job(&format!("BJ:{kind}:{input_hash}:{producer_signature}"))
    }

    pub fn build_job(&self, job_id: &str) -> Result<BuildJob, DerivedError> {
        self.connection
            .query_row(
                "SELECT job_id, kind, input_hash, producer_signature, authority_generation,
                        state, attempt_count, next_attempt_at, last_error_code,
                        last_error_message, created_at, updated_at
                 FROM build_jobs WHERE job_id = ?1",
                params![job_id],
                decode_build_job,
            )
            .map_err(DerivedError::from)
    }

    pub fn mark_build_job_running(&mut self, job_id: &str) -> Result<BuildJob, DerivedError> {
        self.connection.execute(
            "UPDATE build_jobs SET state = 'running', attempt_count = attempt_count + 1,
                    updated_at = ?1 WHERE job_id = ?2",
            params![unix_now(), job_id],
        )?;
        self.build_job(job_id)
    }

    pub fn mark_build_job_succeeded(&mut self, job_id: &str) -> Result<BuildJob, DerivedError> {
        self.connection.execute(
            "UPDATE build_jobs SET state = 'succeeded', updated_at = ?1 WHERE job_id = ?2",
            params![unix_now(), job_id],
        )?;
        self.build_job(job_id)
    }

    pub fn requeue_running_build_jobs(&mut self) -> Result<usize, DerivedError> {
        let changed = self.connection.execute(
            "UPDATE build_jobs SET state = 'queued', updated_at = ?1 WHERE state = 'running'",
            params![unix_now()],
        )?;
        Ok(changed)
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
        let mut has_compatible_semantic_artifact = false;
        for id in &artifact_ids {
            let (state, artifact_generation, kind, version) = transaction
                .query_row(
                    "SELECT state, authority_generation, kind, version
                     FROM artifacts WHERE id = ?1",
                    params![id.value()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)?,
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
            if kind == "semantic" && version == 1 {
                has_compatible_semantic_artifact = true;
            }
            kinds.insert(kind);
        }
        let capabilities = if capabilities.is_empty() {
            capabilities_for_kinds(&kinds, has_compatible_semantic_artifact)
        } else {
            capabilities
                .into_iter()
                .filter(|capability| capability != "semantic" || has_compatible_semantic_artifact)
                .collect()
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

fn decode_vector_payload_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<VectorPayloadRecord> {
    let dimension = u32::try_from(row.get::<_, i64>(1)?).map_err(|_| conversion_error(1))?;
    let byte_length = u64::try_from(row.get::<_, i64>(6)?).map_err(|_| conversion_error(6))?;
    Ok(VectorPayloadRecord {
        payload_hash: VectorPayloadHash::from_bytes(blob_array(row, 0)?),
        dimension,
        normalization: normalization_from_sql(row.get::<_, i64>(2)?)?,
        producer_signature: row.get(3)?,
        projection_input_hash: ProjectionInputHash::from_bytes(blob_array(row, 4)?),
        object_path: row.get(5)?,
        byte_length,
        checksum: blob_array(row, 7)?,
        created_at: row.get(8)?,
    })
}

fn decode_vector_membership_record(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<VectorMembershipRecord> {
    let live_from = generation_from_sql(row.get(9)?, 9)?;
    let live_to = row
        .get::<_, Option<i64>>(10)?
        .map(|value| generation_from_sql(value, 10))
        .transpose()?;
    Ok(VectorMembershipRecord {
        membership_id: row.get(0)?,
        artifact_id: ArtifactId::from_raw(row.get(1)?),
        space_id: SpaceId::from_bytes(blob_array(row, 2)?),
        memory_id: MemoryId::from_bytes(blob_array(row, 3)?),
        revision_id: RevisionId::from_bytes(blob_array(row, 4)?),
        derived_unit_id: row.get(5)?,
        semantic_node_id: row.get(6)?,
        resolution: row.get(7)?,
        payload_hash: VectorPayloadHash::from_bytes(blob_array(row, 8)?),
        live_from_artifact_generation: live_from,
        live_to_artifact_generation: live_to,
    })
}

fn decode_ann_segment_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<AnnSegmentRecord> {
    let vector_count = u64::try_from(row.get::<_, i64>(3)?).map_err(|_| conversion_error(3))?;
    let dimension = u32::try_from(row.get::<_, i64>(4)?).map_err(|_| conversion_error(4))?;
    Ok(AnnSegmentRecord {
        segment_id: row.get(0)?,
        artifact_id: ArtifactId::from_raw(row.get(1)?),
        object_hash: blob_array(row, 2)?,
        vector_count,
        dimension,
        producer_signature: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn decode_build_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<BuildJob> {
    let authority_generation = generation_from_sql(row.get(4)?, 4)?;
    let attempt_count = u32::try_from(row.get::<_, i64>(6)?).map_err(|_| conversion_error(6))?;
    let state = BuildJobState::parse(&row.get::<_, String>(5)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    })?;
    Ok(BuildJob {
        job_id: row.get(0)?,
        kind: row.get(1)?,
        input_hash: row.get(2)?,
        producer_signature: row.get(3)?,
        authority_generation,
        state,
        attempt_count,
        next_attempt_at: row.get(7)?,
        last_error_code: row.get(8)?,
        last_error_message: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn blob_array<const N: usize>(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<[u8; N]> {
    let bytes = row.get::<_, Vec<u8>>(index)?;
    bytes.try_into().map_err(|_| conversion_error(index))
}

fn generation_from_sql(value: i64, index: usize) -> rusqlite::Result<AuthorityGeneration> {
    u64::try_from(value)
        .map(AuthorityGeneration::new)
        .map_err(|_| conversion_error(index))
}

fn normalization_to_sql(normalization: EmbeddingNormalization) -> i64 {
    match normalization {
        EmbeddingNormalization::None => 0,
        EmbeddingNormalization::L2 => 1,
    }
}

fn normalization_from_sql(value: i64) -> rusqlite::Result<EmbeddingNormalization> {
    match value {
        0 => Ok(EmbeddingNormalization::None),
        1 => Ok(EmbeddingNormalization::L2),
        _ => Err(conversion_error(2)),
    }
}

fn conversion_error(index: usize) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        rusqlite::types::Type::Blob,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "derived catalog value has an invalid representation",
        )),
    )
}

fn capabilities_for_kinds(
    kinds: &BTreeSet<String>,
    has_compatible_semantic_artifact: bool,
) -> Vec<String> {
    let mut capabilities = Vec::new();
    if has_compatible_semantic_artifact {
        capabilities.push("semantic".to_owned());
    }
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
