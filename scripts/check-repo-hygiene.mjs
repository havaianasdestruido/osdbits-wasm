import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const rootDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

const ignoredDirs = new Set([
  '.git',
  'node_modules',
  'target',
  'dist',
  'coverage',
  'test-results',
  'playwright-report',
  'blob-report',
  '__pycache__',
]);

const forbiddenExtensions = new Set([
  '.rom',
  '.iso',
  '.mec',
  '.nvm',
  '.p2s',
  '.idb',
  '.elf',
]);

const requiredPreservedPaths = [
  'osdbits/Makefile',
  'osdbits/main.c',
  'osdbits/opening.c',
  'osdbits/menu.c',
  'osdbits/menuback.c',
  'osdbits/menuconfig.c',
  'osdbits/menutext.c',
  'osdbits/pad.c',
  'osdbits/sound.c',
  'osdbits/res.c',
  'osdbits/res.h',
  'osdbits/vucode_1.vsm',
  'osdbits/towerchain.dsm',
  'ps2expand.c',
  'notes.txt',
  'vucode_1.vu',
  'docs/OSDSYS-STRUCTURE-REPORT.txt',
  'docs/towers-analysis.md',
  'docs/glass-refraction.md',
  'matching/README.md',
  'tools/extract-res.py',
];

const violations = [];

for (const rel of requiredPreservedPaths) {
  if (!fs.existsSync(path.join(rootDir, rel))) {
    violations.push(`Missing preserved upstream path: ${rel}`);
  }
}

const resDir = path.join(rootDir, 'osdbits', 'res');
if (fs.existsSync(resDir) && fs.readdirSync(resDir).length > 0) {
  violations.push('osdbits/res/ must remain empty (no extracted Sony BIOS data in workspace)');
}

const resetSig = Buffer.from('RESET\0\0\0\0\0', 'ascii');
const romdirSig = Buffer.from('ROMDIR\0\0\0\0', 'ascii');

function walk(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (ignoredDirs.has(entry.name)) {
      continue;
    }
    const full = path.join(dir, entry.name);
    const rel = path.relative(rootDir, full);
    if (entry.isDirectory()) {
      walk(full);
      continue;
    }
    const ext = path.extname(entry.name).toLowerCase();
    if (forbiddenExtensions.has(ext)) {
      violations.push(`Forbidden BIOS/ROM artifact extension (${ext}): ${rel}`);
      continue;
    }
    if (ext === '.bin' || ext === '.inc') {
      violations.push(`Forbidden binary/extracted table file (${ext}): ${rel}`);
      continue;
    }
    const stat = fs.statSync(full);
    if (stat.size >= 32 && stat.size <= 16 * 1024 * 1024) {
      const fd = fs.openSync(full, 'r');
      const probeLen = Math.min(stat.size, 4096);
      const buf = Buffer.alloc(probeLen);
      fs.readSync(fd, buf, 0, probeLen, 0);
      fs.closeSync(fd);
      const resetIdx = buf.indexOf(resetSig);
      if (resetIdx !== -1 && buf.indexOf(romdirSig, resetIdx) !== -1) {
        violations.push(`Detected PS2 ROMDIR binary header inside: ${rel}`);
      }
    }
  }
}

walk(rootDir);

if (violations.length > 0) {
  console.error('[check:hygiene] Repository hygiene violations found:');
  for (const v of violations) {
    console.error(`  - ${v}`);
  }
  process.exit(1);
}

console.info(
  `[check:hygiene] OK — zero proprietary BIOS/ROM artifacts found; all ${requiredPreservedPaths.length} upstream reference paths intact.`,
);
