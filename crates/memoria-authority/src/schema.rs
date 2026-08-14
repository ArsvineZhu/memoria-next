pub(crate) const SCHEMA_V1_VERSION: i64 = 1;

pub(crate) const SCHEMA_V1_TABLES: &[&str] = &[
    "store_meta",
    "authority_generation",
    "spaces",
    "space_state_history",
    "memories",
    "memory_state_history",
    "revisions",
    "revision_parents",
    "idempotency_records",
    "import_records",
    "purge_operations",
];

pub(crate) const SCHEMA_V1_COLUMNS: &[(&str, &[&str])] = &[
    ("store_meta", &["meta_key", "meta_value"]),
    ("authority_generation", &["id", "generation"]),
    ("spaces", &["space_id", "created_generation"]),
    (
        "space_state_history",
        &[
            "space_id",
            "space_key",
            "display_name",
            "description",
            "lifecycle",
            "embedding_policy",
            "reranking_policy",
            "enrichment_policy",
            "valid_from_generation",
            "valid_to_generation",
        ],
    ),
    ("memories", &["memory_id", "created_generation"]),
    (
        "memory_state_history",
        &[
            "memory_id",
            "space_id",
            "document_key",
            "head_revision_id",
            "lifecycle",
            "valid_from_generation",
            "valid_to_generation",
        ],
    ),
    (
        "revisions",
        &[
            "revision_id",
            "memory_id",
            "source_blob_hash",
            "semantic_intent",
            "committed_generation",
            "committed_at_unix_seconds",
            "committed_at_subsec_nanos",
        ],
    ),
    (
        "revision_parents",
        &["revision_id", "parent_revision_id", "parent_order"],
    ),
    (
        "idempotency_records",
        &[
            "idempotency_key",
            "request_fingerprint",
            "result_kind",
            "result_id",
            "committed_generation",
        ],
    ),
    (
        "import_records",
        &[
            "import_id",
            "origin_store_id",
            "origin_record_id",
            "status",
            "committed_generation",
            "request_fingerprint",
            "target_space_id",
            "mapping_json",
            "unresolved_json",
        ],
    ),
    (
        "purge_operations",
        &[
            "purge_id",
            "memory_id",
            "state",
            "planned_at",
            "committed_generation",
            "updated_at",
            "last_error_code",
            "last_error_message",
        ],
    ),
];

pub(crate) const SCHEMA_V1_TRIGGERS: &[&str] = &[
    "space_state_history_no_overlap_insert",
    "space_state_history_no_overlap_update",
    "memory_state_history_no_overlap_insert",
    "memory_state_history_no_overlap_update",
];

pub(crate) const SCHEMA_V1: &str = r#"

CREATE TABLE IF NOT EXISTS store_meta (
    meta_key TEXT PRIMARY KEY NOT NULL,
    meta_value TEXT NOT NULL
);

INSERT OR IGNORE INTO store_meta (meta_key, meta_value)
VALUES ('schema_version', '1');

CREATE TABLE IF NOT EXISTS authority_generation (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    generation INTEGER NOT NULL CHECK (generation >= 0)
);

INSERT OR IGNORE INTO authority_generation (id, generation)
VALUES (1, 0);

CREATE TABLE IF NOT EXISTS spaces (
    space_id BLOB PRIMARY KEY NOT NULL,
    created_generation INTEGER NOT NULL CHECK (created_generation >= 0)
);

CREATE TABLE IF NOT EXISTS space_state_history (
    space_id BLOB NOT NULL,
    space_key TEXT NOT NULL,
    display_name TEXT,
    description TEXT,
    lifecycle TEXT NOT NULL CHECK (lifecycle IN ('active', 'retired')),
    embedding_policy TEXT NOT NULL DEFAULT 'external-allowed' CHECK (
        embedding_policy IN ('deny', 'local-only', 'external-allowed')
    ),
    reranking_policy TEXT NOT NULL DEFAULT 'external-allowed' CHECK (
        reranking_policy IN ('deny', 'local-only', 'external-allowed')
    ),
    enrichment_policy TEXT NOT NULL DEFAULT 'external-allowed' CHECK (
        enrichment_policy IN ('deny', 'local-only', 'external-allowed')
    ),
    valid_from_generation INTEGER NOT NULL CHECK (valid_from_generation >= 0),
    valid_to_generation INTEGER,
    PRIMARY KEY (space_id, valid_from_generation),
    FOREIGN KEY (space_id) REFERENCES spaces (space_id),
    CHECK (
        valid_to_generation IS NULL
        OR valid_to_generation > valid_from_generation
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS space_state_current_key
    ON space_state_history (space_key)
    WHERE valid_to_generation IS NULL;

CREATE TRIGGER IF NOT EXISTS space_state_history_no_overlap_insert
BEFORE INSERT ON space_state_history
WHEN EXISTS (
    SELECT 1
    FROM space_state_history AS existing
    WHERE existing.space_id = NEW.space_id
      AND existing.valid_from_generation
          < COALESCE(NEW.valid_to_generation, 9223372036854775807)
      AND NEW.valid_from_generation
          < COALESCE(existing.valid_to_generation, 9223372036854775807)
)
BEGIN
    SELECT RAISE(ABORT, 'space state history interval overlaps existing row');
END;

CREATE TRIGGER IF NOT EXISTS space_state_history_no_overlap_update
BEFORE UPDATE OF space_id, valid_from_generation, valid_to_generation
ON space_state_history
WHEN EXISTS (
    SELECT 1
    FROM space_state_history AS existing
    WHERE existing.rowid <> OLD.rowid
      AND existing.space_id = NEW.space_id
      AND existing.valid_from_generation
          < COALESCE(NEW.valid_to_generation, 9223372036854775807)
      AND NEW.valid_from_generation
          < COALESCE(existing.valid_to_generation, 9223372036854775807)
)
BEGIN
    SELECT RAISE(ABORT, 'space state history interval overlaps existing row');
END;

CREATE TABLE IF NOT EXISTS memories (
    memory_id BLOB PRIMARY KEY NOT NULL,
    created_generation INTEGER NOT NULL CHECK (created_generation >= 0)
);

CREATE TABLE IF NOT EXISTS revisions (
    revision_id BLOB PRIMARY KEY NOT NULL,
    memory_id BLOB NOT NULL,
    source_blob_hash TEXT NOT NULL,
    semantic_intent TEXT NOT NULL CHECK (
        semantic_intent IN (
            'edit',
            'transition',
            'correction',
            'supersession',
            'merge'
        )
    ),
    committed_generation INTEGER NOT NULL CHECK (committed_generation >= 0),
    committed_at_unix_seconds INTEGER NOT NULL CHECK (committed_at_unix_seconds >= 0),
    committed_at_subsec_nanos INTEGER NOT NULL CHECK (
        committed_at_subsec_nanos >= 0
        AND committed_at_subsec_nanos < 1000000000
    ),
    FOREIGN KEY (memory_id) REFERENCES memories (memory_id)
);

CREATE TABLE IF NOT EXISTS revision_parents (
    revision_id BLOB NOT NULL,
    parent_revision_id BLOB NOT NULL,
    parent_order INTEGER NOT NULL CHECK (parent_order >= 0),
    PRIMARY KEY (revision_id, parent_revision_id),
    UNIQUE (revision_id, parent_order),
    FOREIGN KEY (revision_id) REFERENCES revisions (revision_id),
    FOREIGN KEY (parent_revision_id) REFERENCES revisions (revision_id)
);

CREATE TABLE IF NOT EXISTS memory_state_history (
    memory_id BLOB NOT NULL,
    space_id BLOB NOT NULL,
    document_key TEXT,
    head_revision_id BLOB,
    lifecycle TEXT NOT NULL CHECK (lifecycle IN ('active', 'retired')),
    valid_from_generation INTEGER NOT NULL CHECK (valid_from_generation >= 0),
    valid_to_generation INTEGER,
    PRIMARY KEY (memory_id, valid_from_generation),
    FOREIGN KEY (memory_id) REFERENCES memories (memory_id),
    FOREIGN KEY (space_id) REFERENCES spaces (space_id),
    FOREIGN KEY (head_revision_id) REFERENCES revisions (revision_id),
    CHECK (
        valid_to_generation IS NULL
        OR valid_to_generation > valid_from_generation
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS memory_state_current_document_key
    ON memory_state_history (space_id, document_key)
    WHERE valid_to_generation IS NULL AND document_key IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS memory_state_history_no_overlap_insert
BEFORE INSERT ON memory_state_history
WHEN EXISTS (
    SELECT 1
    FROM memory_state_history AS existing
    WHERE existing.memory_id = NEW.memory_id
      AND existing.valid_from_generation
          < COALESCE(NEW.valid_to_generation, 9223372036854775807)
      AND NEW.valid_from_generation
          < COALESCE(existing.valid_to_generation, 9223372036854775807)
)
BEGIN
    SELECT RAISE(ABORT, 'memory state history interval overlaps existing row');
END;

CREATE TRIGGER IF NOT EXISTS memory_state_history_no_overlap_update
BEFORE UPDATE OF memory_id, valid_from_generation, valid_to_generation
ON memory_state_history
WHEN EXISTS (
    SELECT 1
    FROM memory_state_history AS existing
    WHERE existing.rowid <> OLD.rowid
      AND existing.memory_id = NEW.memory_id
      AND existing.valid_from_generation
          < COALESCE(NEW.valid_to_generation, 9223372036854775807)
      AND NEW.valid_from_generation
          < COALESCE(existing.valid_to_generation, 9223372036854775807)
)
BEGIN
    SELECT RAISE(ABORT, 'memory state history interval overlaps existing row');
END;

CREATE TABLE IF NOT EXISTS idempotency_records (
    idempotency_key TEXT PRIMARY KEY NOT NULL,
    request_fingerprint TEXT NOT NULL,
    result_kind TEXT NOT NULL,
    result_id BLOB,
    committed_generation INTEGER NOT NULL CHECK (committed_generation >= 0)
);

CREATE TABLE IF NOT EXISTS import_records (
    import_id TEXT PRIMARY KEY NOT NULL,
    origin_store_id TEXT,
    origin_record_id TEXT NOT NULL,
    status TEXT NOT NULL,
    committed_generation INTEGER NOT NULL CHECK (committed_generation >= 0),
    request_fingerprint TEXT NOT NULL,
    target_space_id BLOB NOT NULL CHECK (length(target_space_id) = 16),
    mapping_json TEXT NOT NULL,
    unresolved_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS purge_operations (
    purge_id TEXT PRIMARY KEY NOT NULL,
    memory_id BLOB NOT NULL CHECK (length(memory_id) = 16),
    state TEXT NOT NULL CHECK (state IN ('planned', 'committed', 'cleaning', 'completed')),
    planned_at INTEGER NOT NULL CHECK (planned_at >= 0),
    committed_generation INTEGER CHECK (
        committed_generation IS NULL OR committed_generation >= 0
    ),
    updated_at INTEGER NOT NULL CHECK (updated_at >= 0),
    last_error_code TEXT,
    last_error_message TEXT
);

CREATE INDEX IF NOT EXISTS purge_operations_memory_state
    ON purge_operations (memory_id, state);
"#;
