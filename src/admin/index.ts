import type { MemoriaStatus } from "../engine/memoria.js";
import {
  createPortableExport,
  importPortablePackage,
  type PortableExportOptions,
  type PortableImportInput,
  type PortableImportResult,
  type StoreBackupOptions,
  type StoreBackupResult,
} from "./governance.js";
import type { NativeBackupResult } from "../native/protocol.js";

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

interface BackupEngine {
  createBackup(options?: StoreBackupOptions): Promise<NativeBackupResult>;
  restoreBackup(input: {
    backupPath: string;
    targetDir: string;
  }): Promise<NativeBackupResult>;
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
  engine: StatusEngine & TransferEngine & BackupEngine,
  dataDir: string,
): AdminApi {
  return {
    status: () => engine.status(),
    createBackup: (options) => engine.createBackup(options),
    restoreBackup: (input) => engine.restoreBackup(input),
    export: (input) => createPortableExport(dataDir, engine, input),
    import: (input) => importPortablePackage(engine, input),
    planPurge: (input) => engine.planPurge(input.memory.id),
    executePurge: (input) => engine.executePurge(input.planId),
  };
}
