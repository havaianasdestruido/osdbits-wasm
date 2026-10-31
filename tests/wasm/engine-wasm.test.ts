import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { beforeAll, describe, expect, it } from 'vitest';

import {
  AppMode,
  AudioCueBit,
  BootCompletionPolicy,
  DEFAULT_ENGINE_TUNING,
  loadOsdbitsWasm,
  MenuAction,
  MenuPage,
  OsdbitsWasmEngine,
  TextureSlot,
  type WasmRawExports,
} from '../../src/wasm/bridge';

describe('osdbits-core WASM engine integration (P02-001..P02-006)', () => {
  let raw: WasmRawExports;

  beforeAll(async () => {
    const wasmPath = resolve(process.cwd(), 'public/wasm/osdbits_core.wasm');
    const wasmBytes = await readFile(wasmPath);
    raw = await loadOsdbitsWasm(wasmBytes);
  });

  it('exposes exact C-ABI struct layouts and all 15 procedural clean-room textures', () => {
    expect(raw.osdbits_sizeof_render_vertex()).toBe(32);
    expect(raw.osdbits_sizeof_render_batch()).toBe(32);
    expect(raw.osdbits_sizeof_telemetry()).toBe(64);

    const engine = new OsdbitsWasmEngine(raw, 0x2000_0304);
    try {
      expect(engine.usingImportedAssets).toBe(false);
      expect(engine.textureGeneration).toBe(1);

      for (let slot = TextureSlot.TowerWall; slot <= TextureSlot.UiGlyphAtlas; slot++) {
        const view = engine.getTextureView(slot as TextureSlot);
        expect(view).not.toBeNull();
        if (!view) continue;
        expect(view.pixels.byteLength).toBe(view.width * view.height * 4);
      }

      // Verify GlassBumpInv === bitwise !GlassBump RGB across WASM linear memory
      const bump = engine.getTextureView(TextureSlot.GlassBump);
      const bumpInv = engine.getTextureView(TextureSlot.GlassBumpInv);
      expect(bump).not.toBeNull();
      expect(bumpInv).not.toBeNull();
      if (bump && bumpInv) {
        for (let i = 0; i < bump.pixels.length; i += 4) {
          expect(bumpInv.pixels[i]).toBe((~bump.pixels[i]!) & 0xff);
          expect(bumpInv.pixels[i + 1]).toBe((~bump.pixels[i + 1]!) & 0xff);
          expect(bumpInv.pixels[i + 2]).toBe((~bump.pixels[i + 2]!) & 0xff);
        }
      }
    } finally {
      engine.dispose();
    }
  });

  it('replays bit-identically across WASM boundary for fixed seeds in all four modes', () => {
    const engineA = new OsdbitsWasmEngine(raw, 777_888);
    const engineB = new OsdbitsWasmEngine(raw, 777_888);

    try {
      const fixedRtc = new Date(Date.UTC(2026, 9, 31, 21, 45, 12));
      engineA.setRtc(fixedRtc);
      engineB.setRtc(fixedRtc);

      for (const mode of [
        AppMode.Opening,
        AppMode.Idle,
        AppMode.Illegal,
        AppMode.Menu,
      ]) {
        engineA.setMode(mode, true);
        engineB.setMode(mode, true);

        for (let f = 0; f < 15; f++) {
          const telA = engineA.stepAndRender(1.0);
          const telB = engineB.stepAndRender(1.0);
          expect(telA).toEqual(telB);
          expect(telA.vertexCount).toBeGreaterThan(0);
          expect(telA.batchCount).toBeGreaterThan(0);

          const vertsA = engineA.getVertexBytes().slice();
          const vertsB = engineB.getVertexBytes().slice();
          expect(vertsA.byteLength).toBe(vertsB.byteLength);
          expect(Buffer.compare(vertsA, vertsB)).toBe(0);
          expect(engineA.getBatches()).toEqual(engineB.getBatches());
        }
      }
    } finally {
      engineA.dispose();
      engineB.dispose();
    }
  });

  it('transitions from Boot Opening to Menu and dispatches keyboard + pointer menu actions', () => {
    const engine = new OsdbitsWasmEngine(raw, 12345);
    try {
      engine.setBootCompletionPolicy(BootCompletionPolicy.TransitionToMenu);
      engine.setTuning({ ...DEFAULT_ENGINE_TUNING, speedScale: 2.0 });

      let last = engine.stepAndRender(1.0);
      for (let i = 0; i < 300 && last.mode !== AppMode.Menu; i++) {
        last = engine.stepAndRender(1.0);
      }
      expect(last.mode).toBe(AppMode.Menu);
      expect(last.menuPage).toBe(MenuPage.MainMenu);

      // Navigate Down -> Confirm into System Configuration
      const downCue = engine.dispatchMenuAction(MenuAction.Down);
      expect(downCue & AudioCueBit.CursorTick).not.toBe(0);

      const confirmCue = engine.dispatchMenuAction(MenuAction.Confirm);
      expect(confirmCue & (AudioCueBit.Confirm | AudioCueBit.TransitionIn)).not.toBe(0);

      const cfgFrame = engine.stepAndRender(1.0);
      expect(cfgFrame.menuPage).toBe(MenuPage.SystemConfig);
      expect(cfgFrame.audioCues & AudioCueBit.Confirm).not.toBe(0);

      // Pointer click on Clock Adjust row (row 0 -> y ≈ 68 / 224)
      engine.dispatchPointerClick(0.55, 68.0 / 224.0);
      const clockFrame = engine.stepAndRender(1.0);
      expect(clockFrame.menuPage).toBe(MenuPage.ClockAdjust);
    } finally {
      engine.dispose();
    }
  });

  it('validates synthetic TEXIMAGE imports, rejects corrupt inputs, and resets cleanly', () => {
    const engine = new OsdbitsWasmEngine(raw, 42);
    try {
      const corrupt = engine.importContainer(new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8]));
      expect(corrupt.ok).toBe(false);
      if (!corrupt.ok) {
        expect(corrupt.errorCode).toBe(-3);
      }
      expect(engine.usingImportedAssets).toBe(false);

      const proceduralSmoke = engine.getTextureView(TextureSlot.SmokeTunnel)!.pixels.slice();

      const synthetic = engine.buildSyntheticTeximage(0xc0ffee);
      const res = engine.importContainer(synthetic);
      expect(res.ok).toBe(true);
      if (res.ok) {
        expect(res.importedTextureCount).toBe(3);
        expect(res.sha256Hex).toMatch(/^[0-9a-f]{64}$/);
      }
      expect(engine.usingImportedAssets).toBe(true);
      const importedSmoke = engine.getTextureView(TextureSlot.SmokeTunnel)!.pixels.slice();
      expect(Buffer.compare(importedSmoke, proceduralSmoke)).not.toBe(0);
      expect(importedSmoke[2]).toBe(220);
      expect(importedSmoke[3]).toBe(255);

      engine.resetProceduralTextures();
      expect(engine.usingImportedAssets).toBe(false);
      const restoredSmoke = engine.getTextureView(TextureSlot.SmokeTunnel)!.pixels.slice();
      expect(Buffer.compare(restoredSmoke, proceduralSmoke)).toBe(0);
    } finally {
      engine.dispose();
    }
  });

  it('maintains bounded WASM linear memory across 360 steady-state frames', () => {
    const engine = new OsdbitsWasmEngine(raw, 999);
    try {
      engine.setMode(AppMode.Idle, true);
      for (let i = 0; i < 10; i++) {
        engine.stepAndRender(1.0);
      }
      const warmedBytes = engine.memoryByteLength;
      for (let i = 0; i < 360; i++) {
        engine.stepAndRender(1.0);
      }
      expect(engine.memoryByteLength).toBe(warmedBytes);
    } finally {
      engine.dispose();
    }
  });
});
