export interface EntityObservation {
  readonly entityRef: string;
  readonly surface: string;
  readonly spaceId: string;
}

export interface ScopedEntityQuery {
  readonly surface: string;
  readonly allowedSpaces: readonly string[];
}

/**
 * Apply the caller scope before counting, ranking, or serializing discovery
 * observations. The returned objects contain no aggregate global metadata.
 */
export function discoverScopedEntities(
  observations: readonly EntityObservation[],
  query: ScopedEntityQuery,
): EntityObservation[] {
  const allowedSpaces = new Set(query.allowedSpaces);
  return observations
    .filter(
      (observation) =>
        observation.surface === query.surface &&
        allowedSpaces.has(observation.spaceId),
    )
    .map((observation) => ({ ...observation }));
}

export interface StoreBackupOptions {
  includeAdaptive?: boolean;
  outputPath?: string;
}

export interface StoreBackupResult {
  path: string;
  storeId: string;
  includeAdaptive: boolean;
  fileCount: number;
  manifestHash: string;
}

interface BackupManifest {
  format: "memoria-backup-v1";
  storeId: string;
  includeAdaptive: boolean;
  files: Record<string, string>;
}

export async function createStoreBackup(
  sourceDir: string,
  options: StoreBackupOptions = {},
): Promise<StoreBackupResult> {
  const source = resolve(sourceDir);
  const marker = (await readFile(join(source, "STORE"), "utf8")).trim();
  if (!marker) {
    throw new Error("STORE_CORRUPT: store marker is empty");
  }
  await assertFile(join(source, "authority", "authority.sqlite"));
  const destination = resolve(
    options.outputPath ??
      (await mkdtemp(join(tmpdir(), "memoria-next-backup-"))),
  );
  if (destination === source || destination.startsWith(source + sep)) {
    throw new Error(
      "UNSUPPORTED_OPERATION: backup destination is inside the source store",
    );
  }
  await rm(destination, { recursive: true, force: true });
  await mkdir(destination, { recursive: true });
  await cp(join(source, "STORE"), join(destination, "STORE"));
  await cp(join(source, "authority"), join(destination, "authority"), {
    recursive: true,
  });
  const includeAdaptive = options.includeAdaptive === true;
  if (includeAdaptive && (await exists(join(source, "adaptive")))) {
    await cp(join(source, "adaptive"), join(destination, "adaptive"), {
      recursive: true,
    });
  }
  const files = await hashFiles(destination, ["MANIFEST.json"]);
  const manifest: BackupManifest = {
    format: "memoria-backup-v1",
    storeId: marker,
    includeAdaptive,
    files,
  };
  const manifestText = JSON.stringify(manifest, null, 2) + "\n";
  await writeFile(join(destination, "MANIFEST.json.tmp"), manifestText, "utf8");
  await rename(
    join(destination, "MANIFEST.json.tmp"),
    join(destination, "MANIFEST.json"),
  );
  return {
    path: destination,
    storeId: marker,
    includeAdaptive,
    fileCount: Object.keys(files).length,
    manifestHash: digest(manifestText),
  };
}

export async function restoreStoreBackup(
  backupPath: string,
  targetDir: string,
): Promise<{ path: string; storeId: string }> {
  const backup = resolve(backupPath);
  const target = resolve(targetDir);
  const manifest = JSON.parse(
    await readFile(join(backup, "MANIFEST.json"), "utf8"),
  ) as BackupManifest;
  if (manifest.format !== "memoria-backup-v1") {
    throw new Error("UNSUPPORTED_STORE_FORMAT: unsupported backup format");
  }
  const marker = (await readFile(join(backup, "STORE"), "utf8")).trim();
  if (marker !== manifest.storeId) {
    throw new Error("STORE_CORRUPT: backup identity marker mismatch");
  }
  const files = await hashFiles(backup, ["MANIFEST.json"]);
  if (
    Object.keys(files).length !== Object.keys(manifest.files).length ||
    Object.entries(manifest.files).some(([path, hash]) => files[path] !== hash)
  ) {
    throw new Error("STORE_CORRUPT: backup manifest hash mismatch");
  }
  if (await exists(target)) {
    throw new Error("ALREADY_EXISTS: restore target already exists");
  }
  const stage = await mkdtemp(
    join(dirname(target), "." + basename(target) + "-restore-"),
  );
  try {
    await cp(backup, stage, { recursive: true });
    await rm(join(stage, "MANIFEST.json"), { force: true });
    await rename(stage, target);
  } catch (error) {
    await rm(stage, { recursive: true, force: true });
    throw error;
  }
  return { path: target, storeId: marker };
}

export interface PortableExportOptions {
  scope: string[];
  outputPath?: string;
}

export interface PortableImportInput {
  packagePath: string;
  targetSpace: { key: string };
  idempotencyKey: string;
}

export interface PortableImportResult {
  mappings: {
    memories: Array<{ sourceId: string; targetId: string }>;
  };
  unresolvedExternalReferences: string[];
  targetSpaceId: string;
}

interface TransferMemory {
  sourceId: string;
  spaceId: string;
  revisionId: string;
  mdx: string;
  references?: string[];
}

interface PortablePackage {
  format: "memoria-portable-v1";
  originStoreId: string;
  scope: string[];
  memories: TransferMemory[];
}

interface TransferEngine {
  exportMemories(scope: string[]): Promise<TransferMemory[]>;
  createSpace(key: string): Promise<string>;
  createMemory(request: {
    spaceId: string;
    documentKey?: string;
    idempotencyKey?: string;
    mdx: string;
  }): Promise<{ memoryId: string; authorityGeneration: string }>;
  importPortable?(
    request: import("../native/protocol.js").NativePortableImportRequest,
  ): Promise<import("../native/protocol.js").NativePortableImportResult>;
}

export async function createPortableExport(
  sourceDir: string,
  engine: TransferEngine,
  options: PortableExportOptions,
): Promise<{ path: string; storeId: string }> {
  if (options.scope.length === 0) {
    throw new Error("OUT_OF_SCOPE: portable export requires an explicit scope");
  }
  const storeId = (
    await readFile(join(resolve(sourceDir), "STORE"), "utf8")
  ).trim();
  const memories = (await engine.exportMemories(options.scope)).map(
    (memory) => ({
      ...memory,
      references: extractMemoryReferences(memory.mdx),
    }),
  );
  const packageValue: PortablePackage = {
    format: "memoria-portable-v1",
    originStoreId: storeId,
    scope: [...options.scope],
    memories,
  };
  const outputPath =
    options.outputPath ??
    join(
      await mkdtemp(join(tmpdir(), "memoria-next-export-")),
      "memoria-portable.json",
    );
  await writeFile(
    outputPath,
    JSON.stringify(packageValue, null, 2) + "\n",
    "utf8",
  );
  return { path: resolve(outputPath), storeId };
}

export async function importPortablePackage(
  engine: TransferEngine,
  input: PortableImportInput,
): Promise<PortableImportResult> {
  if (!input.idempotencyKey.trim()) {
    throw new Error("IDEMPOTENCY_CONFLICT: import idempotency key is empty");
  }
  const packagePath = resolve(input.packagePath);
  if ((await stat(packagePath)).size > 32 * 1024 * 1024) {
    throw new Error("RESOURCE_LIMIT: portable package is too large");
  }
  const packageValue = JSON.parse(
    await readFile(packagePath, "utf8"),
  ) as PortablePackage;
  validatePortablePackage(packageValue);
  const sourceIds = new Set<string>();
  for (const memory of packageValue.memories) {
    if (sourceIds.has(memory.sourceId)) {
      throw new Error(
        "INVALID_MDX: portable package contains an invalid memory",
      );
    }
    sourceIds.add(memory.sourceId);
  }

  const requestFingerprint = digest(
    JSON.stringify({
      format: packageValue.format,
      originStoreId: packageValue.originStoreId,
      scope: packageValue.scope,
      memories: packageValue.memories,
    }),
  );
  if (engine.importPortable) {
    const imported = await engine.importPortable({
      targetSpaceKey: input.targetSpace.key,
      idempotencyKey: input.idempotencyKey,
      requestFingerprint,
      originStoreId: packageValue.originStoreId,
      memories: packageValue.memories.map(
        ({ sourceId, spaceId, revisionId, mdx }) => ({
          sourceId,
          spaceId,
          revisionId,
          mdx,
        }),
      ),
    });
    return {
      mappings: { memories: imported.mappings },
      unresolvedExternalReferences: [
        ...imported.unresolvedExternalReferences,
      ].sort(),
      targetSpaceId: imported.targetSpaceId,
    };
  }

  // Lightweight test engines may not expose the Rust import entry point. The
  // native Memoria engine always takes the durable path above.
  const targetSpaceId = await engine.createSpace(input.targetSpace.key);
  const mappings: Array<{ sourceId: string; targetId: string }> = [];
  const mappingBySource = new Map<string, string>();
  for (const memory of packageValue.memories) {
    const created = await engine.createMemory({
      spaceId: targetSpaceId,
      idempotencyKey: input.idempotencyKey + ":memory:" + memory.sourceId,
      mdx: memory.mdx,
    });
    mappings.push({ sourceId: memory.sourceId, targetId: created.memoryId });
    mappingBySource.set(memory.sourceId, created.memoryId);
  }
  const unresolved = new Set<string>();
  for (const memory of packageValue.memories) {
    for (const reference of memory.references ?? []) {
      if (!mappingBySource.has(reference)) {
        unresolved.add(reference);
      }
    }
  }
  return {
    mappings: { memories: mappings },
    unresolvedExternalReferences: [...unresolved].sort(),
    targetSpaceId,
  };
}

function validatePortablePackage(value: PortablePackage): void {
  if (
    value.format !== "memoria-portable-v1" ||
    typeof value.originStoreId !== "string" ||
    !Array.isArray(value.scope) ||
    !value.scope.every(
      (space) => typeof space === "string" && space.length > 0,
    ) ||
    !Array.isArray(value.memories) ||
    value.memories.length === 0 ||
    value.memories.length > 4096
  ) {
    throw new Error("UNSUPPORTED_STORE_FORMAT: unsupported portable package");
  }
  let totalSourceBytes = 0;
  for (const memory of value.memories) {
    if (
      typeof memory.sourceId !== "string" ||
      typeof memory.spaceId !== "string" ||
      typeof memory.revisionId !== "string" ||
      typeof memory.mdx !== "string" ||
      memory.sourceId.length === 0 ||
      memory.spaceId.length === 0 ||
      memory.revisionId.length === 0 ||
      memory.mdx.length === 0 ||
      memory.mdx.length > 4 * 1024 * 1024 ||
      (memory.references !== undefined &&
        (!Array.isArray(memory.references) ||
          !memory.references.every(
            (reference) => typeof reference === "string",
          )))
    ) {
      throw new Error(
        "INVALID_MDX: portable package contains an invalid memory",
      );
    }
    totalSourceBytes += Buffer.byteLength(memory.mdx, "utf8");
    if (totalSourceBytes > 64 * 1024 * 1024) {
      throw new Error("RESOURCE_LIMIT: portable package source is too large");
    }
  }
}

async function hashFiles(
  root: string,
  excluded: readonly string[],
): Promise<Record<string, string>> {
  const entries = await walk(root);
  const result: Record<string, string> = {};
  for (const path of entries) {
    const relativePath = relative(root, path).split(sep).join("/");
    if (excluded.includes(relativePath)) {
      continue;
    }
    result[relativePath] = digest(await readFile(path));
  }
  return result;
}

async function walk(root: string): Promise<string[]> {
  const entries = await readdir(root, { withFileTypes: true });
  const result: string[] = [];
  for (const entry of entries) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) {
      result.push(...(await walk(path)));
    } else if (entry.isFile()) {
      result.push(path);
    }
  }
  return result.sort();
}

function digest(value: Uint8Array | string): string {
  return createHash("sha256").update(value).digest("hex");
}

async function exists(path: string): Promise<boolean> {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

async function assertFile(path: string): Promise<void> {
  if (!(await exists(path))) {
    throw new Error("STORE_CORRUPT: required store file is missing");
  }
}

function extractMemoryReferences(mdx: string): string[] {
  return [...new Set(mdx.match(/M_[A-Z2-7]+/g) ?? [])].sort();
}
import { createHash } from "node:crypto";
import {
  cp,
  mkdtemp,
  mkdir,
  readFile,
  readdir,
  rename,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, join, relative, resolve, sep } from "node:path";
