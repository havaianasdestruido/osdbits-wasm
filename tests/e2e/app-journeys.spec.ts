import { expect, type Page, test } from '@playwright/test';

import type { AppDiagnosticsSnapshot } from '../../src/app/controller';

function attachZeroErrorGuard(page: Page): { assertClean: () => void } {
  const errors: string[] = [];
  page.on('pageerror', (err) => {
    errors.push(`pageerror: ${err.message}`);
  });
  page.on('console', (msg) => {
    if (msg.type() === 'error') {
      errors.push(`console.error: ${msg.text()}`);
    }
  });
  return {
    assertClean: () => {
      expect(errors).toEqual([]);
    },
  };
}

async function readAppDiagnostics(page: Page): Promise<AppDiagnosticsSnapshot> {
  return page.evaluate(() => {
    const app = window.__OSDBITS_APP__;
    if (!app) throw new Error('window.__OSDBITS_APP__ missing');
    return app.getDiagnostics();
  });
}

test.describe('End-to-end OSDSYS browser journeys, WebGPU, fallback, and accessibility', () => {
  test.afterEach(async ({ page }) => {
    await page
      .evaluate(() => window.__OSDBITS_APP__?.dispose())
      .catch(() => undefined);
    await page.waitForTimeout(80);
  });

  test('launches in one click, compiles WGSL cleanly on Dawn WebGPU, unlocks audio, and navigates Modes A–E', async ({
    page,
  }) => {
    const guard = attachZeroErrorGuard(page);
    await page.goto('/');

    await expect(page.locator('#startup-gate')).toBeVisible();
    await expect(page.locator('#gate-badge-gpu')).toContainText('WebGPU');
    await expect(page.locator('#gate-badge-assets')).toContainText('Clean-Room');

    // Single user gesture click on Start OSDSYS
    await page.locator('#start-osdsys-btn').click();
    await expect(page.locator('#startup-gate')).toBeHidden();
    await expect(page.locator('#top-bar')).toBeVisible();
    await expect(page.locator('#hud-drawer')).toBeVisible();

    const diag = await readAppDiagnostics(page);
    expect(diag.started).toBe(true);
    expect(diag.audio.unlockedByGesture).toBe(true);
    expect(diag.renderer?.backend).toBe('webgpu');
    expect(diag.renderer?.wgslCompilationClean).toBe(true);
    // Honest adapter reporting: headless sandbox uses SwiftShader (`isFallbackAdapter: true`)
    if (diag.renderer?.isFallbackAdapter) {
      await expect(page.locator('#badge-renderer')).toContainText('SwiftShader');
    } else {
      await expect(page.locator('#badge-renderer')).toContainText('Hardware');
    }

    // Verify Opening (Mode A) advances frames and renders non-blank WebGPU pixels
    await page.waitForFunction(
      () => (window.__OSDBITS_APP__?.getDiagnostics().telemetry?.frameCount ?? 0) >= 8,
    );
    const openingPixels = await page.evaluate(async () => {
      const p = await window.__OSDBITS_APP__!.renderDeterministicFrame({
        mode: 0,
        seed: 2000,
        framesToAdvance: 30,
      });
      let nonBlack = 0;
      for (let i = 0; i < p.rgba.length; i += 4) {
        if ((p.rgba[i] ?? 0) + (p.rgba[i + 1] ?? 0) + (p.rgba[i + 2] ?? 0) > 12) {
          nonBlack++;
        }
      }
      return { width: p.width, height: p.height, nonBlack };
    });
    expect(openingPixels.nonBlack).toBeGreaterThan(200);

    // Resume live simulation and switch to Mode D (OSDSYS Menu)
    await page.locator('#pause-btn').click();
    await page.locator('.mode-btn[data-mode="3"]').click();
    await expect(page.locator('#badge-active-mode')).toContainText('OSDSYS Menu');
    await expect(page.locator('#badge-fidelity')).toContainText('Verified Reconstruction');

    // Navigate MainMenu -> BrowserShowcase (row 0 -> Enter) to verify Approximate badge
    await page.keyboard.press('Enter');
    await expect(page.locator('#badge-fidelity')).toContainText('Approximate');

    // Escape back to MainMenu -> ArrowDown -> Enter into System Configuration -> Enter into Clock Adjust
    await page.keyboard.press('Escape');
    await expect(page.locator('#badge-fidelity')).toContainText('Verified Reconstruction');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    await page.waitForFunction(
      () => window.__OSDBITS_APP__?.getDiagnostics().telemetry?.menuPage === 1,
    );
    await page.keyboard.press('Enter');
    await page.waitForFunction(
      () => window.__OSDBITS_APP__?.getDiagnostics().telemetry?.menuPage === 2,
    );

    // Mode B (Idle Towers) and Mode C (Illegal Disc)
    await page.locator('.mode-btn[data-mode="1"]').click();
    await expect(page.locator('#badge-active-mode')).toContainText('Idle Towers');
    await page.locator('.mode-btn[data-mode="2"]').click();
    await expect(page.locator('#badge-active-mode')).toContainText('Illegal Disc');

    // Mode E customization + Reset Faithful Defaults
    await page.locator('#language-select').selectOption('6');
    await page.locator('#aspect-select').selectOption('2');
    let customDiag = await readAppDiagnostics(page);
    expect(customDiag.settings.tuning.language).toBe(6);
    expect(customDiag.settings.tuning.screenSize).toBe(2);

    await page.locator('#reset-defaults-btn').click();
    customDiag = await readAppDiagnostics(page);
    expect(customDiag.settings.tuning.language).toBe(0);
    expect(customDiag.settings.tuning.screenSize).toBe(0);

    guard.assertClean();
  });

  test('recovers seamlessly from simulated WebGPU device loss and supports forced Canvas 2D mode', async ({
    page,
  }) => {
    const guard = attachZeroErrorGuard(page);
    await page.goto('/');
    await page.locator('#start-osdsys-btn').click();
    await expect(page.locator('#startup-gate')).toBeHidden();

    // Simulate GPU device loss -> verify seamless Canvas 2D recovery
    await page.evaluate(async () => {
      await window.__OSDBITS_APP__!.simulateDeviceLoss();
    });
    const afterLoss = await readAppDiagnostics(page);
    expect(afterLoss.renderer?.backend).toBe('canvas2d');
    await expect(page.locator('#badge-renderer')).toContainText('Canvas 2D');
    await expect(page.locator('#storage-status-banner')).toContainText('Recovered');

    // Verify Canvas 2D renders non-blank pixels after recovery
    const fallbackPixels = await page.evaluate(async () => {
      const p = await window.__OSDBITS_APP__!.renderDeterministicFrame({
        mode: 0,
        seed: 2026,
        framesToAdvance: 25,
      });
      let sum = 0;
      for (let i = 0; i < p.rgba.length; i += 4) {
        sum += (p.rgba[i] ?? 0) + (p.rgba[i + 1] ?? 0) + (p.rgba[i + 2] ?? 0);
      }
      return sum;
    });
    expect(fallbackPixels).toBeGreaterThan(5_000);

    // Direct URL forced Canvas 2D launch (`/?renderer=canvas2d`)
    await page.evaluate(() => window.__OSDBITS_APP__?.dispose());
    await page.waitForTimeout(80);
    await page.goto('/?renderer=canvas2d');
    await page.locator('#start-osdsys-btn').click();
    await expect(page.locator('#startup-gate')).toBeHidden();
    const forcedDiag = await readAppDiagnostics(page);
    expect(forcedDiag.renderer?.backend).toBe('canvas2d');

    guard.assertClean();
  });

  test('honors prefers-reduced-motion, keyboard-only launch/HUD toggle, corrupt import handling, and mobile/desktop viewports', async ({
    page,
  }) => {
    const guard = attachZeroErrorGuard(page);
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.setViewportSize({ width: 375, height: 667 });
    await page.goto('/');

    // Fresh profile with prefers-reduced-motion: reduce auto-enables reducedMotion
    await expect(page.locator('#chk-reduced-motion')).toBeChecked();

    // Launch via Enter key on focused Start button
    await page.keyboard.press('Enter');
    await expect(page.locator('#startup-gate')).toBeHidden();

    // Toggle HUD drawer via 'H' shortcut
    await expect(page.locator('#hud-drawer')).toBeVisible();
    await page.keyboard.press('h');
    await expect(page.locator('#hud-drawer')).toBeHidden();
    await expect(page.locator('#hud-toggle-btn')).toHaveAttribute(
      'aria-expanded',
      'false',
    );
    await page.keyboard.press('H');
    await expect(page.locator('#hud-drawer')).toBeVisible();

    // Upload a corrupt file -> calm error banner, zero uncaught errors
    await page.locator('#import-asset-input').setInputFiles({
      name: 'corrupt.bin',
      mimeType: 'application/octet-stream',
      buffer: Buffer.from([0xde, 0xad, 0xbe, 0xef, 0x00, 0x11]),
    });
    await expect(page.locator('#asset-status-banner')).toContainText('Import rejected');

    // Check zero horizontal document overflow across mobile (375x667) and desktop (1280x720)
    for (const vp of [
      { width: 375, height: 667 },
      { width: 1280, height: 720 },
    ]) {
      await page.setViewportSize(vp);
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth > window.innerWidth,
      );
      expect(overflow).toBe(false);
    }

    guard.assertClean();
  });
});
