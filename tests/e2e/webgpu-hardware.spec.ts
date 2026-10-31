import { expect, test } from '@playwright/test';

/**
 * Hardware GPU verification gate (`P11-002`).
 * Ignored by default in CPU/SwiftShader environments; enabled via `REQUIRE_HARDWARE_GPU=1`.
 */
test('verifies physical non-fallback WebGPU silicon adapter and < 8 ms render budget', async ({
  page,
}) => {
  await page.goto('/');
  await page.locator('#start-osdsys-btn').click();
  await expect(page.locator('#startup-gate')).toBeHidden();

  await page.waitForFunction(
    () => (window.__OSDBITS_APP__?.getDiagnostics().perf.sampleCount ?? 0) >= 60,
  );

  const diag = await page.evaluate(() => window.__OSDBITS_APP__!.getDiagnostics());
  expect(diag.renderer?.backend).toBe('webgpu');
  expect(diag.renderer?.isFallbackAdapter).toBe(false);
  expect(diag.renderer?.wgslCompilationClean).toBe(true);
  expect(diag.perf.p95Ms).toBeLessThan(8.0);
});
