import type { ProviderSet } from "../providers/types.js";
import { MemoriaError } from "./errors.js";

export interface RuntimePolicy {
  providerConcurrency?: number;
  defaultReadinessTimeoutMs?: number;
  backgroundWorkBatchSize?: number;
}

export interface PrivacyPolicy {
  allowProviderDataEgress?: boolean;
}

export interface ResourceLimits {
  maxSourceBytes?: number;
  maxQueryResults?: number;
}

export interface DiagnosticsPolicy {
  level?: "off" | "errors" | "verbose";
}

export interface MemoriaConfig {
  dataDir: string;
  providers?: ProviderSet;
  runtime?: RuntimePolicy;
  privacy?: PrivacyPolicy;
  resourceLimits?: ResourceLimits;
  diagnostics?: DiagnosticsPolicy;
}

function assertPositiveInteger(name: string, value: number | undefined): void {
  if (value !== undefined && (!Number.isSafeInteger(value) || value < 1)) {
    throw new MemoriaError(
      "UNSUPPORTED_OPERATION",
      `${name} must be a positive integer`,
    );
  }
}

function assertAllowedKeys(
  value: object,
  allowed: readonly string[],
  scope: string,
): void {
  for (const key of Object.keys(value)) {
    if (!allowed.includes(key)) {
      throw new MemoriaError(
        "UNSUPPORTED_OPERATION",
        `${scope}.${key} is not a stable public configuration field`,
      );
    }
  }
}

export function validateMemoriaConfig(config: MemoriaConfig): MemoriaConfig {
  if (!config.dataDir.trim()) {
    throw new MemoriaError(
      "UNSUPPORTED_OPERATION",
      "dataDir must not be empty",
    );
  }
  assertAllowedKeys(
    config,
    [
      "dataDir",
      "providers",
      "runtime",
      "privacy",
      "resourceLimits",
      "diagnostics",
    ],
    "config",
  );
  if (config.runtime) {
    assertAllowedKeys(
      config.runtime,
      [
        "providerConcurrency",
        "defaultReadinessTimeoutMs",
        "backgroundWorkBatchSize",
      ],
      "config.runtime",
    );
    assertPositiveInteger(
      "config.runtime.providerConcurrency",
      config.runtime.providerConcurrency,
    );
    assertPositiveInteger(
      "config.runtime.defaultReadinessTimeoutMs",
      config.runtime.defaultReadinessTimeoutMs,
    );
    assertPositiveInteger(
      "config.runtime.backgroundWorkBatchSize",
      config.runtime.backgroundWorkBatchSize,
    );
  }
  return config;
}
