import { mkdtemp, readdir, readFile, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  asMemoryId,
  asRevisionId,
  asSpaceId,
  createMemoria,
  type Memoria,
  type SpaceId,
} from "../../src/index.js";
import type { ProviderCounts } from "./fake-providers.js";
import { createDeterministicProviders } from "./fake-providers.js";

export interface Relation {
  target: string;
  kind: string;
}

export interface CorpusDocument {
  id: string;
  spaceId: string;
  memoryId: string;
  revisionId: string;
  state: "current" | "historical";
  text: string;
  tags: string[];
  entities: string[];
  relations: Relation[];
}

export interface Fixture {
  memoria: Memoria;
  dataDir: string;
  spaceIds: Map<string, SpaceId>;
  externalIdByMemoryId: Map<string, string>;
  documentByExternalId: Map<string, CorpusDocument>;
  counts: ProviderCounts;
}

export async function createRuntimeFixture(
  corpus: readonly CorpusDocument[],
): Promise<Fixture> {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-retrieval-"));
  const counts: ProviderCounts = { embedding: 0, rerank: 0, enrichment: 0 };
  const generatedTagsByMemoryId = new Map<string, readonly string[]>();
  let memoria: Memoria | undefined;

  try {
    memoria = await createMemoria({
      dataDir,
      providers: createDeterministicProviders(counts, {
        generatedTagsByMemoryId,
      }),
    });
    const spaceIds = new Map<string, SpaceId>();
    const externalIdByMemoryId = new Map<string, string>();
    const documentByExternalId = new Map(
      corpus.map((document) => [document.id, document]),
    );

    for (const spaceKey of [
      ...new Set(corpus.map((document) => document.spaceId)),
    ]) {
      const space = await memoria.spaces.create({ key: spaceKey });
      spaceIds.set(spaceKey, space.id);
    }

    let generation = "0";
    for (const document of corpus) {
      const space = spaceIds.get(document.spaceId);
      if (!space) {
        throw new Error("missing benchmark Space " + document.spaceId);
      }
      const created = await memoria.documents.create({
        space: { id: asSpaceId(space) },
        documentKey: document.id,
        mdx: documentMdx(document),
      });
      generation = created.authorityGeneration;
      externalIdByMemoryId.set(created.memoryId, document.id);
      generatedTagsByMemoryId.set(
        created.memoryId,
        document.tags.slice(0, 1).map((tag) => `generated-${tag}`),
      );
    }
    await waitForCoverage(memoria, generation);

    const heads = await memoria.query({
      scope: { spaces: [...spaceIds.values()] },
      budget: {
        maxResults: corpus.length + 10,
        maxMatchesPerResult: 1,
        maxEvidenceTokens: 100,
      },
    });
    const headByMemoryId = new Map(
      heads.results.map((result) => [result.memoryId, result.revisionId]),
    );

    for (const document of corpus.filter(
      (candidate) => candidate.relations.length > 0,
    )) {
      const memoryId = [...externalIdByMemoryId.entries()].find(
        ([, externalId]) => externalId === document.id,
      )?.[0];
      const space = spaceIds.get(document.spaceId);
      const expectedHead = memoryId ? headByMemoryId.get(memoryId) : undefined;
      if (!memoryId || !space || !expectedHead) {
        throw new Error(
          "could not resolve relation fixture head for " + document.id,
        );
      }
      const revised = await memoria.documents.revise({
        memory: { id: asMemoryId(memoryId) },
        expectedHead: asRevisionId(expectedHead),
        mdx: documentMdx(
          document,
          document.relations.map((relation) => {
            const target = [...externalIdByMemoryId.entries()].find(
              ([, externalId]) => externalId === relation.target,
            )?.[0];
            if (!target) {
              throw new Error(
                "relation target " +
                  relation.target +
                  " is missing from fixture",
              );
            }
            return target;
          }),
        ),
      });
      generation = revised.authorityGeneration;
    }
    await waitForCoverage(memoria, generation);

    return {
      memoria,
      dataDir,
      spaceIds,
      externalIdByMemoryId,
      documentByExternalId,
      counts,
    };
  } catch (error) {
    await memoria?.close();
    await rm(dataDir, { recursive: true, force: true });
    throw error;
  }
}

export function documentMdx(
  document: CorpusDocument,
  relationMemoryIds: readonly string[] = [],
): string {
  const lines = ["# " + document.id];
  for (const entity of document.entities) {
    lines.push(
      '<Entity ref="' +
        escapeAttribute(entity) +
        '">' +
        escapeText(entity.split(":").at(-1) ?? entity) +
        "</Entity>",
    );
  }
  for (const tag of document.tags) {
    lines.push('<Tag value="' + escapeAttribute(tag) + '"/>');
  }
  lines.push(document.text);
  for (const memoryId of relationMemoryIds) {
    lines.push('<MemoryRef memoryId="' + escapeAttribute(memoryId) + '"/>');
  }
  return lines.join("\n") + "\n";
}

export async function waitForCoverage(
  memoria: Memoria,
  generation: string,
  timeoutMs = 10_000,
): Promise<void> {
  const target = BigInt(generation);
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const status = await memoria.status();
    if (
      BigInt(status.baseCoverage) >= target &&
      BigInt(status.semanticCoverage) >= target &&
      BigInt(status.semanticBuildCoverage) >= target
    ) {
      return;
    }
    await delay(2);
  }
  const status = await memoria.status();
  throw new Error(
    "runtime-backed retrieval fixture did not converge: target=" +
      generation +
      " base=" +
      status.baseCoverage +
      " semantic=" +
      status.semanticCoverage +
      " semanticBuild=" +
      status.semanticBuildCoverage,
  );
}

export async function directorySize(path: string): Promise<number> {
  const entries = await readdir(path, { withFileTypes: true });
  let total = 0;
  for (const entry of entries) {
    const child = join(path, entry.name);
    if (entry.isDirectory()) {
      total += await directorySize(child);
    } else {
      total += (await stat(child)).size;
    }
  }
  return total;
}

export async function readJsonLines<T>(url: URL): Promise<T[]> {
  const source = await readFile(url, "utf8");
  return source
    .split(/\r?\n/)
    .filter((line) => line.trim().length > 0)
    .map((line) => JSON.parse(line) as T);
}

function escapeAttribute(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll('"', "&quot;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

function escapeText(value: string): string {
  return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;");
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}
