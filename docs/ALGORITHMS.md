# Algorithms and projection boundaries

## Restricted MDX and canonical IR

The MDX crate parses Markdown syntax and a closed semantic profile. Its
semantic lexer is protected from code spans/fenced code, rejects unsupported
runtime constructs, validates attributes and references, and emits a
canonical `MemoryIr` with a `SemanticHash` and source mappings. Canonical IR is
Derived from Authority source; raw MDX is not sent directly to providers.

Typed patch operations preserve source while changing a bounded semantic
target. Transition, correction, supersession, and merge are explicit revision
intents rather than facts inferred from graph similarity.

## Semantic diff and invalidation

`SemanticDiff` classifies changes into dependency categories such as visible
text, hierarchy, entity binding/surface, explicit Tag, temporal assertion,
relation, Memory reference, source metadata, extension, and semantic-node
lifecycle. Projection input hashes include projection kind, version, producer
signature, and canonical bytes.

The dependency graph keeps invalidation local:

- formatting-only changes do not alter semantic projections;
- visible text reaches lexical and content/context embedding projections;
- hierarchy reaches structural dependents;
- explicit Tags reach the explicit-Tag and graph paths without requiring a
  content embedding;
- temporal edits reach temporal artifacts;
- relation edits reach relation artifacts;
- source metadata and lifecycle changes remain in their own structural
  dependency path.

The release incremental harness records these rules as provider-call
assertions; see [TESTING.md](TESTING.md) when running it.

## Derived artifacts

The Derived compiler can build structural, temporal, relation, entity
observation, explicit-Tag, lexical, local/context embedding, generated-Tag,
Tag-graph, and rerank-view artifacts. Artifacts have lifecycle state and must
be validated before entering an immutable manifest.

Generated Tags are produced by an enrichment provider from a deterministic
projection. They carry `Generated` provenance, are stored only in Derived, and
cannot change source bytes, `SemanticHash`, or Authority.

Tag dictionaries normalize global Tag identities. Space-local memberships and
Tag association evidence remain scoped; association is retrieval evidence, not
causal or authorization authority.

## Serving truth and invalidation boundaries

Current lexical, exact, structural, temporal, relation, and Tag evidence is
served from the published Derived artifacts pinned to the query's Authority
snapshot. A normal current query does not scan Authority source bytes or
reparse every Memory. Authority/source reads remain part of rebuild and
historical paths, not the current serving fast path.

Semantic serving is enabled only by a successful manifest capability for the
pinned generation and a compatible persisted ANN artifact. A text cue alone
does not request semantic work. `semantic.required` fails with a capability
error when the artifact/provider boundary is unavailable; `semantic.preferred`
may degrade to the available local channels. When semantic work is used, the
direct ANN channel and Tag Basis residual channel remain distinct and their
operator trace is recorded.

## Query channels

The structured query compiler separates scope, hard constraints, exact
references, soft cues, readiness behavior, and optional capability requests.
Candidate channels include:

- exact Memory/Entity/Tag/reference evidence;
- lexical text evidence;
- structural and temporal evidence;
- explicit relations and bounded relation expansion;
- semantic vectors and optional context vectors;
- explicit/generated Tag and Tag-basis residual evidence;
- bounded activation/diffusion graph evidence;
- optional provider reranking.

Scope filtering and hard constraints happen before fusion or reranking. Result
consolidation keeps one Memory result with evidence handles rather than
duplicating the same underlying evidence. The balanced release default is the
provider-free lexical profile; advanced operators remain explicit until a
broader corpus and measured budgets justify a default change. See
[ADR 0002](decisions/0002-retrieval-defaults.md).

## Physical serving executor

The production query path is a Rust-owned Physical Query Executor, not a
choice between mutually exclusive Exact, Lexical, and Semantic branches. A
snapshot-pinned query can execute lexical, direct semantic, Tag Basis
residual, Tag readout, Activation, Diffusion, Relation, and exact channels as
its compiled capabilities and quality profile allow. Their ranked evidence is
merged with RRF (`k = 60`), correlation-aware support/structure bonuses, and
one consolidated candidate per Memory before optional reranking.

Semantic direct retrieval remains present when the Tag Basis residual channel
is used. Required capabilities never silently degrade under
`onNotReady: "wait"`; the operation waits for readiness or returns a
capability error.
Reranking consumes the actual consolidated candidate handles and provider
scores are applied once at the query barrier. The optional
`QueryOperatorTrace` is diagnostics-only and reports the channels and bounded
counters that the executor actually ran.

The runtime retrieval benchmark uses this public path end to end: it creates a
temporary Store, writes Authority records, waits for Derived/provider
coverage, calls `Memoria.query()`, and measures the returned trace. It does
not reproduce retrieval scoring in TypeScript. See [runtime retrieval
validation](reports/runtime-retrieval-validation.md).

The planner keeps the original operators behind bounded quality profiles:
Tag-driven seeding and Space-local readout, Tag Basis projection/residuals,
bounded Activation, independent normalized Graph Diffusion, propagation
support, correlation-aware structure evidence, explicit relation expansion,
RRF fusion with `k=60`, hierarchical consolidation, optional post-fusion
reranking, and separate recall/accessibility/effort assessment. Scope and hard
constraints are applied before every candidate channel and before reranking.
Diffusion is not an Activation alias; its personalized normalized iterations
are independently traced and remain available in the Thorough profile.

## Adaptive V1

Adaptive state is built from explicit feedback events keyed by Space and
Memory, with revision evidence retained separately. The event log and
materialized state are persisted in `adaptive.sqlite` and reopened before
query snapshots are built. A query reads the materialized snapshot pinned to
its compiled Adaptive generation; it does not replay the complete event log on
every request.

The prior has a fixed maximum influence and is applied only as a bounded
tie-breaker over admissible base candidates. Adaptive is absent unless the
query explicitly requests the capability. Accessibility decays at read time;
it is not an exclusion filter. See [ADR 0003](decisions/0003-adaptive-v1.md).
