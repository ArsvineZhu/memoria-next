import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  createStoreBackup,
  restoreStoreBackup,
} from "../../src/admin/governance.js";

test("backup restore preserves the Store identity universe", async () => {
  const root = await mkdtemp(join(tmpdir(), "memoria-next-backup-test-"));
  const source = join(root, "source");
  const target = join(root, "restored");
  await mkdir(source);
  await writeFile(
    join(source, "STORE"),
    "ST_ABCDEFGHIJKLMNOPQRSTUVWXYZ234567\n",
  );
  await mkdir(join(source, "authority", "objects"));
  await writeFile(join(source, "authority", "authority.sqlite"), "authority");
  await mkdir(join(source, "adaptive"));
  await writeFile(join(source, "adaptive", "events.log"), "adaptive");

  try {
    const before = await readFile(join(source, "STORE"), "utf8");
    const backup = await createStoreBackup(source, { includeAdaptive: true });
    const restored = await restoreStoreBackup(backup.path, target);

    assert.equal(restored.storeId, before.trim());
    assert.equal(await readFile(join(target, "STORE"), "utf8"), before);
    assert.equal(
      await readFile(join(target, "adaptive", "events.log"), "utf8"),
      "adaptive",
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

async function mkdir(path: string): Promise<void> {
  const { mkdir: createDirectory } = await import("node:fs/promises");
  await createDirectory(path, { recursive: true });
}
