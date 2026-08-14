# Persistence and recovery

## Store layout

Opening a new empty directory creates the Next Store layout:

```text
<dataDir>/
├── STORE                         # stable StoreId marker
├── authority/
│   ├── authority.sqlite          # Authority schema and generation history
│   └── objects/                  # source CAS objects
├── derived/                      # rebuildable catalog and artifacts
├── adaptive/                     # replayable feedback state/checkpoints
├── cache/                        # disposable runtime cache area
└── runtime/
    └── writer.lock               # one logical writer owner
```

The native runtime creates the marker and required directories. A non-empty
directory without the `STORE` marker is rejected as
`UNSUPPORTED_STORE_FORMAT`; an existing Store is opened only when the marker,
Authority database, and required directories are present. This prevents a
different Store layout from being silently initialized as Next.

## Authority model

Authority SQLite stores Space and Memory identity, generation-bounded lifecycle
history, revision rows, ordered revision parents, idempotency records, and
portable-import records. Revision source bytes are addressed by a SHA-256
content hash in the Authority record and stored in the source CAS.

Writes are serialized by the Store writer lock. Each successful mutation
advances `authorityGeneration`. A revision mutation requires the caller's
expected HEAD; if the HEAD changed, the write fails with `HEAD_CONFLICT` and no
automatic merge is attempted.

Space and Memory state history is interval-checked so current and historical
reads cannot overlap accidentally. Retiring a Memory changes lifecycle
authority; it does not manufacture a new content revision.

## Derived and Adaptive durability

Derived data is rebuildable from Authority and source objects. Catalog entries
track artifact state, producer/projection inputs, Authority generation, and
validation. Immutable manifests bind a coherent set of artifacts to a
generation. Provider-dependent coverage can lag while provider-free Base
coverage remains available.

Adaptive data is an explicit event log plus replayed V1 state. It is scoped by
Space and Memory and is never inferred from query exposure. Purge rewrites
Adaptive state, clears retrieval receipts, removes derived artifacts, and
deletes unreferenced source objects as a resumable managed operation.

## Backup and portable transfer

`admin.createBackup` copies the Store marker and Authority tree and optionally
Adaptive data into a manifest-bearing backup directory. The manifest hashes
files and records the Store identity. The destination cannot be inside the
source Store. Restore verifies the manifest, stages the copy beside the target,
and renames it into place only after validation; an existing target is rejected.

Portable export requires an explicit Space scope. It writes a
`memoria-portable-v1` package containing current scoped Memory source and
identity metadata. Import creates a target Space and fresh target Memory IDs,
uses per-Memory idempotency keys, and reports references that could not be
remapped inside the package.

## Crash and lock behavior

Only one logical writer may own a Store at a time. A second opener should report
`STORE_LOCKED`; close the existing owner before retrying. Derived and provider
work may be incomplete after interruption; reopen the Store and let provider-
free rebuild and queued work converge. Do not edit the marker, SQLite files,
manifest, or purge state by hand.

The current release line does not advertise Store-wide encryption at rest.
Use host/filesystem protection when confidentiality is required and consult
[ADR 0004](decisions/0004-at-rest-encryption.md) before proposing a storage
format change.
