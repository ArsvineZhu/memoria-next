# Troubleshooting

Classify the failure before changing files. The Store, Authority database,
Derived catalog, native loader, and provider boundary have different recovery
rules.

| Symptom/code                | Meaning                                                                                        | Safe next step                                                                                                                 |
| --------------------------- | ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `STORE_LOCKED`              | Another logical writer owns the Store                                                          | Close the existing process and retry; do not delete `runtime/writer.lock` while it is active.                                  |
| `UNSUPPORTED_STORE_FORMAT`  | The directory has no valid Next marker/layout                                                  | Choose an empty/new directory or perform an explicit supported transfer; do not initialize over unrelated files.               |
| `STORE_CORRUPT`             | Required file, schema, marker, or manifest validation failed                                   | Preserve the directory, copy it for diagnosis, and restore a verified backup rather than editing SQLite or manifests manually. |
| `CORRUPTION`                | An Authority/CAS integrity check found bytes or identity inconsistent with their recorded hash | Stop writes, preserve the Store, and restore a verified backup; never accept or overwrite the mismatched object in place.      |
| `HEAD_CONFLICT`             | A revision changed after the caller read its expected HEAD                                     | Read the latest revision, re-evaluate the intent, and regenerate a typed patch or ask the Host.                                |
| `CAPABILITY_NOT_READY`      | A provider capability is denied or not converged                                               | Use the provider-free Base path, change explicit privacy/provider configuration, or wait for the capability.                   |
| `PROVIDER_UNAVAILABLE`      | Provider work failed or returned unusable output                                               | Inspect provider credentials/network and payload validation; Authority commits remain durable.                                 |
| `ABORTED` / `QUERY_TIMEOUT` | The caller cancelled or exceeded its query timeout                                             | Retry only with a fresh request and an appropriate signal/timeout.                                                             |
| `RESOURCE_LIMIT`            | Source or another bounded input exceeded a configured limit                                    | Reduce the input or deliberately raise the limit after assessing memory and disk cost.                                         |
| `STORE_CLOSED`              | An operation used a closed `Memoria` instance                                                  | Open a new instance; `close()` is terminal and idempotent.                                                                     |

## Native loading

The package loader resolves `native/index.js` relative to the installed package,
so running a consumer from another working directory is supported. If the
native binding cannot load, verify the pinned Node version, rebuild locally,
and run:

```text
corepack pnpm build:native
corepack pnpm verify:pack
```

The packed consumer check is stronger than checking that `dist/index.js`
exists: it installs the tarball and executes a native create/query flow.

## Provider and semantic coverage

An Authority mutation can succeed before provider work completes. Check
`memoria.status()` for `authorityGeneration`, `baseCoverage`, and
`semanticCoverage`. A degraded query or missing semantic coverage is not a
reason to discard Authority data. Provider egress policy is enforced before
payload dispatch; inspect the capability-specific policy first.

## Backup, import, and purge

Backups require a valid marker and Authority database. The Rust backup path
pins one Authority generation, uses SQLite online backup, verifies reachable
CAS objects, writes a manifest, and writes `COMPLETE` last. Restore validates
the marker, manifest, SQLite integrity, Store identity, and object hashes in a
staging directory; it refuses to overwrite an existing target. A missing or
invalid `COMPLETE` marker is an incomplete backup, not a recoverable live
Store.

Portable export requires an explicit scope. Native import rewrites only
package-internal MemoryRefs, leaves external references unresolved, validates
the rewritten documents before the single Authority commit, and persists its
fingerprint/mapping. The same idempotency key with a different package is an
`IDEMPOTENCY_CONFLICT`; do not retry it blindly with modified input.

Purge is planned before execution and is journaled as `planned`, `committed`,
`cleaning`, or `completed`. Reopen resumes committed/cleaning work. A planned
row remains an explicit plan. Preserve the original Store if a purge, restore,
CAS verification, or integrity check reports a conflict.

## Release checks on Windows

The repository's current evidence is host-specific. Run the commands in
[TESTING.md](TESTING.md) and record the exact Node, Rust, architecture, and
native target. Do not describe Linux or macOS as verified when those hosts have
not run the matrix. GitHub Actions are intentionally disabled.
