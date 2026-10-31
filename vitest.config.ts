import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    include: [
      'tests/unit/**/*.test.ts',
      'tests/wasm/**/*.test.ts',
      'tests/perf/**/*.test.ts',
    ],
    testTimeout: 30_000,
    hookTimeout: 30_000,
  },
});
