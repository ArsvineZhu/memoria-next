# ADR 0013: Runtime remediation completion

## Status

Incomplete — superseded by production serving integration.

The infrastructure remediation recorded below is retained as historical
evidence, but the completion claim is withdrawn until the production query
executor proves multi-channel serving, strict capability readiness, real
post-consolidation reranking, capability-gated Adaptive ordering, and a
benchmark that exercises the Runtime path rather than a retrieval simulator.

## Decision

The remediation plan is implemented as a Rust-owned Authority/Derived/Adaptive
runtime with explicit provider work, manifest-pinned serving, durable
governance state, and local release evidence. A capability is documented as
serving only when the runtime path, persistence boundary, regression test, and
operator/benchmark evidence all agree. Module existence alone is not release
evidence.

## Review-finding traceability

The following records each finding from the remediation traceability matrix,
the implementation commits, the regression evidence, and public API impact.

| Review finding                                                                     | Repair commits                                        | Regression evidence                                                                                                                                                                                | Public API impact                                                                                                                                                  |
| ---------------------------------------------------------------------------------- | ----------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Semantic capability could appear ready without usable vectors or an artifact.      | `22b0691`, `68819f7`, `809fa36`, `74747e9`, `9a4dbca` | `memoria-derived/tests/embedding_publication.rs`, `memoria-runtime/tests/semantic_serving.rs`, `tests/integration/semantic-query.test.ts`                                                          | Semantic serving is still explicit; readiness now requires a validated manifest-pinned ANN artifact.                                                               |
| Embedding work could send raw MDX.                                                 | `2ec56d9`, `16ded00`                                  | `memoria-derived/tests/provider_projection.rs`, `tests/security/provider-egress.test.ts`                                                                                                           | Provider payloads remain projection-shaped; raw source is not a supported provider input.                                                                          |
| Runtime could rescan Authority or use a substring path instead of Derived serving. | `9a4dbca`                                             | `memoria-runtime/tests/remediation_baseline.rs::normal_current_query_does_not_reparse_every_authority_source`                                                                                      | Current query behavior is faster and more deterministic; historical/rebuild paths retain source reads.                                                             |
| Adaptive event/state was process-memory only.                                      | `9a4dbca`                                             | `memoria-runtime/tests/adaptive_restart.rs`, `memoria-adaptive/tests/persistence.rs`                                                                                                               | Feedback and Adaptive generations survive restart without changing feedback method names.                                                                          |
| Background provider/Derived jobs could be lost on restart.                         | `809fa36`, `9a4dbca`                                  | `memoria-derived/tests/scheduler_recovery.rs`, `memoria-runtime/tests/restart_recovery.rs`                                                                                                         | Reopen recovers queued/running durable jobs; provider work remains typed.                                                                                          |
| Backup copied live SQLite files instead of a consistent snapshot.                  | `cef9f05`                                             | `memoria-runtime/tests/backup_consistency.rs`, `tests/security/backup-restore.test.ts`                                                                                                             | `admin.createBackup` and `restoreBackup` retain their names but now use Rust online backup, pinned generations, reachable CAS verification, and staged activation. |
| Portable import did not rewrite package-internal `MemoryRef` values.               | `cef9f05`                                             | `memoria-runtime/tests/import_remap.rs::internal_memory_refs_are_rewritten_to_target_memory_ids`, `tests/security/import.test.ts`                                                                  | Import returns target mappings and unresolved external references; internal references are remapped before commit.                                                 |
| Import idempotency was process-memory only.                                        | `cef9f05`                                             | `memoria-runtime/tests/import_remap.rs::import_retry_after_process_restart_returns_same_mapping`, `::same_idempotency_key_with_different_package_fingerprint_conflicts`                            | Retry is durable; a changed fingerprint returns `IDEMPOTENCY_CONFLICT`.                                                                                            |
| Purge journal/state was process-memory only.                                       | `cef9f05`                                             | `memoria-runtime/tests/purge_recovery.rs`, `tests/security/purge.test.ts`                                                                                                                          | Purge is a durable `planned → committed → cleaning → completed` operation and resumes after reopen.                                                                |
| N-API query conversion collapsed structured intent to scope plus text.             | `1510bb5`, `a83e21c`, `e43d81e`                       | `memoria-napi/tests/query_conversion.rs`, `tests/public/native-protocol.test.ts`                                                                                                                   | Structured scope, constraints, history, readiness, budget, and quality remain explicit in the native contract.                                                     |
| N-API inferred semantic capability from a text cue.                                | `54e535d`, `a83e21c`, `e43d81e`                       | `memoria-napi/tests/query_conversion.rs::text_only_native_query_has_no_semantic_capability`, `tests/integration/remediation-baseline.test.ts`                                                      | Text-only queries remain provider-free; semantic required/preferred must be requested explicitly.                                                                  |
| Merge `RevisionId` depended on parent input order.                                 | `cef9f05`                                             | `memoria-authority/tests/authority_lifecycle.rs::merge_revision_keeps_all_valid_same_memory_parents`, `::merge_batch_publishes_one_generation_and_updates_head`                                    | Merge identity is deterministic; duplicate/invalid parent sets are rejected.                                                                                       |
| Restricted MDX accepted raw lowercase HTML.                                        | `cef9f05`                                             | `memoria-mdx/tests/profile_golden.rs`, including script/img/comment/fenced-code cases                                                                                                              | The restricted authoring boundary is narrower: raw HTML is rejected while comments and code remain nonsemantic source text.                                        |
| EntityRef grammar was duplicated and inconsistent.                                 | `cef9f05`                                             | `memoria-types/tests/entity_ref.rs`, `memoria-mdx/tests/referential_semantics.rs`                                                                                                                  | One canonical namespace-qualified grammar is used across MDX, query, Derived, and N-API conversion.                                                                |
| Core `kind` used a global rather than element-specific whitelist.                  | `cef9f05`                                             | `memoria-mdx/tests/referential_semantics.rs::core_kind_is_checked_against_the_element_schema`                                                                                                      | Invalid Core kinds are rejected; opaque extension `class` metadata remains allowed.                                                                                |
| CAS existing-object reuse trusted a corrupt object.                                | `cef9f05`                                             | `memoria-authority/tests/crash_protocol.rs`, CAS unit coverage                                                                                                                                     | Existing CAS objects are rehashed before reuse; a mismatch is surfaced as `CORRUPTION`.                                                                            |
| SQLite default journaling/busy behavior was weak under reader/writer contention.   | `cef9f05`                                             | `memoria-authority/tests/concurrency.rs`, `memoria-derived/tests/catalog_concurrency.rs`, `memoria-adaptive/tests/concurrency.rs`                                                                  | Authority, Derived, and Adaptive use explicit WAL, busy timeout, and bounded immediate-transaction retry policy.                                                   |
| Provider privacy was Store-global rather than Space-scoped.                        | `16ded00`                                             | `tests/security/space-provider-policy.test.ts`, `memoria-runtime/tests/query_operation.rs`                                                                                                         | Space provider policy is Authority-owned and the most restrictive policy governs a multi-Space scope.                                                              |
| Original algorithms existed but did not participate in serving.                    | `9a4dbca`                                             | `memoria-query/tests/algorithm_trace.rs`, `tag_basis_integration.rs`, `activation_integration.rs`, `relation_rerank.rs`, `memoria-runtime/tests/semantic_serving.rs`, retrieval benchmark profiles | Named channels execute behind bounded profiles; default text retrieval remains provider-free lexical.                                                              |
| Graph Diffusion was an Activation alias.                                           | `9a4dbca`                                             | `memoria-query/tests/diffusion.rs`, `memoria-query/tests/algorithm_trace.rs`, `benchmarks/retrieval/preservation-corpus.jsonl`                                                                     | Diffusion is independently normalized and traced; it remains an advanced/Thorough operator rather than a Balanced default.                                         |

## Durability and security invariants

The final local gate covers zero provider calls for text-only queries, explicit
semantic behavior, Rust-owned pending query work, vector round trips, zero raw
MDX provider leakage, manifest-only semantic readiness, zero normal current
Authority reparses, ANN contribution, Tag Basis/Residual and propagation
traces, hard-constraint ordering, generated-Tag restart persistence, job and
Adaptive recovery, backup identity consistency, import reference remapping,
purge recovery, raw HTML rejection, single-source EntityRef validation, merge
canonicalization, CAS corruption rejection, SQLite contention, Space provider
policy, and the packed consumer.

The public surface changes are intentional and limited to the hard-reset
contract: native backup/restore/import entry points are generated and wired,
`CORRUPTION` is recognized by the TypeScript error mapper, and the restricted
MDX/EntityRef/kind validation boundary rejects inputs that were previously
accepted ambiguously. Existing `Memoria` method names and query response
shapes remain stable.

## Benchmark disposition

The checked-in retrieval fixture reports the following release profiles on the
Windows workstation. `fast` and `balanced` are provider-free lexical profiles;
`thorough` exercises the diagnostic rerank/ANN path. All reported
hard-constraint violation counts are zero. These are deterministic fixture
measurements, not production latency claims.

| Profile  | Recall@K |    MRR | nDCG@K | Provider calls | ANN searches | Rerank calls | Hard-constraint violations |
| -------- | -------: | -----: | -----: | -------------: | -----------: | -----------: | -------------------------: |
| fast     |   1.0000 | 1.0000 | 0.9532 |              0 |            0 |            0 |                          0 |
| balanced |   1.0000 | 1.0000 | 0.9532 |              0 |            0 |            0 |                          0 |
| thorough |   1.0000 | 0.9375 | 0.9387 |              8 |            8 |            8 |                          0 |

The incremental harness passes with two content-embedding calls across the
revision cases: one prose edit and one entity-surface edit. Formatting/comment,
EntityRef-only, explicit-Tag, `validTo`, relation, and source-locator edits use
zero content-embedding calls. The Space-move case reports immutable vector
payload reuse. The canonical Relation fixture uses `associated-with`, matching
the element-specific MDX schema.

## Platform disposition

Windows `win32-x64-msvc` is the executed local matrix. Linux and macOS remain
OPEN until their native hosts execute the same matrix. GitHub Actions remain
disabled by design and are not substituted for local platform evidence.
