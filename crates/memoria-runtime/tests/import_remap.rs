use memoria_authority::{AuthorityDb, SourceCas, StoreLayout};
use memoria_runtime::{MemoriaRuntime, PortableImportRequest, PortableMemory, RuntimeError};
use memoria_types::{MemoriaError, MemoryId, RevisionId, SpaceId};
use rusqlite::Connection;
use tempfile::tempdir;

fn memory(byte: u8) -> MemoryId {
    MemoryId::from_bytes([byte; 16])
}

fn revision(byte: u8) -> RevisionId {
    RevisionId::from_bytes([byte; 32])
}

fn space(byte: u8) -> SpaceId {
    SpaceId::from_bytes([byte; 16])
}

fn request(
    idempotency_key: &str,
    fingerprint: &str,
    memories: Vec<PortableMemory>,
) -> PortableImportRequest {
    PortableImportRequest {
        target_space_key: "portable-import".to_owned(),
        idempotency_key: idempotency_key.to_owned(),
        request_fingerprint: fingerprint.to_owned(),
        origin_store_id: Some("ST_origin".to_owned()),
        memories,
    }
}

fn source_for(store: &std::path::Path, memory_id: MemoryId) -> Vec<u8> {
    let layout = StoreLayout::open(store).unwrap();
    let authority = AuthorityDb::open(layout.authority_database()).unwrap();
    let cas = SourceCas::new(&layout);
    let record = authority.get_memory(memory_id).unwrap();
    cas.get(record.source_blob_hash()).unwrap()
}

#[test]
fn internal_memory_refs_are_rewritten_to_target_memory_ids() {
    let directory = tempdir().unwrap();
    let source_a = memory(1);
    let source_b = memory(2);
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let result = runtime
        .import_portable(request(
            "import-internal",
            "fingerprint-internal",
            vec![
                PortableMemory {
                    source_id: source_a,
                    space_id: space(1),
                    revision_id: revision(1),
                    mdx: format!("# A\n<MemoryRef memoryId=\"{source_b}\"/>").to_owned(),
                },
                PortableMemory {
                    source_id: source_b,
                    space_id: space(1),
                    revision_id: revision(2),
                    mdx: "# B".to_owned(),
                },
            ],
        ))
        .unwrap();
    let target_b = result
        .mappings
        .iter()
        .find(|mapping| mapping.source_id == source_b)
        .unwrap()
        .target_id;
    let target_a = result
        .mappings
        .iter()
        .find(|mapping| mapping.source_id == source_a)
        .unwrap()
        .target_id;
    let source = String::from_utf8(source_for(directory.path(), target_a)).unwrap();
    assert!(source.contains(&format!("memoryId=\"{target_b}\"")));
    assert!(!source.contains(&source_b.to_string()));
    assert!(result.unresolved_external_references.is_empty());
}

#[test]
fn external_refs_remain_unresolved_and_are_reported_not_guessed() {
    let directory = tempdir().unwrap();
    let source_id = memory(3);
    let external_id = memory(99);
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let result = runtime
        .import_portable(request(
            "import-external",
            "fingerprint-external",
            vec![PortableMemory {
                source_id,
                space_id: space(2),
                revision_id: revision(3),
                mdx: format!("# External\n<MemoryRef ref=\"{external_id}\"/>").to_owned(),
            }],
        ))
        .unwrap();
    assert_eq!(
        result.unresolved_external_references,
        [external_id.to_string()]
    );
    let target_id = result.mappings[0].target_id;
    let source = String::from_utf8(source_for(directory.path(), target_id)).unwrap();
    assert!(source.contains(&format!("ref=\"{external_id}\"")));
}

#[test]
fn rewritten_documents_all_validate_before_any_authority_commit() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let error = runtime
        .import_portable(request(
            "import-invalid",
            "fingerprint-invalid",
            vec![
                PortableMemory {
                    source_id: memory(4),
                    space_id: space(3),
                    revision_id: revision(4),
                    mdx: "# Valid".to_owned(),
                },
                PortableMemory {
                    source_id: memory(5),
                    space_id: space(3),
                    revision_id: revision(5),
                    mdx: "<img src=\"x\" onerror=\"alert(1)\">".to_owned(),
                },
            ],
        ))
        .unwrap_err();
    assert!(matches!(error, RuntimeError::Mdx(_)));
    assert_eq!(runtime.status().authority_generation.value(), 0);
    let connection = Connection::open(
        StoreLayout::open(directory.path())
            .unwrap()
            .authority_database(),
    )
    .unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM spaces", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM import_records", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn import_retry_after_process_restart_returns_same_mapping() {
    let directory = tempdir().unwrap();
    let import_request = request(
        "import-retry",
        "fingerprint-retry",
        vec![PortableMemory {
            source_id: memory(6),
            space_id: space(4),
            revision_id: revision(6),
            mdx: "# Retry".to_owned(),
        }],
    );
    let first = {
        let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
        runtime.import_portable(import_request.clone()).unwrap()
    };
    let generation_after_first = {
        let runtime = MemoriaRuntime::open(directory.path()).unwrap();
        runtime.status().authority_generation
    };
    let second = {
        let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
        runtime.import_portable(import_request).unwrap()
    };
    assert_eq!(first, second);
    assert_eq!(second.target_space_id, first.target_space_id);
    assert_eq!(
        MemoriaRuntime::open(directory.path())
            .unwrap()
            .status()
            .authority_generation,
        generation_after_first
    );
}

#[test]
fn same_idempotency_key_with_different_package_fingerprint_conflicts() {
    let directory = tempdir().unwrap();
    let original = request(
        "import-conflict",
        "fingerprint-one",
        vec![PortableMemory {
            source_id: memory(7),
            space_id: space(5),
            revision_id: revision(7),
            mdx: "# One".to_owned(),
        }],
    );
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    runtime.import_portable(original).unwrap();
    let conflict = runtime
        .import_portable(request(
            "import-conflict",
            "fingerprint-two",
            vec![PortableMemory {
                source_id: memory(7),
                space_id: space(5),
                revision_id: revision(7),
                mdx: "# Two".to_owned(),
            }],
        ))
        .unwrap_err();
    assert!(matches!(
        conflict,
        RuntimeError::Authority(MemoriaError::IdempotencyConflict { .. })
    ));
}
