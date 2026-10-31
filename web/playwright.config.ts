import { defineConfig, devices } from "@playwright/test";

// Browser E2E. The specs run in CI (Chromium, Firefox, WebKit) and are
// capability-gated: without a WebGPU adapter they assert the documented Canvas
// 2D fallback instead of skipping, so the failure paths stay covered. The
// development sandbox has no browser binaries, so they were never executed
// locally — docs/TEST-EVIDENCE.md records exactly where each one ran.
export default defineConfig({
  testDir: "./playwright",
  timeout: 60_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  retries: 0,
  reporter: [["list"], ["json", { outputFile: "test-results/report.json" }]],
  use: {
    baseURL: "http://127.0.0.1:5173",
    trace: "retain-on-failure",
    video: "off",
  },
  webServer: {
    command: "npm run dev",
    url: "http://127.0.0.1:5173",
    reuseExistingServer: true,
    timeout: 120_000,
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "firefox", use: { ...devices["Desktop Firefox"] } },
    { name: "webkit", use: { ...devices["Desktop Safari"] } },
  ],
});
