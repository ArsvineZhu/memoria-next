import { access, readdir, readFile, stat } from "node:fs/promises";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const requiredFiles = [
  "README.md",
  "INDEX.md",
  "AGENTS.md",
  "src/README.md",
  "src/AGENTS.md",
  "docs/README.md",
  "docs/INDEX.md",
  "docs/AGENTS.md",
  "docs/ARCHITECTURE.md",
  "docs/API.md",
  "docs/PERSISTENCE.md",
  "docs/ALGORITHMS.md",
  "docs/TESTING.md",
  "docs/CONFIGURATION.md",
  "docs/TROUBLESHOOTING.md",
  "docs/reports/runtime-retrieval-validation.md",
];
const requiredTopics = [
  "MDX Authority",
  "Space",
  "Memory",
  "Revision",
  "Derived",
  "Adaptive",
  "structured query",
  "provider boundary",
  "Tag",
  "backup",
  "purge",
  "Host",
  "Agent",
];
const forbiddenTerms = [
  "createMemoryEngine",
  "MemoryEngine",
  "QueryBuilder",
  "RetrievalStrategy",
  "TDBEngine",
  "TDBStore",
  "VexusVectorStore",
  "VexusIndex",
  "rust-vexus-lite",
  "vexus-lite",
  "memory.sqlite",
];
const ignoredDirectories = new Set([
  ".git",
  ".superpowers",
  "dist",
  "node_modules",
  "target",
]);
const historicalStart = "<!-- historical-start -->";
const historicalEnd = "<!-- historical-end -->";

const files = await collectMarkdownFiles(root);
const contents = new Map();
const failures = [];

for (const relativePath of requiredFiles) {
  const path = join(root, relativePath);
  try {
    await access(path);
  } catch {
    failures.push(`missing required document: ${relativePath}`);
  }
}

for (const path of files) {
  const text = await readFile(path, "utf8");
  contents.set(path, text);
  failures.push(...findForbiddenTerms(path, text));
  failures.push(...findBrokenLinks(path, text));
}

const remediationAdr = contents.get(
  join(root, "docs", "decisions", "0013-runtime-remediation-complete.md"),
);
const runtimeValidationReport = contents.get(
  join(root, "docs", "reports", "runtime-retrieval-validation.md"),
);
const requiredCompletionEvidence = [
  "QueryOperatorTrace",
  "Memoria.query()",
  "benchmarks/retrieval/run.ts",
  "hard-constraint violations",
  "no TypeScript retrieval simulator",
  "Local gate status: PASS",
];
for (const marker of requiredCompletionEvidence) {
  if (
    !remediationAdr?.includes(marker) &&
    !runtimeValidationReport?.includes(marker)
  ) {
    failures.push(
      `ADR 0013 completion evidence is missing required runtime marker: ${marker}`,
    );
  }
}
if (
  remediationAdr?.match(/Status[\s\S]{0,80}Accepted/i) &&
  !remediationAdr.includes("production serving integration complete")
) {
  failures.push(
    "ADR 0013 cannot claim Accepted without the production serving integration status",
  );
}

const allText = [...contents.values()].join("\n").toLowerCase();
for (const topic of requiredTopics) {
  if (!allText.includes(topic.toLowerCase())) {
    failures.push(`missing documented topic: ${topic}`);
  }
}

if (failures.length > 0) {
  console.error("Memoria Next documentation verification failed.");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exitCode = 1;
} else {
  console.log(
    `Memoria Next documentation verification passed (${files.length} Markdown files).`,
  );
}

async function collectMarkdownFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    if (entry.isDirectory() && ignoredDirectories.has(entry.name)) {
      continue;
    }
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await collectMarkdownFiles(path)));
    } else if (entry.isFile() && entry.name.toLowerCase().endsWith(".md")) {
      files.push(path);
    }
  }
  return files.sort((left, right) => left.localeCompare(right));
}

function findForbiddenTerms(path, text) {
  const findings = [];
  let historical = false;
  for (const [index, line] of text.split(/\r?\n/).entries()) {
    if (line.includes(historicalStart)) {
      historical = true;
    }
    if (!historical) {
      for (const term of forbiddenTerms) {
        if (line.toLowerCase().includes(term.toLowerCase())) {
          findings.push(
            `${display(path)}:${index + 1} contains obsolete term ${term}`,
          );
        }
      }
    }
    if (line.includes(historicalEnd)) {
      historical = false;
    }
  }
  return findings;
}

function findBrokenLinks(path, text) {
  const findings = [];
  for (const match of text.matchAll(/\[[^\]]*\]\(([^)]+)\)/g)) {
    const target = match[1].trim().split(/[?#]/, 1)[0];
    if (
      !target ||
      target.startsWith("#") ||
      /^[a-z][a-z0-9+.-]*:/i.test(target)
    ) {
      continue;
    }
    const candidate = resolve(dirname(path), target);
    if (!pathExists(candidate)) {
      findings.push(`${display(path)} links to missing path ${target}`);
    }
  }
  return findings;
}

async function pathExists(path) {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

function display(path) {
  return relative(root, path).split(sep).join("/");
}
