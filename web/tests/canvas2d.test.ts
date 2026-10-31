/**
 * The compatibility renderer, executed against a real rasteriser.
 *
 * CI browsers without a WebGPU adapter draw the scene with this renderer, so it
 * is the path most visitors actually see. It was previously covered only by
 * unit tests of the frame parser; here the core's real draw list is rasterised
 * by @napi-rs/canvas (a Skia binding) and the pixels are inspected.
 *
 * What this proves: the draw list can be rendered without throwing, opaque and
 * additive passes are applied, and the result is a varied image rather than a
 * clear colour. What it does not prove: WebGPU output, GS-accurate blending, or
 * anything about a real browser's canvas implementation.
 */

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { createCanvas } from "@napi-rs/canvas";
import { beforeAll, describe, expect, it } from "vitest";
import { Canvas2dRenderer } from "../src/canvas2d.js";
import { OPT } from "../src/abi.js";
import { createTextureSet } from "../src/textures.js";
import { loadCore, type Core } from "../src/wasm.js";
import type { Frame } from "../src/frame.js";

const wasmPath = resolve(process.cwd(), "public/osdbits_core.wasm");

/** createCanvas has an overload set with different return types; pin one. */
type NapiCanvas = ReturnType<typeof createCanvas>;

/** Minimal canvas factory: the renderer only ever asks for 2D canvases. */
function installCanvasShim(): void {
  const g = globalThis as unknown as {
    document?: { createElement: (tag: string) => unknown };
  };
  g.document = {
    createElement: (tag: string) => {
      if (tag !== "canvas") throw new Error(`unexpected element <${tag}>`);
      return createCanvas(1, 1);
    },
  };
}

/** The napi canvas context typed as the DOM one (Skia implements the same API). */
function ctx2d(canvas: NapiCanvas): CanvasRenderingContext2D {
  return canvas.getContext("2d") as unknown as CanvasRenderingContext2D;
}

/** Mean luminance variance over a downsampled copy, as a "not blank" signal. */
function varianceOf(canvas: NapiCanvas): number {
  const ctx = ctx2d(canvas);
  const data = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
  let sum = 0;
  let sumSq = 0;
  const n = data.length / 4;
  for (let i = 0; i < data.length; i += 4) {
    const l = data[i] * 0.299 + data[i + 1] * 0.587 + data[i + 2] * 0.114;
    sum += l;
    sumSq += l * l;
  }
  return sumSq / n - (sum / n) ** 2;
}

function meanOf(canvas: NapiCanvas): number {
  const ctx = ctx2d(canvas);
  const data = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
  let sum = 0;
  for (let i = 0; i < data.length; i += 4) {
    sum += data[i] * 0.299 + data[i + 1] * 0.587 + data[i + 2] * 0.114;
  }
  return sum / (data.length / 4);
}

describe("canvas2d renderer", () => {
  let core: Core;

  beforeAll(async () => {
    installCanvasShim();
    const bytes = readFileSync(wasmPath);
    const buffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
    core = await loadCore(buffer as ArrayBuffer);
  });

  const renderFrames = (
    frames: number,
    options: { pal?: boolean; scene?: number; cubes?: boolean } = {},
  ): { canvas: NapiCanvas; draws: number; last: Frame } => {
    const width = 640;
    const height = options.pal ? 256 : 224;
    const canvas = createCanvas(width, height) as unknown as NapiCanvas;
    const renderer = new Canvas2dRenderer(
      canvas as unknown as HTMLCanvasElement,
      createTextureSet(),
      options.cubes ?? true,
    );
    renderer.resize(width, height);

    core.setOption(OPT.PAL, options.pal ? 1 : 0);
    core.setOption(OPT.FOG, 1);
    core.setOption(OPT.LIGHTS, 1);
    core.setOption(OPT.CUBES, options.cubes === false ? 0 : 1);
    core.setOption(OPT.TEXT, 1);
    core.init(options.scene ?? 0, 4242);

    let draws = 0;
    let last = core.draw();
    for (let i = 0; i < frames; i++) {
      core.step();
      last = core.draw();
      draws = renderer.draw(last).draws;
    }
    return { canvas, draws, last };
  };

  it("renders the opening scene without throwing and produces a varied image", () => {
    const { canvas, draws, last } = renderFrames(90);
    expect(last.batches.length).toBeGreaterThan(0);
    expect(draws).toBeGreaterThan(0);
    // A tower field with fog and light trails is neither black nor flat.
    expect(varianceOf(canvas)).toBeGreaterThan(4);
    expect(meanOf(canvas)).toBeGreaterThan(0.5);
    expect(meanOf(canvas)).toBeLessThan(254);
  });

  it("honours the PAL mode with a 640x256 surface", () => {
    const { canvas, last } = renderFrames(30, { pal: true });
    expect(canvas.width).toBe(640);
    expect(canvas.height).toBe(256);
    expect(last.height).toBe(256);
    expect(varianceOf(canvas)).toBeGreaterThan(1);
  });

  it("renders the illegal-disc scene", () => {
    const { canvas } = renderFrames(60, { scene: 1 });
    // Red cloud, flare and fades: some light has to reach the canvas.
    const ctx = ctx2d(canvas);
    const data = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
    let reddish = 0;
    for (let i = 0; i < data.length; i += 4) {
      if (data[i] > 40 && data[i] > data[i + 1] + 15) reddish++;
    }
    expect(reddish).toBeGreaterThan(50);
  });

  it("survives a frame with every optional layer disabled", () => {
    const canvas = createCanvas(640, 224) as unknown as NapiCanvas;
    const renderer = new Canvas2dRenderer(
      canvas as unknown as HTMLCanvasElement,
      createTextureSet(),
      false,
    );
    renderer.resize(640, 224);
    for (const option of [OPT.FOG, OPT.LIGHTS, OPT.CUBES, OPT.TEXT, OPT.BLACK_BARS]) {
      core.setOption(option, 0);
    }
    core.init(0, 4242);
    for (let i = 0; i < 20; i++) {
      core.step();
      expect(() => renderer.draw(core.draw())).not.toThrow();
    }
    renderer.destroy();
  });
});
