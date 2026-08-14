import { mkdir, writeFile } from "node:fs/promises";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { basename, join } from "node:path";

import type { QueryMetrics, ProfileMetrics } from "./metrics.js";

const execFileAsync = promisify(execFile);

export interface BenchmarkProfileReport {
  runtimeBacked: true;
  runtimePath: string;
  providerMode: string;
  profile: string;
  corpusDocuments: number;
  queryCount: number;
  topK: number;
  observedChannels: string[];
  metrics: ProfileMetrics;
  queries: QueryMetrics[];
}

export interface BenchmarkReportFiles {
  jsonPath: string;
  markdownPath: string;
}

export function renderMarkdown(
  reports: readonly BenchmarkProfileReport[],
): string {
  const lines = [
    "# Runtime retrieval benchmark",
    "",
    "This report is generated from the public Memoria runtime path and its opt-in operator trace.",
    "",
    "| Profile | Recall@10 | MRR | nDCG@10 | P95 ms | Hard violations | Embedding | Rerank | Enrichment | Adaptive applied |",
    "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
  ];
  for (const report of reports) {
    const metrics = report.metrics;
    lines.push(
      `| ${report.profile} | ${metrics.recallAt10.toFixed(4)} | ${metrics.mrr.toFixed(4)} | ${metrics.ndcgAt10.toFixed(4)} | ${metrics.p95LatencyMs.toFixed(3)} | ${metrics.hardConstraintViolationCount} | ${metrics.embeddingProviderCalls} | ${metrics.rerankProviderCalls} | ${metrics.enrichmentProviderCalls} | ${metrics.adaptiveAppliedQueryCount} |`,
    );
  }
  lines.push(
    "",
    "## Operator totals",
    "",
    "| Profile | Lexical | Semantic direct | Semantic residual | Activation edges | Diffusion iterations | Relation expansions | Rerank candidates | Degraded queries |",
    "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
  );
  for (const report of reports) {
    const metrics = report.metrics;
    lines.push(
      `| ${report.profile} | ${metrics.lexicalSearches} | ${metrics.semanticDirectSearches} | ${metrics.semanticResidualSearches} | ${metrics.activationEdgeVisits} | ${metrics.diffusionIterations} | ${metrics.relationExpansions} | ${metrics.rerankCandidateCount} | ${metrics.degradedCapabilityCount} |`,
    );
  }
  return lines.join("\n") + "\n";
}

export async function writeBenchmarkReports(
  reports: readonly BenchmarkProfileReport[],
  options: {
    resultsDirectory: string;
    timestamp?: string;
    gitSha?: string;
  },
): Promise<BenchmarkReportFiles> {
  await mkdir(options.resultsDirectory, { recursive: true });
  const timestamp =
    options.timestamp ?? new Date().toISOString().replaceAll(/[:.]/g, "-");
  const gitSha = options.gitSha ?? (await currentGitSha());
  const stem = `${timestamp}-${gitSha}`;
  const jsonPath = join(options.resultsDirectory, `${stem}.json`);
  const markdownPath = join(options.resultsDirectory, `${stem}.md`);
  await writeFile(jsonPath, JSON.stringify(reports, null, 2) + "\n", "utf8");
  await writeFile(markdownPath, renderMarkdown(reports), "utf8");
  return { jsonPath, markdownPath };
}

export function reportBasename(files: BenchmarkReportFiles): string {
  return `${basename(files.jsonPath)} / ${basename(files.markdownPath)}`;
}

async function currentGitSha(): Promise<string> {
  try {
    const result = await execFileAsync("git", ["rev-parse", "--short", "HEAD"]);
    return result.stdout.trim() || "unknown";
  } catch {
    return "unknown";
  }
}
