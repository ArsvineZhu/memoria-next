# `src/` Agent Instructions

## Boundaries

- `createMemoria` must open the hard-reset Store through the N-API binding and
  translate native failures with `toMemoriaError`.
- The native loader must resolve `native/index.js` relative to the installed
  package module, never relative to `process.cwd()`.
- Provider work is the only path from Rust to TypeScript provider side effects.
  Validate embedding dimensions, rerank handles/scores, and generated-Tag
  limits before submitting results.
- Provider egress permission is checked before a payload leaves the process.
  Embedding, rerank, and enrichment permissions remain separate.
- Agent mutations require explicit permission and expected HEAD values. On
  `HEAD_CONFLICT`, return the actual head and recovery instruction without an
  automatic retry.

## Public API discipline

- Add public exports only through the intentional root or `authoring` subpath.
- Keep IDs branded at the domain boundary and preserve native result identity.
- Preserve `AbortSignal`, positive timeout validation, close idempotency, and
  read-session cleanup semantics.
- Do not introduce aliases for removed public names or the previous Store model.

## Verification

Use `corepack pnpm build:ts`, `corepack pnpm typecheck`, `corepack pnpm lint`,
and the focused TypeScript test before the full `corepack pnpm test` gate. Run
`corepack pnpm verify:public` and `corepack pnpm verify:pack` whenever package
exports, generated declarations, native loading, or package metadata changes.
