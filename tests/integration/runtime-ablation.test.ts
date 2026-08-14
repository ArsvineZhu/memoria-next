import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { test } from "node:test";

const execFileAsync = promisify(execFile);
interface AblationResult {
  source: "runtime";
  ablation: string;
  resultIds: string[];
  trace: {
    channelsExecuted: string[];
    rerankRequested: boolean;
    rerankApplied: boolean;
    tagBasisRank?: number;
    diffusionIterations: number;
  };
  providerCalls: {
    embedding: number;
    rerank: number;
    enrichment: number;
    rerankCandidateCount: number;
  };
}
let allResults: Promise<Map<string, AblationResult>> | undefined;

function runAblations(): Promise<Map<string, AblationResult>> {
  if (allResults) return allResults;
  allResults = (async () => {
    const script = `
      import { runSingleAblation } from './benchmarks/retrieval/run.ts';
      const names = ['lexical', 'lexical+semantic', 'tag-basis-residual', 'activation', 'diffusion', 'rerank', 'adaptive'];
      const results = [];
      for (const name of names) results.push(await runSingleAblation(name));
      process.stdout.write(JSON.stringify(results));
    `;
    const { stdout } = await execFileAsync(
      process.execPath,
      ["--import", "tsx", "--input-type=module", "--eval", script],
      { cwd: process.cwd(), maxBuffer: 10 * 1024 * 1024 },
    );
    const parsed = JSON.parse(stdout) as AblationResult[];
    return new Map(parsed.map((result) => [result.ablation, result]));
  })();
  return allResults;
}

async function getAblationResult(name: string): Promise<AblationResult> {
  const value = (await runAblations()).get(name);
  assert(value);
  return value;
}

test("lexical ablation disables semantic, associative, and rerank work", async () => {
  const result = await getAblationResult("lexical");

  assert.equal(result.source, "runtime");
  assert(result.trace.channelsExecuted.includes("lexical"));
  assert.equal(
    result.trace.channelsExecuted.includes("semantic-direct"),
    false,
  );
  assert.equal(result.trace.channelsExecuted.includes("tag-readout"), false);
  assert.equal(result.trace.rerankApplied, false);
  assert.equal(result.providerCalls.embedding, 0);
  assert.equal(result.providerCalls.rerank, 0);
});

test("semantic ablation uses lexical and semantic-direct runtime channels", async () => {
  const result = await getAblationResult("lexical+semantic");

  assert.equal(result.source, "runtime");
  assert(result.trace.channelsExecuted.includes("lexical"));
  assert(result.trace.channelsExecuted.includes("semantic-direct"));
  assert.equal(result.trace.channelsExecuted.includes("tag-readout"), false);
});

test("tag-basis ablation is observable in the real runtime trace", async () => {
  const result = await getAblationResult("tag-basis-residual");

  assert.equal(result.source, "runtime");
  assert(result.trace.channelsExecuted.includes("semantic-direct"));
  assert(result.trace.channelsExecuted.includes("semantic-residual"));
  assert(result.trace.tagBasisRank !== undefined);
});

test("diffusion ablation changes the real planner profile", async () => {
  const activation = await getAblationResult("activation");
  const diffusion = await getAblationResult("diffusion");

  assert.equal(activation.source, "runtime");
  assert.equal(diffusion.source, "runtime");
  assert(activation.trace.channelsExecuted.includes("activation"));
  assert.equal(activation.trace.channelsExecuted.includes("diffusion"), false);
  assert(diffusion.trace.channelsExecuted.includes("diffusion"));
  assert(diffusion.trace.diffusionIterations > 0);
});

test("rerank and adaptive ablations use real provider and adaptive stages", async () => {
  const rerank = await getAblationResult("rerank");
  const adaptive = await getAblationResult("adaptive");

  assert.equal(rerank.source, "runtime");
  assert(rerank.trace.rerankRequested);
  assert(rerank.trace.rerankApplied);
  assert.equal(rerank.providerCalls.rerank, 1);
  assert(rerank.providerCalls.rerankCandidateCount > 0);
  assert.equal(adaptive.source, "runtime");
  assert(adaptive.trace.channelsExecuted.includes("adaptive"));
});
