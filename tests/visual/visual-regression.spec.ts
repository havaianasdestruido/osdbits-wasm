/**
 * Phase 10 (`P10-002`) — Deterministic Visual Regression Golden Suite Across Modes A, B, C, D.
 *
 * Renders fixed-seed, fixed-clock (`2026-10-31 12:15:30`), fixed-resolution (`640x448` NTSC 1.0x)
 * WebGPU frames with full temporal feedback warmup (`extraBuf` radial glow + zoom/motion blur),
 * reads back GPU `sceneColor` RGBA8 pixels, and verifies canonical PNG baselines within
 * documented SwiftShader Vulkan tolerances:
 * - `maxMeanChannelDiff <= 1.5 / 255`
 * - `mismatchRatio(> 4 / 255) <= 0.2%` (`0.002`)
 * - `rmse <= 3.0 / 255`
 */

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { expect, test } from "@playwright/test";
import { PNG } from "pngjs";

const BASELINE_DIR = resolve(process.cwd(), "tests/visual/baselines");
const UPDATE_BASELINES = process.env.UPDATE_VISUAL_BASELINES === "1";

interface GoldenCaptureSpec {
  id: string;
  mode: "opening" | "idle" | "illegal-disc" | "menu";
  enterSystemConfig?: boolean;
  targetFrame: number;
  rngSeed: number;
  towerPreset: "captured" | "full";
}

const GOLDEN_SPECS: readonly GoldenCaptureSpec[] = [
  {
    id: "mode-a-opening-f024",
    mode: "opening",
    targetFrame: 24,
    rngSeed: 1,
    towerPreset: "full",
  },
  {
    id: "mode-a-opening-f090",
    mode: "opening",
    targetFrame: 90,
    rngSeed: 1,
    towerPreset: "full",
  },
  {
    id: "mode-a-opening-f215",
    mode: "opening",
    targetFrame: 215,
    rngSeed: 1,
    towerPreset: "full",
  },
  {
    id: "mode-b-idle-f180",
    mode: "idle",
    targetFrame: 180,
    rngSeed: 7,
    towerPreset: "full",
  },
  {
    id: "mode-c-illegal-f160",
    mode: "illegal-disc",
    targetFrame: 160,
    rngSeed: 3,
    towerPreset: "captured",
  },
  {
    id: "mode-d-menu-main-f120",
    mode: "menu",
    targetFrame: 120,
    rngSeed: 5,
    towerPreset: "captured",
  },
  {
    id: "mode-d-menu-sysconfig-f120",
    mode: "menu",
    enterSystemConfig: true,
    targetFrame: 120,
    rngSeed: 5,
    towerPreset: "captured",
  },
];

interface PixelDiffStats {
  meanAbsChannelDiff: number;
  mismatchRatioOver4: number;
  rmse: number;
  meanR: number;
  meanG: number;
  meanB: number;
}

function compareRgbaBuffers(
  actual: Uint8Array,
  expected: Uint8Array,
  width: number,
  height: number,
): PixelDiffStats {
  const pixelCount = width * height;
  const rgbCount = pixelCount * 3;
  let absSum = 0;
  let sqSum = 0;
  let mismatchPixels = 0;
  let sumR = 0;
  let sumG = 0;
  let sumB = 0;

  for (let i = 0; i < pixelCount; i++) {
    const o = i * 4;
    const ar = actual[o]!;
    const ag = actual[o + 1]!;
    const ab = actual[o + 2]!;
    const er = expected[o]!;
    const eg = expected[o + 1]!;
    const eb = expected[o + 2]!;

    sumR += ar;
    sumG += ag;
    sumB += ab;

    const dr = Math.abs(ar - er);
    const dg = Math.abs(ag - eg);
    const db = Math.abs(ab - eb);
    absSum += dr + dg + db;
    sqSum += dr * dr + dg * dg + db * db;
    if (dr > 4 || dg > 4 || db > 4) {
      mismatchPixels++;
    }
  }

  return {
    meanAbsChannelDiff: absSum / rgbCount,
    mismatchRatioOver4: mismatchPixels / pixelCount,
    rmse: Math.sqrt(sqSum / rgbCount),
    meanR: sumR / pixelCount,
    meanG: sumG / pixelCount,
    meanB: sumB / pixelCount,
  };
}

test("Phase 10 deterministic WebGPU visual regression suite across Modes A, B, C, and D", async ({
  page,
}) => {
  test.setTimeout(150_000);
  await page.goto("/");
  await page.waitForFunction(
    () => document.getElementById("app")?.dataset.appState === "ready",
    null,
    { timeout: 15_000 },
  );

  // Initialize isolated deterministic renderer at 1.0x NTSC (640x448)
  await page.evaluate(async () => {
    const h = window.__osdbitsHarness!;
    (window as unknown as { __visRenderer?: unknown }).__visRenderer = await h.createRenderer(1.0);
  });

  for (const spec of GOLDEN_SPECS) {
    const captured = await page.evaluate(async (s) => {
      const h = window.__osdbitsHarness!;
      const renderer = (
        window as unknown as {
          __visRenderer: Awaited<ReturnType<typeof h.createRenderer>>;
        }
      ).__visRenderer;

      // Recreate render targets cleanly so extraBuf temporal state starts from zero for each golden scene
      renderer.setDisplayConfig({ resolutionScale: 0.5 });
      renderer.setDisplayConfig({
        aspectMode: "4:3",
        resolutionScale: 1.0,
        pal: false,
      });

      h.bridge.initScene({
        mode: s.mode,
        rngSeed: s.rngSeed,
        pal: false,
        autoTransitionToMenu: false,
        towerPreset: s.towerPreset === "full" ? { kind: "full" } : { kind: "captured" },
        clock: {
          year: 2026,
          month: 10,
          day: 31,
          hour: 12,
          minute: 15,
          second: 30,
        },
      });

      if (s.enterSystemConfig) {
        h.bridge.enterSystemConfig();
      }

      // Warm temporal feedback buffers over the last 16 frames leading up to targetFrame
      const renderStart = Math.max(1, s.targetFrame - 15);
      for (let f = 1; f <= s.targetFrame; f++) {
        const snap = h.bridge.stepFrame();
        if (f >= renderStart) {
          renderer.renderFrame(h.bridge.getVertexView(), h.bridge.getCommandView(), snap.mode);
        }
      }

      const rb = await renderer.captureScenePixels();
      // Encode raw RGBA bytes to base64 for fast transfer out of browser context
      let binary = "";
      const chunk = 0x8000;
      for (let i = 0; i < rb.rgba.length; i += chunk) {
        binary += String.fromCharCode(...rb.rgba.subarray(i, i + chunk));
      }
      return {
        width: rb.width,
        height: rb.height,
        base64: btoa(binary),
      };
    }, spec);

    expect(captured.width).toBe(640);
    expect(captured.height).toBe(448);

    const actualRgba = new Uint8Array(Buffer.from(captured.base64, "base64"));
    // Force opaque alpha = 255 in saved PNG representation so PNG viewers don't premultiply GS scene alpha
    for (let p = 3; p < actualRgba.length; p += 4) {
      actualRgba[p] = 255;
    }

    const baselinePath = resolve(BASELINE_DIR, `${spec.id}.png`);
    if (UPDATE_BASELINES) {
      mkdirSync(dirname(baselinePath), { recursive: true });
      const png = new PNG({ width: captured.width, height: captured.height });
      Buffer.from(actualRgba).copy(png.data);
      writeFileSync(baselinePath, PNG.sync.write(png));
    }

    expect(
      existsSync(baselinePath),
      `Missing visual baseline PNG: ${baselinePath}`,
    ).toBe(true);

    const baselinePng = PNG.sync.read(readFileSync(baselinePath));
    expect(baselinePng.width).toBe(captured.width);
    expect(baselinePng.height).toBe(captured.height);

    const diff = compareRgbaBuffers(
      actualRgba,
      new Uint8Array(baselinePng.data),
      captured.width,
      captured.height,
    );

    // Structural sanity per mode
    expect(diff.meanR + diff.meanG + diff.meanB, `${spec.id} must be non-blank`).toBeGreaterThan(
      6.0,
    );
    if (spec.mode === "illegal-disc") {
      expect(diff.meanR).toBeGreaterThan(diff.meanG * 1.4);
      expect(diff.meanR).toBeGreaterThan(diff.meanB * 1.4);
    }

    // Strict SwiftShader pixel-level regression tolerances
    expect(diff.meanAbsChannelDiff, `${spec.id} meanAbsChannelDiff`).toBeLessThanOrEqual(1.5);
    expect(diff.mismatchRatioOver4, `${spec.id} mismatchRatio(>4)`).toBeLessThanOrEqual(0.002);
    expect(diff.rmse, `${spec.id} rmse`).toBeLessThanOrEqual(3.0);
  }

  await page.evaluate(() => {
    const r = (window as unknown as { __visRenderer?: { dispose: () => void } }).__visRenderer;
    r?.dispose();
  });
});
