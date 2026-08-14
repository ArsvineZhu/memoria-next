# Memoria Next

Memoria Next is a local-first, embedded long-term memory runtime. It keeps
memory content authoritative in a restricted declarative MDX format, builds
rebuildable Derived artifacts from that authority, and exposes bounded query,
feedback, governance, and Host/Agent integration surfaces.

This repository is the hard-reset Next line. The package has no compatibility
layer for the previous public API, Store layout, or native ABI.

## First use

The repository is currently verified as a local package rather than a
published release. Install the pinned toolchain and build the native binding:

```text
corepack pnpm install
corepack pnpm build
```

The public package name is `@arsvinezhu/memoria`. A minimal local consumer is:

```ts
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createMemoria } from "@arsvinezhu/memoria";
import { serializeMemoryDocument } from "@arsvinezhu/memoria/authoring";

const dataDir = await mkdtemp(join(tmpdir(), "memoria-example-"));
const memoria = await createMemoria({ dataDir });

try {
  const space = await memoria.spaces.create({ key: "personal" });
  await memoria.documents.create({
    space,
    documentKey: "career",
    mdx: serializeMemoryDocument({
      title: "Career",
      body: "Rust systems work",
      tags: ["work"],
    }),
  });
  const response = await memoria.query({
    scope: [space.id],
    text: "Rust",
  });
  console.log(response.results);
} finally {
  await memoria.close();
}
```

The complete package surface is documented in [docs/API.md](docs/API.md).

## Architecture at a glance

- **Authority** owns `Space`, `Memory`, `Revision`, lifecycle, source bytes,
  generations, and compare-and-swap writes.
- **Restricted MDX** is parsed and validated into a canonical Memory IR. It is
  declarative and never evaluated as arbitrary JavaScript.
- **Derived** owns rebuildable indexes, manifests, semantic work, explicit and
  generated Tags, and graph evidence.
- **Query** consumes scoped snapshots and combines exact, lexical, structural,
  temporal, semantic, Tag, relation, and optional rerank evidence.
- **Adaptive** learns only from explicit Host feedback and remains bounded
  behind ordinary admissibility and scope checks.
- **TypeScript** owns provider/network side effects. Rust owns canonical state
  and computation; provider work crosses the N-API boundary as typed work.
- **Host/Agent tools** resolve identity and Memory ownership before allowing
  bounded authoring or lifecycle mutations.

## Important boundaries

The public query path takes a structured scope and optional text cue. Memoria
does not perform query-time language understanding, invent stable EntityRef
values, or treat retrieved content as current instruction authority.

Provider egress is optional and separately guarded for embedding, reranking,
and generated-Tag enrichment. The provider-free lexical path remains usable
when no provider is configured. Store-wide at-rest encryption is not advertised
by this release line; see [ADR 0004](docs/decisions/0004-at-rest-encryption.md).

## Repository map

Start with [INDEX.md](INDEX.md). Human-facing documentation is catalogued in
[docs/INDEX.md](docs/INDEX.md); source-specific working rules are in
[src/AGENTS.md](src/AGENTS.md).

## Verification

The release checks are intentionally local. GitHub Actions are disabled by
design and are not an implementation gate.

```text
corepack pnpm test
corepack pnpm format:check
corepack pnpm lint
corepack pnpm typecheck
corepack pnpm verify:docs
corepack pnpm verify:public
corepack pnpm verify:pack
```

See [docs/TESTING.md](docs/TESTING.md) for the verification layers and
[docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) for bounded recovery steps.
