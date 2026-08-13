pub(crate) const SCHEMA_V1: &str = r#"
PRAGMA foreign_keys = ON;
PRAGMA user_version = 1;

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
        committed_at_subsec_nanos < 1000000000
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

CREATE TABLE IF NOT EXISTS idempotency_records (
    idempotency_key TEXT PRIMARY KEY NOT NULL,
    request_fingerprint TEXT NOT NULL,
    result_kind TEXT NOT NULL,
    result_id BLOB,
    committed_generation INTEGER NOT NULL CHECK (committed_generation >= 0)
);

CREATE TABLE IF NOT EXISTS import_records (
    import_id BLOB PRIMARY KEY NOT NULL,
    origin_store_id BLOB,
    origin_record_id TEXT NOT NULL,
    status TEXT NOT NULL,
    committed_generation INTEGER NOT NULL CHECK (committed_generation >= 0)
);
"#;
