# Memoria Next documentation

This directory is the human-facing documentation root. The implementation and
tests remain authoritative for exact behavior; these documents explain the
current architecture, public contract, operations, and release decisions.

## Reading paths

- New users: [API.md](API.md), then the first-use example in the repository
  [README](../README.md).
- System design: [ARCHITECTURE.md](ARCHITECTURE.md),
  [PERSISTENCE.md](PERSISTENCE.md), and [ALGORITHMS.md](ALGORITHMS.md).
- Operators: [CONFIGURATION.md](CONFIGURATION.md) and
  [TROUBLESHOOTING.md](TROUBLESHOOTING.md).
- Contributors: [TESTING.md](TESTING.md) and the source guide in
  [../src/README.md](../src/README.md).
- Decisions and measured gates: [decisions/](decisions/0001-next-implementation-baseline.md)
  and the benchmark-local documentation.

The maintained catalog is [INDEX.md](INDEX.md). Documentation-specific
working rules are in [AGENTS.md](AGENTS.md).

## Current-release language

These pages describe the hard-reset Next line. They do not document removed
public APIs, previous Store layouts, or historical native package names. A
historical note must be fenced with the explicit markers understood by
`scripts/verify-docs.mjs`; current guidance must never rely on it.
