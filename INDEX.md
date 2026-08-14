# Repository index

Memoria Next is a Rust workspace with a TypeScript package boundary. Use this
file to choose the smallest scope needed for a change.

## Scopes

| Path                       | Responsibility                                                                                         | Entry point                                        |
| -------------------------- | ------------------------------------------------------------------------------------------------------ | -------------------------------------------------- |
| `crates/memoria-types`     | Stable IDs, generations, timestamps, errors, and snapshot value types                                  | crate `lib.rs`                                     |
| `crates/memoria-authority` | Store layout, writer lock, Authority schema, source CAS, revisions, lifecycle, and purge rows          | [docs/PERSISTENCE.md](docs/PERSISTENCE.md)         |
| `crates/memoria-mdx`       | Restricted MDX parsing, validation, canonical IR, semantic diff, lint, and typed patches               | [docs/ALGORITHMS.md](docs/ALGORITHMS.md)           |
| `crates/memoria-derived`   | Rebuildable projections, provider work, artifacts, manifests, leases, and cleanup                      | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)       |
| `crates/memoria-query`     | Structured query compilation, candidate evidence, fusion, history, continuation, and bounded operators | [docs/API.md](docs/API.md)                         |
| `crates/memoria-adaptive`  | Explicit feedback log, reducer, bounded priors, checkpoint, reset, and purge rewrite                   | [docs/ALGORITHMS.md](docs/ALGORITHMS.md)           |
| `crates/memoria-runtime`   | Authority-first runtime orchestration and Rust-side provider work protocol                             | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)       |
| `crates/memoria-napi`      | N-API v3 binding consumed by the TypeScript engine                                                     | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)       |
| `src`                      | Public TypeScript engine, domain APIs, provider host, Agent tools, and administration                  | [src/README.md](src/README.md)                     |
| `skills/memoria`           | Host-agnostic Agent operating rules for long-term memory                                               | [skills/memoria/SKILL.md](skills/memoria/SKILL.md) |
| `benchmarks`               | Deterministic retrieval, Adaptive, encryption, and release measurements                                | benchmark-local README files                       |
| `docs`                     | Human-facing architecture, API, operation, testing, and decision records                               | [docs/INDEX.md](docs/INDEX.md)                     |

## Canonical ownership

- Public imports and method signatures are owned by the TypeScript source and
  its generated declarations; [docs/API.md](docs/API.md) explains them.
- Store paths and persistence invariants are owned by the Rust Authority
  layout/schema; [docs/PERSISTENCE.md](docs/PERSISTENCE.md) summarizes them.
- Projection dependencies and query behavior are owned by Rust crate code and
  focused tests; [docs/ALGORITHMS.md](docs/ALGORITHMS.md) is explanatory.
- Release decisions and measured limits are owned by the ADRs under
  [docs/decisions](docs/decisions/0001-next-implementation-baseline.md).
- AI-facing operational rules are layered through [AGENTS.md](AGENTS.md) and
  the local `AGENTS.md` files.

## Change navigation

1. Read [AGENTS.md](AGENTS.md) and the relevant local instructions.
2. Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) before crossing Rust and
   TypeScript boundaries.
3. Use the narrowest crate/source tests first.
4. Run the release checks in [docs/TESTING.md](docs/TESTING.md) before claiming
   a complete change.
