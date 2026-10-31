/**
 * Types for `scripts/check-firmware.mjs`, the CI firmware guard.
 * The implementation is plain JavaScript so it runs under bare `node` in CI
 * without a build step; these declarations let the strict TypeScript test
 * suite import and exercise it.
 */

export interface Violation {
  /** Repository-relative path that violated a rule. */
  file: string;
  /** Stable identifier of the rule that matched. */
  rule: string;
  /** Human-readable explanation shown in the CI annotation. */
  detail: string;
}

export interface GuardResult {
  ok: boolean;
  violations: Violation[];
}

/** Inspect repository-relative paths and report prohibited files. */
export function evaluate(files: readonly string[]): GuardResult;

/** Collect a stream (e.g. stdin) into a string. */
export function readAll(stream: import('node:stream').Readable): Promise<string>;
