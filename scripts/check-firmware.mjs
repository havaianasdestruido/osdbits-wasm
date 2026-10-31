#!/usr/bin/env node
/**
 * Firmware / proprietary-resource guard.
 *
 * The repository may not contain Sony firmware dumps, savestates, symbol
 * databases, or resources extracted from a BIOS image. It *does* contain the
 * reverse-engineering artefacts that make the port auditable -- the VU
 * microcode disassembly (`vucode_*.vu`, `vucode_*.vsm`), the C reconstruction
 * (`osdbits/*.c`), and the analysis notes. Those must not be flagged.
 *
 * The rule lives in a importable module (rather than a regex pasted into the
 * workflow YAML) so that `tests/unit/ci-firmware-guard.test.ts` can assert both
 * directions: clean trees pass, contaminated trees fail, and the preserved
 * provenance files are never mistaken for extracted resources.
 *
 * Usage (CI and local):
 *   git ls-files | node scripts/check-firmware.mjs
 */

/** @typedef {{ file: string, rule: string, detail: string }} Violation */

/**
 * Extracted Sony resource trees produced by `tools/extract-res.py`.
 * `osdbits/res/` is the documented output directory; blobs whose names begin
 * with TEX, FNT, SND or ICO are the individual extracted resources wherever
 * they are placed.
 */
const EXTRACTED_RESOURCE_DIR = /(^|\/)osdbits\/res\//;
const EXTRACTED_RESOURCE_BLOB = /(^|\/)(TEX|FNT|SND|ICO)[A-Za-z0-9_]*\.(bin|tmx|img|gif|anm)$/i;

/** Firmware images, savestates, and the PCSX2 symbol database. */
const FIRMWARE_OR_STATE = /\.(bin|rom|iso|p2s|idb)$/i;

/**
 * Inspect a list of repository-relative paths.
 * @param {readonly string[]} files
 * @returns {{ ok: boolean, violations: Violation[] }}
 */
export function evaluate(files) {
  /** @type {Violation[]} */
  const violations = [];

  for (const raw of files) {
    const file = raw.trim();
    if (file === '') continue;

    // Specific rules first: a file inside the extracted-resource tree is better
    // reported as such than as a generic ".bin".
    if (EXTRACTED_RESOURCE_DIR.test(file)) {
      violations.push({
        file,
        rule: 'extracted-resource-dir',
        detail: 'extracted resource directory tracked in git',
      });
      continue;
    }

    if (EXTRACTED_RESOURCE_BLOB.test(file)) {
      violations.push({
        file,
        rule: 'extracted-resource-blob',
        detail: 'extracted Sony resource blob tracked in git',
      });
      continue;
    }

    if (FIRMWARE_OR_STATE.test(file)) {
      violations.push({
        file,
        rule: 'firmware-or-savestate',
        detail: 'firmware image, savestate, or symbol database tracked in git',
      });
    }
  }

  return { ok: violations.length === 0, violations };
}

/**
 * Read `git ls-files` output (newline separated) from a stream.
 * @param {import('node:stream').Readable} stream
 * @returns {Promise<string>}
 */
export function readAll(stream) {
  return new Promise((resolve, reject) => {
    let data = '';
    stream.setEncoding('utf8');
    stream.on('data', (chunk) => {
      data += chunk;
    });
    stream.on('end', () => resolve(data));
    stream.on('error', reject);
  });
}

const invokedDirectly = process.argv[1] !== undefined && import.meta.url === `file://${process.argv[1]}`;

if (invokedDirectly) {
  const tracked = (await readAll(process.stdin)).split('\n');
  const { ok, violations } = evaluate(tracked);

  for (const violation of violations) {
    process.stdout.write(`::error file=${violation.file}::${violation.detail} (${violation.rule})\n`);
    process.stderr.write(`  ${violation.file}: ${violation.detail}\n`);
  }

  if (!ok) {
    process.stderr.write(
      `\n${violations.length} prohibited file(s) tracked in git. ` +
        'Firmware dumps, extracted resources and savestates must never be committed.\n',
    );
    process.exitCode = 1;
  } else {
    process.stdout.write(`firmware guard: clean (${tracked.filter((f) => f.trim() !== '').length} tracked files)\n`);
  }
}
