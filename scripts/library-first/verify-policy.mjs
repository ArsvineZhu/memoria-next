import {
  existsSync,
  readFileSync,
  readdirSync,
} from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const defaultRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const baselineCommit = "500580947c830d73373d7a99eb722431f74da5d6";
const allowedExceptionKeys = new Set([
  "QueryOperatorTrace",
  "DerivedManifest",
  "SourceBlobHash",
  "PurgeDomainJournal",
]);

function normalizePath(path) {
  return path.replaceAll("\\", "/");
}

function collectFiles(root, predicate, output = []) {
  if (!existsSync(root)) return output;
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    if ([".git", "node_modules", "target", "dist", "dist-test"].includes(entry.name)) {
      continue;
    }
    const path = join(root, entry.name);
    if (entry.isDirectory()) collectFiles(path, predicate, output);
    else if (entry.isFile() && predicate(path)) output.push(path);
  }
  return output;
}

function candidateSourceFiles(root) {
  const files = [
    ...collectFiles(join(root, "crates"), (path) => path.endsWith(".rs")),
    ...collectFiles(join(root, "src"), (path) => path.endsWith(".ts") || path.endsWith(".rs")),
    ...collectFiles(join(root, "scripts"), (path) => path.endsWith(".mjs")),
    ...collectFiles(join(root, "benchmarks"), (path) => path.endsWith(".ts")),
  ];
  return files.map((path) => normalizePath(relative(root, path))).sort();
}

function changedFiles(root) {
  try {
    const baseline = execFileSync(
      "git",
      ["diff", "--name-only", "--diff-filter=ACMR", `${baselineCommit}..HEAD`],
      { cwd: root, encoding: "utf8" },
    );
    const working = execFileSync(
      "git",
      ["diff", "--name-only", "--diff-filter=ACMR"],
      { cwd: root, encoding: "utf8" },
    );
    const untracked = execFileSync(
      "git",
      ["ls-files", "--others", "--exclude-standard"],
      { cwd: root, encoding: "utf8" },
    );
    return [...new Set(`${baseline}\n${working}\n${untracked}`
      .split(/\r?\n/)
      .map(normalizePath)
      .filter(Boolean))];
  } catch {
    return candidateSourceFiles(root);
  }
}

function readExceptions(root) {
  const path = join(root, "scripts", "library-first", "domain-exceptions.json");
  if (!existsSync(path)) return {};
  return JSON.parse(readFileSync(path, "utf8"));
}

function isExcepted(path, source, exceptions) {
  return Object.keys(exceptions).some((key) =>
    source.toLowerCase().includes(key.toLowerCase()) || path.toLowerCase().includes(key.toLowerCase()),
  );
}

function genericInfrastructureViolation(path, source, exceptions) {
  const genericName = /(^|\/)(infrastructure)(\/|$)|(?:cache|retry|queue|segment|tombstone|object_store|migration)[^/]*\.(?:rs|ts|mjs)$/i;
  if (genericName.test(path) && !isExcepted(path, source, exceptions)) {
    return `generic infrastructure module requires an explicit domain exception: ${path}`;
  }
  return undefined;
}

function handwrittenRetryViolation(path, source) {
  const coveredRuntimePath = /^(src\/(providers|engine)\/|crates\/memoria-(?:authority|derived|adaptive|runtime)\/src\/)/.test(path);
  if (!coveredRuntimePath) return undefined;
  const hasDelay = /\b(?:sleep|delay|setTimeout|timer|wait_for)\b/i.test(source);
  const hasAttemptCounter = /\b(?:attempts?|retries?|retry_count|max_attempts?)\b[\s\S]{0,160}(?:\+\+|\+=|-=|<\s*\d|<=\s*\d)/i.test(source);
  if (hasDelay && hasAttemptCounter) {
    return `handwritten retry loop marker found in ${path}; use p-retry/backon`;
  }
  return undefined;
}

function migrationPassed(root) {
  const markerFiles = [
    ...collectFiles(join(root, "docs"), (path) => path.endsWith(".md")),
    ...collectFiles(join(root, "scripts"), (path) => path.endsWith(".json")),
  ];
  return markerFiles.some((path) => /decision\s*:\s*PASS|"decision"\s*:\s*"PASS"/i.test(readFileSync(path, "utf8")));
}

function bannedDependencyViolation(root) {
  if (!migrationPassed(root)) return undefined;
  const manifests = [
    ...collectFiles(root, (path) => path.endsWith("Cargo.toml") || path.endsWith("package.json")),
  ];
  const banned = manifests.flatMap((path) => {
    const source = readFileSync(path, "utf8");
    return /(^|\n)\s*(?:tantivy|usearch)\s*=|["'](?:tantivy|usearch)["']\s*:/i.test(source)
      ? [`banned retrieval dependency remains after LanceDB PASS: ${normalizePath(relative(root, path))}`]
      : [];
  });
  return banned[0];
}

function findViolations({ root = defaultRoot, files = changedFiles(root), exceptions = readExceptions(root) } = {}) {
  const violations = [];
  for (const key of Object.keys(exceptions)) {
    if (!allowedExceptionKeys.has(key) || typeof exceptions[key] !== "string" || !exceptions[key].trim()) {
      violations.push(`invalid domain exception entry: ${key}`);
    }
  }
  for (const relativePath of files) {
    const path = join(root, relativePath);
    if (!existsSync(path)) continue;
    const source = readFileSync(path, "utf8");
    const genericViolation = genericInfrastructureViolation(relativePath, source, exceptions);
    if (genericViolation) violations.push(genericViolation);
    const retryViolation = handwrittenRetryViolation(relativePath, source);
    if (retryViolation) violations.push(retryViolation);
  }
  const dependencyViolation = bannedDependencyViolation(root);
  if (dependencyViolation) violations.push(dependencyViolation);
  return [...new Set(violations)];
}

function parseArgs(args) {
  const rootIndex = args.indexOf("--root");
  return {
    root: rootIndex >= 0 ? resolve(args[rootIndex + 1]) : defaultRoot,
    allFiles: args.includes("--all-files"),
  };
}

function main(args) {
  const options = parseArgs(args);
  const files = options.allFiles ? candidateSourceFiles(options.root) : changedFiles(options.root);
  const violations = findViolations({ root: options.root, files });
  if (violations.length > 0) {
    for (const violation of violations) console.error(`VIOLATION: ${violation}`);
    process.exitCode = 1;
    return;
  }
  console.log("Library-first policy: PASS");
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2));
}

export { findViolations };
