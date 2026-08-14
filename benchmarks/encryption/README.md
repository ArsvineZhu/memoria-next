# At-rest encryption feasibility benchmark

This directory contains two throwaway feasibility prototypes for the physical
Store layout:

- authority-only: encrypts authority/authority.sqlite with AES-256-GCM;
- managed-store: encrypts Authority, Source CAS, Derived, Adaptive, and Cache
  artifacts with the same envelope format.

The prototypes are benchmark fixtures, not a production storage layer. They do
not change the runtime, SQLite VFS, N-API ABI, backup format, or key-management
contract. In particular, an authority-only result must never be described as
Store-wide confidentiality.

Run the deterministic local harness from the repository root:

    corepack pnpm exec tsx benchmarks/encryption/measure.ts

The harness measures open, write, large sequential read, large random read,
backup/restore, and re-encryption/key rotation. It reports local wall-clock
diagnostics only; results are not a cross-platform performance claim.
