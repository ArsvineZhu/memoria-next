# Architecture

Memoria Next is an embedded pipeline with one logical writer owner per Store.
The layers have deliberately different authority and failure boundaries.

```text
Host / Agent
    │ structured scope, identity, typed intent, explicit feedback
TypeScript package
    │ public domain API, cancellation, provider side effects
N-API v3 protocol
    │ typed mutations, query responses, provider work/results
Rust runtime
    ├─ Authority + source CAS
    ├─ restricted MDX → canonical Memory IR
    ├─ Derived compiler + immutable manifests
    ├─ scoped query planner and evidence fusion
    └─ Adaptive event log and bounded read model
```

## Authority and identity

Authority owns the durable facts:

- `SpaceId` identifies a Memory Space. A Space key is a presentation and
  lookup surface, not the stable identity.
- `MemoryId` identifies the logical Memory across revisions and lifecycle
  changes.
- `RevisionId` identifies a committed content revision and its ordered parent
  revisions.
- a revision stores the source blob hash, semantic intent, commit generation,
  and timestamp.
- lifecycle and historical state are generation-bounded records rather than
  guesses made from the revision graph.

Host-owned real-world entities use explicit `EntityRef` values. Memoria can
discover scoped observations, but it does not turn a name, embedding, or
observation into authoritative identity without Host confirmation.

## MDX Authority and computation boundary

Restricted declarative MDX is parsed, validated, and compiled into a canonical
Memory IR. It supports the semantic elements needed by the current profile,
including visible text, hierarchy, entities, explicit Tags, relations, time,
Memory references, source locators, and quote/evidence regions. It is data, not
executable code. A single canonical `EntityRef` grammar is shared by MDX,
query, Derived, and N-API conversion. Raw HTML and runtime components outside
the closed profile are rejected; comments and code spans remain source text and
do not become semantic/provider input.

Authority is the source of truth. Derived artifacts can be deleted and
rebuilt. A Derived manifest binds validated artifact IDs to an Authority
generation and advertises capability coverage; a partial or stale artifact is
not silently treated as current.

## Runtime and provider flow

An Authority mutation commits first, rebuilds provider-free Base artifacts, and
then queues optional provider work. TypeScript polls typed work items and calls
the configured embedding, rerank, or enrichment provider. Results are validated
before they cross back to Rust. A provider failure does not roll back the
Authority commit.

Provider egress is checked separately for each capability. The default lexical
query path remains local and deterministic. Provider-dependent semantic or
rerank work may be unavailable, degraded, or explicitly denied; those states
are represented as capability/error results rather than permission guesses.

## Durability, transfer, and purge flow

Authority, Derived, and Adaptive each use an explicit SQLite concurrency
policy: WAL journaling, foreign-key enforcement where applicable, bounded
busy timeouts, and bounded retries for immediate write transactions. Rust owns
the transaction boundaries and the TypeScript layer does not copy live SQLite
files as a persistence protocol.

Store backup pins one Authority generation, uses SQLite online backup for the
Authority database (and optionally Adaptive), copies and verifies the source
CAS objects reachable from that snapshot, writes a hashed manifest, and writes
`COMPLETE` last. Restore validates the marker, manifest, database integrity,
Store identity, and every copied object in a staging directory before
activation. Derived and Cache are rebuildable and are not part of the backup
identity snapshot.

Portable import allocates the target Space and Memory identities before
rewriting package-internal `MemoryRef` values. Every rewritten document is
revalidated, all source objects are prepared, and the Authority rows plus the
import fingerprint/mapping are committed in one bounded transaction. External
references remain unresolved and are reported; a retry after restart returns
the persisted mapping, while a changed fingerprint returns an idempotency
conflict.

Purge is a durable Authority journal with the states
`planned → committed → cleaning → completed`. Planning is explicit; reopening
resumes only committed or cleaning work. Cleanup makes the Memory unavailable,
rewrites Adaptive state, clears retrieval receipts, removes Derived artifacts,
collects unreferenced source objects, runs integrity verification, and records
completion. A failed cleanup remains resumable rather than being reported as a
silent success.

## Query and learning flow

The Host supplies a structured scope and cue. The Rust Physical Query Executor
pins the Authority/Derived snapshot, applies scope and hard constraints before
ranking, resolves admissible references, and executes the requested physical
channels. Exact, lexical, direct semantic, Tag Basis residual, Tag readout,
Activation, Diffusion, Relation, and structural/temporal evidence are separate
ranked inputs to RRF fusion and correlation-aware consolidation. Reranking and
advanced graph operators are explicit capabilities; semantic does not replace
lexical, and required capability readiness never silently degrades.

The query operation stores typed provider work and results across asynchronous
barriers. Rerank scores are applied to the actual consolidated candidates, and
the optional `QueryOperatorTrace` is emitted only when diagnostics are
requested. The runtime retrieval benchmark exercises this public
`Memoria.query()` path with a real Store and deterministic providers; it does
not implement a parallel retrieval simulator.

Adaptive ranking reads a bounded materialized snapshot only after ordinary
query admissibility and only when the compiled query requests Adaptive. It
learns from explicit feedback tied to a retrieval receipt; retrieval exposure,
top-K selection, or an unused result is not feedback. The event log and
materialized state are durable in `adaptive.sqlite`, so a restart preserves the
Adaptive generation without requiring a full event-log replay per query.

## Host and Agent boundary

Host conversation bindings and the authoritative Host Directory take priority
over scoped Memoria observations. Agent tools have independent permissions for
discovery, read, write, feedback, and lifecycle operations. Existing Memory
ownership is searched before creation. Mutations use typed patches and an
expected HEAD; a conflict returns a recovery instruction and is never blindly
retried.

## Ownership rule

If a change needs a new stable fact, first decide which layer owns it. Identity,
content, lifecycle, and authorization belong to Authority/Host. Search indexes,
embeddings, generated Tags, graph associations, and Adaptive state belong to
Derived/Adaptive. Network calls belong to TypeScript providers. This rule keeps
rebuildability and privacy boundaries explicit.
