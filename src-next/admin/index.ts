import type { MemoriaStatus } from "../engine/memoria.js";

interface StatusEngine {
  status(): Promise<MemoriaStatus>;
}

export interface AdminApi {
  status(): Promise<MemoriaStatus>;
}

export function createAdminApi(engine: StatusEngine): AdminApi {
  return { status: () => engine.status() };
}
