import {
  createCipheriv,
  createDecipheriv,
  createHash,
  randomBytes,
} from "node:crypto";
import { cp, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { performance } from "node:perf_hooks";
import { dirname, join } from "node:path";

type Prototype = "authority-only" | "managed-store";

interface FixtureFile {
  relativePath: string;
  bytes: Buffer;
}

interface PrototypeMetrics {
  prototype: Prototype;
  encryptedFiles: string[];
  plaintextFiles: string[];
  artifactBytes: number;
  openLatencyMs: number;
  writeLatencyMs: number;
  largeSequentialReadLatencyMs: number;
  largeRandomReadLatencyMs: number;
  backupRestoreLatencyMs: number;
  keyRotationLatencyMs: number;
  checksum: string;
}

interface BenchmarkReport {
  generatedAt: string;
  node: string;
  platform: string;
  artifactBytes: number;
  prototypes: PrototypeMetrics[];
}

const KEY_BYTES = 32;
const LARGE_ARTIFACT_BYTES = 4 * 1024 * 1024;
const LARGE_ARTIFACT_PATH = "derived/index.mmap";
const SENSITIVE_PREFIXES = ["authority/", "derived/", "adaptive/", "cache/"];

const main = async (): Promise<void> => {
  const fixture = createFixture();
  const root = await mkdtemp(join(tmpdir(), "memoria-next-encryption-"));

  try {
    const prototypes: PrototypeMetrics[] = [];
    for (const prototype of ["authority-only", "managed-store"] as const) {
      prototypes.push(await measurePrototype(root, prototype, fixture));
    }

    const report: BenchmarkReport = {
      generatedAt: new Date().toISOString(),
      node: process.version,
      platform: process.platform + "/" + process.arch,
      artifactBytes: LARGE_ARTIFACT_BYTES,
      prototypes,
    };
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rm(root, { recursive: true, force: true });
  }
};

const measurePrototype = async (
  root: string,
  prototype: Prototype,
  fixture: FixtureFile[],
): Promise<PrototypeMetrics> => {
  const store = join(root, prototype);
  const backup = join(root, prototype + "-backup");
  const restored = join(root, prototype + "-restored");
  const oldKey = Buffer.alloc(KEY_BYTES, 0x11);
  const newKey = Buffer.alloc(KEY_BYTES, 0x22);

  await writePrototype(store, prototype, fixture, oldKey);
  const protectedFiles = fixture
    .filter((file) => isProtected(prototype, file.relativePath))
    .map((file) => file.relativePath);
  const plaintextFiles = fixture
    .filter((file) => !isProtected(prototype, file.relativePath))
    .map((file) => file.relativePath);

  const openLatencyMs = await measure(async () => {
    await openPrototype(store, prototype, oldKey, fixture);
  });
  const writeLatencyMs = await measure(async () => {
    const authority = fixture.find(
      (file) => file.relativePath === "authority/authority.sqlite",
    );
    if (authority === undefined) {
      throw new Error("authority fixture is missing");
    }
    await writeStored(
      store,
      prototype,
      authority.relativePath,
      Buffer.concat([authority.bytes, Buffer.from("write")]),
      oldKey,
    );
  });
  const largeSequentialReadLatencyMs = await measure(async () => {
    const bytes = await readStored(
      store,
      prototype,
      LARGE_ARTIFACT_PATH,
      oldKey,
    );
    if (bytes.length !== LARGE_ARTIFACT_BYTES) {
      throw new Error("large artifact length mismatch");
    }
  });
  const largeRandomReadLatencyMs = await measure(async () => {
    const bytes = await readStored(
      store,
      prototype,
      LARGE_ARTIFACT_PATH,
      oldKey,
    );
    const offsets = [0, 1_048_576, LARGE_ARTIFACT_BYTES - 4_096];
    const digest = createHash("sha256");
    for (const offset of offsets) {
      digest.update(bytes.subarray(offset, offset + 4_096));
    }
    if (digest.digest().length !== 32) {
      throw new Error("random read digest failed");
    }
  });
  const backupRestoreLatencyMs = await measure(async () => {
    await rm(backup, { recursive: true, force: true });
    await rm(restored, { recursive: true, force: true });
    await cp(store, backup, { recursive: true });
    await cp(backup, restored, { recursive: true });
    await openPrototype(restored, prototype, oldKey, fixture);
  });
  const keyRotationLatencyMs = await measure(async () => {
    await rotateKey(store, prototype, fixture, oldKey, newKey);
    await openPrototype(store, prototype, newKey, fixture);
  });

  const checksum = createHash("sha256");
  for (const file of fixture) {
    checksum.update(
      await readStored(store, prototype, file.relativePath, newKey),
    );
  }

  return {
    prototype,
    encryptedFiles: protectedFiles,
    plaintextFiles,
    artifactBytes: LARGE_ARTIFACT_BYTES,
    openLatencyMs,
    writeLatencyMs,
    largeSequentialReadLatencyMs,
    largeRandomReadLatencyMs,
    backupRestoreLatencyMs,
    keyRotationLatencyMs,
    checksum: checksum.digest("hex"),
  };
};

const createFixture = (): FixtureFile[] => [
  { relativePath: "STORE", bytes: Buffer.from("memoria-store-v1") },
  {
    relativePath: "authority/authority.sqlite",
    bytes: repeatedBytes(128 * 1024, 0x41),
  },
  {
    relativePath: "authority/objects/source-0001",
    bytes: repeatedBytes(512 * 1024, 0x53),
  },
  {
    relativePath: LARGE_ARTIFACT_PATH,
    bytes: repeatedBytes(LARGE_ARTIFACT_BYTES, 0x44),
  },
  {
    relativePath: "adaptive/events.log",
    bytes: repeatedBytes(256 * 1024, 0x41),
  },
  {
    relativePath: "cache/query.json",
    bytes: repeatedBytes(32 * 1024, 0x43),
  },
];

const repeatedBytes = (length: number, seed: number): Buffer => {
  const bytes = Buffer.alloc(length);
  for (let index = 0; index < bytes.length; index += 1) {
    bytes[index] = (seed + index) % 251;
  }
  return bytes;
};

const isProtected = (prototype: Prototype, relativePath: string): boolean => {
  if (relativePath === "STORE") {
    return false;
  }
  if (prototype === "authority-only") {
    return relativePath === "authority/authority.sqlite";
  }
  return SENSITIVE_PREFIXES.some((prefix) => relativePath.startsWith(prefix));
};

const writePrototype = async (
  store: string,
  prototype: Prototype,
  fixture: FixtureFile[],
  key: Buffer,
): Promise<void> => {
  await mkdir(store, { recursive: true });
  for (const file of fixture) {
    await writeStored(store, prototype, file.relativePath, file.bytes, key);
  }
};

const writeStored = async (
  store: string,
  prototype: Prototype,
  relativePath: string,
  bytes: Buffer,
  key: Buffer,
): Promise<void> => {
  const target = join(store, relativePath);
  await mkdir(dirname(target), { recursive: true });
  await writeFile(
    target,
    isProtected(prototype, relativePath) ? encrypt(bytes, key) : bytes,
  );
};

const readStored = async (
  store: string,
  prototype: Prototype,
  relativePath: string,
  key: Buffer,
): Promise<Buffer> => {
  const bytes = await readFile(join(store, relativePath));
  return isProtected(prototype, relativePath) ? decrypt(bytes, key) : bytes;
};

const openPrototype = async (
  store: string,
  prototype: Prototype,
  key: Buffer,
  fixture: FixtureFile[],
): Promise<void> => {
  for (const file of fixture) {
    const bytes = await readStored(store, prototype, file.relativePath, key);
    if (bytes.length === 0) {
      throw new Error("empty fixture file: " + file.relativePath);
    }
  }
};

const rotateKey = async (
  store: string,
  prototype: Prototype,
  fixture: FixtureFile[],
  oldKey: Buffer,
  newKey: Buffer,
): Promise<void> => {
  for (const file of fixture) {
    if (!isProtected(prototype, file.relativePath)) {
      continue;
    }
    const bytes = await readStored(store, prototype, file.relativePath, oldKey);
    await writeStored(store, prototype, file.relativePath, bytes, newKey);
  }
};

const encrypt = (plaintext: Buffer, key: Buffer): Buffer => {
  const iv = randomBytes(12);
  const cipher = createCipheriv("aes-256-gcm", key, iv);
  const ciphertext = Buffer.concat([cipher.update(plaintext), cipher.final()]);
  const tag = cipher.getAuthTag();
  return Buffer.concat([iv, tag, ciphertext]);
};

const decrypt = (envelope: Buffer, key: Buffer): Buffer => {
  const iv = envelope.subarray(0, 12);
  const tag = envelope.subarray(12, 28);
  const ciphertext = envelope.subarray(28);
  const decipher = createDecipheriv("aes-256-gcm", key, iv);
  decipher.setAuthTag(tag);
  return Buffer.concat([decipher.update(ciphertext), decipher.final()]);
};

const measure = async (operation: () => Promise<void>): Promise<number> => {
  const started = performance.now();
  await operation();
  return Number(Math.max(0.001, performance.now() - started).toFixed(3));
};

await main();
