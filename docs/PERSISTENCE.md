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
history, revision rows, canonicalized revision parents, idempotency records,
portable-import records, and the durable purge journal. Revision source bytes
are addressed by a SHA-256 content hash in the Authority record and stored in
the source CAS. `import_records` keeps the request fingerprint, target
mapping, and unresolved-reference report so a retry does not depend on process
memory.

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
coverage remains available. Derived catalog writes use WAL and bounded busy
retries, and queued/running jobs are recovered from their durable job state on
reopen.

Adaptive data is an explicit SQLite event log plus replayed/materialized V1
state. It is scoped by Space and Memory, uses WAL and bounded write retries,
and is never inferred from query exposure. Reopen restores the persisted
Adaptive generation, events, and materialized state. Purge rewrites Adaptive
state, clears retrieval receipts, removes Derived artifacts, and deletes
unreferenced source objects as a resumable managed operation.

## Backup and portable transfer

`admin.createBackup` is a Rust-coordinated snapshot. It pins one Authority
generation, uses SQLite online backup for `authority.sqlite`, enumerates the
source blob hashes reachable from that pinned snapshot, copies and verifies
those CAS objects, and optionally performs the same online backup for
`adaptive.sqlite`. It writes file hashes, Store identity, generation, options,
and reachable object hashes to `backup-manifest.json`, synchronizes staged
files, and writes `COMPLETE` last. Derived and Cache are intentionally excluded
from the identity snapshot because they are rebuildable/disposable.

The destination cannot be inside the source Store. Restore requires
`COMPLETE`, verifies every manifest hash and CAS object, checks SQLite
integrity and Store identity, stages a new Store with empty rebuildable planes,
and activates it only after validation. It never overwrites an existing target.

Portable export requires an explicit Space scope. It writes a
`memoria-portable-v1` package containing current scoped Memory source and
identity metadata. Native import allocates the target Space and all target
Memory IDs before rewriting package-internal `MemoryRef` source spans. Every
rewritten source is parsed and validated before one Authority transaction
creates the target rows and persists the import fingerprint/mapping. External
references are not guessed: they remain unresolved in the result. A retry with
the same idempotency key and fingerprint returns the stored mapping; a changed
fingerprint is rejected.

## Crash and lock behavior

Only one logical writer may own a Store at a time. A second opener should report
`STORE_LOCKED`; close the existing owner before retrying. Authority, Derived,
and Adaptive connections enable foreign keys where applicable, WAL,
`synchronous=NORMAL`, and a bounded busy timeout. Immediate write transactions
use bounded retries for short reader contention.

After interruption, reopen the Store and let provider-free rebuild, durable
Derived jobs, Adaptive replay, and queued provider work converge. Purge
recovery resumes `committed` or `cleaning` journal rows; a `planned` row is
validated and remains an explicit plan until execution is requested. Do not
edit the marker, SQLite files, manifest, or purge state by hand. A CAS hash
mismatch is `CORRUPTION` and must be recovered from a verified backup rather
than bypassed.

The current release line does not advertise Store-wide encryption at rest.
Use host/filesystem protection when confidentiality is required and consult
[ADR 0004](decisions/0004-at-rest-encryption.md) before proposing a storage
format change.
