import * as naga from "naga-wasm";

let initialized = false;

export interface WgslValidationResult {
  valid: boolean;
  entryPoints: string[];
  error?: string;
}

export async function ensureNagaInitialized(): Promise<void> {
  if (!initialized) {
    await naga.init();
    initialized = true;
  }
}

export async function validateWgslSource(
  source: string,
  expectedEntryPoints: readonly string[] = [],
): Promise<WgslValidationResult> {
  await ensureNagaInitialized();
  try {
    const module = naga.parseWgsl(source);
    const info = naga.validate(module);
    const normalized = naga.writeWgsl(module, info);
    const entryPoints: string[] = [];
    const fnRegex = /@(?:vertex|fragment|compute)\b[\s\S]*?\bfn\s+([A-Za-z_][A-Za-z0-9_]*)/g;
    let match: RegExpExecArray | null;
    while ((match = fnRegex.exec(normalized)) !== null) {
      if (match[1]) {
        entryPoints.push(match[1]);
      }
    }
    for (const expected of expectedEntryPoints) {
      if (!entryPoints.includes(expected)) {
        return {
          valid: false,
          entryPoints,
          error: `Missing expected WGSL entry point '${expected}' (found: [${entryPoints.join(", ")}])`,
        };
      }
    }
    return { valid: true, entryPoints };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      valid: false,
      entryPoints: [],
      error: message,
    };
  }
}
