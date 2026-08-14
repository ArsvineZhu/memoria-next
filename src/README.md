# `src/` source guide

`src/` is the TypeScript host and public package layer. It does not replace the
Rust Authority or query implementation; it validates public input, owns
provider side effects, and maps the N-API protocol into a usable API.

## Module map

| Module       | Role                                                                                                   |
| ------------ | ------------------------------------------------------------------------------------------------------ |
| `engine/`    | `createMemoria` construction, store lifetime, query cancellation, and provider pump                    |
| `domain/`    | Public IDs, configuration, errors, Space/Document APIs, query normalization, and feedback              |
| `authoring/` | Restricted MDX serializer for simple declarative documents                                             |
| `native/`    | N-API protocol types, tagged provider work, and package-root native loader                             |
| `providers/` | Embedding, rerank, enrichment contracts, validation, retries, and egress guard                         |
| `admin/`     | Backup/restore, portable export/import, scoped discovery, and planned purge                            |
| `agent/`     | Host-authoritative identity resolution, owner discovery, permission-gated tools, and conflict recovery |
| `retrieval/` | Narrow TypeScript rerank adapter with scope filtering before provider calls                            |

## Public boundary

`src/index.ts` is the root export surface. The authoring subpath is
`src/authoring/index.ts` and is published as `@arsvinezhu/memoria/authoring`.
Internal modules under `native/`, `providers/`, and `admin/` are implementation
details unless exported through the root or the documented subpath.

The native loader resolves from the installed package directory. It must not
use the consumer process working directory; the packed consumer smoke test
protects this invariant.

## Editing guidance

Keep TypeScript responsible for validation, cancellation, provider/network
effects, and user-facing error mapping. Keep canonical state transitions and
retrieval computation in Rust. When a public type changes, update the source,
generated declarations, focused tests, and [docs/API.md](../docs/API.md).
