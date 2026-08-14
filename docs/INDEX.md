# Documentation index

## Current behavior

| Document                                               | Purpose                                                                                       | Canonical owner                                                       |
| ------------------------------------------------------ | --------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| [ARCHITECTURE.md](ARCHITECTURE.md)                     | Layering, ownership, authority boundaries, and runtime flow                                   | Rust workspace and TypeScript source                                  |
| [API.md](API.md)                                       | Public package imports, `Memoria` methods, errors, providers, and Agent tools                 | `src/index.ts` and generated declarations                             |
| [PERSISTENCE.md](PERSISTENCE.md)                       | Store layout, generations, revision history, CAS, lock, backup, transfer, and purge           | `memoria-authority` and `memoria-runtime`                             |
| [ALGORITHMS.md](ALGORITHMS.md)                         | MDX/IR, invalidation, Derived projections, query channels, Tags, graph evidence, and Adaptive | `memoria-mdx`, `memoria-derived`, `memoria-query`, `memoria-adaptive` |
| [CONFIGURATION.md](CONFIGURATION.md)                   | Validated `createMemoria` configuration and privacy/resource boundaries                       | `src/domain/config.ts` and `src/engine/create-memoria.ts`             |
| [TESTING.md](TESTING.md)                               | Local Rust, TypeScript, package, benchmark, and documentation gates                           | `package.json`, Cargo workspace, scripts                              |
| [TROUBLESHOOTING.md](TROUBLESHOOTING.md)               | Failure classification and safe recovery                                                      | error mapping, tests, and release checks                              |
| [NATIVE-MATRIX.md](NATIVE-MATRIX.md)                   | Native build/load and local platform release matrix                                           | `crates/memoria-napi`, local release checks                           |
| [RELEASE-CHECKLIST-NEXT.md](RELEASE-CHECKLIST-NEXT.md) | Release-hardening checklist and open platform evidence                                        | local release gates and Gate J                                        |

## Decision records

- [ADR 0001: implementation baseline](decisions/0001-next-implementation-baseline.md)
- [ADR 0002: retrieval defaults](decisions/0002-retrieval-defaults.md)
- [ADR 0003: Adaptive V1 defaults](decisions/0003-adaptive-v1.md)
- [ADR 0004: at-rest encryption feasibility](decisions/0004-at-rest-encryption.md)
- [ADR 0005: incremental release baseline](decisions/0005-release-performance-baseline.md)

## Adjacent operational references

- [Repository index](../INDEX.md)
- [Source guide](../src/README.md)
- [Memoria Agent Skill](../skills/memoria/SKILL.md)
- [Encryption benchmark](../benchmarks/encryption/README.md)

The release-hardening references are evidence records, not substitutes for the
current architecture or API documents above. The native matrix explicitly
separates executed Windows evidence from unexecuted Linux and macOS runs.
