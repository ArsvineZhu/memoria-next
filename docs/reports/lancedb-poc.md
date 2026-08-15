# LanceDB Retrieval POC

decision: FAIL

Candidate: `lancedb = 0.33.0` on the pinned Rust toolchain (`rustc 1.97.1`).

## Failed hard gate

The isolated Task 5.1 compile could not reach the POC test because the
LanceDB 0.33.0 dependency graph (`lance-*` crates) runs `prost-build` scripts
that require `protoc`. No `protoc` executable was available on this machine.

Observed command:

```text
cargo test -p memoria-derived --features lancedb-poc --test lancedb_poc_open
```

Observed failure:

```text
Could not find `protoc`. If `protoc` is installed, try setting the `PROTOC`
environment variable to the path of the `protoc` binary.
```

The failure occurred in `lance-datafusion`, `lance-file`, `lance-encoding`,
`lance-table`, and `lance-index` build scripts before any LanceDB adapter code
could compile or run. The pinned toolchain therefore does not satisfy the
LanceDB POC compile gate in the current environment.

## Decision boundary

- Phase 6 LanceDB production migration was not started.
- No LanceDB dependency, feature, adapter, or fallback retrieval path remains
  in the repository.
- The existing Tantivy/USearch retrieval substrate remains unchanged.
- A future run may repeat the POC only after `protoc` is deliberately supplied
  by the build environment and the pinned compile gate is rerun.

The machine-level missing `protoc` tool is recorded as an environment/toolchain
blocker, not as evidence that LanceDB's retrieval semantics pass or fail.
