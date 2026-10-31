import fs from 'node:fs';
import { defineConfig } from '@playwright/test';

const sandboxChromium = '/opt/toolchain/chromium/chromium';
const envChromium = process.env['PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH'];
const resolvedExecutablePath =
  envChromium && fs.existsSync(envChromium)
    ? envChromium
    : fs.existsSync(sandboxChromium)
      ? sandboxChromium
      : undefined;

const isSparticuzBinary = resolvedExecutablePath === sandboxChromium;

const launchArgs = [
  '--no-sandbox',
  '--disable-dev-shm-usage',
  '--ignore-gpu-blocklist',
  '--enable-unsafe-webgpu',
  ...(process.env['REQUIRE_HARDWARE_GPU'] === '1'
    ? []
    : [
        '--use-gl=angle',
        '--use-angle=swiftshader',
        '--enable-unsafe-swiftshader',
        '--use-webgpu-adapter=swiftshader',
      ]),
  ...(isSparticuzBinary ? ['--no-zygote', '--in-process-gpu'] : []),
];

export default defineConfig({
  testDir: './tests/e2e',
  testIgnore:
    process.env['REQUIRE_HARDWARE_GPU'] === '1' ? [] : ['**/webgpu-hardware.spec.ts'],
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 45_000,
  reporter: [['list']],
  use: {
    baseURL: 'http://127.0.0.1:4173',
    headless: true,
    viewport: { width: 960, height: 720 },
    launchOptions: {
      ...(resolvedExecutablePath ? { executablePath: resolvedExecutablePath } : {}),
      args: launchArgs,
    },
  },
  webServer: {
    command: 'npm run build && npx vite preview --host 127.0.0.1 --port 4173 --strictPort',
    url: 'http://127.0.0.1:4173',
    reuseExistingServer: !process.env['CI'],
    timeout: 90_000,
  },
});
