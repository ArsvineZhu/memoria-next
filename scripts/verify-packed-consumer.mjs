import { execFile } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { promisify } from "node:util";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const execFileAsync = promisify(execFile);
const root = fileURLToPath(new URL("..", import.meta.url));
const npmCommand = process.platform === "win32" ? process.execPath : "npm";
const npmPrefixArguments =
  process.platform === "win32"
    ? [
        resolve(
          dirname(process.execPath),
          "node_modules",
          "npm",
          "bin",
          "npm-cli.js",
        ),
      ]
    : [];
const temporaryRoot = await mkdtemp(join(tmpdir(), "memoria-next-packed-"));

try {
  const packDirectory = join(temporaryRoot, "pack");
  const consumerDirectory = join(temporaryRoot, "consumer");
  await mkdir(packDirectory, { recursive: true });
  await mkdir(consumerDirectory, { recursive: true });
  const packOutput = await execFileAsync(
    npmCommand,
    [
      ...npmPrefixArguments,
      "pack",
      "--json",
      "--pack-destination",
      packDirectory,
    ],
    { cwd: root },
  );
  const packed = JSON.parse(packOutput.stdout.trim());
  const tarball = resolve(packDirectory, packed[0].filename);
  const consumerManifest = {
    name: "memoria-packed-consumer",
    private: true,
    type: "module",
  };
  await writeFile(
    join(consumerDirectory, "package.json"),
    JSON.stringify(consumerManifest, null, 2) + "\n",
  );
  await execFileAsync(
    npmCommand,
    [
      ...npmPrefixArguments,
      "install",
      "--no-package-lock",
      "--ignore-scripts",
      tarball,
    ],
    { cwd: consumerDirectory },
  );
  const consumerScript = join(consumerDirectory, "consumer.mjs");
  await writeFile(
    consumerScript,
    [
      'import assert from "node:assert/strict";',
      'import { mkdtemp, rm } from "node:fs/promises";',
      'import { tmpdir } from "node:os";',
      'import { join } from "node:path";',
      'import { createMemoria } from "@arsvinezhu/memoria";',
      'import { serializeMemoryDocument } from "@arsvinezhu/memoria/authoring";',
      "",
      'const dataDir = await mkdtemp(join(tmpdir(), "memoria-packed-store-"));',
      "const memoria = await createMemoria({ dataDir });",
      "try {",
      '  const space = await memoria.spaces.create({ key: "personal" });',
      "  const document = await memoria.documents.create({",
      "    space,",
      '    documentKey: "career",',
      '    mdx: serializeMemoryDocument({ title: "Career", body: "Rust systems work" }),',
      "  });",
      "  const response = await memoria.query({",
      "    scope: { spaces: [space.id] },",
      '    cue: { text: "Rust" },',
      "  });",
      "  assert.equal(response.results.length, 1);",
      "  assert.equal(response.results[0].memoryId, document.memoryId);",
      "} finally {",
      "  await memoria.close();",
      "  await rm(dataDir, { recursive: true, force: true });",
      "}",
    ].join("\n"),
  );
  await execFileAsync(process.execPath, [consumerScript], {
    cwd: consumerDirectory,
  });
  const installedPackage = await readFile(
    join(
      consumerDirectory,
      "node_modules",
      "@arsvinezhu",
      "memoria",
      "package.json",
    ),
    "utf8",
  );
  assertPackageSurface(JSON.parse(installedPackage));
  console.log("packed consumer passed");
} finally {
  await rm(temporaryRoot, { recursive: true, force: true });
}

function assertPackageSurface(manifest) {
  if (
    manifest.name !== "@arsvinezhu/memoria" ||
    manifest.exports?.["./authoring"] === undefined
  ) {
    throw new Error("packed package is missing the public authoring subpath");
  }
}
