mod support;

use memoria_authority::{
    AuthorityDb, AuthorityMutationBatch, AuthorityOperation, MemoryRecord, SourceCas,
    SpaceProviderMode, SpaceProviderPolicy, SpaceRecord, StoreLayout, StoreWriterLock,
};
use memoria_types::{
    AuthorityGeneration, MemoriaError, MemoryId, RevisionId, RevisionSemanticIntent,
    SourceBlobHash, SpaceId,
};

struct TestAuthority {
    _directory: tempfile::TempDir,
    _layout: StoreLayout,
    db: AuthorityDb,
    cas: SourceCas,
}

impl TestAuthority {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let layout = StoreLayout::create(directory.path()).unwrap();
        let db = AuthorityDb::open(layout.authority_database()).unwrap();
        let cas = SourceCas::new(&layout);
        Self {
            _directory: directory,
            _layout: layout,
            db,
            cas,
        }
    }

    fn create_space(&self, space_key: &str) -> Result<SpaceRecord, MemoriaError> {
        self.db
            .create_space(space_key)
            .map(|result| result.into_value())
    }

    fn create_memory(
        &self,
        space_id: SpaceId,
        document_key: Option<&str>,
        source: &[u8],
    ) -> Result<MemoryRecord, MemoriaError> {
        self.db
            .create_memory(&self.cas, space_id, document_key, source)
            .map(|result| result.into_value())
    }

    fn revise_memory(
        &self,
        memory_id: MemoryId,
        expected_head: RevisionId,
        source: &[u8],
    ) -> Result<MemoryRecord, MemoriaError> {
        self.db
            .revise_memory(&self.cas, memory_id, expected_head, source)
            .map(|result| result.into_value())
    }

    fn create_memory_idempotent(
        &self,
        space_id: SpaceId,
        document_key: Option<&str>,
        source: &[u8],
        idempotency_key: &str,
    ) -> Result<MemoryRecord, MemoriaError> {
        self.db
            .create_memory_idempotent(&self.cas, space_id, document_key, source, idempotency_key)
            .map(|result| result.into_value())
    }

    fn move_memory(
        &self,
        memory_id: MemoryId,
        space_id: SpaceId,
        document_key: Option<&str>,
        expected_generation: AuthorityGeneration,
    ) -> Result<MemoryRecord, MemoriaError> {
        self.db
            .move_memory(memory_id, space_id, document_key, expected_generation)
            .map(|result| result.into_value())
    }

    fn rename_document_key(
        &self,
        memory_id: MemoryId,
        document_key: Option<&str>,
        expected_generation: AuthorityGeneration,
    ) -> Result<MemoryRecord, MemoriaError> {
        self.db
            .rename_document_key(memory_id, document_key, expected_generation)
            .map(|result| result.into_value())
    }

    fn retire_memory(
        &self,
        memory_id: MemoryId,
        expected_generation: AuthorityGeneration,
    ) -> Result<MemoryRecord, MemoriaError> {
        self.db
            .retire_memory(memory_id, expected_generation)
            .map(|result| result.into_value())
    }

    fn restore_memory(
        &self,
        memory_id: MemoryId,
        expected_generation: AuthorityGeneration,
    ) -> Result<MemoryRecord, MemoriaError> {
        self.db
            .restore_memory(memory_id, expected_generation)
            .map(|result| result.into_value())
    }

    fn rename_space_key(
        &self,
        space_id: SpaceId,
        space_key: &str,
        expected_generation: AuthorityGeneration,
    ) -> Result<SpaceRecord, MemoriaError> {
        self.db
            .rename_space_key(space_id, space_key, expected_generation)
            .map(|result| result.into_value())
    }

    fn retire_space(
        &self,
        space_id: SpaceId,
        expected_generation: AuthorityGeneration,
    ) -> Result<SpaceRecord, MemoriaError> {
        self.db
            .retire_space(space_id, expected_generation)
            .map(|result| result.into_value())
    }

    fn restore_space(
        &self,
        space_id: SpaceId,
        expected_generation: AuthorityGeneration,
    ) -> Result<SpaceRecord, MemoriaError> {
        self.db
            .restore_space(space_id, expected_generation)
            .map(|result| result.into_value())
    }

    fn apply_batch(
        &self,
        batch: AuthorityMutationBatch,
    ) -> Result<memoria_authority::AuthorityWriteResult<()>, MemoriaError> {
        self.db.apply_mutation_batch(&self.cas, batch)
    }

    fn test_only_create_detached_revision(
        &self,
        memory_id: MemoryId,
        parents: Vec<RevisionId>,
        source: &[u8],
        semantic_intent: RevisionSemanticIntent,
    ) -> Result<RevisionId, MemoriaError> {
        support::test_only_create_detached_revision(
            &self.db,
            &self.cas,
            memory_id,
            parents,
            source,
            semantic_intent,
        )
    }

    fn merge_memory(
        &self,
        memory_id: MemoryId,
        expected_head: RevisionId,
        parents: Vec<RevisionId>,
        source: &[u8],
    ) -> Result<MemoryRecord, MemoriaError> {
        self.db
            .merge_memory(&self.cas, memory_id, expected_head, parents, source)
            .map(|result| result.into_value())
    }

    fn revision_parents(&self, revision_id: RevisionId) -> Result<Vec<RevisionId>, MemoriaError> {
        Ok(self.db.get_revision(revision_id)?.parents)
    }

    fn get_memory(&self, memory_id: MemoryId) -> Result<MemoryRecord, MemoriaError> {
        self.db.get_memory(memory_id)
    }

    fn get_memory_at(
        &self,
        memory_id: MemoryId,
        generation: AuthorityGeneration,
    ) -> Result<MemoryRecord, MemoriaError> {
        self.db.get_memory_at(memory_id, generation)
    }

    fn read_memory(
        &self,
        memory_id: MemoryId,
    ) -> Result<memoria_authority::MemoryRead, MemoriaError> {
        self.db.read_memory(&self.cas, memory_id)
    }

    fn read_memory_at(
        &self,
        memory_id: MemoryId,
        generation: AuthorityGeneration,
    ) -> Result<memoria_authority::MemoryRead, MemoriaError> {
        self.db.read_memory_at(&self.cas, memory_id, generation)
    }
}

#[test]
fn store_layout_creates_declared_roots() {
    let dir = tempfile::tempdir().unwrap();
    let layout = StoreLayout::create(dir.path()).unwrap();
    assert!(layout.authority_dir().is_dir());
    assert!(layout.objects_dir().is_dir());
    assert!(layout.derived_dir().is_dir());
    assert!(layout.adaptive_dir().is_dir());
    assert!(layout.cache_dir().is_dir());
    assert!(layout.runtime_dir().is_dir());
}

#[test]
fn store_layout_opens_existing_store() {
    let dir = tempfile::tempdir().unwrap();
    StoreLayout::create(dir.path()).unwrap();
    let writer = StoreWriterLock::acquire(dir.path()).unwrap();

    StoreLayout::open(dir.path()).unwrap();
    drop(writer);
}

#[test]
fn second_writer_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    StoreLayout::create(dir.path()).unwrap();
    let first = StoreWriterLock::acquire(dir.path()).unwrap();
    let second = StoreWriterLock::acquire(dir.path());
    assert!(
        matches!(&second, Err(MemoriaError::StoreLocked { .. })),
        "second writer result: {second:?}"
    );
    drop(first);
    StoreWriterLock::acquire(dir.path()).unwrap();
}

#[test]
fn wrong_expected_head_never_overwrites() {
    let store = TestAuthority::new();
    let space = store.create_space("personal").unwrap();
    let created = store
        .create_memory(space.id(), Some("career"), b"# Career\n")
        .unwrap();

    let err = store
        .revise_memory(
            created.memory_id(),
            RevisionId::from_bytes([7; 32]),
            b"# Career\nchanged\n",
        )
        .unwrap_err();

    assert_eq!(err.code(), "HEAD_CONFLICT");
    assert!(matches!(err, MemoriaError::HeadConflict { .. }));
    assert_eq!(
        store
            .get_memory(created.memory_id())
            .unwrap()
            .head_revision_id,
        created.revision_id()
    );
    assert!(
        store
            .cas
            .contains(SourceBlobHash::from_bytes(b"# Career\nchanged\n"))
            .unwrap(),
        "CAS publication precedes the transactional HEAD check"
    );
}

#[test]
fn revise_preserves_historical_head_and_reads_current_source() {
    let store = TestAuthority::new();
    let space = store.create_space("personal").unwrap();
    let created = store
        .create_memory(space.id(), Some("career"), b"# Career\n")
        .unwrap();

    let revised = store
        .revise_memory(
            created.memory_id(),
            created.revision_id(),
            b"# Career\nchanged\n",
        )
        .unwrap();

    assert_eq!(revised.memory_id(), created.memory_id());
    assert_ne!(revised.revision_id(), created.revision_id());
    assert_eq!(revised.generation(), AuthorityGeneration::new(3));
    assert_eq!(store.get_memory(created.memory_id()).unwrap(), revised);
    assert_eq!(
        store
            .get_memory_at(created.memory_id(), created.generation())
            .unwrap()
            .head_revision_id,
        created.revision_id()
    );
    assert_eq!(
        store
            .read_memory(created.memory_id())
            .unwrap()
            .source_bytes(),
        b"# Career\nchanged\n"
    );
    assert_eq!(
        store
            .read_memory_at(created.memory_id(), created.generation())
            .unwrap()
            .source_bytes(),
        b"# Career\n"
    );
    let revision = store.db.get_revision(revised.revision_id()).unwrap();
    assert_eq!(revision.parent_revision_ids(), &[created.revision_id()]);
}

#[test]
fn duplicate_document_key_is_rejected_without_advancing_generation() {
    let store = TestAuthority::new();
    let space = store.create_space("personal").unwrap();
    store
        .create_memory(space.id(), Some("career"), b"# Career\n")
        .unwrap();
    let error = store
        .create_memory(space.id(), Some("career"), b"# Career\nother\n")
        .unwrap_err();

    assert!(matches!(
        error,
        MemoriaError::KeyConflict {
            scope: "document",
            ..
        }
    ));
    assert_eq!(
        store.db.current_generation().unwrap(),
        AuthorityGeneration::new(2)
    );
}

#[test]
fn move_preserves_memory_and_head_identity() {
    let store = TestAuthority::new();
    let a = store.create_space("a").unwrap();
    let b = store.create_space("b").unwrap();
    let memory = store
        .create_memory(a.id(), Some("career"), b"# Career\n")
        .unwrap();

    let moved = store
        .move_memory(
            memory.memory_id(),
            b.id(),
            Some("career"),
            memory.generation(),
        )
        .unwrap();

    assert_eq!(moved.memory_id(), memory.memory_id());
    assert_eq!(moved.head_revision_id(), memory.revision_id());
    assert_eq!(moved.generation(), AuthorityGeneration::new(4));
    assert_eq!(
        store.db.get_revision(moved.revision_id()).unwrap().parents,
        []
    );
}

#[test]
fn reused_idempotency_key_with_changed_request_conflicts() {
    let store = TestAuthority::new();
    let space = store.create_space("p").unwrap();
    store
        .create_memory_idempotent(space.id(), Some("a"), b"# A\n", "k1")
        .unwrap();

    assert!(matches!(
        store.create_memory_idempotent(space.id(), Some("b"), b"# B\n", "k1"),
        Err(MemoriaError::IdempotencyConflict { .. })
    ));
    assert_eq!(
        store.db.current_generation().unwrap(),
        AuthorityGeneration::new(2)
    );
}

#[test]
fn idempotent_create_replays_original_memory_without_advancing_generation() {
    let store = TestAuthority::new();
    let space = store.create_space("p").unwrap();
    let first = store
        .create_memory_idempotent(space.id(), Some("a"), b"# A\n", "k1")
        .unwrap();

    let replay = store
        .create_memory_idempotent(space.id(), Some("a"), b"# A\n", "k1")
        .unwrap();

    assert_eq!(replay, first);
    assert_eq!(store.db.current_generation().unwrap(), first.generation());
}

#[test]
fn rename_document_key_preserves_identity_and_snapshot_history() {
    let store = TestAuthority::new();
    let space = store.create_space("p").unwrap();
    let created = store
        .create_memory(space.id(), Some("old"), b"# A\n")
        .unwrap();

    let renamed = store
        .rename_document_key(created.memory_id(), Some("new"), created.generation())
        .unwrap();

    assert_eq!(renamed.memory_id(), created.memory_id());
    assert_eq!(renamed.head_revision_id(), created.head_revision_id());
    assert_eq!(renamed.document_key.as_deref(), Some("new"));
    assert_eq!(
        store
            .get_memory_at(created.memory_id(), created.generation())
            .unwrap()
            .document_key
            .as_deref(),
        Some("old")
    );
}

#[test]
fn retire_and_restore_memory_are_snapshot_versioned_without_revision() {
    let store = TestAuthority::new();
    let space = store.create_space("p").unwrap();
    let created = store
        .create_memory(space.id(), Some("a"), b"# A\n")
        .unwrap();

    let retired = store
        .retire_memory(created.memory_id(), created.generation())
        .unwrap();
    assert_eq!(retired.memory_id(), created.memory_id());
    assert_eq!(retired.head_revision_id(), created.head_revision_id());
    assert_eq!(
        retired.lifecycle,
        memoria_authority::MemoryLifecycle::Retired
    );
    assert_eq!(
        store
            .get_memory_at(created.memory_id(), created.generation())
            .unwrap()
            .lifecycle,
        memoria_authority::MemoryLifecycle::Active
    );

    let restored = store
        .restore_memory(created.memory_id(), retired.generation())
        .unwrap();
    assert_eq!(restored.memory_id(), created.memory_id());
    assert_eq!(restored.head_revision_id(), created.head_revision_id());
    assert_eq!(
        restored.lifecycle,
        memoria_authority::MemoryLifecycle::Active
    );
    assert_eq!(
        store
            .db
            .get_revision(created.revision_id())
            .unwrap()
            .parents,
        []
    );
}

#[test]
fn space_key_collision_and_lifecycle_are_snapshot_versioned() {
    let store = TestAuthority::new();
    let first = store.create_space("first").unwrap();
    let second = store.create_space("second").unwrap();

    assert!(matches!(
        store.rename_space_key(second.id(), "first", second.generation()),
        Err(MemoriaError::KeyConflict { scope: "space", .. })
    ));

    let retired = store.retire_space(first.id(), second.generation()).unwrap();
    assert_eq!(
        retired.lifecycle,
        memoria_authority::SpaceLifecycle::Retired
    );
    assert_eq!(
        store
            .db
            .get_space_at(first.id(), second.generation())
            .unwrap()
            .lifecycle,
        memoria_authority::SpaceLifecycle::Active
    );

    let restored = store
        .restore_space(first.id(), retired.generation())
        .unwrap();
    assert_eq!(restored.id(), first.id());
    assert_eq!(restored.space_key, "first");
    assert_eq!(
        restored.lifecycle,
        memoria_authority::SpaceLifecycle::Active
    );
}

#[test]
fn retired_space_is_rejected_as_create_and_move_target() {
    let store = TestAuthority::new();
    let source = store.create_space("source").unwrap();
    let target = store.create_space("target").unwrap();
    let memory = store
        .create_memory(source.id(), Some("a"), b"# A\n")
        .unwrap();
    let retired = store
        .retire_space(target.id(), target.generation())
        .unwrap();

    assert!(matches!(
        store.create_memory(target.id(), Some("b"), b"# B\n"),
        Err(MemoriaError::SpaceRetired { .. })
    ));
    assert!(matches!(
        store.move_memory(
            memory.memory_id(),
            target.id(),
            Some("a"),
            retired.generation(),
        ),
        Err(MemoriaError::SpaceRetired { .. })
    ));
    assert_eq!(store.db.current_generation().unwrap(), retired.generation());
}

#[test]
fn batch_commits_multiple_topology_operations_at_one_generation() {
    let store = TestAuthority::new();
    let space = store.create_space("p").unwrap();
    let first = store
        .create_memory(space.id(), Some("first"), b"# First\n")
        .unwrap();
    let second = store
        .create_memory(space.id(), Some("second"), b"# Second\n")
        .unwrap();

    let result = store
        .apply_batch(AuthorityMutationBatch {
            idempotency_key: None,
            expected_generation: Some(second.generation()),
            operations: vec![
                AuthorityOperation::RenameDocumentKey {
                    memory_id: first.memory_id(),
                    document_key: Some("first-renamed".to_owned()),
                },
                AuthorityOperation::RetireMemory {
                    memory_id: second.memory_id(),
                },
            ],
        })
        .unwrap();

    assert_eq!(result.generation(), AuthorityGeneration::new(4));
    assert_eq!(store.db.current_generation().unwrap(), result.generation());
    assert_eq!(
        store
            .get_memory_at(first.memory_id(), second.generation())
            .unwrap()
            .document_key
            .as_deref(),
        Some("first")
    );
    assert_eq!(
        store
            .get_memory(first.memory_id())
            .unwrap()
            .document_key
            .as_deref(),
        Some("first-renamed")
    );
    assert_eq!(
        store.get_memory(second.memory_id()).unwrap().lifecycle,
        memoria_authority::MemoryLifecycle::Retired
    );
}

#[test]
fn batch_rolls_back_earlier_operations_when_later_operation_fails() {
    let store = TestAuthority::new();
    let space = store.create_space("p").unwrap();
    let first = store
        .create_memory(space.id(), Some("first"), b"# First\n")
        .unwrap();
    let second = store
        .create_memory(space.id(), Some("second"), b"# Second\n")
        .unwrap();

    let result = store.apply_batch(AuthorityMutationBatch {
        idempotency_key: None,
        expected_generation: Some(second.generation()),
        operations: vec![
            AuthorityOperation::RenameDocumentKey {
                memory_id: first.memory_id(),
                document_key: Some("first-renamed".to_owned()),
            },
            AuthorityOperation::RenameDocumentKey {
                memory_id: second.memory_id(),
                document_key: Some("first-renamed".to_owned()),
            },
        ],
    });

    assert!(matches!(result, Err(MemoriaError::KeyConflict { .. })));
    assert_eq!(store.db.current_generation().unwrap(), second.generation());
    assert_eq!(
        store
            .get_memory(first.memory_id())
            .unwrap()
            .document_key
            .as_deref(),
        Some("first")
    );
}

#[test]
fn idempotent_batch_replays_without_advancing_generation_or_accepting_changes() {
    let store = TestAuthority::new();
    let space = store.create_space("p").unwrap();
    let memory = store
        .create_memory(space.id(), Some("old"), b"# A\n")
        .unwrap();
    let batch = AuthorityMutationBatch {
        idempotency_key: Some("batch-1".to_owned()),
        expected_generation: Some(memory.generation()),
        operations: vec![AuthorityOperation::RenameDocumentKey {
            memory_id: memory.memory_id(),
            document_key: Some("new".to_owned()),
        }],
    };

    let first = store.apply_batch(batch.clone()).unwrap();
    let replay = store.apply_batch(batch).unwrap();
    assert_eq!(replay.generation(), first.generation());
    assert_eq!(store.db.current_generation().unwrap(), first.generation());

    let conflict = store.apply_batch(AuthorityMutationBatch {
        idempotency_key: Some("batch-1".to_owned()),
        expected_generation: Some(first.generation()),
        operations: vec![AuthorityOperation::RetireMemory {
            memory_id: memory.memory_id(),
        }],
    });
    assert!(matches!(
        conflict,
        Err(MemoriaError::IdempotencyConflict { .. })
    ));
}

#[test]
fn batch_expected_generation_conflict_does_not_mutate() {
    let store = TestAuthority::new();
    let space = store.create_space("p").unwrap();
    let memory = store
        .create_memory(space.id(), Some("a"), b"# A\n")
        .unwrap();

    let result = store.apply_batch(AuthorityMutationBatch {
        idempotency_key: None,
        expected_generation: Some(AuthorityGeneration::new(0)),
        operations: vec![AuthorityOperation::RetireMemory {
            memory_id: memory.memory_id(),
        }],
    });

    assert!(matches!(
        result,
        Err(MemoriaError::GenerationConflict { .. })
    ));
    assert_eq!(store.db.current_generation().unwrap(), memory.generation());
    assert_eq!(
        store.get_memory(memory.memory_id()).unwrap().lifecycle,
        memoria_authority::MemoryLifecycle::Active
    );
}

#[test]
fn authority_mutation_batch_exposes_task6_operation_shapes() {
    let space_id = SpaceId::from_bytes([1; 16]);
    let memory_id = MemoryId::from_bytes([2; 16]);
    let revision_id = RevisionId::from_bytes([3; 32]);
    let other_revision_id = RevisionId::from_bytes([4; 32]);

    let batch = AuthorityMutationBatch {
        idempotency_key: Some("batch-1".to_owned()),
        expected_generation: Some(AuthorityGeneration::new(7)),
        operations: vec![
            AuthorityOperation::CreateMemory {
                space_id,
                document_key: Some("create".to_owned()),
                source: b"# Create\n".to_vec(),
            },
            AuthorityOperation::ReviseMemory {
                memory_id,
                expected_head: revision_id,
                source: b"# Revise\n".to_vec(),
            },
            AuthorityOperation::MergeMemory {
                memory_id,
                expected_head: revision_id,
                parents: vec![revision_id, other_revision_id],
                source: b"# Merge\n".to_vec(),
            },
            AuthorityOperation::MoveMemory {
                memory_id,
                space_id,
                document_key: Some("move".to_owned()),
            },
            AuthorityOperation::RenameDocumentKey {
                memory_id,
                document_key: Some("rename".to_owned()),
            },
            AuthorityOperation::RetireMemory { memory_id },
            AuthorityOperation::RestoreMemory { memory_id },
            AuthorityOperation::RenameSpaceKey {
                space_id,
                space_key: "renamed".to_owned(),
            },
            AuthorityOperation::RetireSpace { space_id },
            AuthorityOperation::RestoreSpace { space_id },
        ],
    };

    assert_eq!(batch.operations.len(), 10);
}

#[test]
fn merge_revision_keeps_all_valid_same_memory_parents() {
    let store = TestAuthority::new();
    let space = store.create_space("p").unwrap();
    let base = store
        .create_memory(space.id(), Some("m"), b"# M\nbase")
        .unwrap();
    let left = store
        .test_only_create_detached_revision(
            base.memory_id(),
            vec![base.revision_id()],
            b"# M\nleft",
            RevisionSemanticIntent::Edit,
        )
        .unwrap();
    let right = store
        .test_only_create_detached_revision(
            base.memory_id(),
            vec![base.revision_id()],
            b"# M\nright",
            RevisionSemanticIntent::Edit,
        )
        .unwrap();

    let merged = store
        .merge_memory(
            base.memory_id(),
            base.revision_id(),
            vec![left, right],
            b"# M\ncaller resolved merge",
        )
        .unwrap();

    let mut expected_parents = vec![left, right];
    expected_parents.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    assert_eq!(
        store.revision_parents(merged.revision_id()).unwrap(),
        expected_parents
    );
    assert_eq!(
        store
            .db
            .get_revision(merged.revision_id())
            .unwrap()
            .semantic_intent,
        RevisionSemanticIntent::Merge
    );
}

#[test]
fn merge_batch_publishes_one_generation_and_updates_head() {
    let store = TestAuthority::new();
    let space = store.create_space("p").unwrap();
    let base = store
        .create_memory(space.id(), Some("m"), b"# M\nbase")
        .unwrap();
    let left = store
        .test_only_create_detached_revision(
            base.memory_id(),
            vec![base.revision_id()],
            b"# M\nleft",
            RevisionSemanticIntent::Edit,
        )
        .unwrap();
    let right = store
        .test_only_create_detached_revision(
            base.memory_id(),
            vec![base.revision_id()],
            b"# M\nright",
            RevisionSemanticIntent::Edit,
        )
        .unwrap();
    let before = store.db.current_generation().unwrap();

    let result = store
        .apply_batch(AuthorityMutationBatch {
            idempotency_key: None,
            expected_generation: Some(before),
            operations: vec![AuthorityOperation::MergeMemory {
                memory_id: base.memory_id(),
                expected_head: base.revision_id(),
                parents: vec![left, right],
                source: b"# M\nmerged".to_vec(),
            }],
        })
        .unwrap();

    assert_eq!(result.generation(), before.next());
    let merged = store.get_memory(base.memory_id()).unwrap();
    assert_eq!(merged.generation(), result.generation());
    let mut expected_parents = vec![left, right];
    expected_parents.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    assert_eq!(
        store.db.get_revision(merged.revision_id()).unwrap().parents,
        expected_parents
    );
}

#[test]
fn merge_rejects_invalid_parent_sets_without_mutation() {
    let store = TestAuthority::new();
    let first_space = store.create_space("first").unwrap();
    let second_space = store.create_space("second").unwrap();
    let first = store
        .create_memory(first_space.id(), Some("first"), b"# First\n")
        .unwrap();
    let second = store
        .create_memory(second_space.id(), Some("second"), b"# Second\n")
        .unwrap();

    let fewer_than_two = store.merge_memory(
        first.memory_id(),
        first.revision_id(),
        vec![first.revision_id()],
        b"# First\nmerge",
    );
    assert!(matches!(
        fewer_than_two,
        Err(MemoriaError::InvalidMutationBatch { .. })
    ));

    let wrong_memory = store.merge_memory(
        first.memory_id(),
        first.revision_id(),
        vec![first.revision_id(), second.revision_id()],
        b"# First\nmerge",
    );
    assert!(matches!(
        wrong_memory,
        Err(MemoriaError::InvalidMutationBatch { .. })
    ));

    let duplicate = store.merge_memory(
        first.memory_id(),
        first.revision_id(),
        vec![first.revision_id(), first.revision_id()],
        b"# First\nmerge",
    );
    assert!(matches!(
        duplicate,
        Err(MemoriaError::InvalidMutationBatch { .. })
    ));
    assert_eq!(store.db.current_generation().unwrap(), second.generation());
    assert_eq!(
        store.get_memory(first.memory_id()).unwrap().revision_id(),
        first.revision_id()
    );
}

#[test]
fn retired_space_is_not_a_default_writable_target() {
    let store = TestAuthority::new();
    let space = store.create_space("p").unwrap();
    let retired = store.retire_space(space.id(), space.generation()).unwrap();

    assert!(matches!(
        store.create_memory(space.id(), Some("m"), b"# M\n"),
        Err(MemoriaError::SpaceRetired { .. })
    ));
    assert_eq!(store.db.current_generation().unwrap(), retired.generation());
}

#[test]
fn provider_policy_is_snapshot_versioned_space_state() {
    let store = TestAuthority::new();
    let space = store.create_space("private").unwrap();
    assert_eq!(space.provider_policy, SpaceProviderPolicy::default());

    let policy = SpaceProviderPolicy {
        embedding: SpaceProviderMode::LocalOnly,
        reranking: SpaceProviderMode::Deny,
        enrichment: SpaceProviderMode::ExternalAllowed,
    };
    let updated = store
        .db
        .update_space_provider_policy(space.id(), policy, space.generation())
        .unwrap()
        .into_value();

    let historical = store
        .db
        .get_space_at(space.id(), space.generation())
        .unwrap();
    assert_eq!(historical.provider_policy, SpaceProviderPolicy::default());
    assert_eq!(
        store.db.get_space(space.id()).unwrap().provider_policy,
        policy
    );
    assert_eq!(updated.provider_policy, policy);
}

#[test]
fn space_policy_update_does_not_change_space_identity() {
    let store = TestAuthority::new();
    let space = store.create_space("stable").unwrap();
    let updated = store
        .db
        .update_space_provider_policy(
            space.id(),
            SpaceProviderPolicy {
                embedding: SpaceProviderMode::Deny,
                ..SpaceProviderPolicy::default()
            },
            space.generation(),
        )
        .unwrap()
        .into_value();

    assert_eq!(updated.id(), space.id());
    assert_eq!(updated.space_key, space.space_key);
    assert_ne!(updated.generation(), space.generation());
}
