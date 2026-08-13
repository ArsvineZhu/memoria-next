import { readdir } from "node:fs/promises";
import { spawn } from "node:child_process";
import { join, resolve } from "node:path";

async function collectTestFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await collectTestFiles(path)));
    } else if (entry.isFile() && entry.name.endsWith(".test.js")) {
      files.push(path);
    }
  }
  return files;
}

const testRoot = resolve("dist-test", "tests");
const testFiles = (await collectTestFiles(testRoot)).sort();
if (testFiles.length === 0) {
  throw new Error(`No compiled test files found under ${testRoot}`);
}

const child = spawn(process.execPath, ["--test", ...testFiles], {
  stdio: "inherit",
});
child.on("exit", (code, signal) => {
  if (signal) {
    process.exitCode = 1;
  } else {
    process.exitCode = code ?? 1;
  }
});
