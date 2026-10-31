import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["web/src/**/*.test.ts", "tests/unit/**/*.test.ts"],
    environment: "happy-dom",
    globals: false,
    testTimeout: 30_000,
  },
});
