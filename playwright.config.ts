import { defineConfig } from "@playwright/test";
import { preparePlaywrightChromiumOptions } from "./scripts/chromium-env.mjs";

const { launchOptions } = await preparePlaywrightChromiumOptions();
const isProdSuite =
  process.env.OSDBITS_PROD_PREVIEW === "1" ||
  process.argv.some((arg) => arg.includes("tests/prod"));
const port = isProdSuite ? 4173 : 5173;

export default defineConfig({
  testDir: "./tests",
  testIgnore: ["**/unit/**"],
  timeout: 60_000,
  expect: {
    timeout: 15_000,
  },
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [["list"]],
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    viewport: { width: 960, height: 720 },
    launchOptions,
  },
  webServer: isProdSuite
    ? {
        command: "node scripts/serve-production-test.mjs",
        url: "http://127.0.0.1:4173/osdbits-wasm/",
        reuseExistingServer: false,
        timeout: 30_000,
      }
    : {
        command: "npx vite --host 127.0.0.1 --port 5173 --strictPort",
        url: "http://127.0.0.1:5173",
        reuseExistingServer: !process.env.CI,
        timeout: 30_000,
      },
});
