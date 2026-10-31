import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const rootDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const outDir = path.join(rootDir, 'public', 'wasm');
const srcWasm = path.join(
  rootDir,
  'target',
  'wasm32-unknown-unknown',
  'release',
  'osdbits_core.wasm',
);
const dstWasm = path.join(outDir, 'osdbits_core.wasm');

execFileSync(
  'cargo',
  ['build', '--package', 'osdbits-core', '--target', 'wasm32-unknown-unknown', '--release'],
  { cwd: rootDir, stdio: 'inherit' },
);

const bytes = fs.readFileSync(srcWasm);
if (
  bytes.length < 8 ||
  bytes[0] !== 0x00 ||
  bytes[1] !== 0x61 ||
  bytes[2] !== 0x73 ||
  bytes[3] !== 0x6d
) {
  throw new Error(`Invalid WASM magic header in ${srcWasm}`);
}

const maxBudgetBytes = 256 * 1024;
if (bytes.length > maxBudgetBytes) {
  throw new Error(
    `WASM size budget exceeded: ${bytes.length} bytes > ${maxBudgetBytes} bytes`,
  );
}

fs.mkdirSync(outDir, { recursive: true });
fs.copyFileSync(srcWasm, dstWasm);
console.info(
  `[build:wasm] Wrote ${path.relative(rootDir, dstWasm)} (${(bytes.length / 1024).toFixed(2)} KiB)`,
);
