import type { MemoriaStatus } from "../engine/memoria.js";
import {
  createPortableExport,
  importPortablePackage,
  createStoreBackup,
  restoreStoreBackup,
  type PortableExportOptions,
  type PortableImportInput,
  type PortableImportResult,
  type StoreBackupOptions,
  type StoreBackupResult,
} from "./governance.js";

interface StatusEngine {
  status(): Promise<MemoriaStatus>;
}

interface TransferEngine {
  exportMemories(scope: string[]): Promise<PortableExportMemory[]>;
  createSpace(key: string): Promise<string>;
  createMemory(request: {
    spaceId: string;
    documentKey?: string;
    idempotencyKey?: string;
    mdx: string;
  }): Promise<{ memoryId: string; authorityGeneration: string }>;
  planPurge(memoryId: string): Promise<NativePurgePlan>;
  executePurge(planId: string): Promise<NativePurgePlan>;
}

export interface PortableExportMemory {
  sourceId: string;
  spaceId: string;
  revisionId: string;
  mdx: string;
}

export interface NativePurgePlan {
  id: string;
  memoryId: string;
  state: "planned" | "committed" | "cleaning" | "completed";
}

export interface AdminApi {
  status(): Promise<MemoriaStatus>;
  createBackup(options?: StoreBackupOptions): Promise<StoreBackupResult>;
  restoreBackup(input: {
    backupPath: string;
    targetDir: string;
  }): Promise<{ path: string; storeId: string }>;
  export(
    input: PortableExportOptions,
  ): Promise<{ path: string; storeId: string }>;
  import(input: PortableImportInput): Promise<PortableImportResult>;
  planPurge(input: { memory: { id: string } }): Promise<NativePurgePlan>;
  executePurge(input: { planId: string }): Promise<NativePurgePlan>;
}

export function createAdminApi(
  engine: StatusEngine & TransferEngine,
  dataDir: string,
): AdminApi {
  const imports = new Map<string, PortableImportResult>();
  return {
    status: () => engine.status(),
    createBackup: (options) => createStoreBackup(dataDir, options),
    restoreBackup: (input) =>
      restoreStoreBackup(input.backupPath, input.targetDir),
    export: (input) => createPortableExport(dataDir, engine, input),
    import: (input) => {
      const existing = imports.get(input.idempotencyKey);
      if (existing) {
        return Promise.resolve(existing);
      }
      return importPortablePackage(engine, input).then((result) => {
        imports.set(input.idempotencyKey, result);
        return result;
      });
    },
    planPurge: (input) => engine.planPurge(input.memory.id),
    executePurge: (input) => engine.executePurge(input.planId),
  };
}
