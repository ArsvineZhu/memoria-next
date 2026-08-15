import { mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(process.cwd());
const reportPath = join(root, "docs", "reports", "lancedb-gate.json");
const gate = {
  decision: "FAIL",
  candidate: "lancedb@0.33.0",
  rustc: "1.97.1",
  hardGates: {
    pinnedCompile: {
      status: "FAIL",
      reason:
        "lance-* build scripts require protoc, which is unavailable in the current environment",
    },
  },
  phase6Allowed: false,
  report: "docs/reports/lancedb-poc.md",
};

mkdirSync(join(root, "docs", "reports"), { recursive: true });
writeFileSync(reportPath, `${JSON.stringify(gate, null, 2)}\n`);

if (process.argv.includes("--require-pass")) {
  console.error("LanceDB gate: FAIL (pinned compile requires protoc)");
  process.exitCode = 1;
} else {
  console.log(JSON.stringify(gate, null, 2));
}
