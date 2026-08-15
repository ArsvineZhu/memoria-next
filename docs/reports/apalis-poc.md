# Apalis Durable Job POC

decision: FAIL

Candidate: `apalis = 0.7.4` with `apalis-sql = 0.7.4` SQLite storage.

## Failed hard gate

Cargo could not resolve the isolated SQLite POC in the current workspace:

```text
cargo test -p memoria-derived --test apalis_poc
```

`apalis-sql 0.7.4` requires `sqlx-sqlite 0.8.1`, which requires
`libsqlite3-sys 0.30.1`. Memoria's pinned `rusqlite 0.40.2` requires
`libsqlite3-sys 0.38.2`. Cargo rejects both packages because they declare the
same native `links = "sqlite3"` value.

The failure happens during dependency resolution before an Apalis queue or
characterization test can compile. The SQLite backend therefore cannot be
evaluated safely without changing the repository's pinned Authority storage
stack or introducing an unsupported native-link workaround.

## Decision boundary

- Apalis was not promoted into the production scheduler.
- No Apalis source, dependency, compatibility shim, or second queue state
  machine remains in the repository.
- The existing Memoria durable build-job table and scheduler remain unchanged.
- The split-phase provider bridge gate was not run because the isolated queue
  prerequisite failed at dependency resolution.

The native SQLite link conflict is recorded as the exact POC rejection reason;
it is not a request to weaken the Authority storage contract.
