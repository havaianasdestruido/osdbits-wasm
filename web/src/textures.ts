/**
 * Procedural texture set.
 *
 * The original reads its artwork from the PS2 BIOS (`res/*.inc`, gitignored
 * here). None of it may ship, so every texture is generated at runtime from a
 * seeded PRNG: bright vertical streaks for the tower walls, radial gradients for
 * the fog and flare sprites, a soft blob for the light streaks and a
 * pseudo-refraction pattern for the cube environment. Sizes match the
 * reconstruction's (256x256 wall, 128x128 fog, 128x128 flare) so the UVs and the
 * additive layering keep behaving.
 *
 * Determinism matters: the same build must produce the same pixels, so the
 * generator is seeded with fixed constants and tested.
 */

import { TEX } from "./abi.js";

export interface ProceduralTexture {
  /** Draw-list texture id (`TEX.*`). */
  id: number;
  name: string;
  width: number;
  height: number;
  /** RGBA8, row-major, `width * height * 4` bytes. */
  data: Uint8Array;
}

/** mulberry32: tiny, deterministic, good enough for texture noise. */
export function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function clamp255(v: number): number {
  return v < 0 ? 0 : v > 255 ? 255 : v | 0;
}

function alloc(width: number, height: number): { data: Uint8Array; put: (x: number, y: number, r: number, g: number, b: number, a: number) => void } {
  const data = new Uint8Array(width * height * 4);
  const put = (x: number, y: number, r: number, g: number, b: number, a: number): void => {
    const o = (y * width + x) * 4;
    data[o] = clamp255(r);
    data[o + 1] = clamp255(g);
    data[o + 2] = clamp255(b);
    data[o + 3] = clamp255(a);
  };
  return { data, put };
}

/**
 * The tower wall: a vertical streak field. The reconstruction clamps the
 * tower's S coordinate to the texture edge, so a wide bright band is smeared
 * across the wall; the streaks here are spaced to survive that smear.
 */
export function makeWall(size = 256, seed = 0x51ce): ProceduralTexture {
  const rand = mulberry32(seed);
  const { data, put } = alloc(size, size);
  const columns = new Float32Array(size);
  for (let x = 0; x < size; x++) columns[x] = 0.15 + rand() * 0.1;
  // A handful of bright streaks with soft horizontal falloff.
  for (let s = 0; s < 18; s++) {
    const centre = rand() * size;
    const width = 1.5 + rand() * 5;
    const gain = 0.35 + rand() * 0.65;
    for (let x = 0; x < size; x++) {
      const dx = Math.abs(((x - centre + size * 1.5) % size) - size * 0.5);
      columns[x] += gain * Math.exp(-(dx * dx) / (2 * width * width));
    }
  }
  for (let y = 0; y < size; y++) {
    const band = 0.82 + 0.18 * Math.sin((y / size) * Math.PI * 2 + 0.7);
    for (let x = 0; x < size; x++) {
      const v = columns[x] * band;
      const r = 255 * Math.min(1, v * 0.96);
      const g = 255 * Math.min(1, v * 0.92);
      const b = 255 * Math.min(1, v * 1.0);
      put(x, y, r, g, b, 255);
    }
  }
  return { id: TEX.WALL, name: "wall", width: size, height: size, data };
}

/** Radial gradient cloud used by the fog layers and the illegal-scene sprites. */
export function makeCloud(size: number, id: number, name: string, seed: number, power: number, tint: readonly [number, number, number] = [1, 1, 1]): ProceduralTexture {
  const rand = mulberry32(seed);
  const { data, put } = alloc(size, size);
  const half = size / 2;
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const dx = (x + 0.5 - half) / half;
      const dy = (y + 0.5 - half) / half;
      const d = Math.min(1, Math.sqrt(dx * dx + dy * dy));
      // Smooth falloff plus a little noise so the additive layers do not band.
      const fall = Math.pow(1 - d, power);
      const n = 0.9 + rand() * 0.2;
      const v = 255 * Math.min(1, fall * n);
      put(x, y, v * tint[0], v * tint[1], v * tint[2], v);
    }
  }
  return { id, name, width: size, height: size, data };
}

/** Soft white core with a coloured rim: the flare disc sprite (`FLAR`). */
export function makeFlare(size = 128): ProceduralTexture {
  const { data, put } = alloc(size, size);
  const half = size / 2;
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const dx = (x + 0.5 - half) / half;
      const dy = (y + 0.5 - half) / half;
      const d = Math.min(1, Math.sqrt(dx * dx + dy * dy));
      const core = Math.exp(-(d * d) / 0.02);
      const halo = Math.exp(-(d * d) / 0.25) * 0.45;
      const v = Math.min(1, core + halo);
      put(x, y, 255 * v, 255 * v * (0.72 + 0.28 * (1 - d)), 255 * v * (0.6 + 0.4 * (1 - d)), 255 * v);
    }
  }
  return { id: TEX.FLAR, name: "flar", width: size, height: size, data };
}

/** Pseudo-refraction pattern for the cube environment layers (`REF`). */
export function makeRefraction(size = 256, seed = 0x2e6f): ProceduralTexture {
  const rand = mulberry32(seed);
  const { data, put } = alloc(size, size);
  const phase = new Float32Array(size);
  for (let i = 0; i < size; i++) phase[i] = rand() * Math.PI * 2;
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const u = x / size;
      const v = y / size;
      const rings = 0.5 + 0.5 * Math.sin((u * u + v * v) * 60 + phase[x]);
      const bands = 0.5 + 0.5 * Math.sin(v * 40 + phase[y]);
      const v2 = 0.25 + 0.5 * rings * bands;
      // Cyan-tinted glass.
      put(x, y, 255 * v2 * 0.6, 255 * v2 * 0.85, 255 * v2, 255 * v2);
    }
  }
  return { id: TEX.REF, name: "ref", width: size, height: size, data };
}

/**
 * The captured-framebuffer substitutes (`BLP`, `BLPR`, `SCREEN`, `WORK`).
 * The real renderer feeds these from the previous frame; we use a soft
 * vertical gradient so the passes composite into a plausible glow.
 */
export function makeBackdrop(
  size = 128,
  id: number = TEX.BLP,
  name = "blp",
  flip = false,
  seed = 0x77a1,
): ProceduralTexture {
  const rand = mulberry32(seed);
  const { data, put } = alloc(size, size);
  for (let y = 0; y < size; y++) {
    const v = (flip ? 1 - y / size : y / size) * 0.6 + 0.2;
    for (let x = 0; x < size; x++) {
      const n = 0.92 + rand() * 0.16;
      put(x, y, 255 * v * 0.7 * n, 255 * v * 0.9 * n, 255 * v * n, 255 * v);
    }
  }
  return { id, name, width: size, height: size, data };
}

/** The whole set, keyed by draw-list texture id. */
export function createTextureSet(): ProceduralTexture[] {
  return [
    makeWall(),
    makeCloud(128, TEX.FOG0, "fog0", 0x1111, 1.6),
    makeCloud(128, TEX.FOG1, "fog1", 0x2222, 1.1),
    makeCloud(128, TEX.FOG2, "fog2", 0x3333, 2.2),
    makeCloud(128, TEX.FOG3, "fog3", 0x4444, 0.9, [0.9, 0.95, 1]),
    makeCloud(128, TEX.FOG4, "fog4", 0x5555, 1.9, [0.8, 0.9, 1]),
    makeRefraction(),
    makeBackdrop(128, TEX.BLP, "blp", false, 0x77a1),
    makeBackdrop(128, TEX.BLPR, "blpr", true, 0x77a2),
    makeFlare(),
    makeCloud(64, TEX.CRBL, "crbl", 0x3333a, 1.4, [0.55, 0.85, 1]),
    makeCloud(64, TEX.CRLE, "crle", 0x3333b, 2.4, [1, 0.6, 0.45]),
    makeBackdrop(128, TEX.SCREEN, "screen", true, 0x77a3),
    makeBackdrop(128, TEX.WORK, "work", false, 0x77a4),
  ];
}

/** Deterministic digest of a texture, for regression tests. */
export function textureDigest(t: ProceduralTexture): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < t.data.length; i++) {
    h ^= t.data[i];
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h >>> 0;
}
