import { describe, expect, it } from 'vitest';

import { evaluate } from '../../scripts/check-firmware.mjs';

/**
 * The guard that runs in CI (`git ls-files | node scripts/check-firmware.mjs`)
 * has two jobs, and both are load-bearing:
 *
 *  1. fail when proprietary firmware or extracted resources are committed;
 *  2. NOT fail on the provenance files this project is required to preserve.
 *
 * Regression: an earlier inline regex (`^osdbits/res/|^vucode_[0-9]`) matched the
 * VU microcode disassembly and turned the whole CI job red even though the tree
 * was clean.
 */

/** Files that genuinely exist in this repository and must never be flagged. */
const REAL_PROVENANCE_FILES = [
  'vucode_1.vu',
  'vucode_2.vu',
  'vucode_2.vsm',
  'vucode_3.vu',
  'vucode_3.vsm',
  'vucode_4.vu',
  'vucode_4.vsm',
  'vucode_5.vu',
  'vucode_5.vsm',
  'vucode_6.vu',
  'vucode_6.vsm',
  'vucode_7.vu',
  'osdbits/main.c',
  'osdbits/towers.c',
  'ps2expand.c',
  'tools/extract-res.py',
  'notes.txt',
  'docs/opening.png',
  'docs/LIMITATIONS.md',
  'crates/osdbits-core/src/assets.rs',
  'src/main.ts',
  'tests/e2e/app.spec.ts',
  'public/wasm/osdbits_core.wasm',
];

describe('firmware guard - accepts clean repositories', () => {
  it('does not flag the VU microcode or any other preserved provenance file', () => {
    const result = evaluate(REAL_PROVENANCE_FILES);
    expect(result.violations).toEqual([]);
    expect(result.ok).toBe(true);
  });

  it('ignores blank lines and surrounding whitespace', () => {
    const result = evaluate(['', '  ', ' vucode_1.vu ', 'osdbits/main.c\n']);
    expect(result.ok).toBe(true);
  });

  it('accepts an empty repository', () => {
    expect(evaluate([])).toEqual({ ok: true, violations: [] });
  });
});

describe('firmware guard - rejects proprietary artefacts', () => {
  const cases: Array<[string, string, string]> = [
    ['bios.bin', 'firmware-or-savestate', 'a BIOS dump'],
    ['BIOS.BIN', 'firmware-or-savestate', 'an upper-case BIOS dump'],
    ['scph10000.bin', 'firmware-or-savestate', 'a console-specific dump name'],
    ['saves/state.p2s', 'firmware-or-savestate', 'a PCSX2 savestate'],
    ['analysis/SCPH10000.idb', 'firmware-or-savestate', 'the PCSX2 symbol database'],
    ['release/game.iso', 'firmware-or-savestate', 'a disc image'],
    ['res/TEX0000.bin', 'extracted-resource-blob', 'an extracted texture'],
    ['res/FNTIMAGE.tmx', 'extracted-resource-blob', 'an extracted font page'],
    ['assets/ICO0042.img', 'extracted-resource-blob', 'an extracted icon page'],
    ['osdbits/res/TEX0123.bin', 'extracted-resource-dir', 'an extracted texture in the res tree'],
    ['osdbits/res/romdir.bin', 'extracted-resource-dir', 'a dumped ROMDIR in the res tree'],
    ['osdbits/res/notes.txt', 'extracted-resource-dir', 'anything inside the extracted tree'],
  ];

  for (const [file, rule, description] of cases) {
    it(`rejects ${description} (${file})`, () => {
      const result = evaluate([file]);
      expect(result.ok).toBe(false);
      expect(result.violations).toHaveLength(1);
      expect(result.violations[0]?.file).toBe(file);
      expect(result.violations[0]?.rule).toBe(rule);
    });
  }

  it('reports every offending file, not just the first', () => {
    const result = evaluate(['vucode_1.vu', 'bios.bin', 'osdbits/main.c', 'state.p2s']);
    expect(result.ok).toBe(false);
    expect(result.violations.map((v) => v.file)).toEqual(['bios.bin', 'state.p2s']);
  });

  it('does not mistake similarly named source files for firmware', () => {
    // `bin` as a directory name, `.vsm`/`.vu` sources, and a `res` prefix that is
    // part of a different directory name must all pass.
    const result = evaluate([
      'bin/README.md',
      'src/resource.ts',
      'crates/osdbits-core/src/resource.rs',
      'docs/testing-and-deployment.md',
      'osdbits/res.rb',
    ]);
    expect(result.ok).toBe(true);
  });
});
