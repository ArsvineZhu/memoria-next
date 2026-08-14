import type { MemoriaStatus } from "../engine/memoria.js";
import {
  createStoreBackup,
  restoreStoreBackup,
  type StoreBackupOptions,
  type StoreBackupResult,
} from "./governance.js";

interface StatusEngine {
  status(): Promise<MemoriaStatus>;
}

export interface AdminApi {
  status(): Promise<MemoriaStatus>;
  createBackup(options?: StoreBackupOptions): Promise<StoreBackupResult>;
  restoreBackup(input: {
    backupPath: string;
    targetDir: string;
  }): Promise<{ path: string; storeId: string }>;
}

export function createAdminApi(
  engine: StatusEngine,
  dataDir: string,
): AdminApi {
  return {
    status: () => engine.status(),
    createBackup: (options) => createStoreBackup(dataDir, options),
    restoreBackup: (input) =>
      restoreStoreBackup(input.backupPath, input.targetDir),
  };
}
