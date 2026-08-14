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
executable code.

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

## Query and learning flow

The Host supplies a structured scope and cue. The Rust query pipeline applies
scope and hard constraints, resolves admissible references at a pinned
snapshot, gathers exact/lexical/structural/temporal/relation/semantic/Tag
evidence, and consolidates results without duplicate evidence. Reranking and
advanced graph operators are explicit capabilities.

Adaptive ranking reads a bounded replayed model only after ordinary query
admissibility. It learns from explicit feedback tied to a retrieval receipt;
retrieval exposure, top-K selection, or an unused result is not feedback.

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
