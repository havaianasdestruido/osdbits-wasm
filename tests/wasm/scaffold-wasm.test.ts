import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const rootDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const wasmPath = path.join(rootDir, 'public', 'wasm', 'osdbits_core.wasm');

interface CoreExports {
  osdbits_abi_version(): number;
}

describe('WASM module instantiation smoke check', () => {
  it('loads public/wasm/osdbits_core.wasm and verifies ABI version', async () => {
    const bytes = fs.readFileSync(wasmPath);
    const { instance } = await WebAssembly.instantiate(bytes);
    const exports = instance.exports as unknown as CoreExports;
    expect(exports.osdbits_abi_version()).toBe(1);
  });
});
