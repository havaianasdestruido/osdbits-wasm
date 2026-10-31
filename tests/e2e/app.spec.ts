/**
 * End-to-end tests in a real Chromium with a real WebGPU device.
 *
 * Capability-aware by design: this sandbox's SwiftShader WebGPU device is destroyed by the
 * browser after a handful of frames (verified independently of this app with a minimal
 * clear-only loop), so the assertions require that rendering *starts* and that the device-loss
 * recovery path behaves, rather than requiring unlimited sustained GPU execution. The
 * deterministic simulation, command generation and asset decoding are covered frame-by-frame in
 * `tests/unit/wasm.test.ts`, which runs the real compiled module.
 */

import { expect, test, type Page } from '@playwright/test';

interface EngineState {
  mode: number;
  frame: number;
  vertices: number;
  commands: number;
  active_towers: number;
  screen: string;
  language: number;
  speed?: number;
  asset_imported: boolean;
}

async function state(page: Page): Promise<EngineState | null> {
  return page.evaluate(() => window.__osdbits?.state() as EngineState | null);
}

async function start(page: Page): Promise<void> {
  await page.goto('/');
  await expect(page.locator('#start-btn')).toBeVisible();
  await page.locator('#start-btn').click();
  await page.waitForFunction(() => window.__osdbits?.running() === true, undefined, {
    timeout: 30000,
  });
}

test.beforeEach(async ({ page }) => {
  await page.context().clearCookies();
});

test('activation screen states its capabilities honestly', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('#overlay')).toBeVisible();
  await expect(page.locator('#start-btn')).toHaveText(/Start OSDSYS/);
  const wasm = page.locator('[data-cap="wasm"]');
  await expect(wasm).toHaveAttribute('data-state', 'ok');
  const gpu = page.locator('[data-cap="webgpu"]');
  await expect(gpu).toHaveAttribute('data-state', /ok|fail/);
  // The GPU badge must be resolved before the click, not left in the default state.
  expect(await gpu.getAttribute('title')).toBeTruthy();
});

test('one click starts the engine and renders content', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(String(error)));

  await start(page);
  await expect(page.locator('#overlay')).toBeHidden();

  // Evidence that the GPU path really drew. This sandbox's SwiftShader device is torn down by
  // the browser after a handful of frames (reproducible with a bare clear-only loop, so it is
  // environmental), which makes a pixel readback unreliable; the draw counts come from the
  // frame that was actually submitted to the device.
  const renderInfo = await page.evaluate(() => window.__osdbits?.renderInfo() ?? null);
  expect(renderInfo).not.toBeNull();
  expect(renderInfo!.draws).toBeGreaterThan(0);
  expect(renderInfo!.passes).toBeGreaterThan(0);
  expect(renderInfo!.vertices).toBeGreaterThan(0);
  const signature = await page.evaluate(() => window.__osdbits?.signature() ?? null);

  // The simulation advanced and produced geometry.
  await page.waitForFunction(
    () => (window.__osdbits?.state() as { frame: number } | null)?.frame ?? 0 >= 2,
    undefined,
    { timeout: 20000 },
  );
  const first = await state(page);
  expect(first).not.toBeNull();
  expect(first!.frame).toBeGreaterThanOrEqual(2);
  expect(first!.vertices).toBeGreaterThan(0);
  expect(first!.commands).toBeGreaterThan(0);
  expect(first!.active_towers).toBeGreaterThan(0);

  if (signature) {
    // Where the readback succeeds (a healthy GPU), the frame must contain real content.
    expect(signature.width).toBe(640);
    expect(signature.height).toBe(448);
    expect(signature.lum).toBeGreaterThan(0);
    expect(signature.lit / signature.total).toBeGreaterThan(0.02);
  }

  expect(errors).toEqual([]);
});

test('a lost GPU device degrades to simulation-only and never blocks the UI', async ({ page }) => {
  await start(page);
  // Wait until this environment's software renderer tears the device down.
  const lost = await page
    .waitForFunction(
      () => document.querySelector('#hud')?.textContent?.includes('gpu unavailable') ?? false,
      undefined,
      { timeout: 15000 },
    )
    .then(() => true)
    .catch(() => false);

  // Either the GPU survived (ideal) or it was lost (this sandbox) — both must leave the
  // experience running and interactive rather than frozen on the activation screen.
  expect(await page.evaluate(() => window.__osdbits?.running())).toBe(true);
  await expect(page.locator('#overlay')).toBeHidden();
  const before = await state(page);
  await page.evaluate(() => window.__osdbits?.step(3));
  const after = await state(page);
  expect(after!.frame).toBeGreaterThan(before!.frame);
  if (lost) {
    await expect(page.locator('#hud')).toContainText(/simulation running/);
  }
});

test('mode switching produces geometry for every scene', async ({ page }) => {
  await start(page);
  for (const mode of [2, 3, 1, 0]) {
    const result = await page.evaluate((m) => {
      window.__osdbits?.setMode(m);
      const s = window.__osdbits?.step(3);
      return s;
    }, mode);
    expect(result).not.toBeNull();
    expect(result!.commands).toBeGreaterThan(0);
    expect(result!.vertices).toBeGreaterThan(0);
    expect(result!.mode).toBe(mode);
  }
});

test('deterministic stepping drives the menu, the config screen and the browser', async ({ page }) => {
  await start(page);
  const flow = await page.evaluate(() => {
    const api = window.__osdbits!;
    api.setMode(3);
    api.step(40);
    const afterOpen = api.state()!;
    // Confirm "System Configuration" (cursor starts on Browser, so press Down first).
    api.step(1, api.pad.Down);
    api.step(10);
    api.step(1, api.pad.Cross); // World region: physical Cross == confirm
    api.step(10);
    const inConfig = api.state()!;
    api.step(1, api.pad.Circle); // cancel back to the main menu
    api.step(10);
    const back = api.state()!;
    // From the main menu, move the cursor back to the first item and open the browser.
    api.step(1, api.pad.Up);
    api.step(4);
    api.step(1, api.pad.Cross);
    api.step(6);
    const browser = api.state()!;
    return { afterOpen, inConfig, back, browser };
  });

  expect(flow.afterOpen.screen).toBe('main_menu');
  expect(flow.inConfig.screen).toBe('system_config');
  expect(flow.back.screen).toBe('main_menu');
  expect(flow.browser.screen).toBe('browser');
});

test('settings survive a page reload and are restored after one click', async ({ page }) => {
  await start(page);

  // Change the language and the animation speed through the panel's live bindings.
  await page.evaluate(() => {
    const api = window.__osdbits!;
    api.preset('moderate');
  });
  await page.keyboard.press('F2');
  const languageSelect = page.locator('#field-language');
  await expect(languageSelect).toBeVisible();
  await languageSelect.selectOption('4');
  const speed = page.locator('#field-speed');
  await speed.fill('1.5');

  // Give the debounced writer time, then verify it reported a verified save.
  await page.waitForFunction(
    () => document.querySelector('#hud')?.textContent?.includes('saved & verified') ?? false,
    undefined,
    { timeout: 15000 },
  );

  await page.reload();
  await expect(page.locator('#start-btn')).toBeVisible();
  await page.locator('#start-btn').click();
  await page.waitForFunction(() => window.__osdbits?.running() === true, undefined, {
    timeout: 30000,
  });
  const restored = await state(page);
  expect(restored).not.toBeNull();
  expect(restored!.language).toBe(4);
  // The boot history preset persisted too.
  expect(restored!.active_towers).toBeGreaterThan(0);
});

test('keyboard navigation and the pause shortcut work', async ({ page }) => {
  await start(page);
  await page.keyboard.press('F2');
  await expect(page.locator('#panel')).toBeVisible();
  await page.keyboard.press('F2');
  await expect(page.locator('#panel')).toBeHidden();

  // Pause first, then take the baseline: the render loop may step once between two evaluates.
  await page.evaluate(() => window.__osdbits?.pause(true));
  await page.waitForTimeout(120);
  const before = await state(page);
  await page.evaluate(() => window.__osdbits?.step(3));
  await page.waitForTimeout(120);
  const paused = await state(page);
  // Paused: the frame counter must not advance, even though the render loop keeps ticking.
  expect(paused!.frame).toBe(before!.frame);
  await page.evaluate(() => window.__osdbits?.pause(false));
  await page.evaluate(() => window.__osdbits?.step(3));
  const resumed = await state(page);
  expect(resumed!.frame).toBeGreaterThan(paused!.frame);
});

test('resizing the viewport keeps the canvas and simulation healthy', async ({ page }) => {
  await start(page);
  await page.setViewportSize({ width: 900, height: 600 });
  await page.waitForTimeout(300);
  await page.setViewportSize({ width: 1400, height: 900 });
  const result = await page.evaluate(() => window.__osdbits?.step(2) ?? null);
  expect(result).not.toBeNull();
  expect(result!.commands).toBeGreaterThan(0);
  const size = await page.evaluate(() => {
    const canvas = document.querySelector<HTMLCanvasElement>('#osd-canvas');
    return canvas ? { width: canvas.width, height: canvas.height } : null;
  });
  expect(size!.width).toBeGreaterThan(0);
  expect(size!.height).toBeGreaterThan(0);
});

test('importing a non-BIOS file is rejected with a clear message', async ({ page }) => {
  await start(page);
  await page.keyboard.press('F2');
  await page.locator('button', { hasText: 'Import BIOS' }).click();
  await expect(page.locator('#import-dialog')).toBeVisible();
  await page.setInputFiles('#import-file', {
    name: 'not-a-bios.bin',
    mimeType: 'application/octet-stream',
    buffer: Buffer.alloc(8192, 0x5a),
  });
  await page.locator('#import-confirm').click();
  await expect(page.locator('#import-status')).toContainText(/no osdsys resources/i);
});

test('importing a synthetic ROMDIR container replaces a texture and persists it', async ({ page }) => {
  await start(page);
  const merged = await page.evaluate(async () => {
    // Build a minimal, valid single-resource ROMDIR container in the page (LZSS literal blocks).
    const payload = new Uint8Array(4096).fill(0x42);
    const compressed: number[] = [];
    const size = payload.length;
    compressed.push(size & 0xff, (size >>> 8) & 0xff, (size >>> 16) & 0xff, (size >>> 24) & 0xff);
    let pos = 0;
    while (pos < payload.length) {
      compressed.push(0); // shift = 0
      const cmdPos = compressed.length;
      compressed.push(0, 0, 0, 0);
      let cmd = 0;
      for (let bit = 0; bit < 30 && pos < payload.length; bit += 1) {
        compressed.push(payload[pos]!);
        pos += 1;
      }
      const cmdBytes = new Uint8Array(new Uint32Array([cmd]).buffer);
      for (let i = 0; i < 4; i += 1) compressed[cmdPos + i] = cmdBytes[i]!;
      void cmd;
    }
    const files: Array<[string, Uint8Array]> = [['TEXCBUMP', new Uint8Array(compressed)]];
    // Retail container layout: RESET first with its size covering the table, then ROMDIR/EXTINFO
    // with zero sizes, then the resource entries; data packed after the table.
    const tableLen = (files.length + 4) * 16;
    const table = new Uint8Array(tableLen);
    let cursor = 0;
    const writeEntry = (name: string, fs: number): void => {
      const bytes = new TextEncoder().encode(name);
      table.set(bytes.subarray(0, Math.min(10, bytes.length)), cursor);
      table[cursor + 10] = 0;
      table[cursor + 11] = 0;
      table[cursor + 12] = fs & 0xff;
      table[cursor + 13] = (fs >>> 8) & 0xff;
      table[cursor + 14] = (fs >>> 16) & 0xff;
      table[cursor + 15] = (fs >>> 24) & 0xff;
      cursor += 16;
    };
    writeEntry('RESET', tableLen);
    writeEntry('ROMDIR', 0);
    writeEntry('EXTINFO', 0);
    for (const [name, data] of files) writeEntry(name, data.length);
    const total = new Uint8Array(tableLen + compressed.length);
    total.set(table, 0);
    total.set(new Uint8Array(compressed), tableLen);
    return window.__osdbits?.importRomdir(total) ?? -1;
  });
  expect(merged).toBe(1);
  const state = await page.evaluate(() => window.__osdbits?.state() ?? null);
  expect(state!.asset_imported).toBe(true);
});

test('reset restores defaults and clears stored data', async ({ page }) => {
  await start(page);
  await page.keyboard.press('F2');
  await page.locator('#field-language').selectOption('5');
  page.once('dialog', (dialog) => void dialog.accept());
  await page.locator('button', { hasText: 'Reset to defaults' }).click();
  await page.waitForTimeout(600);
  const state = await page.evaluate(() => window.__osdbits?.state() ?? null);
  expect(state).not.toBeNull();
  expect(state!.language).toBe(1);
});
