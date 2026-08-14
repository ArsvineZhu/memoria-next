# Authoring long-term Memory

## Resolve before writing

Turn a raw statement into a durable narrative only after resolving its
participants, deictic terms, and relevant time. Prefer an explicit narrative
that preserves uncertainty instead of silently increasing certainty.

Example:

Raw statement: I may move to Japan next year.

Durable form: The user is considering moving to Japan next year.

Keep the original quote or source attribution when it carries epistemic
meaning. Stable EntityRef, Space, valid time, and provenance must come from
the Host or existing Memory data. Stable references are retrieved, never
invented.

## Discover the owner

Use discover_memory_owner with the explicit Space keys, resolved entity
references, and a referentially explicit cue. Read each admissible candidate
before deciding. Treat multiple candidates as ambiguous. Treat none-found as a
decision point, not an automatic create.

Most conversational turns do not create long-term memory. A greeting, filler,
joke, transient mood, already represented repetition, or low-value uncertain
detail normally becomes NO-OP.

## Choose the semantic operation

- NO-OP: the turn does not justify durable change.
- CREATE: no reasonable existing owner exists and the Host authorizes a new
  Memory.
- ADD: add an independent Event, State, Relation, Source, or other semantic
  node to the existing owner.
- UPDATE: add or correct expression for the same semantic object without
  changing world state.
- TRANSITION: close the old State with validTo and add the new State with
  validFrom. Preserve historical validity.
- CORRECT: the prior representation was factually or structurally wrong.

Explicit user intent such as remember this raises persistence intent but does
not bypass identity, Space, owner discovery, provenance, validation, or
expected HEAD.

## Patch discipline

Read the current Memory first. Prefer a typed patch targeted at an existing
semantic object over an unconstrained full rewrite. Include expectedHead on
every mutation. Never infer semantic continuity from text similarity alone;
the Agent or Host makes that decision.

Association evidence is not causal authority. Preserve source and uncertainty
when a relation is merely observed, correlated, or reported.
