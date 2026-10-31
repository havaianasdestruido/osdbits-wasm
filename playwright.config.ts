import { defineConfig } from '@playwright/test';

/**
 * Which Chromium to drive.
 *
 * CI installs Playwright's own build (`npx playwright install chromium`) and expects
 * Playwright to find it by itself. This development sandbox, however, only has a
 * locally unpacked Chromium, which is selected by exporting `OSDBITS_CHROMIUM`.
 * Hardcoding that sandbox path here made every CI test fail with "Executable doesn't
 * exist at /usr/local/bin/chromium", so the override is opt-in.
 */
const chromiumPath = process.env['OSDBITS_CHROMIUM'];

/**
 * The standard headless recipe for a machine without a GPU: Dawn is forced onto the
 * SwiftShader Vulkan adapter. This is what makes `navigator.gpu` produce a device in
 * CI at all; the tests are written to tolerate that device being torn down mid-run
 * (see `tests/e2e/app.spec.ts`).
 */
const GPU_ARGS = [
  '--no-sandbox',
  '--disable-dev-shm-usage',
  '--enable-unsafe-webgpu',
  '--enable-features=Vulkan',
  '--use-vulkan=swiftshader',
  '--use-webgpu-adapter=swiftshader',
  '--autoplay-policy=no-user-gesture-required',
];

export default defineConfig({
  testDir: './tests/e2e',
  timeout: 60000,
  expect: {
    timeout: 15000,
  },
  fullyParallel: false,
  retries: 0,
  workers: 1,
  reporter: 'list',
  use: {
    baseURL: 'http://localhost:5173',
    trace: 'retain-on-failure',
    viewport: { width: 1280, height: 800 },
    launchOptions: {
      // `exactOptionalPropertyTypes` forbids passing `undefined`, so spread instead.
      ...(chromiumPath !== undefined && chromiumPath !== '' ? { executablePath: chromiumPath } : {}),
      args: GPU_ARGS,
    },
  },
  webServer: {
    command: 'npx vite --host 0.0.0.0 --port 5173',
    url: 'http://localhost:5173',
    reuseExistingServer: true,
    timeout: 30000,
  },
});
