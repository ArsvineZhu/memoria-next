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

## Store path guidance

Use a dedicated directory for each logical Store. Do not place backups inside
the active Store directory, and do not share one Store between independent
writers. See [PERSISTENCE.md](PERSISTENCE.md) for marker, lock, and recovery
semantics.
