/**
 * Phase 10 (`P10-003`) — Endurance, Percentile Timing (`p50`/`p95`/`p99`) & Memory/VRAM Leak Suite.
 *
 * Executes:
 * - 600 simulation + WebGPU frames across Modes A, B, C, and D (System Configuration)
 * - 30 rapid mode switches
 * - 20 internal render-target & aspect-ratio resizes
 *
 * Verifies:
 * - Zero unbounded WASM linear-memory growth (`postBytes === warmBytes`)
 * - Bounded GPU `activeTextures` (`23` static + `7` active offscreen targets = `30`, zero VRAM leaks)
 * - Zero WebGPU validation errors
 * - Percentile simulation budgets (`p50 < 4ms`, `p95 < 8ms`, `p99 < 16ms`) + reported SwiftShader CPU-GPU timings
 */

import { expect, test } from "@playwright/test";

function pct(sorted: readonly number[], q: number): number {
  if (sorted.length === 0) return 0;
  const idx = Math.min(sorted.length - 1, Math.max(0, Math.floor((sorted.length - 1) * q)));
  return sorted[idx]!;
}

test("Phase 10 endurance, percentile latency & zero-leak benchmark (600 frames + 30 mode switches + 20 resizes)", async ({
  page,
}) => {
  test.setTimeout(180_000);
  await page.goto("/");
  await page.waitForFunction(
    () => document.getElementById("app")?.dataset.appState === "ready",
    null,
    { timeout: 15_000 },
  );

  const metrics = await page.evaluate(async () => {
    const h = window.__osdbitsHarness!;
    const renderer = await h.createRenderer(0.5);

    // Warm up all 4 modes + one full reconfigure cycle so dlmalloc high-water mark settles
    const modes = ["opening", "idle", "illegal-disc", "menu"] as const;
    for (let pass = 0; pass < 2; pass++) {
      for (const m of modes) {
        h.bridge.initScene({ mode: m, rngSeed: 11 + pass, towerPreset: { kind: "full" } });
        if (m === "menu") h.bridge.enterSystemConfig();
        let s = h.bridge.stepFrame();
        for (let w = 0; w < 150; w++) {
          s = h.bridge.stepFrame();
        }
        renderer.renderFrame(h.bridge.getVertexView(), h.bridge.getCommandView(), s.mode);
      }
    }

    const warmWasmBytes = h.bridge.memoryByteLength;

    const wasmTimesMs: number[] = [];
    const gpuSubmitTimesMs: number[] = [];

    // 1. 600-frame multi-mode endurance (150 frames x 4 modes)
    for (const m of modes) {
      h.bridge.initScene({
        mode: m,
        rngSeed: 42,
        autoTransitionToMenu: false,
        towerPreset: { kind: "full" },
      });
      if (m === "menu") {
        h.bridge.enterSystemConfig();
      }
      for (let f = 0; f < 150; f++) {
        const t0 = performance.now();
        const snap = h.bridge.stepFrame();
        const t1 = performance.now();
        renderer.renderFrame(h.bridge.getVertexView(), h.bridge.getCommandView(), snap.mode);
        const t2 = performance.now();
        wasmTimesMs.push(t1 - t0);
        gpuSubmitTimesMs.push(t2 - t1);
      }
    }

    // 2. 30 rapid mode switches
    for (let i = 0; i < 30; i++) {
      const m = modes[i % modes.length]!;
      h.bridge.initScene({ mode: m, rngSeed: i + 1 });
      if (m === "menu" && (i & 1) === 0) {
        h.bridge.enterSystemConfig();
      }
      const snap = h.bridge.stepFrame();
      renderer.renderFrame(h.bridge.getVertexView(), h.bridge.getCommandView(), snap.mode);
    }

    // 3. 20 resolution-scale & aspect-mode reallocations -> verify old GPUTextures are destroyed!
    const scales = [0.5, 1.0, 1.5, 0.75] as const;
    const aspects = ["4:3", "16:9", "fit"] as const;
    for (let r = 0; r < 20; r++) {
      renderer.setDisplayConfig({
        resolutionScale: scales[r % scales.length]!,
        aspectMode: aspects[r % aspects.length]!,
        pal: (r & 1) === 1,
      });
      const snap = h.bridge.stepFrame();
      renderer.renderFrame(h.bridge.getVertexView(), h.bridge.getCommandView(), snap.mode);
    }

    // Final validation-checked frame + GPU queue flush via pixel readback
    const finalSnap = h.bridge.stepFrame();
    const validationErr = await renderer.renderFrameChecked(
      h.bridge.getVertexView(),
      h.bridge.getCommandView(),
      finalSnap.mode,
    );
    const rb = await renderer.captureScenePixels();
    const gpuStats = renderer.getResourceStats();
    const postWasmBytes = h.bridge.memoryByteLength;

    renderer.dispose();
    const afterDisposeStats = renderer.getResourceStats();

    return {
      warmWasmBytes,
      postWasmBytes,
      wasmTimesMs,
      gpuSubmitTimesMs,
      validationErr,
      readbackSize: rb.rgba.length,
      gpuStats,
      afterDisposeStats,
    };
  });

  const sortedWasm = [...metrics.wasmTimesMs].sort((a, b) => a - b);
  const sortedGpu = [...metrics.gpuSubmitTimesMs].sort((a, b) => a - b);

  const summary = {
    wasmMemoryWarmKiB: metrics.warmWasmBytes / 1024,
    wasmMemoryPostKiB: metrics.postWasmBytes / 1024,
    wasmStepMs: {
      p50: Number(pct(sortedWasm, 0.5).toFixed(3)),
      p95: Number(pct(sortedWasm, 0.95).toFixed(3)),
      p99: Number(pct(sortedWasm, 0.99).toFixed(3)),
    },
    swiftShaderSubmitMs: {
      p50: Number(pct(sortedGpu, 0.5).toFixed(3)),
      p95: Number(pct(sortedGpu, 0.95).toFixed(3)),
      p99: Number(pct(sortedGpu, 0.99).toFixed(3)),
    },
    gpuStatsBeforeDispose: metrics.gpuStats,
    gpuStatsAfterDispose: metrics.afterDisposeStats,
  };
  console.log("[perf-endurance-report]", JSON.stringify(summary));

  // 1. Zero unbounded WASM linear-memory growth across 650+ frames & 30 mode re-inits
  expect(metrics.postWasmBytes).toBe(metrics.warmWasmBytes);

  // 2. Zero WebGPU validation errors & non-empty pixel readback
  expect(metrics.validationErr).toBeNull();
  expect(metrics.readbackSize).toBeGreaterThan(0);

  // 3. Zero VRAM render-target leaks across 20 full resize reallocations:
  //    Exactly 23 static textures + 1 procedural overlay font texture + 7 live render targets = 31 active textures
  //    prior to dispose, and 0 active textures after dispose()!
  expect(metrics.gpuStats.destroyedTextures).toBe(19 * 7);
  expect(metrics.gpuStats.createdTextures - metrics.gpuStats.destroyedTextures).toBe(31);
  expect(
    metrics.afterDisposeStats.createdTextures - metrics.afterDisposeStats.destroyedTextures,
  ).toBe(0);

  // 4. Deterministic WASM simulation percentile budgets
  expect(summary.wasmStepMs.p50).toBeLessThan(4.0);
  expect(summary.wasmStepMs.p95).toBeLessThan(8.0);
  expect(summary.wasmStepMs.p99).toBeLessThan(16.0);
});
