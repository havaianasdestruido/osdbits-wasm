import fs from 'node:fs';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { chromium, expect, test } from '@playwright/test';

const sandboxChromium = '/opt/toolchain/chromium/chromium';
const resolvedChromium = fs.existsSync(sandboxChromium) ? sandboxChromium : undefined;
const persistentArgs = [
  '--no-sandbox',
  '--disable-dev-shm-usage',
  '--ignore-gpu-blocklist',
  '--enable-unsafe-webgpu',
  '--use-gl=angle',
  '--use-angle=swiftshader',
  '--enable-unsafe-swiftshader',
  '--use-webgpu-adapter=swiftshader',
  ...(resolvedChromium ? ['--no-zygote', '--in-process-gpu'] : []),
];

test.describe('Real-browser IndexedDB/OPFS persistence across reload AND full browser restart (P06-002)', () => {
  test('survives in-context page.reload() with read-back verified settings and imported asset hash', async ({
    page,
  }) => {
    await page.goto('/');
    await page.locator('#start-osdsys-btn').click();
    await expect(page.locator('#startup-gate')).toBeHidden();

    // Customize mode (Illegal = 2), seed (919191), language (Français = 1), mute = true
    await page.locator('.mode-btn[data-mode="2"]').click();
    await page.locator('#seed-input').fill('919191');
    await page.locator('#apply-seed-btn').click();
    await page.locator('#language-select').selectOption('1');
    await page.locator('#chk-mute').check();

    // Import clean-room synthetic TEXIMAGE and verify read-back
    const imp = await page.evaluate(async () => {
      return window.__OSDBITS_APP__!.importSyntheticTeximage(0x5150);
    });
    expect(imp.ok).toBe(true);
    expect(imp.sha256Hex).toMatch(/^[0-9a-f]{64}$/);
    await expect(page.locator('#storage-status-banner')).toContainText('Read-back verified');

    // Cleanly stop RAF before reloading in single-process Chromium
    await page.evaluate(() => window.__OSDBITS_APP__?.dispose());
    await page.waitForTimeout(80);

    // Reload page within same browser context
    await page.reload();
    await expect(page.locator('#gate-badge-storage')).toContainText('Saved r');
    await expect(page.locator('#gate-badge-assets')).toContainText('Saved Asset (3 tex)');

    await page.locator('#start-osdsys-btn').click();
    await expect(page.locator('#startup-gate')).toBeHidden();
    const afterReload = await page.evaluate(() => window.__OSDBITS_APP__!.getDiagnostics());
    expect(afterReload.settings.preferredMode).toBe(2);
    expect(afterReload.settings.seed).toBe(919191);
    expect(afterReload.settings.tuning.language).toBe(1);
    expect(afterReload.settings.audio.muted).toBe(true);
    expect(afterReload.usingImportedAssets).toBe(true);
    expect(afterReload.importedSha256Hex).toBe(imp.sha256Hex);

    await page.evaluate(() => window.__OSDBITS_APP__?.dispose());
    await page.waitForTimeout(80);
  });

  test('survives full browser termination and relaunch (launchPersistentContext) and wipes cleanly on confirmed Factory Reset', async ({
    baseURL,
  }) => {
    const url = baseURL ?? 'http://127.0.0.1:4173';
    const userDataDir = await mkdtemp(join(tmpdir(), 'osdbits-persist-profile-'));

    try {
      // --- Session 1: Launch browser, customize & import asset, close entire browser ---
      const ctx1 = await chromium.launchPersistentContext(userDataDir, {
        ...(resolvedChromium ? { executablePath: resolvedChromium } : {}),
        headless: true,
        args: persistentArgs,
      });
      const page1 = ctx1.pages()[0] ?? (await ctx1.newPage());
      await page1.goto(url);
      await page1.locator('#start-osdsys-btn').click();
      await expect(page1.locator('#startup-gate')).toBeHidden();

      await page1.locator('.mode-btn[data-mode="1"]').click(); // Idle Towers
      await page1.locator('#seed-input').fill('424242');
      await page1.locator('#apply-seed-btn').click();
      await page1.locator('#language-select').selectOption('6'); // Português
      await expect(page1.locator('#storage-status-banner')).toContainText(
        'Read-back verified',
      );

      const imported1 = await page1.evaluate(async () => {
        return window.__OSDBITS_APP__!.importSyntheticTeximage(0xbeef42);
      });
      expect(imported1.ok).toBe(true);
      await ctx1.close();

      // --- Session 2: Relaunch brand-new browser process against same disk profile ---
      const ctx2 = await chromium.launchPersistentContext(userDataDir, {
        ...(resolvedChromium ? { executablePath: resolvedChromium } : {}),
        headless: true,
        args: persistentArgs,
      });
      const page2 = ctx2.pages()[0] ?? (await ctx2.newPage());
      await page2.goto(url);

      // Even on startup gate, disk-backed IndexedDB state is already detected!
      await expect(page2.locator('#gate-badge-storage')).toContainText('Saved r');
      await expect(page2.locator('#gate-badge-assets')).toContainText('Saved Asset (3 tex)');

      await page2.locator('#start-osdsys-btn').click();
      await expect(page2.locator('#startup-gate')).toBeHidden();
      const diag2 = await page2.evaluate(() => window.__OSDBITS_APP__!.getDiagnostics());
      expect(diag2.settings.preferredMode).toBe(1);
      expect(diag2.settings.seed).toBe(424242);
      expect(diag2.settings.tuning.language).toBe(6);
      expect(diag2.usingImportedAssets).toBe(true);
      expect(diag2.importedSha256Hex).toBe(imported1.sha256Hex);

      // Perform 2-step Confirmed Factory Reset
      await page2.locator('#factory-reset-btn').click();
      await expect(page2.locator('#confirm-factory-reset-btn')).toBeVisible();
      await page2.locator('#confirm-factory-reset-btn').click();
      await expect(page2.locator('#storage-status-banner')).toContainText(
        'Factory reset completed',
      );
      await ctx2.close();

      // --- Session 3: Relaunch to verify factory reset persisted to disk ---
      const ctx3 = await chromium.launchPersistentContext(userDataDir, {
        ...(resolvedChromium ? { executablePath: resolvedChromium } : {}),
        headless: true,
        args: persistentArgs,
      });
      const page3 = ctx3.pages()[0] ?? (await ctx3.newPage());
      await page3.goto(url);
      await expect(page3.locator('#gate-badge-assets')).toContainText(
        'Procedural Clean-Room Assets',
      );
      await page3.locator('#start-osdsys-btn').click();
      await expect(page3.locator('#startup-gate')).toBeHidden();
      const diag3 = await page3.evaluate(() => window.__OSDBITS_APP__!.getDiagnostics());
      expect(diag3.settings.preferredMode).toBe(0);
      expect(diag3.settings.seed).toBe(0x2000_0304);
      expect(diag3.usingImportedAssets).toBe(false);
      await ctx3.close();
    } finally {
      await rm(userDataDir, { recursive: true, force: true });
    }
  });
});
