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
