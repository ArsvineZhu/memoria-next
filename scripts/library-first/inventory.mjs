import {
  existsSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const baselineReportPath = join(
  repoRoot,
  "docs",
  "reports",
  "library-first-baseline.json",
);

const classes = [
  "DOMAIN",
  "ALGORITHM",
  "INTEGRATION",
  "INFRASTRUCTURE",
  "REVIEW_REQUIRED",
];

const candidates = [
  ["Markdown/MDX syntax foundation", "markdown = 1.0.0"],
  ["SQLite schema migration", "rusqlite_migration = 2.6.0"],
  ["atomic source-object writes", "atomic-write-file = 0.3.0"],
  ["TypeScript provider retry", "p-retry = 8.0.0"],
  ["Rust transient retry", "backon = 1.6.0"],
  ["generic Rust diagnostics", "tracing = 0.1.44"],
  ["association graph container", "petgraph = 0.8.3"],
  ["process cache eviction and TTL", "moka = 0.12.15"],
  ["physical lexical/vector retrieval", "lancedb = 0.33.0 (POC-gated)"],
  ["durable build-job queue", "apalis = 0.7.4 (POC-gated)"],
];

function normalizePath(path) {
  return path.replaceAll("\\", "/");
}

function collectFiles(root, predicate, output = []) {
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    if (
      [".git", "node_modules", "target", "dist", "dist-test"].includes(
        entry.name,
      )
    ) {
      continue;
    }
    const entryPath = join(root, entry.name);
    if (entry.isDirectory()) {
      collectFiles(entryPath, predicate, output);
    } else if (entry.isFile() && predicate(entryPath)) {
      output.push(entryPath);
    }
  }
  return output;
}

function sourceFiles() {
  const files = [];
  const cratesRoot = join(repoRoot, "crates");
  for (const entry of readdirSync(cratesRoot, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const sourceRoot = join(cratesRoot, entry.name, "src");
    if (existsSync(sourceRoot)) {
      files.push(...collectFiles(sourceRoot, (path) => path.endsWith(".rs")));
    }
  }
  files.push(
    ...collectFiles(join(repoRoot, "src"), (path) => path.endsWith(".ts")),
  );
  files.push(
    ...collectFiles(join(repoRoot, "scripts"), (path) => path.endsWith(".mjs")),
  );
  files.push(
    ...collectFiles(join(repoRoot, "benchmarks"), (path) =>
      path.endsWith(".ts"),
    ),
  );
  return files.sort();
}

function rule(className, reason, predicate) {
  return { class: className, reason, predicate };
}

const rules = [
  rule("DOMAIN", "Memoria type and identity semantics", (path) =>
    path.startsWith("crates/memoria-types/src/"),
  ),
  rule("DOMAIN", "Authority lifecycle and mutation semantics", (path) =>
    /^(crates\/memoria-authority\/src\/(model|mutation|read|purge)\.rs)$/.test(
      path,
    ),
  ),
  rule("INFRASTRUCTURE", "Authority storage and file mechanics", (path) =>
    /^crates\/memoria-authority\/src\//.test(path),
  ),
  rule("DOMAIN", "Restricted MDX and canonical Memory semantics", (path) =>
    path.startsWith("crates/memoria-mdx/src/"),
  ),
  rule(
    "ALGORITHM",
    "Original query algorithms and ranking semantics",
    (path) =>
      /^crates\/memoria-query\/src\/algorithms\//.test(path) ||
      /^(crates\/memoria-query\/src\/(adaptive|fusion|relation_expand|semantic|tags|exact|history|lexical|rerank)\.rs)$/.test(
        path,
      ),
  ),
  rule("DOMAIN", "Structured query and retrieval contract types", (path) =>
    /^(crates\/memoria-query\/src\/(assessment|evidence|model|response|snapshot|trace|validate)\.rs)$/.test(
      path,
    ),
  ),
  rule(
    "INFRASTRUCTURE",
    "Custom query graph container",
    (path) => path === "crates/memoria-query/src/association.rs",
  ),
  rule("INTEGRATION", "Query planning and execution integration", (path) =>
    path.startsWith("crates/memoria-query/src/"),
  ),
  rule("DOMAIN", "Adaptive event and feedback semantics", (path) =>
    /^(crates\/memoria-adaptive\/src\/(event|model|reducer|purge)\.rs)$/.test(
      path,
    ),
  ),
  rule(
    "INFRASTRUCTURE",
    "Adaptive persistence and checkpoint mechanics",
    (path) => path.startsWith("crates/memoria-adaptive/src/"),
  ),
  rule(
    "INFRASTRUCTURE",
    "Custom Derived physical artifact infrastructure",
    (path) =>
      /^(crates\/memoria-derived\/src\/(artifact|catalog|gc|lease|lexical_artifact|scheduler|vector)\.rs)$/.test(
        path,
      ),
  ),
  rule("DOMAIN", "Derived snapshot and lifecycle domain metadata", (path) =>
    /^(crates\/memoria-derived\/src\/(manifest|status)\.rs)$/.test(path),
  ),
  rule("INTEGRATION", "Derived projection and provider integration", (path) =>
    path.startsWith("crates/memoria-derived/src/"),
  ),
  rule(
    "INFRASTRUCTURE",
    "Generic runtime diagnostics plumbing",
    (path) => path === "crates/memoria-runtime/src/diagnostics.rs",
  ),
  rule("INTEGRATION", "Runtime and provider boundary integration", (path) =>
    path.startsWith("crates/memoria-runtime/src/"),
  ),
  rule("INTEGRATION", "N-API integration boundary", (path) =>
    path.startsWith("crates/memoria-napi/src/"),
  ),
  rule(
    "DOMAIN",
    "Public authoring, agent, administration, and domain API",
    (path) => /^(src\/(domain|authoring|agent|admin)\/)/.test(path),
  ),
  rule("ALGORITHM", "TypeScript retrieval integration algorithm", (path) =>
    path.startsWith("src/retrieval/"),
  ),
  rule(
    "INTEGRATION",
    "TypeScript native and provider integration",
    (path) =>
      /^(src\/(engine|native|providers)\/)/.test(path) ||
      path === "src/index.ts",
  ),
  rule(
    "INTEGRATION",
    "Library-first tooling and benchmark integration",
    (path) => path.startsWith("scripts/") || path.startsWith("benchmarks/"),
  ),
];

function classify(path) {
  const match = rules.find((candidate) => candidate.predicate(path));
  if (!match) {
    return {
      class: "REVIEW_REQUIRED",
      reason:
        "No explicit domain, algorithm, integration, or infrastructure rule",
    };
  }
  return { class: match.class, reason: match.reason };
}

function locOf(source) {
  return source.split(/\r?\n/).filter((line) => line.trim().length > 0).length;
}

function collectDependencyCount() {
  const dependencies = new Set();
  const manifests = [join(repoRoot, "package.json")];
  const cratesRoot = join(repoRoot, "crates");
  for (const entry of readdirSync(cratesRoot, { withFileTypes: true })) {
    if (entry.isDirectory())
      manifests.push(join(cratesRoot, entry.name, "Cargo.toml"));
  }

  for (const manifest of manifests) {
    if (!existsSync(manifest)) continue;
    if (manifest.endsWith("package.json")) {
      const packageJson = JSON.parse(readFileSync(manifest, "utf8"));
      for (const section of [
        "dependencies",
        "optionalDependencies",
        "devDependencies",
      ]) {
        for (const name of Object.keys(packageJson[section] ?? {}))
          dependencies.add(`npm:${name}`);
      }
      continue;
    }
    let section = "";
    for (const line of readFileSync(manifest, "utf8").split(/\r?\n/)) {
      const sectionMatch = line.match(/^\[([^\]]+)\]/);
      if (sectionMatch) section = sectionMatch[1];
      if (!/(^|\.)dependencies$/.test(section)) continue;
      const dependencyMatch = line.match(/^([A-Za-z0-9_-]+)\s*=/);
      if (dependencyMatch) dependencies.add(`cargo:${dependencyMatch[1]}`);
    }
  }
  return dependencies.size;
}

function customMarkers(inventory) {
  const markers = new Map();
  for (const [path] of Object.entries(inventory)) {
    if (path.startsWith("scripts/library-first/")) continue;
    const source = readFileSync(join(repoRoot, path), "utf8");
    for (const marker of [
      "MEMVEC01",
      "PRAGMA user_version",
      "tombstone",
      "compaction",
      "segment",
    ]) {
      if (source.toLowerCase().includes(marker.toLowerCase())) {
        if (!markers.has(marker)) markers.set(marker, []);
        markers.get(marker).push(path);
      }
    }
  }
  return [...markers.entries()].map(([marker, paths]) => ({ marker, paths }));
}

function buildInventory() {
  const inventory = {};
  for (const file of sourceFiles()) {
    const path = normalizePath(relative(repoRoot, file));
    const classification = classify(path);
    inventory[path] = {
      class: classification.class,
      language: file.endsWith(".rs")
        ? "rust"
        : file.endsWith(".ts")
          ? "typescript"
          : "javascript",
      loc: locOf(readFileSync(file, "utf8")),
      reason: classification.reason,
    };
  }
  return inventory;
}

function summaryOf(inventory) {
  const byClass = Object.fromEntries(
    classes.map((className) => [className, { files: 0, loc: 0 }]),
  );
  for (const entry of Object.values(inventory)) {
    byClass[entry.class].files += 1;
    byClass[entry.class].loc += entry.loc;
  }
  const totalLoc = Object.values(inventory).reduce(
    (sum, entry) => sum + entry.loc,
    0,
  );
  const infrastructureLoc = byClass.INFRASTRUCTURE.loc;
  const physicalRetrievalLoc = Object.entries(inventory)
    .filter(
      ([path, entry]) =>
        entry.class === "INFRASTRUCTURE" &&
        /memoria-derived|memoria-query/.test(path),
    )
    .reduce((sum, [, entry]) => sum + entry.loc, 0);
  const customStateMachines = Object.keys(inventory).filter((path) =>
    /(^|\/)(scheduler|gc|lease|ann_segments|vector_membership)\.rs$/.test(path),
  );

  return {
    totalFiles: Object.keys(inventory).length,
    totalLoc,
    byClass,
    infrastructurePercentage: totalLoc === 0 ? 0 : infrastructureLoc / totalLoc,
    physicalRetrievalLoc,
    dependencyCount: collectDependencyCount(),
    customPersistentFormats: customMarkers(inventory),
    customStateMachines,
    candidates,
  };
}

function markdownReport(inventory, summary) {
  const lines = [
    "# Library-First Baseline",
    "",
    `Reviewed commit: \`500580947c830d73373d7a99eb722431f74da5d6\``,
    "",
    "This report classifies source modules before library substitution. `REVIEW_REQUIRED` is a hard failure for the architecture inventory gate.",
    "",
    "## Summary",
    "",
    "| Metric | Value |",
    "| --- | ---: |",
    `| Source files | ${summary.totalFiles} |`,
    `| Non-empty LOC | ${summary.totalLoc} |`,
    `| Direct dependency count | ${summary.dependencyCount} |`,
    `| Self-authored infrastructure LOC | ${summary.byClass.INFRASTRUCTURE.loc} |`,
    `| Infrastructure LOC percentage | ${(summary.infrastructurePercentage * 100).toFixed(2)}% |`,
    `| Physical retrieval infrastructure LOC | ${summary.physicalRetrievalLoc} |`,
    "",
    "| Class | Files | LOC |",
    "| --- | ---: | ---: |",
  ];
  for (const className of classes) {
    lines.push(
      `| ${className} | ${summary.byClass[className].files} | ${summary.byClass[className].loc} |`,
    );
  }
  lines.push("", "## Custom persistent markers", "");
  if (summary.customPersistentFormats.length === 0) {
    lines.push("None detected.");
  } else {
    for (const item of summary.customPersistentFormats) {
      lines.push(
        `- \`${item.marker}\`: ${item.paths.map((path) => `\`${path}\``).join(", ")}`,
      );
    }
  }
  lines.push("", "## Custom lifecycle/state-machine files", "");
  for (const path of summary.customStateMachines) lines.push(`- \`${path}\``);
  if (summary.customStateMachines.length === 0) lines.push("None detected.");
  lines.push(
    "",
    "## Candidate ecosystem replacements",
    "",
    "| Concern | Candidate |",
    "| --- | --- | ",
  );
  for (const [concern, candidate] of summary.candidates)
    lines.push(`| ${concern} | ${candidate} |`);
  lines.push(
    "",
    "## Module inventory",
    "",
    "| Path | Class | LOC | Reason |",
    "| --- | --- | ---: | --- | ",
  );
  for (const [path, entry] of Object.entries(inventory)) {
    lines.push(
      `| \`${path}\` | ${entry.class} | ${entry.loc} | ${entry.reason} |`,
    );
  }
  lines.push("");
  return lines.join("\n");
}

function writeReports(inventory, summary) {
  const reportDir = join(repoRoot, "docs", "reports");
  writeFileSync(
    join(reportDir, "library-first-baseline.json"),
    `${JSON.stringify({ summary, files: inventory }, null, 2)}\n`,
  );
  writeFileSync(
    join(reportDir, "library-first-baseline.md"),
    markdownReport(inventory, summary),
  );
}

function assertNoReviewRequired(inventory) {
  const unresolved = Object.entries(inventory)
    .filter(([, entry]) => entry.class === "REVIEW_REQUIRED")
    .map(([path]) => path);
  if (unresolved.length > 0) {
    throw new Error(`Unclassified source modules:\n${unresolved.join("\n")}`);
  }
}

function compareWithBaseline(summary) {
  if (!existsSync(baselineReportPath))
    throw new Error(`Missing baseline report: ${baselineReportPath}`);
  const baseline = JSON.parse(readFileSync(baselineReportPath, "utf8")).summary;
  const reduction = (before, after) =>
    before === 0 ? 0 : (before - after) / before;
  return {
    baselineInfrastructureLoc: baseline.byClass.INFRASTRUCTURE.loc,
    currentInfrastructureLoc: summary.byClass.INFRASTRUCTURE.loc,
    totalInfrastructureReduction: reduction(
      baseline.byClass.INFRASTRUCTURE.loc,
      summary.byClass.INFRASTRUCTURE.loc,
    ),
    baselinePhysicalRetrievalLoc: baseline.physicalRetrievalLoc,
    currentPhysicalRetrievalLoc: summary.physicalRetrievalLoc,
    physicalRetrievalReduction: reduction(
      baseline.physicalRetrievalLoc,
      summary.physicalRetrievalLoc,
    ),
  };
}

function main(args) {
  const inventory = buildInventory();
  if (args.includes("--json")) {
    process.stdout.write(`${JSON.stringify(inventory, null, 2)}\n`);
    return;
  }
  if (args.includes("--check")) assertNoReviewRequired(inventory);
  const summary = summaryOf(inventory);
  if (args.includes("--compare-baseline")) {
    const comparison = compareWithBaseline(summary);
    process.stdout.write(
      `${JSON.stringify({ summary, comparison }, null, 2)}\n`,
    );
    if (
      args.includes("--require-pass") &&
      comparison.totalInfrastructureReduction < 0.3
    ) {
      throw new Error(
        `Infrastructure deletion budget failed: ${(comparison.totalInfrastructureReduction * 100).toFixed(2)}% < 30%`,
      );
    }
    return;
  }
  if (args.includes("--check")) {
    process.stdout.write(`${JSON.stringify(summary, null, 2)}\n`);
    return;
  }
  writeReports(inventory, summary);
  process.stdout.write(`${JSON.stringify(summary, null, 2)}\n`);
}

try {
  main(process.argv.slice(2));
} catch (error) {
  process.stderr.write(
    `${error instanceof Error ? error.message : String(error)}\n`,
  );
  process.exitCode = 1;
}

export { buildInventory, repoRoot, summaryOf };
