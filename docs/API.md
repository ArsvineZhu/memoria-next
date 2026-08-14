# Public API

The package root is `@arsvinezhu/memoria`. The restricted authoring serializer
is also available from `@arsvinezhu/memoria/authoring`.

## Construction and lifecycle

```ts
import { createMemoria } from "@arsvinezhu/memoria";

const memoria = await createMemoria({
  dataDir: "./local-store",
});

try {
  // use spaces, documents, query, feedback, and admin here
} finally {
  await memoria.close();
}
```

`createMemoria` validates the configuration, loads the installed native
binding, opens the hard-reset Store, and starts the optional TypeScript
provider pump. `close()` is idempotent and releases active operations and read
leases on the terminal boundary.

## Domain methods

| Surface                                                                   | Contract                                                                                 |
| ------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `memoria.spaces.create({ key, providerPolicy? })`                         | Create a Space and return its branded `id`, key, and snapshot-versioned provider policy. |
| `memoria.documents.create({ space, documentKey?, mdx, idempotencyKey? })` | Commit a new Memory in the selected Space.                                               |
| `memoria.documents.revise({ memory, expectedHead, mdx })`                 | Commit a new revision using compare-and-swap against the expected HEAD.                  |
| `memoria.query({ scope: { spaces }, cue? }, { signal?, timeoutMs? })`     | Run a scoped query and return a retrieval receipt plus result handles.                   |
| `memoria.status()`                                                        | Read Authority generation, Base and semantic coverage, active leases, and close state.   |
| `memoria.openReadSession(query)` / `closeReadSession(id)`                 | Hold an explicit native read lease; close it when the caller is done.                    |

The query response has this shape:

```ts
{
  resultCount: number;
  authorityGeneration: string;
  degraded: boolean;
  retrievalId: string;
  results: Array<{
    resultId: string;
    spaceId: string;
    memoryId: string;
    revisionId: string;
  }>;
}
```

`scope.spaces` is mandatory. It is the hard confidentiality and tenancy
boundary; ranking does not recover out-of-scope results. The optional
`cue.text` is a cue, not an instruction to infer missing entity or time
constraints.

## Authoring

```ts
import {
  serializeMemoryDocument,
  type MemoryDocumentInput,
} from "@arsvinezhu/memoria/authoring";

const mdx = serializeMemoryDocument({
  title: "Career",
  body: "Rust systems work",
  tags: ["work"],
});
```

`serializeRestrictedMdx` is the canonical name in the source module;
`serializeMemoryDocument` is the authoring subpath convenience export. The
serializer emits restricted declarative MDX, trims empty fields, and escapes
Tag attributes. General documents should still be validated by the native
MDX path before they are committed.

## Feedback

`memoria.feedback.submit({ retrievalId, idempotencyKey, events })` accepts one
or more events with a result handle and an explicit outcome. Outcomes are
`used`, `rejected`, `correct_for_query`, `incorrect_for_query`,
`preferred_over`, `sufficient`, and `insufficient`. Submit only after the Host
or Agent used and judged the evidence. The returned generation and event
identities form the Adaptive commit receipt.

## Providers

`config.providers` may supply `embedding`, `rerank`, and `enrichment` objects
with `execute(work, signal)` methods. The host validates dimensions and item
counts for embeddings, candidate handles and `[0, 1]` scores for reranking,
and bounded non-empty strings for generated Tags. `config.privacy` can deny
each egress capability before the payload is sent.

Providers are optional. Provider-free Authority and Base operations remain
available; a missing or denied provider is reported as unavailable/not-ready,
not converted into a broad access decision.

## Administration

`memoria.admin` exposes:

- `status()`;
- `createBackup({ includeAdaptive?, outputPath? })` and
  `restoreBackup({ backupPath, targetDir })`;
- `export({ scope, outputPath? })` and
  `import({ packagePath, targetSpace: { key }, idempotencyKey })`;
- `planPurge({ memory: { id } })` and `executePurge({ planId })`.

Backups are managed Store snapshots. Rust pins the Authority generation,
creates consistent SQLite backups, copies reachable source CAS objects, and
publishes a manifest with a final `COMPLETE` marker. Restore refuses an
existing target and activates only after staged validation; Derived and Cache
are rebuilt rather than treated as Authority.

Portable export/import is a scoped data transfer with new target IDs. The
native import path rewrites package-internal `MemoryRef` values from source
IDs to target IDs, validates all rewritten MDX before one Authority commit,
persists the request fingerprint and mapping, and reports unresolved external
references. Retrying the same idempotency key after restart returns the same
mapping; changing the package under that key returns `IDEMPOTENCY_CONFLICT`.

Purge is a planned, durable operation with `planned`, `committed`, `cleaning`,
and `completed` states. Reopen resumes committed/cleaning cleanup; planning
alone does not silently start destructive work. It is not equivalent to an
ordinary content correction.

## Agent surface

The root export includes identity resolution, Memory owner discovery, and
permission-gated intent tools from `src/agent`. The preferred operations are
`listMemorySpaces`, `findEntity`, `findMemory`, `readMemory`, `recordMemory`,
`updateMemory`, `transitionMemoryState`, `correctMemory`,
`submitMemoryFeedback`, and `forgetMemory`.

Agent tools default to no permissions. Host policy must grant discovery/read,
write, feedback, and lifecycle independently. A `HEAD_CONFLICT` result includes
the actual latest head when it can be read and explicitly says not to retry the
stale patch.

## Errors

Public failures are mapped to `MemoriaError` with a stable `code`. Important
codes include `INVALID_MDX`, `HEAD_CONFLICT`, `OUT_OF_SCOPE`,
`CAPABILITY_NOT_READY`, `PROVIDER_UNAVAILABLE`, `STORE_LOCKED`,
`STORE_CORRUPT`, `CORRUPTION`, `UNSUPPORTED_STORE_FORMAT`, `RESOURCE_LIMIT`,
`ABORTED`, `QUERY_TIMEOUT`, `QUERY_ERROR`, and `STORE_CLOSED`. Use
`isMemoriaError(error, code)` instead of parsing message text.
