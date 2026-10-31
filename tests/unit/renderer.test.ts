import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, expect, it, vi } from 'vitest';

import { createOsdbitsRenderer } from '../../src/renderer/createRenderer';
import { OSDBITS_WGSL_SOURCE } from '../../src/renderer/shaders/osdbitsShaderSource';
import { screenSizeIndexToAspect } from '../../src/renderer/types';
import { computeViewportLayout } from '../../src/renderer/viewport';
import {
  AppMode,
  loadOsdbitsWasm,
  OsdbitsWasmEngine,
} from '../../src/wasm/bridge';

function createMock2dContext(): CanvasRenderingContext2D {
  const imageBytes = new Uint8ClampedArray(640 * 480 * 4);
  imageBytes[0] = 12;
  imageBytes[1] = 34;
  imageBytes[2] = 88;
  imageBytes[3] = 255;
  return {
    globalCompositeOperation: 'source-over',
    globalAlpha: 1,
    fillStyle: '#000000',
    fillRect: vi.fn(),
    beginPath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    closePath: vi.fn(),
    fill: vi.fn(),
    drawImage: vi.fn(),
    putImageData: vi.fn(),
    getImageData: vi.fn((_x: number, _y: number, w: number, h: number) => ({
      width: w,
      height: h,
      data: new Uint8ClampedArray(w * h * 4).fill(42),
    })),
  } as unknown as CanvasRenderingContext2D;
}

function createMockCanvas(lockFirst2d = false): {
  canvas: HTMLCanvasElement;
  getReplaced: () => HTMLCanvasElement | null;
} {
  let replaced: HTMLCanvasElement | null = null;
  const ctx2d = createMock2dContext();
  const makeNode = (locked: boolean): HTMLCanvasElement => {
    const doc = {
      createElement: () => makeNode(false),
    };
    const node = {
      width: 640,
      height: 480,
      clientWidth: 640,
      clientHeight: 480,
      ownerDocument: doc,
      getContext: vi.fn((kind: string) => {
        if (kind === '2d') {
          return locked ? null : ctx2d;
        }
        return null;
      }),
      cloneNode: vi.fn(() => makeNode(false)),
      replaceWith: vi.fn((next: HTMLCanvasElement) => {
        replaced = next;
      }),
    };
    return node as unknown as HTMLCanvasElement;
  };
  return {
    canvas: makeNode(lockFirst2d),
    getReplaced: () => replaced,
  };
}

describe('Phase 3 renderer unit suite (P03-001, P03-002)', () => {
  it('keeps osdbits.wgsl byte-identical to OSDBITS_WGSL_SOURCE with flat integer interpolation', async () => {
    const wgslDisk = await readFile(
      resolve(process.cwd(), 'src/renderer/shaders/osdbits.wgsl'),
      'utf8',
    );
    expect(OSDBITS_WGSL_SOURCE).toBe(wgslDisk);
    expect(OSDBITS_WGSL_SOURCE).toContain('fn vs_main');
    expect(OSDBITS_WGSL_SOURCE).toContain('fn fs_main');
    expect(OSDBITS_WGSL_SOURCE).toContain('fn vs_blit');
    expect(OSDBITS_WGSL_SOURCE).toContain('fn fs_blit');
    expect(OSDBITS_WGSL_SOURCE).toContain('fn fs_downsample');
    expect(OSDBITS_WGSL_SOURCE).toContain('@interpolate(flat) flags: u32');
  });

  it('computes pillarboxed and letterboxed viewports without aspect distortion', () => {
    // Ultrawide 2560x1080 displaying 4:3 -> pillarboxed left/right
    const ultra43 = computeViewportLayout(2560, 1080, 1.0, 1.0, '4:3');
    expect(ultra43.viewportHeight).toBe(1080);
    expect(ultra43.viewportWidth).toBe(1440);
    expect(ultra43.viewportX).toBe(560);
    expect(ultra43.viewportY).toBe(0);

    // Portrait 400x800 displaying 16:9 -> letterboxed top/bottom
    const portrait169 = computeViewportLayout(400, 800, 1.0, 1.0, '16:9');
    expect(portrait169.viewportWidth).toBe(400);
    expect(portrait169.viewportHeight).toBe(225);
    expect(portrait169.viewportX).toBe(0);
    expect(portrait169.viewportY).toBe(287);

    // Extreme/NaN inputs clamp safely to finite even dimensions
    const clamped = computeViewportLayout(Number.NaN, -100, 99, 99, 'full');
    expect(clamped.canvasWidth).toBeGreaterThanOrEqual(64);
    expect(clamped.renderWidth % 2).toBe(0);
    expect(clamped.renderHeight % 2).toBe(0);

    expect(screenSizeIndexToAspect(0)).toBe('4:3');
    expect(screenSizeIndexToAspect(1)).toBe('full');
    expect(screenSizeIndexToAspect(2)).toBe('16:9');
  });

  it('gracefully falls back to FallbackCanvasRenderer and replaces locked canvases when WebGPU fails', async () => {
    const { canvas, getReplaced } = createMockCanvas(true);
    const result = await createOsdbitsRenderer({ canvas });
    expect(result.renderer.backend).toBe('canvas2d');
    expect(result.renderer.diagnostics.isFallbackAdapter).toBe(true);
    expect(result.renderer.diagnostics.degradedReason).toContain('WebGPU unavailable');
    expect(getReplaced()).not.toBeNull();

    // Render real WASM frames across Opening, Illegal, and Menu through FallbackCanvasRenderer
    const wasmBytes = await readFile(
      resolve(process.cwd(), 'public/wasm/osdbits_core.wasm'),
    );
    const raw = await loadOsdbitsWasm(wasmBytes);
    const engine = new OsdbitsWasmEngine(raw, 2026);
    try {
      for (const mode of [AppMode.Opening, AppMode.Illegal, AppMode.Menu]) {
        engine.setMode(mode, true);
        const tel = engine.stepAndRender(1.0);
        result.renderer.renderFrame(engine, tel);
      }
      result.renderer.resize(1280, 720, 1.5, 1.0, '16:9');
      const readback = await result.renderer.readPixels();
      expect(readback.rgba.byteLength).toBe(readback.width * readback.height * 4);
      expect(readback.rgba[0]).toBe(42);
    } finally {
      engine.dispose();
      result.renderer.dispose();
    }
  });
});
