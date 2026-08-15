use std::path::{Path, PathBuf};
use std::{collections::BTreeSet, fs, time::Duration};

use backon::{BlockingRetryable, ExponentialBuilder};
use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};
use rusqlite::{
    Connection, ErrorCode, OptionalExtension, Transaction, TransactionBehavior, params,
};
use rusqlite_migration::{M, Migrations};

use crate::artifact::{ArtifactDescriptor, ArtifactId, ArtifactState, BuildJob, BuildJobState};
use crate::gc::DerivedGc;
use crate::lease::{ManifestLease, unix_now};
use crate::manifest::{DerivedManifest, ManifestId};
use crate::projection::tags::TagProvenance;
use crate::{
    DerivedError, EmbeddingNormalization, ProjectionInputHash, ServingRecord, TagId,
    VectorPayloadHash, generation_to_sql,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LexicalArtifactRecord {
    pub artifact_id: ArtifactId,
    pub object_hash: [u8; 32],
    pub object_path: String,
    pub document_count: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TagMembershipRecord {
    pub membership_id: i64,
    pub space_id: SpaceId,
    pub memory_id: MemoryId,
    pub revision_id: RevisionId,
    pub tag_id: TagId,
    pub normalized_value: String,
    pub node_id: Option<String>,
    pub provenance: TagProvenance,
    pub producer_signature: Option<String>,
    pub projection_input_hash: Option<ProjectionInputHash>,
    pub score: Option<f32>,
    pub confidence: Option<f32>,
}

pub struct DerivedCatalog {
    connection: Connection,
    database_path: PathBuf,
}

const SCHEMA_V1: &str = r#"
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
    created_at INTEGER NOT NULL,
    UNIQUE (artifact_id, object_hash)
);
CREATE TABLE IF NOT EXISTS ann_tombstones (
    artifact_id INTEGER NOT NULL REFERENCES artifacts(id),
    target_key TEXT NOT NULL,
    PRIMARY KEY (artifact_id, target_key)
);
CREATE TABLE IF NOT EXISTS lexical_artifacts (
    artifact_id INTEGER PRIMARY KEY REFERENCES artifacts(id),
    object_hash BLOB NOT NULL CHECK (length(object_hash) = 32),
    object_path TEXT NOT NULL,
    document_count INTEGER NOT NULL CHECK (document_count >= 0)
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
CREATE TABLE IF NOT EXISTS serving_records (
    space_id BLOB NOT NULL CHECK (length(space_id) = 16),
    memory_id BLOB NOT NULL CHECK (length(memory_id) = 16),
    revision_id BLOB NOT NULL CHECK (length(revision_id) = 32),
    authority_generation INTEGER NOT NULL,
    text TEXT NOT NULL,
    entity_refs BLOB NOT NULL,
    tags BLOB NOT NULL,
    node_ids BLOB NOT NULL,
    current INTEGER NOT NULL CHECK (current IN (0, 1)),
    retired INTEGER NOT NULL CHECK (retired IN (0, 1)),
    PRIMARY KEY (space_id, memory_id)
);
CREATE TABLE IF NOT EXISTS serving_relations (
    space_id BLOB NOT NULL CHECK (length(space_id) = 16),
    memory_id BLOB NOT NULL CHECK (length(memory_id) = 16),
    revision_id BLOB NOT NULL CHECK (length(revision_id) = 32),
    relation TEXT NOT NULL,
    PRIMARY KEY (space_id, memory_id, revision_id, relation)
);
CREATE TABLE IF NOT EXISTS tag_dictionary (
    tag_id BLOB PRIMARY KEY CHECK (length(tag_id) = 32),
    normalized_value TEXT NOT NULL UNIQUE
);
CREATE TABLE IF NOT EXISTS tag_memberships (
    membership_id INTEGER PRIMARY KEY AUTOINCREMENT,
    space_id BLOB NOT NULL CHECK (length(space_id) = 16),
    memory_id BLOB NOT NULL CHECK (length(memory_id) = 16),
    revision_id BLOB NOT NULL CHECK (length(revision_id) = 32),
    tag_id BLOB NOT NULL REFERENCES tag_dictionary(tag_id),
    normalized_value TEXT NOT NULL,
    node_id TEXT,
    provenance TEXT NOT NULL CHECK (provenance IN ('explicit', 'generated')),
    producer_signature TEXT,
    projection_input_hash BLOB CHECK (projection_input_hash IS NULL OR length(projection_input_hash) = 32),
    score REAL,
    confidence REAL,
    UNIQUE (space_id, memory_id, revision_id, tag_id, node_id, provenance, projection_input_hash)
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
"#;

const MIGRATION_LIST: &[M<'_>] = &[M::up(SCHEMA_V1)];
const MIGRATIONS: Migrations<'_> = Migrations::from_slice(MIGRATION_LIST);

impl DerivedCatalog {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DerivedError> {
        let database_path = path.as_ref().to_path_buf();
        if let Some(parent) = database_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut connection = Connection::open(&database_path)?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;",
        )?;
        MIGRATIONS.to_latest(&mut connection).map_err(|error| {
            DerivedError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
        })?;
        connection.execute(
            "UPDATE build_jobs SET state = 'queued', updated_at = ?1 WHERE state = 'running'",
            params![unix_now()],
        )?;
        Ok(Self {
            connection,
            database_path,
        })
    }

    #[must_use]
    pub fn derived_dir(&self) -> &Path {
        self.database_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
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
        if stored.payload_hash != record.payload_hash
            || stored.dimension != record.dimension
            || stored.normalization != record.normalization
            || stored.producer_signature != record.producer_signature
            || stored.projection_input_hash != record.projection_input_hash
            || stored.object_path != record.object_path
            || stored.byte_length != record.byte_length
            || stored.checksum != record.checksum
        {
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

    pub fn replace_serving_records(
        &mut self,
        space_id: SpaceId,
        records: &[ServingRecord],
    ) -> Result<(), DerivedError> {
        let transaction = self.begin_immediate_with_retry()?;
        transaction.execute(
            "DELETE FROM serving_records WHERE space_id = ?1",
            params![space_id.as_bytes().as_slice()],
        )?;
        transaction.execute(
            "DELETE FROM serving_relations WHERE space_id = ?1",
            params![space_id.as_bytes().as_slice()],
        )?;
        for record in records {
            if record.space_id != space_id {
                return Err(DerivedError::InvalidProjectionValue {
                    value: "serving record belongs to a different Space".to_owned(),
                });
            }
            transaction.execute(
                "INSERT INTO serving_records(
                    space_id, memory_id, revision_id, authority_generation, text,
                    entity_refs, tags, node_ids, current, retired
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    record.space_id.as_bytes().as_slice(),
                    record.memory_id.as_bytes().as_slice(),
                    record.revision_id.as_bytes().as_slice(),
                    generation_to_sql(record.authority_generation)?,
                    record.text,
                    encode_strings(&record.entity_refs)?,
                    encode_strings(&record.tags)?,
                    encode_strings(&record.node_ids)?,
                    i64::from(u8::from(record.current)),
                    i64::from(u8::from(record.retired)),
                ],
            )?;
            for relation in &record.relations {
                transaction.execute(
                    "INSERT INTO serving_relations(
                        space_id, memory_id, revision_id, relation
                     ) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        record.space_id.as_bytes().as_slice(),
                        record.memory_id.as_bytes().as_slice(),
                        record.revision_id.as_bytes().as_slice(),
                        relation,
                    ],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn serving_records_for_spaces(
        &self,
        space_ids: &[SpaceId],
        generation: AuthorityGeneration,
    ) -> Result<Vec<ServingRecord>, DerivedError> {
        let mut statement = self.connection.prepare(
            "SELECT space_id, memory_id, revision_id, authority_generation, text,
                    entity_refs, tags, node_ids, current, retired
             FROM serving_records ORDER BY space_id, memory_id",
        )?;
        let rows = statement.query_map([], decode_serving_record)?;
        let mut records = rows
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|record| {
                space_ids.contains(&record.space_id) && record.authority_generation <= generation
            })
            .collect::<Vec<_>>();
        let mut relation_statement = self.connection.prepare(
            "SELECT relation FROM serving_relations
             WHERE space_id = ?1 AND memory_id = ?2 AND revision_id = ?3
             ORDER BY relation",
        )?;
        for record in &mut records {
            let rows = relation_statement.query_map(
                params![
                    record.space_id.as_bytes().as_slice(),
                    record.memory_id.as_bytes().as_slice(),
                    record.revision_id.as_bytes().as_slice(),
                ],
                |row| row.get(0),
            )?;
            record.relations = rows.collect::<Result<Vec<_>, _>>()?;
        }
        Ok(records)
    }

    pub fn register_tag_dictionary_value(
        &mut self,
        tag_id: TagId,
        normalized_value: impl AsRef<str>,
    ) -> Result<(), DerivedError> {
        self.connection.execute(
            "INSERT OR IGNORE INTO tag_dictionary(tag_id, normalized_value)
             VALUES (?1, ?2)",
            params![tag_id.as_bytes().as_slice(), normalized_value.as_ref()],
        )?;
        let stored = self.connection.query_row(
            "SELECT normalized_value FROM tag_dictionary WHERE tag_id = ?1",
            params![tag_id.as_bytes().as_slice()],
            |row| row.get::<_, String>(0),
        )?;
        if stored != normalized_value.as_ref() {
            return Err(DerivedError::InvalidProjectionValue {
                value: "Tag dictionary identity conflicts with normalized value".to_owned(),
            });
        }
        Ok(())
    }

    pub fn tag_dictionary_entries(&self) -> Result<Vec<(TagId, String)>, DerivedError> {
        let mut statement = self.connection.prepare(
            "SELECT tag_id, normalized_value FROM tag_dictionary ORDER BY normalized_value",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((TagId::from_bytes(blob_array(row, 0)?), row.get(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(DerivedError::from)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_tag_membership(
        &mut self,
        space_id: SpaceId,
        memory_id: MemoryId,
        revision_id: RevisionId,
        tag_id: TagId,
        normalized_value: impl AsRef<str>,
        node_id: Option<&str>,
        provenance: TagProvenance,
        producer_signature: Option<&str>,
        projection_input_hash: Option<ProjectionInputHash>,
        score: Option<f32>,
        confidence: Option<f32>,
    ) -> Result<TagMembershipRecord, DerivedError> {
        self.register_tag_dictionary_value(tag_id, normalized_value.as_ref())?;
        self.connection.execute(
            "INSERT OR IGNORE INTO tag_memberships(
                space_id, memory_id, revision_id, tag_id, normalized_value, node_id,
                provenance, producer_signature, projection_input_hash, score, confidence
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                space_id.as_bytes().as_slice(),
                memory_id.as_bytes().as_slice(),
                revision_id.as_bytes().as_slice(),
                tag_id.as_bytes().as_slice(),
                normalized_value.as_ref(),
                node_id,
                provenance_to_sql(provenance),
                producer_signature,
                projection_input_hash
                    .as_ref()
                    .map(|hash| hash.as_bytes().as_slice()),
                score,
                confidence,
            ],
        )?;
        self.connection
            .query_row(
                "SELECT membership_id, space_id, memory_id, revision_id, tag_id,
                        normalized_value, node_id, provenance, producer_signature,
                        projection_input_hash, score, confidence
                 FROM tag_memberships
                 WHERE space_id = ?1 AND memory_id = ?2 AND revision_id = ?3
                   AND tag_id = ?4 AND normalized_value = ?5
                   AND provenance = ?6
                   AND (node_id IS ?7)",
                params![
                    space_id.as_bytes().as_slice(),
                    memory_id.as_bytes().as_slice(),
                    revision_id.as_bytes().as_slice(),
                    tag_id.as_bytes().as_slice(),
                    normalized_value.as_ref(),
                    provenance_to_sql(provenance),
                    node_id,
                ],
                decode_tag_membership_record,
            )
            .map_err(DerivedError::from)
    }

    pub fn tag_memberships(&self) -> Result<Vec<TagMembershipRecord>, DerivedError> {
        let mut statement = self.connection.prepare(
            "SELECT membership_id, space_id, memory_id, revision_id, tag_id,
                    normalized_value, node_id, provenance, producer_signature,
                    projection_input_hash, score, confidence
             FROM tag_memberships ORDER BY membership_id",
        )?;
        let rows = statement.query_map([], decode_tag_membership_record)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(DerivedError::from)
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

    pub fn vector_memberships_for_memory(
        &self,
        memory_id: MemoryId,
    ) -> Result<Vec<VectorMembershipRecord>, DerivedError> {
        let mut statement = self.connection.prepare(
            "SELECT membership_id, artifact_id, space_id, memory_id, revision_id,
                    derived_unit_id, semantic_node_id, resolution, payload_hash,
                    live_from_artifact_generation, live_to_artifact_generation
             FROM vector_memberships WHERE memory_id = ?1 ORDER BY membership_id",
        )?;
        let rows = statement.query_map(
            params![memory_id.as_bytes().as_slice()],
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
            "INSERT OR IGNORE INTO ann_segments(
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
        self.connection
            .query_row(
                "SELECT segment_id FROM ann_segments
                 WHERE artifact_id = ?1 AND object_hash = ?2
                 ORDER BY segment_id LIMIT 1",
                params![artifact_id.value(), object_hash.as_slice()],
                |row| row.get(0),
            )
            .map_err(DerivedError::from)
            .and_then(|segment_id| self.ann_segment(segment_id))
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

    pub fn ann_segments_for_artifact(
        &self,
        artifact_id: ArtifactId,
    ) -> Result<Vec<AnnSegmentRecord>, DerivedError> {
        let mut statement = self.connection.prepare(
            "SELECT segment_id, artifact_id, object_hash, vector_count, dimension,
                    producer_signature, created_at
             FROM ann_segments WHERE artifact_id = ?1 ORDER BY segment_id",
        )?;
        let rows = statement.query_map(params![artifact_id.value()], decode_ann_segment_record)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(DerivedError::from)
    }

    pub fn ann_segment_count(&self, artifact_id: ArtifactId) -> Result<usize, DerivedError> {
        let count = self.connection.query_row(
            "SELECT COUNT(*) FROM ann_segments WHERE artifact_id = ?1",
            params![artifact_id.value()],
            |row| row.get::<_, i64>(0),
        )?;
        usize::try_from(count).map_err(DerivedError::from)
    }

    pub fn register_lexical_artifact(
        &mut self,
        artifact_id: ArtifactId,
        object_hash: [u8; 32],
        object_path: impl AsRef<str>,
        document_count: u64,
    ) -> Result<LexicalArtifactRecord, DerivedError> {
        let artifact = self.artifact(artifact_id)?;
        if artifact.kind() != "lexical" || artifact.version() != 1 {
            return Err(DerivedError::InvalidProjectionValue {
                value: format!("artifact {artifact_id} is not lexical schema V1"),
            });
        }
        self.connection.execute(
            "INSERT OR IGNORE INTO lexical_artifacts(
                artifact_id, object_hash, object_path, document_count
             ) VALUES (?1, ?2, ?3, ?4)",
            params![
                artifact_id.value(),
                object_hash.as_slice(),
                object_path.as_ref(),
                i64::try_from(document_count)?,
            ],
        )?;
        let record = self.lexical_artifact(artifact_id)?;
        if record.object_hash != object_hash
            || record.object_path != object_path.as_ref()
            || record.document_count != document_count
        {
            return Err(DerivedError::InvalidProjectionValue {
                value: format!("lexical artifact {artifact_id} registration is not immutable"),
            });
        }
        Ok(record)
    }

    pub fn lexical_artifact(
        &self,
        artifact_id: ArtifactId,
    ) -> Result<LexicalArtifactRecord, DerivedError> {
        self.connection
            .query_row(
                "SELECT artifact_id, object_hash, object_path, document_count
                 FROM lexical_artifacts WHERE artifact_id = ?1",
                params![artifact_id.value()],
                decode_lexical_artifact_record,
            )
            .optional()?
            .ok_or_else(|| DerivedError::InvalidProjectionValue {
                value: format!("lexical artifact {artifact_id} is not registered"),
            })
    }

    pub fn lexical_artifact_for_manifest(
        &self,
        manifest_id: ManifestId,
    ) -> Result<LexicalArtifactRecord, DerivedError> {
        let artifact_id = self
            .connection
            .query_row(
                "SELECT ma.artifact_id
                 FROM manifest_artifacts ma
                 JOIN artifacts a ON a.id = ma.artifact_id
                 JOIN lexical_artifacts la ON la.artifact_id = ma.artifact_id
                 WHERE ma.manifest_id = ?1 AND a.kind = 'lexical'
                 ORDER BY ma.artifact_id LIMIT 1",
                params![manifest_id.value()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .map(ArtifactId::from_raw)
            .ok_or_else(|| DerivedError::InvalidProjectionValue {
                value: format!("manifest {manifest_id} has no lexical artifact"),
            })?;
        self.lexical_artifact(artifact_id)
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

    pub fn ann_tombstone_count(&self, artifact_id: ArtifactId) -> Result<u64, DerivedError> {
        let count = self.connection.query_row(
            "SELECT COUNT(*) FROM ann_tombstones WHERE artifact_id = ?1",
            params![artifact_id.value()],
            |row| row.get::<_, i64>(0),
        )?;
        u64::try_from(count).map_err(DerivedError::from)
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

    pub fn build_job_for(
        &self,
        kind: &str,
        input_hash: &str,
        producer_signature: &str,
    ) -> Result<Option<BuildJob>, DerivedError> {
        let job_id = format!("BJ:{kind}:{input_hash}:{producer_signature}");
        self.connection
            .query_row(
                "SELECT job_id, kind, input_hash, producer_signature, authority_generation,
                        state, attempt_count, next_attempt_at, last_error_code,
                        last_error_message, created_at, updated_at
                 FROM build_jobs WHERE job_id = ?1",
                params![job_id],
                decode_build_job,
            )
            .optional()
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

    pub fn mark_build_job_failed(
        &mut self,
        job_id: &str,
        code: impl AsRef<str>,
        message: impl AsRef<str>,
        retryable: bool,
    ) -> Result<BuildJob, DerivedError> {
        let attempt = self.build_job(job_id)?.attempt_count;
        let delay = if !retryable {
            0
        } else {
            match attempt {
                0 | 1 => 1,
                2 => 5,
                3 => 30,
                4 => 120,
                _ => 600,
            }
        };
        let next_attempt_at = unix_now().saturating_add(delay);
        let state = if retryable && attempt < 5 {
            BuildJobState::Queued
        } else {
            BuildJobState::Failed
        };
        self.connection.execute(
            "UPDATE build_jobs
             SET state = ?1, next_attempt_at = ?2, last_error_code = ?3,
                 last_error_message = ?4, updated_at = ?5
             WHERE job_id = ?6",
            params![
                state.as_str(),
                next_attempt_at,
                code.as_ref(),
                message.as_ref(),
                unix_now(),
                job_id,
            ],
        )?;
        self.build_job(job_id)
    }

    pub fn mark_build_job_superseded(&mut self, job_id: &str) -> Result<BuildJob, DerivedError> {
        self.connection.execute(
            "UPDATE build_jobs SET state = 'superseded', updated_at = ?1 WHERE job_id = ?2",
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
        self.publish_manifest_at_generation_internal(artifact_ids, generation, capabilities, false)
    }

    pub fn publish_manifest_rebased_at_generation(
        &mut self,
        artifact_ids: Vec<ArtifactId>,
        generation: AuthorityGeneration,
        capabilities: Vec<String>,
    ) -> Result<DerivedManifest, DerivedError> {
        self.publish_manifest_at_generation_internal(artifact_ids, generation, capabilities, true)
    }

    fn publish_manifest_at_generation_internal(
        &mut self,
        artifact_ids: Vec<ArtifactId>,
        generation: AuthorityGeneration,
        capabilities: Vec<String>,
        allow_older_semantic_artifact: bool,
    ) -> Result<DerivedManifest, DerivedError> {
        let transaction = self.begin_immediate_with_retry()?;
        let mut kinds = BTreeSet::new();
        let mut has_compatible_semantic_artifact = false;
        let mut has_compatible_lexical_artifact = false;
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
            let compatible_older_semantic = allow_older_semantic_artifact
                && kind == "semantic"
                && version == 1
                && artifact_generation < generation;
            if artifact_generation != generation && !compatible_older_semantic {
                return Err(DerivedError::ArtifactGenerationMismatch {
                    id: *id,
                    expected: generation,
                    actual: artifact_generation,
                });
            }
            let has_vector_membership: bool = transaction.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM vector_memberships WHERE artifact_id = ?1
                )",
                params![id.value()],
                |row| row.get(0),
            )?;
            if kind == "semantic" && version == 1 && has_vector_membership {
                has_compatible_semantic_artifact = true;
            }
            if kind == "lexical" && version == 1 {
                has_compatible_lexical_artifact = transaction.query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM lexical_artifacts WHERE artifact_id = ?1
                    )",
                    params![id.value()],
                    |row| row.get(0),
                )?;
            }
            kinds.insert(kind);
        }
        let capabilities = if capabilities.is_empty() {
            capabilities_for_kinds(
                &kinds,
                has_compatible_semantic_artifact,
                has_compatible_lexical_artifact,
            )
        } else {
            capabilities
                .into_iter()
                .filter(|capability| {
                    (capability != "semantic" || has_compatible_semantic_artifact)
                        && (capability != "lexical" || has_compatible_lexical_artifact)
                })
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
        let transaction = self.begin_immediate_with_retry()?;
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
        let transaction = self.begin_immediate_with_retry()?;
        transaction.execute_batch(
            "
            DELETE FROM manifest_capabilities;
            DELETE FROM manifest_artifacts;
            DELETE FROM leases;
            UPDATE serving_pointer SET manifest_id = NULL WHERE singleton = 1;
            DELETE FROM manifests;
            DELETE FROM artifact_dependencies;
            DELETE FROM serving_records;
            DELETE FROM tag_memberships;
            DELETE FROM tag_dictionary;
            DELETE FROM build_jobs;
            DELETE FROM lexical_artifacts;
            DELETE FROM artifacts;
            ",
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn begin_immediate_with_retry(&self) -> Result<Transaction<'_>, DerivedError> {
        (|| Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate))
            .retry(Self::sqlite_retry_policy())
            .when(is_busy_or_locked)
            .call()
            .map_err(Into::into)
    }

    fn sqlite_retry_policy() -> ExponentialBuilder {
        ExponentialBuilder::default()
            .with_min_delay(Duration::from_millis(10))
            .with_factor(4.0)
            .with_max_delay(Duration::from_millis(200))
            .with_max_times(3)
            .with_jitter()
    }
}

fn is_busy_or_locked(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if matches!(failure.code, ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
    )
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

fn decode_lexical_artifact_record(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<LexicalArtifactRecord> {
    let document_count = u64::try_from(row.get::<_, i64>(3)?).map_err(|_| conversion_error(3))?;
    Ok(LexicalArtifactRecord {
        artifact_id: ArtifactId::from_raw(row.get(0)?),
        object_hash: blob_array(row, 1)?,
        object_path: row.get(2)?,
        document_count,
    })
}

fn encode_strings(values: &[String]) -> Result<Vec<u8>, DerivedError> {
    let count = u32::try_from(values.len())?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&count.to_le_bytes());
    for value in values {
        let value = value.as_bytes();
        let length = u32::try_from(value.len())?;
        bytes.extend_from_slice(&length.to_le_bytes());
        bytes.extend_from_slice(value);
    }
    Ok(bytes)
}

fn decode_strings(bytes: &[u8]) -> rusqlite::Result<Vec<String>> {
    let mut cursor = 0_usize;
    let count = read_u32(bytes, &mut cursor)?;
    let mut values = Vec::with_capacity(usize::try_from(count).map_err(|_| conversion_error(0))?);
    for _ in 0..count {
        let length =
            usize::try_from(read_u32(bytes, &mut cursor)?).map_err(|_| conversion_error(0))?;
        let end = cursor
            .checked_add(length)
            .ok_or_else(|| conversion_error(0))?;
        let value = bytes.get(cursor..end).ok_or_else(|| conversion_error(0))?;
        values.push(String::from_utf8(value.to_vec()).map_err(|_| conversion_error(0))?);
        cursor = end;
    }
    if cursor != bytes.len() {
        return Err(conversion_error(0));
    }
    Ok(values)
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> rusqlite::Result<u32> {
    let end = cursor.checked_add(4).ok_or_else(|| conversion_error(0))?;
    let value = bytes.get(*cursor..end).ok_or_else(|| conversion_error(0))?;
    *cursor = end;
    Ok(u32::from_le_bytes(
        value.try_into().map_err(|_| conversion_error(0))?,
    ))
}

fn decode_serving_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ServingRecord> {
    let authority_generation = generation_from_sql(row.get(3)?, 3)?;
    let current = row.get::<_, i64>(8)? != 0;
    let retired = row.get::<_, i64>(9)? != 0;
    Ok(ServingRecord {
        space_id: SpaceId::from_bytes(blob_array(row, 0)?),
        memory_id: MemoryId::from_bytes(blob_array(row, 1)?),
        revision_id: RevisionId::from_bytes(blob_array(row, 2)?),
        authority_generation,
        text: row.get(4)?,
        entity_refs: decode_strings(&row.get::<_, Vec<u8>>(5)?)?,
        tags: decode_strings(&row.get::<_, Vec<u8>>(6)?)?,
        node_ids: decode_strings(&row.get::<_, Vec<u8>>(7)?)?,
        relations: Vec::new(),
        current,
        retired,
    })
}

fn decode_tag_membership_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<TagMembershipRecord> {
    let projection_input_hash = row
        .get::<_, Option<Vec<u8>>>(9)?
        .map(|bytes| bytes.try_into().map(ProjectionInputHash::from_bytes))
        .transpose()
        .map_err(|_| conversion_error(9))?;
    Ok(TagMembershipRecord {
        membership_id: row.get(0)?,
        space_id: SpaceId::from_bytes(blob_array(row, 1)?),
        memory_id: MemoryId::from_bytes(blob_array(row, 2)?),
        revision_id: RevisionId::from_bytes(blob_array(row, 3)?),
        tag_id: TagId::from_bytes(blob_array(row, 4)?),
        normalized_value: row.get(5)?,
        node_id: row.get(6)?,
        provenance: provenance_from_sql(row.get::<_, String>(7)?.as_str())?,
        producer_signature: row.get(8)?,
        projection_input_hash,
        score: row.get(10)?,
        confidence: row.get(11)?,
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

fn provenance_to_sql(provenance: TagProvenance) -> &'static str {
    match provenance {
        TagProvenance::Explicit => "explicit",
        TagProvenance::Generated => "generated",
    }
}

fn provenance_from_sql(value: &str) -> rusqlite::Result<TagProvenance> {
    match value {
        "explicit" => Ok(TagProvenance::Explicit),
        "generated" => Ok(TagProvenance::Generated),
        _ => Err(conversion_error(7)),
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
    has_compatible_lexical_artifact: bool,
) -> Vec<String> {
    let mut capabilities = Vec::new();
    if has_compatible_semantic_artifact {
        capabilities.push("semantic".to_owned());
    }
    if kinds.contains("lexical") && has_compatible_lexical_artifact {
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
    ]
    .iter()
    .all(|kind| kinds.contains(*kind))
        && has_compatible_lexical_artifact
    {
        capabilities.push("base-search".to_owned());
    }
    capabilities
}
