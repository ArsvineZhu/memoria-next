import { mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(process.cwd());
const reportPath = join(root, "docs", "reports", "apalis-gate.json");
const gate = {
  decision: "FAIL",
  candidate: {
    apalis: "0.7.4",
    apalisSql: "0.7.4",
  },
  hardGates: {
    sqliteDependencyResolution: {
      status: "FAIL",
      reason:
        "apalis-sql/sqlx-sqlite libsqlite3-sys 0.30.1 conflicts with rusqlite 0.40.2 libsqlite3-sys 0.38.2",
    },
    splitPhaseProviderBridge: {
      status: "NOT_RUN",
      reason: "isolated SQLite queue prerequisite failed",
    },
  },
  productionMigrationAllowed: false,
  report: "docs/reports/apalis-poc.md",
};

mkdirSync(join(root, "docs", "reports"), { recursive: true });
writeFileSync(reportPath, `${JSON.stringify(gate, null, 2)}\n`);

if (process.argv.includes("--require-pass")) {
  console.error("Apalis gate: FAIL (SQLite native-link dependency conflict)");
  process.exitCode = 1;
} else {
  console.log(JSON.stringify(gate, null, 2));
}
