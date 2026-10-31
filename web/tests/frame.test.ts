import { describe, expect, it } from "vitest";
import {
  BATCH_WORDS,
  BLEND,
  CUBE_WORDS,
  FLAG,
  H,
  HEADER_WORDS,
  PRIM,
  TEX,
  VERTEX_WORDS,
} from "../src/abi.js";
import { FrameError, cubeAt, describeFrame, parseFrame, vertexAt, MAGIC, VERSION } from "../src/frame.js";

interface Spec {
  batches?: { prim: number; tex: number; blend: number; vertices: number }[];
  cubes?: number;
  width?: number;
  height?: number;
  flags?: number;
  cubeAfterBatch?: number;
}

/** Build a synthetic but well-formed frame, mirroring FrameWriter::finish. */
function buildFrame(spec: Spec = {}): Float32Array {
  const batches = spec.batches ?? [
    { prim: PRIM.TRIANGLES, tex: TEX.WALL, blend: BLEND.NONE, vertices: 3 },
    { prim: PRIM.SPRITES, tex: TEX.CRBL, blend: BLEND.ADD, vertices: 2 },
  ];
  const cubeCount = spec.cubes ?? 0;
  const vertexCount = batches.reduce((n, b) => n + b.vertices, 0);
  const total = HEADER_WORDS + batches.length * BATCH_WORDS + vertexCount * VERTEX_WORDS + cubeCount * CUBE_WORDS;
  const words = new Float32Array(total);
  const bits = (v: number): number => new Float32Array(new Uint32Array([v >>> 0]).buffer)[0];

  words[H.MAGIC] = bits(MAGIC);
  words[H.VERSION] = bits(VERSION);
  words[H.FRAME] = 12;
  words[H.MODE] = bits(0);
  words[H.SCENE] = 0;
  words[H.FLAGS] = bits(spec.flags ?? 0);
  words[H.WIDTH] = spec.width ?? 640;
  words[H.HEIGHT] = spec.height ?? 224;
  words[H.SCRZ] = 1024;
  words[H.AX] = 1;
  words[H.AY] = 0.52627105;
  words[H.BATCH_COUNT] = batches.length;
  words[H.VERTEX_COUNT] = vertexCount;
  words[H.CUBE_COUNT] = cubeCount;
  words[H.OFF_BATCHES] = HEADER_WORDS;
  words[H.OFF_VERTS] = HEADER_WORDS + batches.length * BATCH_WORDS;
  words[H.OFF_CUBES] = words[H.OFF_VERTS] + vertexCount * VERTEX_WORDS;
  words[H.TOTAL_WORDS] = total;
  words[H.CUBE_AFTER_BATCH] = bits(spec.cubeAfterBatch ?? batches.length);
  for (let r = 0; r < 4; r++) {
    words[H.VIEW + r * 4 + r] = 1;
  }
  words[H.CAM_POS + 2] = 16;

  // The batch records start right after the header; `H.OFF_BATCHES` is the
  // header *word* that stores that offset, not the offset itself.
  const batchesStart = HEADER_WORDS;
  let offset = 0;
  batches.forEach((b, i) => {
    const o = batchesStart + i * BATCH_WORDS;
    words[o] = b.prim;
    words[o + 1] = b.tex;
    words[o + 2] = b.blend;
    words[o + 3] = 0;
    words[o + 4] = offset;
    words[o + 5] = b.vertices;
    offset += b.vertices;
  });
  // Section *offsets* live in header words; the data starts after the header
  // and the batch records (the same arithmetic FrameWriter::finish uses).
  const vertsStart = batchesStart + batches.length * BATCH_WORDS;
  for (let i = 0; i < vertexCount; i++) {
    const o = vertsStart + i * VERTEX_WORDS;
    words[o] = i;
    words[o + 1] = i + 1;
    words[o + 2] = 10;
    words[o + 3] = 0.5;
    words[o + 4] = 0.5;
    words[o + 5] = 1;
    words[o + 6] = 0.5;
    words[o + 7] = 0.25;
    words[o + 8] = 1;
    words[o + 9] = 0;
  }
  const cubesStart = vertsStart + vertexCount * VERTEX_WORDS;
  for (let c = 0; c < cubeCount; c++) {
    words[cubesStart + c * CUBE_WORDS] = c;
  }
  return words;
}

describe("parseFrame", () => {
  it("decodes the header, batches and vertices", () => {
    const frame = parseFrame(buildFrame());
    expect(frame.frame).toBe(12);
    expect(frame.width).toBe(640);
    expect(frame.height).toBe(224);
    expect(frame.batches).toHaveLength(2);
    expect(frame.batches[0]).toMatchObject({
      prim: PRIM.TRIANGLES,
      tex: TEX.WALL,
      blend: BLEND.NONE,
      vertexOffset: 0,
      vertexCount: 3,
    });
    expect(frame.batches[1].vertexOffset).toBe(3);
    expect(frame.stats.vertices).toBe(5);
    expect(frame.vertices).toHaveLength(5 * VERTEX_WORDS);
    expect(vertexAt(frame, 1)).toMatchObject({ x: 1, y: 2, z: 10, u: 0.5, v: 0.5 });
    expect(frame.ended).toBe(false);
    expect(frame.cubeAfterBatch).toBe(2);
  });

  it("exposes the view matrix and camera", () => {
    const frame = parseFrame(buildFrame());
    expect(Array.from(frame.view)).toEqual([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
    expect(frame.cameraPos[2]).toBe(16);
  });

  it("decodes cubes", () => {
    const frame = parseFrame(buildFrame({ cubes: 2 }));
    expect(frame.stats.cubes).toBe(2);
    expect(cubeAt(frame, 1).index).toBe(1);
    expect(cubeAt(frame, 0).faceSign).toHaveLength(6);
    expect(cubeAt(frame, 0).corners).toHaveLength(32);
  });

  it("flags the ended and PAL bits", () => {
    const flags = FLAG.ENDED | FLAG.PAL;
    const frame = parseFrame(buildFrame({ flags }));
    expect(frame.ended).toBe(true);
    expect(frame.pal).toBe(true);
  });

  it("rejects a bad magic", () => {
    const words = buildFrame();
    words[H.MAGIC] = 0;
    expect(() => parseFrame(words)).toThrow(FrameError);
    expect(() => parseFrame(words)).toThrow(/magic/);
  });

  it("rejects an unknown version", () => {
    const words = buildFrame();
    words[H.VERSION] = 99;
    expect(() => parseFrame(words)).toThrow(/version/);
  });

  it("rejects a truncated frame", () => {
    const words = buildFrame();
    words[H.TOTAL_WORDS] = words.length + 8;
    expect(() => parseFrame(words)).toThrow(/only .* are available/);
  });

  it("rejects inconsistent section offsets", () => {
    const words = buildFrame();
    words[H.OFF_VERTS] += 1;
    expect(() => parseFrame(words)).toThrow(/vertex section starts at/);
  });

  it("rejects a batch range outside the vertex section", () => {
    const words = buildFrame();
    // Batch 1's vertex range, addressed inside the batch *section*.
    words[HEADER_WORDS + 1 * BATCH_WORDS + 5] = 99;
    expect(() => parseFrame(words)).toThrow(/exceeds/);
  });

  it("rejects a cube insertion index outside the batch list", () => {
    const words = buildFrame();
    words[H.CUBE_AFTER_BATCH] = 7;
    expect(() => parseFrame(words)).toThrow(/insertion index/);
  });

  it("rejects non-finite words", () => {
    const words = buildFrame();
    words[H.CAM_POS] = Number.NaN;
    expect(() => parseFrame(words)).toThrow(/not finite/);
  });

  it("rejects a header-only buffer", () => {
    expect(() => parseFrame(new Float32Array(8))).toThrow(/too short/);
  });

  it("describes itself for the diagnostics panel", () => {
    const text = describeFrame(parseFrame(buildFrame()));
    expect(text).toContain("2 batches");
    expect(text).toContain("5 verts");
  });
});
