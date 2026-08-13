export type MemoriaErrorCode =
  | "NOT_FOUND"
  | "ALREADY_EXISTS"
  | "KEY_CONFLICT"
  | "HEAD_CONFLICT"
  | "GENERATION_CONFLICT"
  | "INVALID_MDX"
  | "SEMANTIC_VALIDATION_FAILED"
  | "OUT_OF_SCOPE"
  | "SNAPSHOT_UNAVAILABLE"
  | "CONTINUATION_EXPIRED"
  | "CAPABILITY_NOT_READY"
  | "PROVIDER_UNAVAILABLE"
  | "STORE_LOCKED"
  | "STORE_CORRUPT"
  | "IDEMPOTENCY_CONFLICT"
  | "PURGE_CONFLICT"
  | "RESOURCE_LIMIT"
  | "UNSUPPORTED_PROFILE"
  | "UNSUPPORTED_STORE_FORMAT"
  | "ABORTED"
  | "QUERY_TIMEOUT"
  | "UNSUPPORTED_OPERATION"
  | "NATIVE_ERROR";

const knownCodes: readonly MemoriaErrorCode[] = [
  "NOT_FOUND",
  "ALREADY_EXISTS",
  "KEY_CONFLICT",
  "HEAD_CONFLICT",
  "GENERATION_CONFLICT",
  "INVALID_MDX",
  "SEMANTIC_VALIDATION_FAILED",
  "OUT_OF_SCOPE",
  "SNAPSHOT_UNAVAILABLE",
  "CONTINUATION_EXPIRED",
  "CAPABILITY_NOT_READY",
  "PROVIDER_UNAVAILABLE",
  "STORE_LOCKED",
  "STORE_CORRUPT",
  "IDEMPOTENCY_CONFLICT",
  "PURGE_CONFLICT",
  "RESOURCE_LIMIT",
  "UNSUPPORTED_PROFILE",
  "UNSUPPORTED_STORE_FORMAT",
  "ABORTED",
  "QUERY_TIMEOUT",
  "UNSUPPORTED_OPERATION",
  "NATIVE_ERROR",
];

export class MemoriaError extends Error {
  readonly code: MemoriaErrorCode;

  constructor(code: MemoriaErrorCode, message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = "MemoriaError";
    this.code = code;
  }
}

function codeFromMessage(message: string): MemoriaErrorCode {
  for (const code of knownCodes) {
    if (message.includes(`${code}:`) || message.startsWith(code)) {
      return code;
    }
  }
  return "NATIVE_ERROR";
}

export function toMemoriaError(error: unknown): MemoriaError {
  if (error instanceof MemoriaError) {
    return error;
  }
  const message = error instanceof Error ? error.message : String(error);
  return new MemoriaError(codeFromMessage(message), message, { cause: error });
}

export function isMemoriaError(error: unknown, code: MemoriaErrorCode): boolean {
  return toMemoriaError(error).code === code;
}
