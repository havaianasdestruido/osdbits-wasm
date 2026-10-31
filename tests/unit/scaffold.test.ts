import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { APP_VERSION } from '../../src/main';

const rootDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

describe('Phase 1 scaffold and repository hygiene', () => {
  it('exports strict-typed APP_VERSION', () => {
    expect(APP_VERSION).toBe('0.1.0');
  });

  it('passes BIOS-free repository hygiene scanner', () => {
    const out = execFileSync('node', ['scripts/check-repo-hygiene.mjs'], {
      cwd: rootDir,
      encoding: 'utf8',
    });
    expect(out).toContain('[check:hygiene] OK');
  });
});
