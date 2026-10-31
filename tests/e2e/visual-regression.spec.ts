import fs from 'node:fs';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { expect, test } from '@playwright/test';
import pixelmatch from 'pixelmatch';
import { PNG } from 'pngjs';

interface CapturedScene {
  name: string;
  width: number;
  height: number;
  rgba: Uint8Array;
}

function analyzeScenePixels(rgba: Uint8Array): {
  sumR: number;
  sumG: number;
  sumB: number;
  litRatio: number;
  lumaStdDev: number;
} {
  const n = Math.max(1, rgba.length / 4);
  let sumR = 0;
  let sumG = 0;
  let sumB = 0;
  let lit = 0;
  let sumLuma = 0;
  const lumas = new Float32Array(n);

  for (let i = 0, p = 0; i < rgba.length; i += 4, p++) {
    const r = rgba[i] ?? 0;
    const g = rgba[i + 1] ?? 0;
    const b = rgba[i + 2] ?? 0;
    sumR += r;
    sumG += g;
    sumB += b;
    const y = 0.299 * r + 0.587 * g + 0.114 * b;
    lumas[p] = y;
    sumLuma += y;
    if (r + g + b > 18) lit++;
  }

  const mean = sumLuma / n;
  let sq = 0;
  for (let p = 0; p < n; p++) {
    const d = (lumas[p] ?? 0) - mean;
    sq += d * d;
  }

  return {
    sumR,
    sumG,
    sumB,
    litRatio: lit / n,
    lumaStdDev: Math.sqrt(sq / n),
  };
}

function encodePng(width: number, height: number, rgba: Uint8Array): Buffer {
  const png = new PNG({ width, height });
  png.data.set(rgba);
  return PNG.sync.write(png);
}

const GOLDENS_DIR = resolve(process.cwd(), 'tests/e2e/goldens');

test.describe('Deterministic visual regression & golden snapshot suite (P08-001)', () => {
  test('renders non-blank, color-accurate, structurally distinct frames matching golden PNGs', async ({
    page,
  }) => {
    await mkdir(GOLDENS_DIR, { recursive: true });
    await page.setViewportSize({ width: 640, height: 480 });
    await page.goto('/');
    await page.locator('#start-osdsys-btn').click();
    await expect(page.locator('#startup-gate')).toBeHidden();
    await page.locator('#pause-btn').click();
    await page.waitForTimeout(80);

    // Use compact 320x240 internal target (resScale = 0.5 on 640x480 4:3)
    await page.locator('#res-scale-range').fill('0.5');
    await page.locator('#res-scale-range').dispatchEvent('input');

    const webgpuScenes = await page.evaluate(async () => {
      const app = window.__OSDBITS_APP__!;
      const iso = '2026-10-31T21:45:12.000Z';

      const s1 = await app.renderDeterministicFrame({
        mode: 0,
        seed: 2000,
        framesToAdvance: 24,
        rtcIso: iso,
      });
      const s2 = await app.renderDeterministicFrame({
        mode: 0,
        seed: 2000,
        framesToAdvance: 110,
        rtcIso: iso,
      });
      const s3 = await app.renderDeterministicFrame({
        mode: 1,
        seed: 2000,
        framesToAdvance: 45,
        rtcIso: iso,
      });
      const s4 = await app.renderDeterministicFrame({
        mode: 2,
        seed: 2000,
        framesToAdvance: 55,
        rtcIso: iso,
      });
      const s5 = await app.renderDeterministicFrame({
        mode: 3,
        seed: 2000,
        framesToAdvance: 40,
        rtcIso: iso,
      });

      // Enter System Configuration (MenuAction.Down = 1, MenuAction.Confirm = 4) and advance 95 frames
      await app.renderDeterministicFrame({
        mode: 3,
        seed: 2000,
        framesToAdvance: 2,
        rtcIso: iso,
      });
      app.triggerMenuAction(1);
      app.triggerMenuAction(4);
      for (let i = 0; i < 95; i++) {
        document.getElementById('step-frame-btn')!.click();
      }
      const s6SysCfg = await app.readCurrentPixels();

      return [
        { name: 'opening-early', width: s1.width, height: s1.height, rgba: Array.from(s1.rgba) },
        { name: 'opening-canopy', width: s2.width, height: s2.height, rgba: Array.from(s2.rgba) },
        { name: 'idle-towers', width: s3.width, height: s3.height, rgba: Array.from(s3.rgba) },
        { name: 'illegal-disc-red', width: s4.width, height: s4.height, rgba: Array.from(s4.rgba) },
        { name: 'menu-orbs-main', width: s5.width, height: s5.height, rgba: Array.from(s5.rgba) },
        {
          name: 'menu-sysconfig-rods-cubes',
          width: s6SysCfg.width,
          height: s6SysCfg.height,
          rgba: Array.from(s6SysCfg.rgba),
        },
      ];
    });

    // Capture 7th scene: Canvas 2D compatibility fallback
    await page.evaluate(() => window.__OSDBITS_APP__?.dispose());
    await page.waitForTimeout(80);
    await page.goto('/?renderer=canvas2d');
    await page.locator('#start-osdsys-btn').click();
    await expect(page.locator('#startup-gate')).toBeHidden();
    await page.locator('#pause-btn').click();
    await page.locator('#res-scale-range').fill('0.5');
    await page.locator('#res-scale-range').dispatchEvent('input');
    const fallbackScene = await page.evaluate(async () => {
      const f = await window.__OSDBITS_APP__!.renderDeterministicFrame({
        mode: 0,
        seed: 2000,
        framesToAdvance: 40,
        rtcIso: '2026-10-31T21:45:12.000Z',
      });
      return {
        name: 'canvas2d-fallback-opening',
        width: f.width,
        height: f.height,
        rgba: Array.from(f.rgba),
      };
    });

    const allScenes: CapturedScene[] = [
      ...webgpuScenes.map((s) => ({ ...s, rgba: new Uint8Array(s.rgba) })),
      { ...fallbackScene, rgba: new Uint8Array(fallbackScene.rgba) },
    ];

    for (const scene of allScenes) {
      const stats = analyzeScenePixels(scene.rgba);
      // Every scene must be non-blank with visible luminance structure
      expect(stats.litRatio, `${scene.name} litRatio`).toBeGreaterThan(0.02);
      expect(stats.lumaStdDev, `${scene.name} lumaStdDev`).toBeGreaterThan(3.0);

      if (scene.name === 'illegal-disc-red') {
        expect(stats.sumR).toBeGreaterThan(stats.sumB * 1.5);
      } else {
        expect(stats.sumB).toBeGreaterThan(stats.sumR * 1.15);
      }
    }

    // Verify pairwise distinctness between WebGPU scenes
    for (let i = 0; i < webgpuScenes.length; i++) {
      for (let j = i + 1; j < webgpuScenes.length; j++) {
        const a = allScenes[i]!;
        const b = allScenes[j]!;
        const diffCount = pixelmatch(a.rgba, b.rgba, null, a.width, a.height, {
          threshold: 0.1,
        });
        expect(
          diffCount,
          `${a.name} vs ${b.name} must be structurally distinct`,
        ).toBeGreaterThan(a.width * a.height * 0.015);
      }
    }

    // Compare or update tracked golden PNGs
    const updateGoldens = process.env['UPDATE_GOLDENS'] === '1';
    for (const scene of allScenes) {
      const goldenPath = resolve(GOLDENS_DIR, `${scene.name}.png`);
      if (updateGoldens) {
        await writeFile(goldenPath, encodePng(scene.width, scene.height, scene.rgba));
      }
      expect(
        fs.existsSync(goldenPath),
        `Missing golden snapshot ${goldenPath}`,
      ).toBe(true);

      const goldenBuf = await readFile(goldenPath);
      const goldenPng = PNG.sync.read(goldenBuf);
      expect(goldenPng.width).toBe(scene.width);
      expect(goldenPng.height).toBe(scene.height);

      const mismatched = pixelmatch(
        scene.rgba,
        new Uint8Array(
          goldenPng.data.buffer,
          goldenPng.data.byteOffset,
          goldenPng.data.byteLength,
        ),
        null,
        scene.width,
        scene.height,
        { threshold: 0.05 },
      );
      const mismatchRatio = mismatched / (scene.width * scene.height);
      expect(
        mismatchRatio,
        `Golden regression mismatch on ${scene.name}: ${mismatched} pixels`,
      ).toBeLessThan(0.0025);
    }
  });
});
