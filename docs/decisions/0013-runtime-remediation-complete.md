# ADR 0013: Runtime remediation completion

## Status

Accepted — production serving integration verified on Windows.

The remediation is complete for the executed Windows matrix. Linux and macOS
remain open under Platform disposition, and GitHub Actions remain disabled by
design.

## Decision

The remediation plan is implemented as a Rust-owned Authority/Derived/Adaptive
runtime with explicit provider work, manifest-pinned serving, durable
governance state, and local release evidence. The production executor pins a
snapshot, applies hard constraints before ranking, executes the bounded
lexical, semantic, Tag, propagation, relation, fusion, consolidation, and
rerank operators, and only applies Adaptive when the compiled query requests
it. A capability is documented as serving only when the runtime path,
persistence boundary, regression test, and operator/benchmark evidence all
agree. Module existence alone is not release evidence.

## Review-finding traceability

The following records each finding from the remediation traceability matrix,
the implementation commits, the regression evidence, and public API impact.

| Review finding                                                                     | Repair commits                                        | Regression evidence                                                                                                                                                                                 | Public API impact                                                                                                                                                  |
| ---------------------------------------------------------------------------------- | ----------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Semantic capability could appear ready without usable vectors or an artifact.      | `22b0691`, `68819f7`, `809fa36`, `74747e9`, `9a4dbca` | `memoria-derived/tests/embedding_publication.rs`, `memoria-runtime/tests/semantic_serving.rs`, `tests/integration/semantic-query.test.ts`                                                           | Semantic serving is still explicit; readiness now requires a validated manifest-pinned ANN artifact.                                                               |
| Embedding work could send raw MDX.                                                 | `2ec56d9`, `16ded00`                                  | `memoria-derived/tests/provider_projection.rs`, `tests/security/provider-egress.test.ts`                                                                                                            | Provider payloads remain projection-shaped; raw source is not a supported provider input.                                                                          |
| Runtime could rescan Authority or use a substring path instead of Derived serving. | `61f2244`, `c2d4a21`                                  | `memoria-runtime/tests/remediation_baseline.rs::normal_current_query_does_not_reparse_every_authority_source`, `memoria-runtime/tests/serving_path.rs`, `memoria-derived/tests/lexical_artifact.rs` | Current queries use a manifest-pinned Tantivy artifact and Derived records; historical/rebuild paths retain source reads.                                          |
| Adaptive event/state was process-memory only.                                      | `9a4dbca`, `7461ebe`                                  | `memoria-runtime/tests/adaptive_restart.rs`, `memoria-adaptive/tests/persistence.rs`, `memoria-runtime/tests/adaptive_query_gate.rs`                                                                | Feedback and Adaptive generations survive restart, and query ordering is capability-gated.                                                                         |
| Background provider/Derived jobs could be lost on restart.                         | `809fa36`, `9a4dbca`                                  | `memoria-derived/tests/scheduler_recovery.rs`, `memoria-runtime/tests/restart_recovery.rs`                                                                                                          | Reopen recovers queued/running durable jobs; provider work remains typed.                                                                                          |
| Backup copied live SQLite files instead of a consistent snapshot.                  | `cef9f05`                                             | `memoria-runtime/tests/backup_consistency.rs`, `tests/security/backup-restore.test.ts`                                                                                                              | `admin.createBackup` and `restoreBackup` retain their names but now use Rust online backup, pinned generations, reachable CAS verification, and staged activation. |
| Portable import did not rewrite package-internal `MemoryRef` values.               | `cef9f05`                                             | `memoria-runtime/tests/import_remap.rs::internal_memory_refs_are_rewritten_to_target_memory_ids`, `tests/security/import.test.ts`                                                                   | Import returns target mappings and unresolved external references; internal references are remapped before commit.                                                 |
| Import idempotency was process-memory only.                                        | `cef9f05`                                             | `memoria-runtime/tests/import_remap.rs::import_retry_after_process_restart_returns_same_mapping`, `::same_idempotency_key_with_different_package_fingerprint_conflicts`                             | Retry is durable; a changed fingerprint returns `IDEMPOTENCY_CONFLICT`.                                                                                            |
| Purge journal/state was process-memory only.                                       | `cef9f05`                                             | `memoria-runtime/tests/purge_recovery.rs`, `tests/security/purge.test.ts`                                                                                                                           | Purge is a durable `planned → committed → cleaning → completed` operation and resumes after reopen.                                                                |
| N-API query conversion collapsed structured intent to scope plus text.             | `1510bb5`, `a83e21c`, `e43d81e`                       | `memoria-napi/tests/query_conversion.rs`, `tests/public/native-protocol.test.ts`                                                                                                                    | Structured scope, constraints, history, readiness, budget, and quality remain explicit in the native contract.                                                     |
| N-API inferred semantic capability from a text cue.                                | `54e535d`, `a83e21c`, `e43d81e`                       | `memoria-napi/tests/query_conversion.rs::text_only_native_query_has_no_semantic_capability`, `tests/integration/remediation-baseline.test.ts`                                                       | Text-only queries remain provider-free; semantic required/preferred must be requested explicitly.                                                                  |
| Merge `RevisionId` depended on parent input order.                                 | `cef9f05`                                             | `memoria-authority/tests/authority_lifecycle.rs::merge_revision_keeps_all_valid_same_memory_parents`, `::merge_batch_publishes_one_generation_and_updates_head`                                     | Merge identity is deterministic; duplicate/invalid parent sets are rejected.                                                                                       |
| Restricted MDX accepted raw lowercase HTML.                                        | `cef9f05`                                             | `memoria-mdx/tests/profile_golden.rs`, including script/img/comment/fenced-code cases                                                                                                               | The restricted authoring boundary is narrower: raw HTML is rejected while comments and code remain nonsemantic source text.                                        |
| EntityRef grammar was duplicated and inconsistent.                                 | `cef9f05`                                             | `memoria-types/tests/entity_ref.rs`, `memoria-mdx/tests/referential_semantics.rs`                                                                                                                   | One canonical namespace-qualified grammar is used across MDX, query, Derived, and N-API conversion.                                                                |
| Core `kind` used a global rather than element-specific whitelist.                  | `cef9f05`                                             | `memoria-mdx/tests/referential_semantics.rs::core_kind_is_checked_against_the_element_schema`                                                                                                       | Invalid Core kinds are rejected; opaque extension `class` metadata remains allowed.                                                                                |
| CAS existing-object reuse trusted a corrupt object.                                | `cef9f05`                                             | `memoria-authority/tests/crash_protocol.rs`, CAS unit coverage                                                                                                                                      | Existing CAS objects are rehashed before reuse; a mismatch is surfaced as `CORRUPTION`.                                                                            |
| SQLite default journaling/busy behavior was weak under reader/writer contention.   | `cef9f05`                                             | `memoria-authority/tests/concurrency.rs`, `memoria-derived/tests/catalog_concurrency.rs`, `memoria-adaptive/tests/concurrency.rs`                                                                   | Authority, Derived, and Adaptive use explicit WAL, busy timeout, and bounded immediate-transaction retry policy.                                                   |
| Provider privacy was Store-global rather than Space-scoped.                        | `16ded00`, `3219f28`, `c2d4a21`                       | `tests/security/space-provider-policy.test.ts`, `memoria-runtime/tests/query_operation.rs`, `memoria-runtime/tests/provider_trust_gate.rs`                                                          | Space policy is Authority-owned; Rust gates route trust before work emission and TypeScript gates network egress.                                                  |
| Original algorithms existed but did not participate in serving.                    | `6e1c72f`, `10ec95e`, `39edd0e`, `002e825`, `4b2f6a5` | `memoria-runtime/tests/physical_executor_e2e.rs`, `multichannel_executor.rs`, `physical_executor_regressions.rs`, `retrieval benchmark trace`                                                       | Named channels execute behind bounded profiles; default text retrieval remains provider-free lexical.                                                              |
| Graph Diffusion was an Activation alias.                                           | `9a4dbca`, `002e825`                                  | `memoria-query/tests/diffusion.rs`, `memoria-query/tests/algorithm_trace.rs`, `memoria-runtime/tests/physical_executor_e2e.rs`                                                                      | Diffusion is independently normalized, budgeted, and traced; it remains a Thorough operator rather than a Balanced default.                                        |
| Provider rerank scores were returned but discarded.                                | `39edd0e`, `4b2f6a5`                                  | `memoria-runtime/tests/rerank_barrier.rs`, `physical_executor_regressions.rs`, operator trace `rerankApplied`                                                                                       | Provider scores now cross the query operation barrier and affect post-consolidation ordering.                                                                      |
| Required semantic plus `wait` could silently degrade to lexical fallback.          | `09977d2`                                             | `memoria-runtime/tests/capability_wait.rs`, `physical_executor_regressions.rs`                                                                                                                      | Required capabilities remain hard contracts; timeout/readiness failure is surfaced instead of degraded fallback.                                                   |
| Semantic and lexical execution were mutually exclusive.                            | `d5f3774`, `6e1c72f`, `10ec95e`                       | `memoria-runtime/tests/multichannel_executor.rs`, `physical_executor_e2e.rs`                                                                                                                        | Explicit semantic queries retain lexical evidence and fuse multiple retrieval channels.                                                                            |
| Semantic publication could be lost after a compatible Authority generation moved.  | `fa731ab`, `c2d4a21`                                  | `memoria-derived/tests/semantic_rebase.rs`, `memoria-runtime/tests/semantic_rebase.rs`, `semantic_serving.rs`                                                                                       | Compatible immutable payloads rebase to the latest revision/space/generation; changed content supersedes them.                                                     |
| Tag provenance weights were defined but absent from propagation dynamics.          | `f863611`                                             | `memoria-query/tests/provenance_weighting.rs`                                                                                                                                                       | Activation and Diffusion seed mass now honors Explicit, ExactSupport, Semantic, Generated, and Inherited provenance weights.                                       |
| The retrieval benchmark simulated algorithms outside the production Runtime.       | `344f4b0`, `4b2f6a5`                                  | `benchmarks/retrieval/run.ts`, runtime operator trace, deterministic local providers                                                                                                                | Benchmark profiles create a real Store, write through Authority, build Derived, call native `Memoria.query()`, and measure the returned trace.                     |

## Durability and security invariants

The final local gate covers zero provider calls for text-only queries, explicit
semantic behavior, Rust-owned pending query work, vector round trips, zero raw
MDX provider leakage, manifest-only semantic readiness, zero normal current
Authority reparses, persisted lexical and ANN contribution, Tag
Basis/Residual, Activation, Diffusion, Relation, fusion, consolidation, and
post-consolidation rerank traces, hard-constraint ordering, generated-Tag
restart persistence, job and Adaptive recovery, backup identity consistency,
import reference remapping, purge recovery, raw HTML rejection, single-source
EntityRef validation, merge canonicalization, CAS corruption rejection,
SQLite contention, Space provider policy, and the packed consumer.

The public surface changes are intentional and limited to the hard-reset
contract: native backup/restore/import entry points are generated and wired,
`CORRUPTION` is recognized by the TypeScript error mapper, provider route
availability is explicit, and the restricted MDX/EntityRef/kind validation
boundary rejects inputs that were previously accepted ambiguously. The query
response now carries an optional operator trace for serving evidence; existing
`Memoria` method names remain stable.

## Benchmark disposition

The checked-in retrieval fixture now runs through the real Windows native
Runtime path: temporary Store, Authority writes, Derived build, deterministic
local provider work, `Memoria.query()`, and returned `QueryOperatorTrace`.
Every profile in the final `--profile all` run reported `runtimeBacked: true`
and zero hard-constraint violations. The table is one final fixture run, not a
production latency claim or a normative quality threshold.

| Profile            | Recall@K |    MRR | nDCG@K | Provider calls | ANN searches | Graph visits | Diffusion iterations | Rerank calls |
| ------------------ | -------: | -----: | -----: | -------------: | -----------: | -----------: | -------------------: | -----------: |
| lexical            |   0.4583 | 0.4167 | 0.3585 |              0 |            0 |            0 |                    0 |            0 |
| lexical+semantic   |   0.9583 | 0.7542 | 0.7125 |              8 |           16 |            0 |                    0 |            0 |
| tag-basis-residual |   1.0000 | 0.6750 | 0.7377 |              8 |           16 |          132 |                    0 |            0 |
| diffusion          |   0.6250 | 0.5625 | 0.5666 |              0 |            0 |           51 |                   32 |            0 |
| rerank             |   0.4583 | 0.4000 | 0.3247 |              4 |            0 |            0 |                    0 |            4 |
| thorough           |   1.0000 | 0.6250 | 0.6602 |             16 |           16 |          132 |                   64 |            8 |

The trace also observed `exact`, `lexical`, `semantic-direct`,
`semantic-residual`, `tag-readout`, `activation`, `diffusion`, and `relation`
channels in the profiles that request them. The benchmark deliberately uses
local deterministic providers and does not claim external-provider latency or
quality.

## Platform disposition

Windows `win32-x64-msvc` is the executed local matrix. Linux and macOS remain
OPEN until their native hosts execute the same matrix. GitHub Actions remain
disabled by design and are not substituted for local platform evidence.
