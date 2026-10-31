import { readFile, stat } from 'node:fs/promises';
import { performance } from 'node:perf_hooks';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

import {
  AppMode,
  loadOsdbitsWasm,
  MenuAction,
  OsdbitsWasmEngine,
} from '../../src/wasm/bridge';

describe('Phase 8 WASM size, CPU budget, and 1,200-frame endurance suite (P08-002)', () => {
  const wasmPath = resolve(process.cwd(), 'public/wasm/osdbits_core.wasm');

  it('keeps release WASM binary strictly under 250 KiB uncompressed budget', async () => {
    const info = await stat(wasmPath);
    const sizeKiB = info.size / 1024;
    expect(sizeKiB).toBeGreaterThan(20);
    expect(sizeKiB).toBeLessThan(250);
  });

  it('sustains < 2.5 ms/frame mean CPU step and 0 bytes WASM memory growth over 1,200 frames and 24 mode switches', async () => {
    const bytes = await readFile(wasmPath);
    const raw = await loadOsdbitsWasm(bytes);
    const engine = new OsdbitsWasmEngine(raw, 0x2000_0304);

    try {
      // Warm up across all four modes so RenderList Vec capacities settle
      for (const m of [
        AppMode.Opening,
        AppMode.Idle,
        AppMode.Illegal,
        AppMode.Menu,
      ]) {
        engine.setMode(m, true);
        if (m === AppMode.Menu) {
          engine.dispatchMenuAction(MenuAction.Down);
          engine.dispatchMenuAction(MenuAction.Confirm);
        }
        for (let i = 0; i < 6; i++) {
          engine.stepAndRender(1.0);
        }
      }

      const baselineMemBytes = engine.memoryByteLength;
      const modes = [
        AppMode.Opening,
        AppMode.Idle,
        AppMode.Illegal,
        AppMode.Menu,
      ];

      const totalFrames = 1_200;
      const tStart = performance.now();

      for (let f = 0; f < totalFrames; f++) {
        // Switch modes every 50 frames -> 24 full mode transitions!
        if (f % 50 === 0) {
          const nextMode = modes[(f / 50) % modes.length]!;
          engine.setMode(nextMode, true);
          if (nextMode === AppMode.Menu && (f / 50) % 8 === 3) {
            engine.dispatchMenuAction(MenuAction.Down);
            engine.dispatchMenuAction(MenuAction.Confirm);
          }
        }
        const tel = engine.stepAndRender(1.0);
        expect(tel.vertexCount).toBeGreaterThan(0);

        // Spot-check finite clip coordinates every 200 frames
        if (f % 200 === 0) {
          const vBytes = engine.getVertexBytes();
          const f32 = new Float32Array(
            vBytes.buffer,
            vBytes.byteOffset,
            Math.min(64, vBytes.byteLength / 4),
          );
          for (const val of f32) {
            expect(Number.isFinite(val)).toBe(true);
          }
        }
      }

      const elapsedMs = performance.now() - tStart;
      const meanMsPerFrame = elapsedMs / totalFrames;

      expect(meanMsPerFrame).toBeLessThan(2.5);
      expect(engine.memoryByteLength).toBe(baselineMemBytes);
    } finally {
      engine.dispose();
    }
  });
});
