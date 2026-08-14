# Configuration

`createMemoria` accepts a `MemoriaConfig` with a required non-empty `dataDir`.
Unknown top-level and nested keys are rejected as
`UNSUPPORTED_OPERATION`; numeric runtime knobs must be positive safe integers.

## Shape

```ts
interface MemoriaConfig {
  dataDir: string;
  providers?: {
    embedding?: {
      execute(work: unknown, signal: AbortSignal): Promise<unknown>;
    };
    rerank?: { execute(work: unknown, signal: AbortSignal): Promise<unknown> };
    enrichment?: {
      execute(work: unknown, signal: AbortSignal): Promise<unknown>;
    };
  };
  runtime?: {
    providerConcurrency?: number;
    defaultReadinessTimeoutMs?: number;
    backgroundWorkBatchSize?: number;
  };
  privacy?: {
    allowProviderDataEgress?: boolean;
    allowEmbeddingDataEgress?: boolean;
    allowRerankDataEgress?: boolean;
    allowEnrichmentDataEgress?: boolean;
  };
  resourceLimits?: {
    maxSourceBytes?: number;
    maxQueryResults?: number;
  };
  diagnostics?: { level?: "off" | "errors" | "verbose" };
}
```

The provider interfaces are described in [API.md](API.md). Provider objects
are supplied as functions/objects at construction time and are not serialized
into the Store.

## Privacy policy

`allowProviderDataEgress: false` denies all provider work. The three specific
flags can deny embedding, reranking, or enrichment independently. The check is
performed before the work item is handed to the provider, and a denial is
reported as `CAPABILITY_NOT_READY`.

## Runtime and limits

`maxSourceBytes` is enforced before a create or revise crosses the native
boundary. Query cancellation uses `AbortSignal`; `timeoutMs` is a per-query
positive integer option rather than a persisted configuration field.

The `runtime` fields are part of the validated public configuration shape. The
current TypeScript bridge does not forward independent scheduler overrides to
the native binding, so callers must not assume those fields change provider
concurrency or batch scheduling until a binding exposes that behavior.

`maxQueryResults` and diagnostics are reserved configuration fields in the
current contract; their detailed runtime behavior is not advertised by this
release line.

## Space policy and durable operations

Provider egress is also governed by the Authority-owned policy on each Space:
`deny`, `local-only`, or `external-allowed` for embedding, reranking, and
enrichment. When a query spans multiple Spaces, the effective policy is the
most restrictive policy in the scope. A global privacy denial still blocks the
corresponding capability before work is dispatched.

Backup, portable import, and purge do not use configuration flags to weaken
their durability boundaries. Backup destinations must be outside the active
Store; restore will not overwrite an existing target. Import requires an
explicit idempotency key and bounded package/source sizes, and purge is
planned before its durable cleanup journal can advance.

SQLite WAL mode, foreign-key enforcement, busy timeout, and bounded write
retries are runtime persistence policy rather than public tuning knobs. Keep
the Store on a filesystem that supports the required SQLite locking and atomic
rename behavior.

## Store path guidance

Use a dedicated directory for each logical Store. Do not place backups inside
the active Store directory, and do not share one Store between independent
writers. See [PERSISTENCE.md](PERSISTENCE.md) for marker, lock, and recovery
semantics.
