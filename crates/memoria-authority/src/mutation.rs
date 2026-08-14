use std::{collections::HashSet, fmt::Write as _};

use memoria_types::{
    AuthorityGeneration, MemoriaError, MemoryId, RevisionId, RevisionSemanticIntent,
    SourceBlobHash, SpaceId, Timestamp,
};
use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cas::SourceCas;
use crate::db::AuthorityDb;
use crate::model::{
    AuthorityTransaction, AuthorityWriteAction, AuthorityWriteResult, MemoryLifecycle,
    MemoryRecord, SpaceLifecycle, SpaceProviderPolicy, SpaceRecord, database_error,
};

const REVISION_FORMAT_VERSION: u32 = 1;
const CREATE_MEMORY_RESULT_KIND: &str = "CreateMemory";
const BATCH_RESULT_KIND: &str = "AuthorityMutationBatch";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorityMutationBatch {
    pub idempotency_key: Option<String>,
    pub expected_generation: Option<AuthorityGeneration>,
    pub operations: Vec<AuthorityOperation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthorityOperation {
    CreateMemory {
        space_id: SpaceId,
        document_key: Option<String>,
        source: Vec<u8>,
    },
    ReviseMemory {
        memory_id: MemoryId,
        expected_head: RevisionId,
        source: Vec<u8>,
    },
    MergeMemory {
        memory_id: MemoryId,
        expected_head: RevisionId,
        parents: Vec<RevisionId>,
        source: Vec<u8>,
    },
    MoveMemory {
        memory_id: MemoryId,
        space_id: SpaceId,
        document_key: Option<String>,
    },
    RenameDocumentKey {
        memory_id: MemoryId,
        document_key: Option<String>,
    },
    RetireMemory {
        memory_id: MemoryId,
    },
    RestoreMemory {
        memory_id: MemoryId,
    },
    RenameSpaceKey {
        space_id: SpaceId,
        space_key: String,
    },
    RetireSpace {
        space_id: SpaceId,
    },
    RestoreSpace {
        space_id: SpaceId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportMemoryAllocation {
    pub source_id: MemoryId,
    pub memory_id: MemoryId,
    pub document_key: Option<String>,
    pub source: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortableImportAllocation {
    pub target_space_id: SpaceId,
    pub memories: Vec<ImportMemoryAllocation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortableImportCommit {
    pub target_space_key: String,
    pub allocation: PortableImportAllocation,
    pub idempotency_key: String,
    pub request_fingerprint: String,
    pub origin_store_id: Option<String>,
    pub unresolved_external_references: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PortableImportMapping {
    pub source_id: MemoryId,
    pub target_id: MemoryId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PortableImportResult {
    pub target_space_id: SpaceId,
    pub mappings: Vec<PortableImportMapping>,
    pub unresolved_external_references: Vec<String>,
}

enum PreparedOperation {
    CreateMemory {
        space_id: SpaceId,
        document_key: Option<String>,
        source_blob_hash: SourceBlobHash,
    },
    ReviseMemory {
        memory_id: MemoryId,
        expected_head: RevisionId,
        source_blob_hash: SourceBlobHash,
    },
    MergeMemory {
        memory_id: MemoryId,
        expected_head: RevisionId,
        parents: Vec<RevisionId>,
        source_blob_hash: SourceBlobHash,
    },
    MoveMemory {
        memory_id: MemoryId,
        space_id: SpaceId,
        document_key: Option<String>,
    },
    RenameDocumentKey {
        memory_id: MemoryId,
        document_key: Option<String>,
    },
    RetireMemory {
        memory_id: MemoryId,
    },
    RestoreMemory {
        memory_id: MemoryId,
    },
    RenameSpaceKey {
        space_id: SpaceId,
        space_key: String,
    },
    RetireSpace {
        space_id: SpaceId,
    },
    RestoreSpace {
        space_id: SpaceId,
    },
}

struct IdempotencyRecord {
    request_fingerprint: String,
    result_kind: String,
    result_id: Option<Vec<u8>>,
    committed_generation: AuthorityGeneration,
}

impl AuthorityDb {
    pub fn create_space(
        &self,
        space_key: impl AsRef<str>,
    ) -> Result<AuthorityWriteResult<SpaceRecord>, MemoriaError> {
        self.create_space_with_policy(space_key, SpaceProviderPolicy::default())
    }

    pub fn create_space_with_policy(
        &self,
        space_key: impl AsRef<str>,
        provider_policy: SpaceProviderPolicy,
    ) -> Result<AuthorityWriteResult<SpaceRecord>, MemoriaError> {
        let space_key = space_key.as_ref().to_owned();
        self.write_memoria(|tx| {
            ensure_space_key_available(tx, &space_key, None)?;
            let space_id = tx
                .create_space_record_with_policy(&space_key, provider_policy)
                .map_err(database_error)?;
            Ok(SpaceRecord {
                space_id,
                space_key,
                lifecycle: SpaceLifecycle::Active,
                generation: tx.generation(),
                provider_policy,
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
        let document_key = document_key.map(str::to_owned);

        self.write_memoria(|tx| {
            insert_new_memory(tx, space_id, document_key.as_deref(), source_blob_hash)
        })
    }

    pub fn create_memory_idempotent(
        &self,
        cas: &SourceCas,
        space_id: SpaceId,
        document_key: Option<&str>,
        source: &[u8],
        idempotency_key: &str,
    ) -> Result<AuthorityWriteResult<MemoryRecord>, MemoriaError> {
        let source_blob_hash = cas.put(source)?;
        let document_key = document_key.map(str::to_owned);
        let idempotency_key = idempotency_key.to_owned();
        let request_fingerprint =
            create_memory_fingerprint(space_id, document_key.as_deref(), source);

        self.write_memoria_action(move |tx| {
            if let Some(existing) = find_idempotency_record(tx, &idempotency_key)? {
                if existing.request_fingerprint != request_fingerprint {
                    return Err(MemoriaError::IdempotencyConflict { idempotency_key });
                }
                if existing.result_kind != CREATE_MEMORY_RESULT_KIND {
                    return Err(MemoriaError::Database {
                        message: "idempotency record result kind does not match CreateMemory"
                            .to_owned(),
                    });
                }
                let result_id = existing.result_id.ok_or_else(|| MemoriaError::Database {
                    message: "CreateMemory idempotency record has no result id".to_owned(),
                })?;
                let memory_id = MemoryId::from_bytes(
                    parse_fixed_bytes(result_id, "memory id").map_err(database_error)?,
                );
                let record =
                    memory_record_at_generation(tx, memory_id, existing.committed_generation)?;
                return Ok(AuthorityWriteAction::Noop(AuthorityWriteResult::new(
                    record,
                    existing.committed_generation,
                )));
            }

            let record =
                insert_new_memory(tx, space_id, document_key.as_deref(), source_blob_hash)?;
            tx.insert_idempotency_record(
                &idempotency_key,
                &request_fingerprint,
                CREATE_MEMORY_RESULT_KIND,
                record.memory_id.as_bytes(),
            )
            .map_err(database_error)?;
            Ok(AuthorityWriteAction::Commit(record))
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
        self.write_memoria(|tx| {
            revise_memory_in_transaction(tx, memory_id, expected_head, source_blob_hash)
        })
    }

    pub fn merge_memory(
        &self,
        cas: &SourceCas,
        memory_id: MemoryId,
        expected_head: RevisionId,
        parents: Vec<RevisionId>,
        source: &[u8],
    ) -> Result<AuthorityWriteResult<MemoryRecord>, MemoriaError> {
        let source_blob_hash = cas.put(source)?;
        self.write_memoria(|tx| {
            merge_memory_in_transaction(tx, memory_id, expected_head, &parents, source_blob_hash)
        })
    }

    #[doc(hidden)]
    pub fn test_only_create_detached_revision(
        &self,
        cas: &SourceCas,
        memory_id: MemoryId,
        parents: Vec<RevisionId>,
        source: &[u8],
        semantic_intent: RevisionSemanticIntent,
    ) -> Result<AuthorityWriteResult<RevisionId>, MemoriaError> {
        let source_blob_hash = cas.put(source)?;
        self.write_memoria(|tx| {
            current_memory_state(tx, memory_id)?;
            insert_validated_revision(tx, memory_id, &parents, source_blob_hash, semantic_intent)
        })
    }

    pub fn move_memory(
        &self,
        memory_id: MemoryId,
        space_id: SpaceId,
        document_key: Option<&str>,
        expected_generation: AuthorityGeneration,
    ) -> Result<AuthorityWriteResult<MemoryRecord>, MemoriaError> {
        let document_key = document_key.map(str::to_owned);
        self.write_memoria(|tx| {
            let state = current_memory_state(tx, memory_id)?;
            ensure_direct_expected_generation(
                tx,
                state.valid_from_generation,
                expected_generation,
            )?;
            move_memory_in_transaction(tx, memory_id, space_id, document_key.as_deref())
        })
    }

    pub fn rename_document_key(
        &self,
        memory_id: MemoryId,
        document_key: Option<&str>,
        expected_generation: AuthorityGeneration,
    ) -> Result<AuthorityWriteResult<MemoryRecord>, MemoriaError> {
        let document_key = document_key.map(str::to_owned);
        self.write_memoria(|tx| {
            let state = current_memory_state(tx, memory_id)?;
            ensure_direct_expected_generation(
                tx,
                state.valid_from_generation,
                expected_generation,
            )?;
            rename_document_key_in_transaction(tx, memory_id, document_key.as_deref())
        })
    }

    pub fn retire_memory(
        &self,
        memory_id: MemoryId,
        expected_generation: AuthorityGeneration,
    ) -> Result<AuthorityWriteResult<MemoryRecord>, MemoriaError> {
        self.write_memoria(|tx| {
            let state = current_memory_state(tx, memory_id)?;
            ensure_direct_expected_generation(
                tx,
                state.valid_from_generation,
                expected_generation,
            )?;
            retire_memory_in_transaction(tx, memory_id)
        })
    }

    pub fn restore_memory(
        &self,
        memory_id: MemoryId,
        expected_generation: AuthorityGeneration,
    ) -> Result<AuthorityWriteResult<MemoryRecord>, MemoriaError> {
        self.write_memoria(|tx| {
            let state = current_memory_state(tx, memory_id)?;
            ensure_direct_expected_generation(
                tx,
                state.valid_from_generation,
                expected_generation,
            )?;
            restore_memory_in_transaction(tx, memory_id)
        })
    }

    pub fn rename_space_key(
        &self,
        space_id: SpaceId,
        space_key: &str,
        expected_generation: AuthorityGeneration,
    ) -> Result<AuthorityWriteResult<SpaceRecord>, MemoriaError> {
        let space_key = space_key.to_owned();
        self.write_memoria(|tx| {
            let state = current_space_state(tx, space_id)?;
            ensure_direct_expected_generation(
                tx,
                state.valid_from_generation,
                expected_generation,
            )?;
            rename_space_key_in_transaction(tx, space_id, &space_key)
        })
    }

    pub fn retire_space(
        &self,
        space_id: SpaceId,
        expected_generation: AuthorityGeneration,
    ) -> Result<AuthorityWriteResult<SpaceRecord>, MemoriaError> {
        self.write_memoria(|tx| {
            let state = current_space_state(tx, space_id)?;
            ensure_direct_expected_generation(
                tx,
                state.valid_from_generation,
                expected_generation,
            )?;
            retire_space_in_transaction(tx, space_id)
        })
    }

    pub fn restore_space(
        &self,
        space_id: SpaceId,
        expected_generation: AuthorityGeneration,
    ) -> Result<AuthorityWriteResult<SpaceRecord>, MemoriaError> {
        self.write_memoria(|tx| {
            let state = current_space_state(tx, space_id)?;
            ensure_direct_expected_generation(
                tx,
                state.valid_from_generation,
                expected_generation,
            )?;
            restore_space_in_transaction(tx, space_id)
        })
    }

    pub fn update_space_provider_policy(
        &self,
        space_id: SpaceId,
        provider_policy: SpaceProviderPolicy,
        expected_generation: AuthorityGeneration,
    ) -> Result<AuthorityWriteResult<SpaceRecord>, MemoriaError> {
        self.write_memoria(|tx| {
            let state = current_space_state(tx, space_id)?;
            ensure_direct_expected_generation(
                tx,
                state.valid_from_generation,
                expected_generation,
            )?;
            update_space_provider_policy_in_transaction(tx, space_id, provider_policy)
        })
    }

    pub fn apply_mutation_batch(
        &self,
        cas: &SourceCas,
        batch: AuthorityMutationBatch,
    ) -> Result<AuthorityWriteResult<()>, MemoriaError> {
        let batch = canonicalize_mutation_batch(batch)?;
        if batch.operations.is_empty() {
            return Err(MemoriaError::InvalidMutationBatch {
                message: "operations must not be empty".to_owned(),
            });
        }
        let request_fingerprint = mutation_batch_fingerprint(&batch);
        let prepared_operations = prepare_operations(cas, &batch.operations)?;
        let idempotency_key = batch.idempotency_key;
        let expected_generation = batch.expected_generation;

        self.write_memoria_action(move |tx| {
            if let Some(idempotency_key) = idempotency_key.as_deref()
                && let Some(existing) = find_idempotency_record(tx, idempotency_key)?
            {
                if existing.request_fingerprint != request_fingerprint {
                    return Err(MemoriaError::IdempotencyConflict {
                        idempotency_key: idempotency_key.to_owned(),
                    });
                }
                if existing.result_kind != BATCH_RESULT_KIND {
                    return Err(MemoriaError::Database {
                        message: "idempotency record result kind does not match mutation batch"
                            .to_owned(),
                    });
                }
                return Ok(AuthorityWriteAction::Noop(AuthorityWriteResult::new(
                    (),
                    existing.committed_generation,
                )));
            }

            if let Some(expected_generation) = expected_generation {
                ensure_expected_generation(tx, expected_generation)?;
            }

            for operation in prepared_operations {
                apply_prepared_operation(tx, operation)?;
            }

            if let Some(idempotency_key) = idempotency_key.as_deref() {
                tx.insert_idempotency_record(
                    idempotency_key,
                    &request_fingerprint,
                    BATCH_RESULT_KIND,
                    &[],
                )
                .map_err(database_error)?;
            }
            Ok(AuthorityWriteAction::Commit(()))
        })
    }

    /// Commit a previously validated portable import in one Authority
    /// transaction. IDs are allocated by the runtime before this call so the
    /// source-aware rewrite can use the complete mapping without exposing a
    /// partially committed import.
    pub fn import_portable(
        &self,
        cas: &SourceCas,
        request: PortableImportCommit,
    ) -> Result<AuthorityWriteResult<PortableImportResult>, MemoriaError> {
        let PortableImportCommit {
            target_space_key,
            allocation,
            idempotency_key,
            request_fingerprint,
            origin_store_id,
            mut unresolved_external_references,
        } = request;
        if idempotency_key.trim().is_empty() {
            return Err(MemoriaError::IdempotencyConflict {
                idempotency_key: idempotency_key.to_owned(),
            });
        }
        if request_fingerprint.trim().is_empty() {
            return Err(MemoriaError::InvalidMutationBatch {
                message: "portable import fingerprint must not be empty".to_owned(),
            });
        }
        if allocation.memories.is_empty() {
            return Err(MemoriaError::InvalidMutationBatch {
                message: "portable import must contain at least one memory".to_owned(),
            });
        }

        let mut source_ids = HashSet::with_capacity(allocation.memories.len());
        let mut target_ids = HashSet::with_capacity(allocation.memories.len());
        for memory in &allocation.memories {
            if !source_ids.insert(memory.source_id) {
                return Err(MemoriaError::InvalidMutationBatch {
                    message: format!(
                        "portable import source memory {} is listed more than once",
                        memory.source_id
                    ),
                });
            }
            if !target_ids.insert(memory.memory_id) {
                return Err(MemoriaError::InvalidMutationBatch {
                    message: format!(
                        "portable import target memory {} is listed more than once",
                        memory.memory_id
                    ),
                });
            }
        }

        let PortableImportAllocation {
            target_space_id,
            memories,
        } = allocation;
        let mut prepared = Vec::with_capacity(memories.len());
        for memory in memories {
            let source_blob_hash = cas.put(&memory.source)?;
            prepared.push((memory, source_blob_hash));
        }

        unresolved_external_references.sort_unstable();
        unresolved_external_references.dedup();
        let mappings = prepared
            .iter()
            .map(|(memory, _)| PortableImportMapping {
                source_id: memory.source_id,
                target_id: memory.memory_id,
            })
            .collect::<Vec<_>>();
        let mapping_json = serde_json::to_string(&mappings)
            .map_err(|error| MemoriaError::Serialization(error.to_string()))?;
        let unresolved_json = serde_json::to_string(&unresolved_external_references)
            .map_err(|error| MemoriaError::Serialization(error.to_string()))?;
        self.write_memoria_action(move |transaction| {
            let existing = transaction
                .transaction
                .query_row(
                    "SELECT request_fingerprint, committed_generation,
                            target_space_id, mapping_json, unresolved_json
                     FROM import_records WHERE import_id = ?1",
                    params![idempotency_key],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            crate::model::authority_generation(row.get(1)?)?,
                            row.get::<_, Vec<u8>>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                        ))
                    },
                )
                .optional()
                .map_err(database_error)?;
            if let Some((
                stored_fingerprint,
                committed_generation,
                stored_space,
                mapping_json,
                unresolved_json,
            )) = existing
            {
                if stored_fingerprint != request_fingerprint {
                    return Err(MemoriaError::IdempotencyConflict {
                        idempotency_key: idempotency_key.clone(),
                    });
                }
                let target_space_bytes: [u8; 16] =
                    stored_space
                        .try_into()
                        .map_err(|_| MemoriaError::Database {
                            message: "portable import target space id has invalid length"
                                .to_owned(),
                        })?;
                let result = PortableImportResult {
                    target_space_id: SpaceId::from_bytes(target_space_bytes),
                    mappings: serde_json::from_str(&mapping_json).map_err(|error| {
                        MemoriaError::Database {
                            message: format!("portable import mapping is invalid: {error}"),
                        }
                    })?,
                    unresolved_external_references: serde_json::from_str(&unresolved_json)
                        .map_err(|error| MemoriaError::Database {
                            message: format!(
                                "portable import unresolved report is invalid: {error}"
                            ),
                        })?,
                };
                return Ok(AuthorityWriteAction::Noop(AuthorityWriteResult::new(
                    result,
                    committed_generation,
                )));
            }

            ensure_space_key_available(transaction, &target_space_key, None)?;
            let space_exists = transaction
                .transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM spaces WHERE space_id = ?1)",
                    params![target_space_id.as_bytes().as_slice()],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(database_error)?;
            if space_exists {
                return Err(MemoriaError::Database {
                    message: format!(
                        "portable import target space id collision: {target_space_id}"
                    ),
                });
            }
            transaction
                .insert_space_record_with_id(
                    target_space_id,
                    &target_space_key,
                    SpaceProviderPolicy::default(),
                )
                .map_err(database_error)?;

            for (memory, source_blob_hash) in &prepared {
                let memory_exists = transaction
                    .transaction
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM memories WHERE memory_id = ?1)",
                        params![memory.memory_id.as_bytes().as_slice()],
                        |row| row.get::<_, bool>(0),
                    )
                    .map_err(database_error)?;
                if memory_exists {
                    return Err(MemoriaError::Database {
                        message: format!(
                            "portable import target memory id collision: {}",
                            memory.memory_id
                        ),
                    });
                }
                ensure_document_key_available(
                    transaction,
                    target_space_id,
                    memory.document_key.as_deref(),
                    None,
                )?;
                let revision_id = derive_revision_id(
                    memory.memory_id,
                    &[],
                    *source_blob_hash,
                    RevisionSemanticIntent::Edit,
                );
                let record = MemoryRecord {
                    memory_id: memory.memory_id,
                    space_id: target_space_id,
                    document_key: memory.document_key.clone(),
                    head_revision_id: revision_id,
                    lifecycle: MemoryLifecycle::Active,
                    source_blob_hash: *source_blob_hash,
                    generation: transaction.generation(),
                };
                transaction
                    .insert_memory_record(&record, RevisionSemanticIntent::Edit, Timestamp::now()?)
                    .map_err(database_error)?;
            }

            let result = PortableImportResult {
                target_space_id,
                mappings,
                unresolved_external_references,
            };
            transaction
                .transaction
                .execute(
                    "INSERT INTO import_records(
                         import_id, origin_store_id, origin_record_id, status,
                         committed_generation, request_fingerprint, target_space_id,
                         mapping_json, unresolved_json
                     ) VALUES (?1, ?2, ?1, 'completed', ?3, ?4, ?5, ?6, ?7)",
                    params![
                        idempotency_key,
                        origin_store_id,
                        i64::try_from(transaction.generation().value()).unwrap_or(i64::MAX),
                        request_fingerprint,
                        target_space_id.as_bytes().as_slice(),
                        mapping_json,
                        unresolved_json,
                    ],
                )
                .map_err(database_error)?;
            Ok(AuthorityWriteAction::Commit(result))
        })
    }
}

fn insert_new_memory(
    tx: &mut AuthorityTransaction<'_>,
    space_id: SpaceId,
    document_key: Option<&str>,
    source_blob_hash: SourceBlobHash,
) -> Result<MemoryRecord, MemoriaError> {
    ensure_space_is_active(tx, space_id)?;
    ensure_document_key_available(tx, space_id, document_key, None)?;

    let memory_id = MemoryId::try_new()?;
    let revision_id = derive_revision_id(
        memory_id,
        &[],
        source_blob_hash,
        RevisionSemanticIntent::Edit,
    );
    let committed_at = Timestamp::now()?;
    let record = MemoryRecord {
        memory_id,
        space_id,
        document_key: document_key.map(str::to_owned),
        head_revision_id: revision_id,
        lifecycle: MemoryLifecycle::Active,
        source_blob_hash,
        generation: tx.generation(),
    };
    tx.insert_memory_record(&record, RevisionSemanticIntent::Edit, committed_at)
        .map_err(database_error)?;
    Ok(record)
}

fn revise_memory_in_transaction(
    tx: &mut AuthorityTransaction<'_>,
    memory_id: MemoryId,
    expected_head: RevisionId,
    source_blob_hash: SourceBlobHash,
) -> Result<MemoryRecord, MemoriaError> {
    let state = current_memory_state(tx, memory_id)?;
    if state.head_revision_id != expected_head {
        return Err(MemoriaError::HeadConflict {
            memory_id,
            expected_head,
            actual_head: state.head_revision_id,
        });
    }
    ensure_memory_is_active(memory_id, &state)?;

    let revision_id = insert_validated_revision(
        tx,
        memory_id,
        &[expected_head],
        source_blob_hash,
        RevisionSemanticIntent::Edit,
    )?;
    replace_memory_state(
        tx,
        memory_id,
        state.space_id,
        state.document_key.as_deref(),
        revision_id,
        state.lifecycle,
    )?;

    Ok(memory_record(
        memory_id,
        state.space_id,
        state.document_key.clone(),
        revision_id,
        state.lifecycle,
        source_blob_hash,
        tx.generation(),
    ))
}

fn merge_memory_in_transaction(
    tx: &mut AuthorityTransaction<'_>,
    memory_id: MemoryId,
    expected_head: RevisionId,
    parents: &[RevisionId],
    source_blob_hash: SourceBlobHash,
) -> Result<MemoryRecord, MemoriaError> {
    if parents.len() < 2 {
        return Err(MemoriaError::InvalidMutationBatch {
            message: "MergeMemory requires at least two parents".to_owned(),
        });
    }
    let state = current_memory_state(tx, memory_id)?;
    if state.head_revision_id != expected_head {
        return Err(MemoriaError::HeadConflict {
            memory_id,
            expected_head,
            actual_head: state.head_revision_id,
        });
    }
    ensure_memory_is_active(memory_id, &state)?;
    let revision_id = insert_validated_revision(
        tx,
        memory_id,
        parents,
        source_blob_hash,
        RevisionSemanticIntent::Merge,
    )?;
    replace_memory_state(
        tx,
        memory_id,
        state.space_id,
        state.document_key.as_deref(),
        revision_id,
        state.lifecycle,
    )?;

    Ok(memory_record(
        memory_id,
        state.space_id,
        state.document_key,
        revision_id,
        state.lifecycle,
        source_blob_hash,
        tx.generation(),
    ))
}

fn move_memory_in_transaction(
    tx: &mut AuthorityTransaction<'_>,
    memory_id: MemoryId,
    space_id: SpaceId,
    document_key: Option<&str>,
) -> Result<MemoryRecord, MemoriaError> {
    let state = current_memory_state(tx, memory_id)?;
    ensure_memory_is_active(memory_id, &state)?;
    ensure_space_is_active(tx, space_id)?;
    ensure_document_key_available(tx, space_id, document_key, Some(memory_id))?;
    replace_memory_state(
        tx,
        memory_id,
        space_id,
        document_key,
        state.head_revision_id,
        state.lifecycle,
    )?;
    Ok(memory_record(
        memory_id,
        space_id,
        document_key.map(str::to_owned),
        state.head_revision_id,
        state.lifecycle,
        state.source_blob_hash,
        tx.generation(),
    ))
}

fn rename_document_key_in_transaction(
    tx: &mut AuthorityTransaction<'_>,
    memory_id: MemoryId,
    document_key: Option<&str>,
) -> Result<MemoryRecord, MemoriaError> {
    let state = current_memory_state(tx, memory_id)?;
    ensure_memory_is_active(memory_id, &state)?;
    ensure_document_key_available(tx, state.space_id, document_key, Some(memory_id))?;
    replace_memory_state(
        tx,
        memory_id,
        state.space_id,
        document_key,
        state.head_revision_id,
        state.lifecycle,
    )?;
    Ok(memory_record(
        memory_id,
        state.space_id,
        document_key.map(str::to_owned),
        state.head_revision_id,
        state.lifecycle,
        state.source_blob_hash,
        tx.generation(),
    ))
}

fn retire_memory_in_transaction(
    tx: &mut AuthorityTransaction<'_>,
    memory_id: MemoryId,
) -> Result<MemoryRecord, MemoriaError> {
    let state = current_memory_state(tx, memory_id)?;
    if state.lifecycle == MemoryLifecycle::Retired {
        return Err(MemoriaError::MemoryRetired { memory_id });
    }
    replace_memory_state(
        tx,
        memory_id,
        state.space_id,
        state.document_key.as_deref(),
        state.head_revision_id,
        MemoryLifecycle::Retired,
    )?;
    Ok(memory_record(
        memory_id,
        state.space_id,
        state.document_key,
        state.head_revision_id,
        MemoryLifecycle::Retired,
        state.source_blob_hash,
        tx.generation(),
    ))
}

fn restore_memory_in_transaction(
    tx: &mut AuthorityTransaction<'_>,
    memory_id: MemoryId,
) -> Result<MemoryRecord, MemoriaError> {
    let state = current_memory_state(tx, memory_id)?;
    if state.lifecycle == MemoryLifecycle::Active {
        return Err(MemoriaError::MemoryAlreadyActive { memory_id });
    }
    replace_memory_state(
        tx,
        memory_id,
        state.space_id,
        state.document_key.as_deref(),
        state.head_revision_id,
        MemoryLifecycle::Active,
    )?;
    Ok(memory_record(
        memory_id,
        state.space_id,
        state.document_key,
        state.head_revision_id,
        MemoryLifecycle::Active,
        state.source_blob_hash,
        tx.generation(),
    ))
}

fn rename_space_key_in_transaction(
    tx: &mut AuthorityTransaction<'_>,
    space_id: SpaceId,
    space_key: &str,
) -> Result<SpaceRecord, MemoriaError> {
    let state = current_space_state(tx, space_id)?;
    ensure_space_key_available(tx, space_key, Some(space_id))?;
    replace_space_state(tx, space_id, space_key, &state)?;
    Ok(space_record(
        space_id,
        space_key.to_owned(),
        state.lifecycle,
        state.provider_policy,
        tx.generation(),
    ))
}

fn retire_space_in_transaction(
    tx: &mut AuthorityTransaction<'_>,
    space_id: SpaceId,
) -> Result<SpaceRecord, MemoriaError> {
    let state = current_space_state(tx, space_id)?;
    if state.lifecycle == SpaceLifecycle::Retired {
        return Err(MemoriaError::SpaceRetired { space_id });
    }
    let space_key = state.space_key.clone();
    let next_state = SpaceState {
        space_key: space_key.clone(),
        display_name: state.display_name.clone(),
        description: state.description.clone(),
        lifecycle: SpaceLifecycle::Retired,
        provider_policy: state.provider_policy,
        valid_from_generation: state.valid_from_generation,
    };
    replace_space_state(tx, space_id, &space_key, &next_state)?;
    Ok(space_record(
        space_id,
        space_key,
        SpaceLifecycle::Retired,
        state.provider_policy,
        tx.generation(),
    ))
}

fn restore_space_in_transaction(
    tx: &mut AuthorityTransaction<'_>,
    space_id: SpaceId,
) -> Result<SpaceRecord, MemoriaError> {
    let state = current_space_state(tx, space_id)?;
    if state.lifecycle == SpaceLifecycle::Active {
        return Err(MemoriaError::SpaceAlreadyActive { space_id });
    }
    let space_key = state.space_key.clone();
    let next_state = SpaceState {
        space_key: space_key.clone(),
        display_name: state.display_name.clone(),
        description: state.description.clone(),
        lifecycle: SpaceLifecycle::Active,
        provider_policy: state.provider_policy,
        valid_from_generation: state.valid_from_generation,
    };
    replace_space_state(tx, space_id, &space_key, &next_state)?;
    Ok(space_record(
        space_id,
        space_key,
        SpaceLifecycle::Active,
        state.provider_policy,
        tx.generation(),
    ))
}

fn update_space_provider_policy_in_transaction(
    tx: &mut AuthorityTransaction<'_>,
    space_id: SpaceId,
    provider_policy: SpaceProviderPolicy,
) -> Result<SpaceRecord, MemoriaError> {
    let state = current_space_state(tx, space_id)?;
    let space_key = state.space_key.clone();
    let next_state = SpaceState {
        space_key: space_key.clone(),
        display_name: state.display_name.clone(),
        description: state.description.clone(),
        lifecycle: state.lifecycle,
        provider_policy,
        valid_from_generation: state.valid_from_generation,
    };
    replace_space_state(tx, space_id, &space_key, &next_state)?;
    Ok(space_record(
        space_id,
        space_key,
        state.lifecycle,
        provider_policy,
        tx.generation(),
    ))
}

fn replace_memory_state(
    tx: &mut AuthorityTransaction<'_>,
    memory_id: MemoryId,
    space_id: SpaceId,
    document_key: Option<&str>,
    head_revision_id: RevisionId,
    lifecycle: MemoryLifecycle,
) -> Result<(), MemoriaError> {
    let generation = crate::model::sqlite_generation(tx.generation()).map_err(database_error)?;
    tx.replace_memory_state(
        memory_id,
        space_id,
        document_key,
        head_revision_id,
        lifecycle,
        generation,
    )
    .map_err(database_error)
}

fn replace_space_state(
    tx: &mut AuthorityTransaction<'_>,
    space_id: SpaceId,
    space_key: &str,
    state: &SpaceState,
) -> Result<(), MemoriaError> {
    let generation = crate::model::sqlite_generation(tx.generation()).map_err(database_error)?;
    tx.replace_space_state(
        space_id,
        space_key,
        state.display_name.as_deref(),
        state.description.as_deref(),
        state.lifecycle,
        state.provider_policy,
        generation,
    )
    .map_err(database_error)
}

fn apply_prepared_operation(
    tx: &mut AuthorityTransaction<'_>,
    operation: PreparedOperation,
) -> Result<(), MemoriaError> {
    match operation {
        PreparedOperation::CreateMemory {
            space_id,
            document_key,
            source_blob_hash,
        } => {
            insert_new_memory(tx, space_id, document_key.as_deref(), source_blob_hash)?;
        }
        PreparedOperation::ReviseMemory {
            memory_id,
            expected_head,
            source_blob_hash,
        } => {
            revise_memory_in_transaction(tx, memory_id, expected_head, source_blob_hash)?;
        }
        PreparedOperation::MergeMemory {
            memory_id,
            expected_head,
            parents,
            source_blob_hash,
        } => {
            merge_memory_in_transaction(tx, memory_id, expected_head, &parents, source_blob_hash)?;
        }
        PreparedOperation::MoveMemory {
            memory_id,
            space_id,
            document_key,
        } => {
            move_memory_in_transaction(tx, memory_id, space_id, document_key.as_deref())?;
        }
        PreparedOperation::RenameDocumentKey {
            memory_id,
            document_key,
        } => {
            rename_document_key_in_transaction(tx, memory_id, document_key.as_deref())?;
        }
        PreparedOperation::RetireMemory { memory_id } => {
            retire_memory_in_transaction(tx, memory_id)?;
        }
        PreparedOperation::RestoreMemory { memory_id } => {
            restore_memory_in_transaction(tx, memory_id)?;
        }
        PreparedOperation::RenameSpaceKey {
            space_id,
            space_key,
        } => {
            rename_space_key_in_transaction(tx, space_id, &space_key)?;
        }
        PreparedOperation::RetireSpace { space_id } => {
            retire_space_in_transaction(tx, space_id)?;
        }
        PreparedOperation::RestoreSpace { space_id } => {
            restore_space_in_transaction(tx, space_id)?;
        }
    }
    Ok(())
}

fn prepare_operations(
    cas: &SourceCas,
    operations: &[AuthorityOperation],
) -> Result<Vec<PreparedOperation>, MemoriaError> {
    operations
        .iter()
        .map(|operation| match operation {
            AuthorityOperation::CreateMemory {
                space_id,
                document_key,
                source,
            } => Ok(PreparedOperation::CreateMemory {
                space_id: *space_id,
                document_key: document_key.clone(),
                source_blob_hash: cas.put(source)?,
            }),
            AuthorityOperation::ReviseMemory {
                memory_id,
                expected_head,
                source,
            } => Ok(PreparedOperation::ReviseMemory {
                memory_id: *memory_id,
                expected_head: *expected_head,
                source_blob_hash: cas.put(source)?,
            }),
            AuthorityOperation::MergeMemory {
                memory_id,
                expected_head,
                parents,
                source,
            } => Ok(PreparedOperation::MergeMemory {
                memory_id: *memory_id,
                expected_head: *expected_head,
                parents: parents.clone(),
                source_blob_hash: cas.put(source)?,
            }),
            AuthorityOperation::MoveMemory {
                memory_id,
                space_id,
                document_key,
            } => Ok(PreparedOperation::MoveMemory {
                memory_id: *memory_id,
                space_id: *space_id,
                document_key: document_key.clone(),
            }),
            AuthorityOperation::RenameDocumentKey {
                memory_id,
                document_key,
            } => Ok(PreparedOperation::RenameDocumentKey {
                memory_id: *memory_id,
                document_key: document_key.clone(),
            }),
            AuthorityOperation::RetireMemory { memory_id } => Ok(PreparedOperation::RetireMemory {
                memory_id: *memory_id,
            }),
            AuthorityOperation::RestoreMemory { memory_id } => {
                Ok(PreparedOperation::RestoreMemory {
                    memory_id: *memory_id,
                })
            }
            AuthorityOperation::RenameSpaceKey {
                space_id,
                space_key,
            } => Ok(PreparedOperation::RenameSpaceKey {
                space_id: *space_id,
                space_key: space_key.clone(),
            }),
            AuthorityOperation::RetireSpace { space_id } => Ok(PreparedOperation::RetireSpace {
                space_id: *space_id,
            }),
            AuthorityOperation::RestoreSpace { space_id } => Ok(PreparedOperation::RestoreSpace {
                space_id: *space_id,
            }),
        })
        .collect()
}

#[derive(Clone, Debug)]
struct CurrentMemoryState {
    space_id: SpaceId,
    document_key: Option<String>,
    head_revision_id: RevisionId,
    lifecycle: MemoryLifecycle,
    source_blob_hash: SourceBlobHash,
    valid_from_generation: AuthorityGeneration,
}

#[derive(Clone, Debug)]
struct SpaceState {
    space_key: String,
    display_name: Option<String>,
    description: Option<String>,
    lifecycle: SpaceLifecycle,
    provider_policy: SpaceProviderPolicy,
    valid_from_generation: AuthorityGeneration,
}

fn ensure_expected_generation(
    transaction: &AuthorityTransaction<'_>,
    expected_generation: AuthorityGeneration,
) -> Result<(), MemoriaError> {
    let actual = transaction.base_generation();
    if actual != expected_generation {
        return Err(MemoriaError::GenerationConflict {
            expected: expected_generation,
            actual,
        });
    }
    Ok(())
}

fn ensure_direct_expected_generation(
    transaction: &AuthorityTransaction<'_>,
    entity_generation: AuthorityGeneration,
    expected: AuthorityGeneration,
) -> Result<(), MemoriaError> {
    let global_generation = transaction.base_generation();
    if expected != global_generation && expected != entity_generation {
        return Err(MemoriaError::GenerationConflict {
            expected,
            actual: global_generation,
        });
    }
    Ok(())
}

fn ensure_memory_is_active(
    memory_id: MemoryId,
    state: &CurrentMemoryState,
) -> Result<(), MemoriaError> {
    if state.lifecycle == MemoryLifecycle::Retired {
        return Err(MemoriaError::MemoryRetired { memory_id });
    }
    Ok(())
}

fn insert_validated_revision(
    tx: &mut AuthorityTransaction<'_>,
    memory_id: MemoryId,
    parents: &[RevisionId],
    source_blob_hash: SourceBlobHash,
    semantic_intent: RevisionSemanticIntent,
) -> Result<RevisionId, MemoriaError> {
    let canonical_parents = canonicalize_parents(parents)?;
    let revision_id = derive_revision_id(
        memory_id,
        &canonical_parents,
        source_blob_hash,
        semantic_intent,
    );
    validate_revision_parents(tx, memory_id, revision_id, &canonical_parents)?;
    let committed_at = Timestamp::now()?;
    tx.insert_revision_record(
        memory_id,
        revision_id,
        source_blob_hash,
        semantic_intent,
        committed_at,
        &canonical_parents,
    )
    .map_err(database_error)?;
    Ok(revision_id)
}

fn canonicalize_mutation_batch(
    mut batch: AuthorityMutationBatch,
) -> Result<AuthorityMutationBatch, MemoriaError> {
    for operation in &mut batch.operations {
        if let AuthorityOperation::MergeMemory { parents, .. } = operation {
            *parents = canonicalize_parents(parents)?;
        }
    }
    Ok(batch)
}

fn canonicalize_parents(parents: &[RevisionId]) -> Result<Vec<RevisionId>, MemoriaError> {
    let mut canonical = parents.to_vec();
    let mut seen = HashSet::with_capacity(canonical.len());
    for parent in &canonical {
        if !seen.insert(*parent) {
            return Err(MemoriaError::InvalidMutationBatch {
                message: format!("revision parent {parent} is listed more than once"),
            });
        }
    }
    canonical.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    Ok(canonical)
}

fn validate_revision_parents(
    transaction: &AuthorityTransaction<'_>,
    memory_id: MemoryId,
    revision_id: RevisionId,
    parents: &[RevisionId],
) -> Result<(), MemoriaError> {
    let mut seen = HashSet::with_capacity(parents.len());
    for parent_revision_id in parents {
        if !seen.insert(*parent_revision_id) {
            return Err(MemoriaError::InvalidMutationBatch {
                message: format!("revision parent {parent_revision_id} is listed more than once"),
            });
        }
        let parent_memory_id = transaction
            .transaction
            .query_row(
                "SELECT memory_id FROM revisions WHERE revision_id = ?1",
                params![parent_revision_id.as_bytes().as_slice()],
                |row| {
                    Ok(MemoryId::from_bytes(parse_fixed_bytes(
                        row.get::<_, Vec<u8>>(0)?,
                        "memory id",
                    )?))
                },
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => MemoriaError::InvalidMutationBatch {
                    message: format!("revision parent {parent_revision_id} does not exist"),
                },
                other => database_error(other),
            })?;
        if parent_memory_id != memory_id {
            return Err(MemoriaError::InvalidMutationBatch {
                message: format!(
                    "revision parent {parent_revision_id} belongs to {parent_memory_id}, not {memory_id}"
                ),
            });
        }
        if revision_reaches(transaction, *parent_revision_id, revision_id)? {
            return Err(MemoriaError::InvalidMutationBatch {
                message: format!(
                    "revision edge {revision_id} -> {parent_revision_id} would create a cycle"
                ),
            });
        }
    }
    Ok(())
}

fn revision_reaches(
    transaction: &AuthorityTransaction<'_>,
    start: RevisionId,
    target: RevisionId,
) -> Result<bool, MemoriaError> {
    let mut pending = vec![start];
    let mut visited = HashSet::new();
    while let Some(revision_id) = pending.pop() {
        if revision_id == target {
            return Ok(true);
        }
        if !visited.insert(revision_id) {
            continue;
        }
        let mut statement = transaction
            .transaction
            .prepare(
                "SELECT parent_revision_id
                 FROM revision_parents
                 WHERE revision_id = ?1
                 ORDER BY parent_order",
            )
            .map_err(database_error)?;
        let parents = statement
            .query_map(params![revision_id.as_bytes().as_slice()], |row| {
                Ok(RevisionId::from_bytes(parse_fixed_bytes(
                    row.get::<_, Vec<u8>>(0)?,
                    "revision id",
                )?))
            })
            .map_err(database_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(database_error)?;
        pending.extend(parents);
    }
    Ok(false)
}

fn ensure_space_key_available(
    transaction: &AuthorityTransaction<'_>,
    space_key: &str,
    excluding: Option<SpaceId>,
) -> Result<(), MemoriaError> {
    let exists = match excluding {
        Some(space_id) => transaction
            .transaction
            .query_row(
                "SELECT EXISTS (
                     SELECT 1
                     FROM space_state_history
                     WHERE space_key = ?1
                       AND valid_to_generation IS NULL
                       AND space_id <> ?2
                 )",
                params![space_key, space_id.as_bytes().as_slice()],
                |row| row.get::<_, bool>(0),
            )
            .map_err(database_error)?,
        None => transaction
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
            .map_err(database_error)?,
    };
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
    excluding: Option<MemoryId>,
) -> Result<(), MemoriaError> {
    let Some(document_key) = document_key else {
        return Ok(());
    };
    let exists = match excluding {
        Some(memory_id) => transaction
            .transaction
            .query_row(
                "SELECT EXISTS (
                     SELECT 1
                     FROM memory_state_history
                     WHERE space_id = ?1
                       AND document_key = ?2
                       AND valid_to_generation IS NULL
                       AND memory_id <> ?3
                 )",
                params![
                    space_id.as_bytes().as_slice(),
                    document_key,
                    memory_id.as_bytes().as_slice()
                ],
                |row| row.get::<_, bool>(0),
            )
            .map_err(database_error)?,
        None => transaction
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
            .map_err(database_error)?,
    };
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
        "SELECT state.space_id,
                state.document_key,
                state.head_revision_id,
                state.lifecycle,
                revisions.source_blob_hash,
                state.valid_from_generation
         FROM memory_state_history AS state
         JOIN revisions
           ON revisions.revision_id = state.head_revision_id
          AND revisions.memory_id = state.memory_id
         WHERE state.memory_id = ?1 AND state.valid_to_generation IS NULL
         LIMIT 1",
        params![memory_id.as_bytes().as_slice()],
        memory_state_from_row,
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

fn current_space_state(
    transaction: &AuthorityTransaction<'_>,
    space_id: SpaceId,
) -> Result<SpaceState, MemoriaError> {
    let result = transaction.transaction.query_row(
        "SELECT space_key, display_name, description, lifecycle,
                embedding_policy, reranking_policy, enrichment_policy,
                valid_from_generation
         FROM space_state_history
         WHERE space_id = ?1 AND valid_to_generation IS NULL
         LIMIT 1",
        params![space_id.as_bytes().as_slice()],
        |row| {
            let lifecycle = match row.get::<_, String>(3)?.as_str() {
                "active" => SpaceLifecycle::Active,
                "retired" => SpaceLifecycle::Retired,
                _ => return Err(rusqlite::Error::InvalidQuery),
            };
            let provider_policy = SpaceProviderPolicy::from_sql(
                row.get::<_, String>(4)?.as_str(),
                row.get::<_, String>(5)?.as_str(),
                row.get::<_, String>(6)?.as_str(),
            )
            .ok_or(rusqlite::Error::InvalidQuery)?;
            Ok(SpaceState {
                space_key: row.get(0)?,
                display_name: row.get(1)?,
                description: row.get(2)?,
                lifecycle,
                provider_policy,
                valid_from_generation: crate::model::authority_generation(row.get(7)?)?,
            })
        },
    );
    match result {
        Ok(state) => Ok(state),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(MemoriaError::NotFound {
            kind: "space",
            id: space_id.to_string(),
        }),
        Err(error) => Err(database_error(error)),
    }
}

fn memory_record_at_generation(
    transaction: &AuthorityTransaction<'_>,
    memory_id: MemoryId,
    generation: AuthorityGeneration,
) -> Result<MemoryRecord, MemoriaError> {
    let generation_value = crate::model::sqlite_generation(generation).map_err(database_error)?;
    let result = transaction.transaction.query_row(
        "SELECT state.space_id,
                state.document_key,
                state.head_revision_id,
                state.lifecycle,
                revisions.source_blob_hash,
                state.valid_from_generation
         FROM memory_state_history AS state
         JOIN revisions
           ON revisions.revision_id = state.head_revision_id
          AND revisions.memory_id = state.memory_id
         WHERE state.memory_id = ?1
           AND state.valid_from_generation <= ?2
           AND (state.valid_to_generation IS NULL OR ?2 < state.valid_to_generation)
         ORDER BY state.valid_from_generation DESC
         LIMIT 1",
        params![memory_id.as_bytes().as_slice(), generation_value],
        memory_state_from_row,
    );
    match result {
        Ok(state) => Ok(memory_record(
            memory_id,
            state.space_id,
            state.document_key,
            state.head_revision_id,
            state.lifecycle,
            state.source_blob_hash,
            generation,
        )),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(MemoriaError::NotFound {
            kind: "memory",
            id: memory_id.to_string(),
        }),
        Err(error) => Err(database_error(error)),
    }
}

fn memory_state_from_row(row: &Row<'_>) -> rusqlite::Result<CurrentMemoryState> {
    let space_id = SpaceId::from_bytes(parse_fixed_bytes(row.get::<_, Vec<u8>>(0)?, "space id")?);
    let head_revision_id =
        RevisionId::from_bytes(parse_fixed_bytes(row.get::<_, Vec<u8>>(2)?, "revision id")?);
    let lifecycle = match row.get::<_, String>(3)?.as_str() {
        "active" => MemoryLifecycle::Active,
        "retired" => MemoryLifecycle::Retired,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
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
    Ok(CurrentMemoryState {
        space_id,
        document_key: row.get(1)?,
        head_revision_id,
        lifecycle,
        source_blob_hash,
        valid_from_generation: crate::model::authority_generation(row.get(5)?)?,
    })
}

fn find_idempotency_record(
    transaction: &AuthorityTransaction<'_>,
    idempotency_key: &str,
) -> Result<Option<IdempotencyRecord>, MemoriaError> {
    let result = transaction.transaction.query_row(
        "SELECT request_fingerprint,
                result_kind,
                result_id,
                committed_generation
         FROM idempotency_records
         WHERE idempotency_key = ?1",
        params![idempotency_key],
        |row| {
            Ok(IdempotencyRecord {
                request_fingerprint: row.get(0)?,
                result_kind: row.get(1)?,
                result_id: row.get(2)?,
                committed_generation: crate::model::authority_generation(row.get(3)?)?,
            })
        },
    );
    match result {
        Ok(record) => Ok(Some(record)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(database_error(error)),
    }
}

fn memory_record(
    memory_id: MemoryId,
    space_id: SpaceId,
    document_key: Option<String>,
    head_revision_id: RevisionId,
    lifecycle: MemoryLifecycle,
    source_blob_hash: SourceBlobHash,
    generation: AuthorityGeneration,
) -> MemoryRecord {
    MemoryRecord {
        memory_id,
        space_id,
        document_key,
        head_revision_id,
        lifecycle,
        source_blob_hash,
        generation,
    }
}

fn space_record(
    space_id: SpaceId,
    space_key: String,
    lifecycle: SpaceLifecycle,
    provider_policy: SpaceProviderPolicy,
    generation: AuthorityGeneration,
) -> SpaceRecord {
    SpaceRecord {
        space_id,
        space_key,
        lifecycle,
        generation,
        provider_policy,
    }
}

fn create_memory_fingerprint(
    space_id: SpaceId,
    document_key: Option<&str>,
    source: &[u8],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"memoria.authority.create-memory.v1\0");
    update_bytes(&mut hasher, space_id.as_bytes());
    update_optional_bytes(&mut hasher, document_key.map(str::as_bytes));
    update_bytes(&mut hasher, source);
    fingerprint_hex(hasher)
}

fn mutation_batch_fingerprint(batch: &AuthorityMutationBatch) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"memoria.authority.mutation-batch.v1\0");
    match batch.expected_generation {
        Some(generation) => {
            hasher.update([1]);
            hasher.update(generation.value().to_be_bytes());
        }
        None => hasher.update([0]),
    }
    for operation in &batch.operations {
        match operation {
            AuthorityOperation::CreateMemory {
                space_id,
                document_key,
                source,
            } => {
                hasher.update([0]);
                update_bytes(&mut hasher, space_id.as_bytes());
                update_optional_bytes(&mut hasher, document_key.as_deref().map(str::as_bytes));
                update_bytes(&mut hasher, source);
            }
            AuthorityOperation::ReviseMemory {
                memory_id,
                expected_head,
                source,
            } => {
                hasher.update([1]);
                update_bytes(&mut hasher, memory_id.as_bytes());
                update_bytes(&mut hasher, expected_head.as_bytes());
                update_bytes(&mut hasher, source);
            }
            AuthorityOperation::MergeMemory {
                memory_id,
                expected_head,
                parents,
                source,
            } => {
                hasher.update([2]);
                update_bytes(&mut hasher, memory_id.as_bytes());
                update_bytes(&mut hasher, expected_head.as_bytes());
                update_count(&mut hasher, parents.len());
                for parent in parents {
                    update_bytes(&mut hasher, parent.as_bytes());
                }
                update_bytes(&mut hasher, source);
            }
            AuthorityOperation::MoveMemory {
                memory_id,
                space_id,
                document_key,
            } => {
                hasher.update([3]);
                update_bytes(&mut hasher, memory_id.as_bytes());
                update_bytes(&mut hasher, space_id.as_bytes());
                update_optional_bytes(&mut hasher, document_key.as_deref().map(str::as_bytes));
            }
            AuthorityOperation::RenameDocumentKey {
                memory_id,
                document_key,
            } => {
                hasher.update([4]);
                update_bytes(&mut hasher, memory_id.as_bytes());
                update_optional_bytes(&mut hasher, document_key.as_deref().map(str::as_bytes));
            }
            AuthorityOperation::RetireMemory { memory_id } => {
                hasher.update([5]);
                update_bytes(&mut hasher, memory_id.as_bytes());
            }
            AuthorityOperation::RestoreMemory { memory_id } => {
                hasher.update([6]);
                update_bytes(&mut hasher, memory_id.as_bytes());
            }
            AuthorityOperation::RenameSpaceKey {
                space_id,
                space_key,
            } => {
                hasher.update([7]);
                update_bytes(&mut hasher, space_id.as_bytes());
                update_bytes(&mut hasher, space_key.as_bytes());
            }
            AuthorityOperation::RetireSpace { space_id } => {
                hasher.update([8]);
                update_bytes(&mut hasher, space_id.as_bytes());
            }
            AuthorityOperation::RestoreSpace { space_id } => {
                hasher.update([9]);
                update_bytes(&mut hasher, space_id.as_bytes());
            }
        }
    }
    fingerprint_hex(hasher)
}

fn update_optional_bytes(hasher: &mut Sha256, value: Option<&[u8]>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            update_bytes(hasher, value);
        }
        None => hasher.update([0]),
    }
}

fn update_bytes(hasher: &mut Sha256, value: &[u8]) {
    update_count(hasher, value.len());
    hasher.update(value);
}

fn update_count(hasher: &mut Sha256, value: usize) {
    hasher.update(u64::try_from(value).unwrap_or(u64::MAX).to_be_bytes());
}

fn fingerprint_hex(hasher: Sha256) -> String {
    let mut output = String::with_capacity(64);
    for byte in hasher.finalize() {
        let _ = write!(output, "{byte:02x}");
    }
    output
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

#[cfg(test)]
mod tests {
    use super::derive_revision_id;
    use memoria_types::{MemoryId, RevisionId, RevisionSemanticIntent, SourceBlobHash};

    #[test]
    fn revision_id_derivation_is_parent_order_independent_after_canonicalization() {
        let memory_id = MemoryId::from_bytes([1; 16]);
        let low = RevisionId::from_bytes([2; 32]);
        let high = RevisionId::from_bytes([3; 32]);
        let source = SourceBlobHash::from_bytes(b"merge");
        let mut left = vec![low, high];
        let mut right = vec![high, low];
        left.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        right.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        assert_eq!(
            derive_revision_id(memory_id, &left, source, RevisionSemanticIntent::Merge),
            derive_revision_id(memory_id, &right, source, RevisionSemanticIntent::Merge)
        );
    }
}
