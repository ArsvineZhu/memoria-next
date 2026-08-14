# ADR 0004: Store at-rest encryption feasibility gate

## Status

Accepted for this release line: Memoria Next does not advertise encrypted
at-rest storage or Store-wide confidentiality.

## Context

The physical Store is wider than Authority SQLite. A complete decision must
cover Authority, Source CAS, Derived artifacts, Adaptive state, Cache, backup
copies, key rotation, and the supported operating systems. Encrypting only the
Authority database would leave source text, derived indexes, feedback history,
or cache material readable and would therefore be an incorrect Store-wide
security claim.

The repository does not currently contain a production SQLite encryption VFS,
encrypted random-access artifact format, key-provider contract, crash-safe
rotation protocol, or platform-specific filesystem requirement. This gate
therefore evaluates feasibility without wiring a throwaway envelope format
into the runtime.

## Coverage matrix

| Store area             | Authority-only prototype | Managed-Store prototype | Production conclusion                                                        |
| ---------------------- | ------------------------ | ----------------------- | ---------------------------------------------------------------------------- |
| Authority SQLite       | AES-256-GCM envelope     | AES-256-GCM envelope    | Requires a production SQLite/VFS design                                      |
| Source CAS             | Plaintext                | AES-256-GCM envelope    | Must preserve content addressing and safe random reads                       |
| Derived mmap artifacts | Plaintext                | AES-256-GCM envelope    | Must define encrypted rebuildable artifact semantics                         |
| Adaptive               | Plaintext                | AES-256-GCM envelope    | Must include event/checkpoint files and recovery                             |
| Cache                  | Plaintext                | AES-256-GCM envelope    | Cache policy and purge behavior need explicit treatment                      |
| backup                 | Copies prototype bytes   | Copies prototype bytes  | Backup keys, manifest identity, and restore failure semantics are unresolved |
| key rotation           | One Authority file       | All protected files     | Needs durable journal, atomic replacement, and crash recovery                |
| Windows                | Measured on win32/x64    | Measured on win32/x64   | No production Windows integration selected                                   |
| Linux                  | Not measured             | Not measured            | Platform result is unknown                                                   |
| macOS                  | Not measured             | Not measured            | Platform result is unknown                                                   |

Only the Node-maintained AES-256-GCM primitive was used in the throwaway
benchmark. It is a measurement vehicle, not an approval of an application
storage format or a replacement for a reviewed SQLite/VFS/library design.
No new cryptographic dependency was added.

## Prototypes and benchmark

The two minimal prototypes are in benchmarks/encryption/measure.ts and are
documented in benchmarks/encryption/README.md. They use the same fixture and
measure open, write, large sequential read, large random read, backup/restore,
and re-encryption/key rotation. The benchmark copies files for backup/restore;
it does not claim to reproduce the production backup API.

Command:

    corepack pnpm exec tsx benchmarks/encryption/measure.ts

The final local run used Node v24.19.0 on win32/x64 with a 4,194,304-byte large
artifact. Values are wall-clock diagnostics from one run and are not a
cross-platform performance claim.

| prototype      | open ms | write ms | sequential read ms | random read ms | backup/restore ms | key rotation ms |
| -------------- | ------: | -------: | -----------------: | -------------: | ----------------: | --------------: |
| authority-only |   3.220 |    1.230 |              1.617 |          1.849 |            35.183 |           6.694 |
| managed-store  |   7.186 |    1.065 |              4.564 |          5.033 |            40.984 |          26.383 |

The checksums matched after rotation, so the fixture content survived both
prototype paths. This is integrity evidence for the harness only, not
evidence of production key management.

## Decision

Do not ship either prototype as the Store encryption implementation. Do not
label Authority-only encryption as Store-wide confidentiality. This release
does not advertise at-rest encryption.

A future implementation may be proposed only after selecting a maintained and
reviewed SQLite/VFS or managed-store approach, defining key ownership and
rotation, preserving CAS and Derived invariants, specifying backup/restore
behavior, and completing Windows, Linux, and macOS measurements. The future
gate must also include crash interruption tests and migration from the current
plaintext layout.

## Consequences

- The current Store remains plaintext at rest; callers must use filesystem and
  host-level protection when confidentiality is required.
- No unreviewed encryption dependency or opaque file format enters the runtime.
- The public API must not promise encrypted storage until a later decision
  changes this ADR and the complete Store path is implemented and tested.
