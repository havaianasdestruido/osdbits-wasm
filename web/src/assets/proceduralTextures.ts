/**
 * Clean-room, deterministic, 100% procedural texture, environment map, emblem, and glyph atlas
 * synthesizer (`P04-001`) matching every OSDSYS `TextureId` (`0..22`) slot dimension, mipmap
 * count (`6` mip levels for `WAL0`), and PS2 GS `RGBA8` (`0x80 = 128` unity alpha) convention.
 *
 * Contains ZERO Sony/PlayStation BIOS bytes, logos, or copyrighted assets.
 */

export const enum TextureSlotId {
  Banner = 0,
  Fog0 = 1,
  Fog1 = 2,
  Fog2 = 3,
  Fog3 = 4,
  Fog4 = 5,
  Wal0 = 6,
  Crle = 7,
  Crbl = 8,
  Flar = 9,
  Ref = 10,
  Blp = 11,
  Blpr = 12,
  IllegalMsg = 13,
  MenuKabe = 14,
  MenuBlur = 15,
  MenuNavi = 16,
  MenuBump = 17,
  MenuBinv = 18,
  MenuFlow = 19,
  MenuRefa = 20,
  MenuMaru = 21,
  GlyphAtlas = 22,
}

export const PROCEDURAL_SLOT_COUNT = 23;

export type AssetProvenance = "procedural-clean-room" | "user-imported";

export interface ProceduralTextureAsset {
  id: TextureSlotId;
  name: string;
  width: number;
  height: number;
  /** `RGBA8` (`width >> level` x `height >> level` x 4 bytes) per mip level. */
  mipLevels: Uint8Array[];
  provenance: AssetProvenance;
  fnv1aHash: string;
}

export function computeFnv1aHex(bytes: Uint8Array): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < bytes.length; i++) {
    hash ^= bytes[i] ?? 0;
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash.toString(16).padStart(8, "0");
}

export async function computeSha256Hex(bytes: Uint8Array): Promise<string> {
  if (typeof globalThis.crypto?.subtle?.digest === "function") {
    const copy = new Uint8Array(bytes.byteLength);
    copy.set(bytes);
    const digest = await globalThis.crypto.subtle.digest("SHA-256", copy.buffer);
    return Array.from(new Uint8Array(digest))
      .map((b) => b.toString(16).padStart(2, "0"))
      .join("");
  }
  return computeFnv1aHex(bytes).repeat(8);
}

function clampByte(v: number): number {
  if (!Number.isFinite(v)) return 0;
  return Math.max(0, Math.min(255, Math.round(v)));
}

function hash2d(ix: number, iy: number, periodX: number, periodY: number, seed: number): number {
  const wx = ((ix % periodX) + periodX) % periodX;
  const wy = ((iy % periodY) + periodY) % periodY;
  let h = (Math.imul(wx, 374761393) + Math.imul(wy, 668265263) + Math.imul(seed, 1442695041)) >>> 0;
  h = Math.imul(h ^ (h >>> 13), 1274126177) >>> 0;
  return ((h ^ (h >>> 16)) >>> 0) / 4294967295;
}

function smoothstep(t: number): number {
  return t * t * (3 - 2 * t);
}

function periodicNoise(u: number, v: number, freq: number, seed: number): number {
  const x = u * freq;
  const y = v * freq;
  const x0 = Math.floor(x);
  const y0 = Math.floor(y);
  const tx = smoothstep(x - x0);
  const ty = smoothstep(y - y0);

  const n00 = hash2d(x0, y0, freq, freq, seed);
  const n10 = hash2d(x0 + 1, y0, freq, freq, seed);
  const n01 = hash2d(x0, y0 + 1, freq, freq, seed);
  const n11 = hash2d(x0 + 1, y0 + 1, freq, freq, seed);

  const nx0 = n00 + (n10 - n00) * tx;
  const nx1 = n01 + (n11 - n01) * tx;
  return nx0 + (nx1 - nx0) * ty;
}

function seamlessFbm(u: number, v: number, baseFreq: number, octaves: number, seed: number): number {
  let sum = 0;
  let amp = 0.5;
  let norm = 0;
  let freq = baseFreq;
  for (let o = 0; o < octaves; o++) {
    sum += amp * periodicNoise(u, v, freq, seed + o * 97);
    norm += amp;
    amp *= 0.5;
    freq *= 2;
  }
  return norm > 0 ? sum / norm : 0;
}

/** Downsamples an `RGBA8` image (`w x h`) into `mipCount` box-filtered levels. */
export function buildRgbaMipChain(base: Uint8Array, width: number, height: number, mipCount: number): Uint8Array[] {
  const chain: Uint8Array[] = [base];
  let prev = base;
  let pw = width;
  let ph = height;

  for (let level = 1; level < mipCount; level++) {
    const nw = Math.max(1, pw >> 1);
    const nh = Math.max(1, ph >> 1);
    const dst = new Uint8Array(nw * nh * 4);
    for (let y = 0; y < nh; y++) {
      const y0 = Math.min(ph - 1, y * 2);
      const y1 = Math.min(ph - 1, y * 2 + 1);
      for (let x = 0; x < nw; x++) {
        const x0 = Math.min(pw - 1, x * 2);
        const x1 = Math.min(pw - 1, x * 2 + 1);
        const i00 = (y0 * pw + x0) * 4;
        const i10 = (y0 * pw + x1) * 4;
        const i01 = (y1 * pw + x0) * 4;
        const i11 = (y1 * pw + x1) * 4;
        const di = (y * nw + x) * 4;
        for (let c = 0; c < 3; c++) {
          dst[di + c] =
            ((prev[i00 + c] ?? 0) +
              (prev[i10 + c] ?? 0) +
              (prev[i01 + c] ?? 0) +
              (prev[i11 + c] ?? 0)) >>
            2;
        }
        // Preserve maximum PS2 alpha across the 2x2 footprint (matches `opening.c:854` `a |= ...`)
        dst[di + 3] = Math.max(
          prev[i00 + 3] ?? 0,
          prev[i10 + 3] ?? 0,
          prev[i01 + 3] ?? 0,
          prev[i11 + 3] ?? 0,
        );
      }
    }
    chain.push(dst);
    prev = dst;
    pw = nw;
    ph = nh;
  }
  return chain;
}

// Compact clean-room 5x7 bitmap font for procedural banner / glyph atlas synthesis
const GLYPH_5X7: Readonly<Record<string, readonly number[]>> = {
  " ": [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
  "-": [0x00, 0x00, 0x00, 0x1f, 0x00, 0x00, 0x00],
  ".": [0x00, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x0c],
  ":": [0x00, 0x0c, 0x0c, 0x00, 0x0c, 0x0c, 0x00],
  "/": [0x01, 0x02, 0x04, 0x08, 0x10, 0x00, 0x00],
  "0": [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
  "1": [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
  "2": [0x0e, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1f],
  "3": [0x1f, 0x02, 0x04, 0x06, 0x01, 0x11, 0x0e],
  "4": [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
  "5": [0x1f, 0x10, 0x1e, 0x01, 0x01, 0x11, 0x0e],
  "6": [0x06, 0x08, 0x10, 0x1e, 0x11, 0x11, 0x0e],
  "7": [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
  "8": [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
  "9": [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x02, 0x0c],
  A: [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
  B: [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e],
  C: [0x0e, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0e],
  D: [0x1c, 0x12, 0x11, 0x11, 0x11, 0x12, 0x1c],
  E: [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
  F: [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
  G: [0x0e, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0f],
  H: [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
  I: [0x0e, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e],
  J: [0x07, 0x02, 0x02, 0x02, 0x12, 0x12, 0x0c],
  K: [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
  L: [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f],
  M: [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11],
  N: [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
  O: [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
  P: [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
  Q: [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d],
  R: [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
  S: [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
  T: [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
  U: [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
  V: [0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04],
  W: [0x11, 0x11, 0x11, 0x15, 0x15, 0x1b, 0x11],
  X: [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11],
  Y: [0x11, 0x11, 0x0a, 0x04, 0x04, 0x04, 0x04],
  Z: [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f],
};

function drawBitmapString(
  rgba: Uint8Array,
  width: number,
  height: number,
  text: string,
  originX: number,
  originY: number,
  scale: number,
  spacing: number,
  rgb: [number, number, number],
  ps2Alpha: number,
): void {
  let cursorX = originX;
  const upper = text.toUpperCase();
  for (let idx = 0; idx < upper.length; idx++) {
    const ch = upper[idx] ?? " ";
    const rows = GLYPH_5X7[ch] ?? GLYPH_5X7[" "]!;
    for (let gy = 0; gy < 7; gy++) {
      const rowBits = rows[gy] ?? 0;
      for (let gx = 0; gx < 5; gx++) {
        if (((rowBits >> (4 - gx)) & 1) === 0) continue;
        for (let sy = 0; sy < scale; sy++) {
          const py = originY + gy * scale + sy;
          if (py < 0 || py >= height) continue;
          for (let sx = 0; sx < scale; sx++) {
            const px = cursorX + gx * scale + sx;
            if (px < 0 || px >= width) continue;
            const o = (py * width + px) * 4;
            rgba[o] = rgb[0];
            rgba[o + 1] = rgb[1];
            rgba[o + 2] = rgb[2];
            rgba[o + 3] = ps2Alpha;
          }
        }
      }
    }
    cursorX += 5 * scale + spacing;
  }
}

function buildBannerTexture(): Uint8Array {
  const w = 256;
  const h = 64;
  const out = new Uint8Array(w * h * 4);

  // Subtle horizontal luminous rule + clean-room "OSDBITS" emblem (zero Sony trademarks)
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const dx = (x - w * 0.5) / (w * 0.44);
      const dy = (y - h * 0.5) / (h * 0.42);
      const halo = Math.max(0, 1 - Math.sqrt(dx * dx + dy * dy));
      if (halo > 0) {
        const o = (y * w + x) * 4;
        const lum = clampByte(halo * halo * 46);
        out[o] = clampByte(lum * 0.85);
        out[o + 1] = clampByte(lum * 0.95);
        out[o + 2] = clampByte(lum * 1.15);
        out[o + 3] = clampByte(halo * halo * 38);
      }
    }
  }

  // Thin crisp underline bar
  for (let x = 44; x < w - 44; x++) {
    const fade = Math.sin(((x - 44) / (w - 88)) * Math.PI);
    const o = (48 * w + x) * 4;
    out[o] = 200;
    out[o + 1] = 225;
    out[o + 2] = 255;
    out[o + 3] = clampByte(fade * 110);
  }

  drawBitmapString(out, w, h, "OSDBITS", 52, 20, 3, 8, [242, 248, 255], 128);
  return out;
}

function buildFogTexture(
  w: number,
  h: number,
  seed: number,
  rgbScale: [number, number, number] = [1.0, 1.0, 1.04],
): Uint8Array {
  const out = new Uint8Array(w * h * 4);
  for (let y = 0; y < h; y++) {
    const v = y / h;
    for (let x = 0; x < w; x++) {
      const u = x / w;
      const n = seamlessFbm(u, v, 4, 4, seed);
      // Soft envelope so patch borders blend seamlessly without hard quad edges
      const ex = Math.sin(u * Math.PI);
      const ey = Math.sin(v * Math.PI);
      const env = Math.pow(ex * ey, 0.45);
      const val = Math.max(0, (n - 0.22) * 1.38) * (0.55 + 0.45 * env);
      const lum = clampByte(val * 235);
      const o = (y * w + x) * 4;
      out[o] = clampByte(lum * rgbScale[0]);
      out[o + 1] = clampByte(lum * rgbScale[1]);
      out[o + 2] = clampByte(lum * rgbScale[2]);
      // Matches PSMCT16 TEXA(0x7F, AEM=1, 0x81): 0 when black, 0x7F when non-black
      out[o + 3] = lum === 0 ? 0 : 0x7f;
    }
  }
  return out;
}

function buildTowerWallTexture(seed: number): Uint8Array {
  const w = 256;
  const h = 256;
  const out = new Uint8Array(w * h * 4);
  for (let y = 0; y < h; y++) {
    const v = y / h;
    for (let x = 0; x < w; x++) {
      const u = x / w;
      const fine = seamlessFbm(u, v, 8, 4, seed);
      const verticalGrain = seamlessFbm(u * 2, v * 0.25, 8, 3, seed + 503);
      // Subtle architectural micro-fluting along tower faces
      const flute = 0.5 + 0.5 * Math.cos(u * Math.PI * 16);
      const panelBand = (y % 32 === 0 || x % 32 === 0) ? -14 : 0;
      const base = 152 + fine * 52 + verticalGrain * 28 + flute * 14 + panelBand;
      const r = clampByte(base * 0.94);
      const g = clampByte(base * 0.98);
      const b = clampByte(base * 1.05);
      const o = (y * w + x) * 4;
      out[o] = Math.max(24, r);
      out[o + 1] = Math.max(26, g);
      out[o + 2] = Math.max(30, b);
      out[o + 3] = 0x80;
    }
  }
  return out;
}

function buildRadialSprite(
  w: number,
  h: number,
  exponent: number,
  rgbTint: [number, number, number],
  format3WhiteAlphaOnly: boolean,
): Uint8Array {
  const out = new Uint8Array(w * h * 4);
  const cx = (w - 1) * 0.5;
  const cy = (h - 1) * 0.5;
  const invR = 1 / (w * 0.5);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const dx = (x - cx) * invR;
      const dy = (y - cy) * invR;
      const r = Math.min(1, Math.sqrt(dx * dx + dy * dy));
      const falloff = Math.pow(Math.max(0, 1 - r * r), exponent);
      const o = (y * w + x) * 4;
      if (format3WhiteAlphaOnly) {
        out[o] = 255;
        out[o + 1] = 255;
        out[o + 2] = 255;
        out[o + 3] = clampByte(falloff * 128);
      } else {
        out[o] = clampByte(rgbTint[0] * falloff);
        out[o + 1] = clampByte(rgbTint[1] * falloff);
        out[o + 2] = clampByte(rgbTint[2] * falloff);
        out[o + 3] = clampByte(falloff * 128);
      }
    }
  }
  return out;
}

function buildFlareTexture(): Uint8Array {
  const w = 128;
  const h = 128;
  const out = new Uint8Array(w * h * 4);
  const cx = (w - 1) * 0.5;
  const cy = (h - 1) * 0.5;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const dx = (x - cx) / 64;
      const dy = (y - cy) / 64;
      const r = Math.sqrt(dx * dx + dy * dy);
      const ang = Math.atan2(dy, dx);
      const core = Math.exp(-r * r * 12);
      const rays = Math.pow(Math.max(0, Math.cos(ang * 3)), 10) * Math.max(0, 1 - r);
      const ring = Math.exp(-Math.pow((r - 0.58) * 14, 2)) * 0.45;
      const val = Math.min(1, core + rays * 0.65 + ring);
      const lum = clampByte(val * 255);
      const o = (y * w + x) * 4;
      out[o] = lum;
      out[o + 1] = clampByte(lum * 0.86);
      out[o + 2] = clampByte(lum * 0.78);
      out[o + 3] = lum === 0 ? 0 : 0x7f;
    }
  }
  return out;
}

function buildReflectionMap(w: number, h: number, seed: number): Uint8Array {
  const out = new Uint8Array(w * h * 4);
  for (let y = 0; y < h; y++) {
    const ny = (y / (h - 1)) * 2 - 1;
    for (let x = 0; x < w; x++) {
      const nx = (x / (w - 1)) * 2 - 1;
      const r2 = nx * nx + ny * ny;
      const fresnel = Math.min(1, Math.pow(Math.min(1.2, Math.sqrt(r2)), 1.6));
      const specArc = Math.exp(-Math.pow((ny + 0.32 + nx * 0.25) * 4.2, 2));
      const horizon = Math.exp(-Math.pow(ny * 3.0, 2)) * 0.55;
      const shimmer = seamlessFbm(x / w, y / h, 4, 3, seed) * 0.22;
      const intensity = Math.min(1, 0.18 + fresnel * 0.38 + specArc * 0.62 + horizon + shimmer);
      const o = (y * w + x) * 4;
      out[o] = clampByte(intensity * 215);
      out[o + 1] = clampByte(intensity * 238);
      out[o + 2] = clampByte(intensity * 255);
      out[o + 3] = 0x7f;
    }
  }
  return out;
}

/** `format 4` (`opening.c:774`): RGB = 0, Alpha = 8-bit caustic mask for two-pass `Ad` specular. */
function buildFormat4CausticAlpha(w: number, h: number, seed: number): Uint8Array {
  const out = new Uint8Array(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const u = x / w;
      const v = y / h;
      const n1 = seamlessFbm(u, v, 4, 4, seed);
      const n2 = seamlessFbm(v + 0.37, u + 0.19, 6, 3, seed + 311);
      const ridge = 1 - Math.abs(n1 - n2) * 2.4;
      const alpha = clampByte(Math.pow(Math.max(0, ridge), 2.0) * 168);
      const o = (y * w + x) * 4;
      out[o] = 0;
      out[o + 1] = 0;
      out[o + 2] = 0;
      out[o + 3] = alpha;
    }
  }
  return out;
}

function buildIllegalMsgBanner(): Uint8Array {
  const w = 512;
  const h = 128;
  const out = new Uint8Array(w * h * 4);
  drawBitmapString(out, w, h, "PLEASE INSERT A", 112, 34, 3, 4, [240, 224, 224], 128);
  drawBitmapString(out, w, h, "SUPPORTED DISC.", 112, 72, 3, 4, [240, 224, 224], 128);
  return out;
}

/** Matches `DecodeKabe` (`menuback.c:61`): `64x64` tile replicated across all `4` quadrants of `128x128`. */
function buildMenuKabeTexture(seed: number): Uint8Array {
  const halfW = 64;
  const halfH = 64;
  const fullW = 128;
  const fullH = 128;
  const out = new Uint8Array(fullW * fullH * 4);

  for (let y = 0; y < halfH; y++) {
    const v = y / halfH;
    for (let x = 0; x < halfW; x++) {
      const u = x / halfW;
      const fbm = seamlessFbm(u, v, 4, 4, seed);
      const wave = 0.5 + 0.5 * Math.sin((u * 2 + v * 3 + fbm * 1.4) * Math.PI * 2);
      const lum = 48 + fbm * 135 + wave * 52;
      const r = clampByte(lum * 0.72);
      const g = clampByte(lum * 0.86);
      const b = clampByte(lum * 1.18);

      const coords: ReadonlyArray<[number, number]> = [
        [x, y],
        [x + halfW, y],
        [x, y + halfH],
        [x + halfW, y + halfH],
      ];
      for (const [qx, qy] of coords) {
        const o = (qy * fullW + qx) * 4;
        out[o] = r;
        out[o + 1] = g;
        out[o + 2] = b;
        out[o + 3] = 0x7f;
      }
    }
  }
  return out;
}

function buildGrayscale0x7f(w: number, h: number, fn: (u: number, v: number) => number): Uint8Array {
  const out = new Uint8Array(w * h * 4);
  for (let y = 0; y < h; y++) {
    const v = y / h;
    for (let x = 0; x < w; x++) {
      const u = x / w;
      const lum = clampByte(fn(u, v));
      const o = (y * w + x) * 4;
      out[o] = lum;
      out[o + 1] = lum;
      out[o + 2] = lum;
      out[o + 3] = 0x7f;
    }
  }
  return out;
}

/** Exact bitwise complement `~src` of `MenuBump` RGB (`menuconfig.c:245`). */
export function invertBumpTexture(bumpRgba: Uint8Array): Uint8Array {
  const out = new Uint8Array(bumpRgba.length);
  for (let i = 0; i < bumpRgba.length; i += 4) {
    const inv = (~(bumpRgba[i] ?? 0)) & 0xff;
    out[i] = inv;
    out[i + 1] = inv;
    out[i + 2] = inv;
    out[i + 3] = 0x7f;
  }
  return out;
}

/** Clean-room `64x64` (`2x2` quadrants of `32x32`) geometric hint icons (`menutext.c:1699`). */
function buildMenuMaruIcons(): Uint8Array {
  const w = 64;
  const h = 64;
  const out = new Uint8Array(w * h * 4);

  const put = (x: number, y: number, r: number, g: number, b: number, a: number): void => {
    if (x < 0 || x >= w || y < 0 || y >= h) return;
    const o = (y * w + x) * 4;
    out[o] = r;
    out[o + 1] = g;
    out[o + 2] = b;
    out[o + 3] = a;
  };

  // Top-left (0,0..31,31): Clean-room geometric box outline
  for (let y = 7; y <= 24; y++) {
    for (let x = 7; x <= 24; x++) {
      const edge = x === 7 || x === 8 || x === 23 || x === 24 || y === 7 || y === 8 || y === 23 || y === 24;
      if (edge) put(x, y, 225, 190, 235, 128);
    }
  }

  // Top-right (32,0..63,31): Clean-room upward chevron / delta outline
  for (let y = 7; y <= 24; y++) {
    const span = Math.round(((y - 7) / 17) * 9);
    for (let dx = -span; dx <= span; dx++) {
      const edge = Math.abs(dx) >= span - 1 || y >= 23;
      if (edge) put(48 + dx, y, 165, 235, 200, 128);
    }
  }

  // Bottom-left (0,32..31,63): Clean-room diagonal cross
  for (let i = -8; i <= 8; i++) {
    for (let t = -1; t <= 1; t++) {
      put(16 + i + t, 48 + i, 175, 205, 250, 128);
      put(16 + i + t, 48 - i, 175, 205, 250, 128);
    }
  }

  // Bottom-right (32,32..63,63): Clean-room circular ring
  for (let y = 32; y < 64; y++) {
    for (let x = 32; x < 64; x++) {
      const dist = Math.hypot(x - 47.5, y - 47.5);
      if (Math.abs(dist - 8.5) <= 1.35) {
        put(x, y, 245, 175, 175, 128);
      }
    }
  }

  return out;
}

function buildGlyphAtlas(): Uint8Array {
  const w = 256;
  const h = 128;
  const out = new Uint8Array(w * h * 4);
  // 16 columns x 8 rows of 16x16 cells covering ASCII 0x20..0x7F
  for (let code = 0x20; code < 0x80; code++) {
    const cell = code - 0x20;
    const col = cell % 16;
    const row = Math.floor(cell / 16);
    const ch = String.fromCharCode(code);
    drawBitmapString(out, w, h, ch, col * 16 + 3, row * 16 + 1, 2, 0, [255, 255, 255], 128);
  }
  return out;
}

export function synthesizeProceduralTextures(seed = 0x5053_3230): ProceduralTextureAsset[] {
  const bumpRgba = buildGrayscale0x7f(64, 64, (u, v) => {
    const n = seamlessFbm(u, v, 6, 4, seed + 1700);
    const bevel = 0.5 + 0.5 * Math.sin((u * 4 - v * 2) * Math.PI * 2);
    return 40 + n * 145 + bevel * 55;
  });
  const binvRgba = invertBumpTexture(bumpRgba);

  const specs: Array<{
    id: TextureSlotId;
    name: string;
    width: number;
    height: number;
    mipCount: number;
    base: Uint8Array;
  }> = [
    { id: TextureSlotId.Banner, name: "Banner", width: 256, height: 64, mipCount: 1, base: buildBannerTexture() },
    // TEXOFOG0 (group = 2 in opening.c:61) is exclusively Mode C IllegalDisc's crimson cloud texture!
    { id: TextureSlotId.Fog0, name: "TEXOFOG0", width: 128, height: 128, mipCount: 1, base: buildFogTexture(128, 128, seed + 101, [1.0, 0.2, 0.24]) },
    { id: TextureSlotId.Fog1, name: "TEXOFOG1", width: 64, height: 64, mipCount: 1, base: buildFogTexture(64, 64, seed + 202) },
    { id: TextureSlotId.Fog2, name: "TEXOFOG2", width: 64, height: 64, mipCount: 1, base: buildFogTexture(64, 64, seed + 303) },
    { id: TextureSlotId.Fog3, name: "TEXOFOG3", width: 64, height: 64, mipCount: 1, base: buildFogTexture(64, 64, seed + 404) },
    { id: TextureSlotId.Fog4, name: "TEXOFOG4", width: 64, height: 64, mipCount: 1, base: buildFogTexture(64, 64, seed + 505) },
    { id: TextureSlotId.Wal0, name: "TEXOWAL0", width: 256, height: 256, mipCount: 6, base: buildTowerWallTexture(seed + 606) },
    { id: TextureSlotId.Crle, name: "TEXOCRLE", width: 64, height: 64, mipCount: 1, base: buildRadialSprite(64, 64, 2.4, [245, 250, 255], false) },
    { id: TextureSlotId.Crbl, name: "TEXOCRBL", width: 64, height: 64, mipCount: 1, base: buildRadialSprite(64, 64, 1.5, [220, 238, 255], false) },
    { id: TextureSlotId.Flar, name: "TEXOFLAR", width: 128, height: 128, mipCount: 1, base: buildFlareTexture() },
    { id: TextureSlotId.Ref, name: "TEXOREF", width: 128, height: 128, mipCount: 1, base: buildReflectionMap(128, 128, seed + 1010) },
    { id: TextureSlotId.Blp, name: "TEXOBLP", width: 64, height: 64, mipCount: 1, base: buildFormat4CausticAlpha(64, 64, seed + 1111) },
    { id: TextureSlotId.Blpr, name: "TEXOBLPR", width: 64, height: 64, mipCount: 1, base: buildFormat4CausticAlpha(64, 64, seed + 1212) },
    { id: TextureSlotId.IllegalMsg, name: "IllegalMsg", width: 512, height: 128, mipCount: 1, base: buildIllegalMsgBanner() },
    { id: TextureSlotId.MenuKabe, name: "TEXCKABE", width: 128, height: 128, mipCount: 1, base: buildMenuKabeTexture(seed + 1414) },
    { id: TextureSlotId.MenuBlur, name: "TEXCBLUR", width: 64, height: 64, mipCount: 1, base: buildRadialSprite(64, 64, 1.8, [255, 255, 255], true) },
    { id: TextureSlotId.MenuNavi, name: "TEXCNAVI", width: 64, height: 64, mipCount: 1, base: buildRadialSprite(64, 64, 4.5, [255, 255, 255], true) },
    { id: TextureSlotId.MenuBump, name: "TEXCBUMP", width: 64, height: 64, mipCount: 1, base: bumpRgba },
    { id: TextureSlotId.MenuBinv, name: "TEXCBINV", width: 64, height: 64, mipCount: 1, base: binvRgba },
    {
      id: TextureSlotId.MenuFlow,
      name: "TEXCFLOW",
      width: 64,
      height: 64,
      mipCount: 1,
      base: buildGrayscale0x7f(64, 64, (u, v) => 45 + seamlessFbm(u, v, 4, 4, seed + 1919) * 180),
    },
    {
      id: TextureSlotId.MenuRefa,
      name: "TEXCREFA",
      width: 64,
      height: 64,
      mipCount: 1,
      base: buildGrayscale0x7f(64, 64, (u, v) => {
        const dx = u * 2 - 1;
        const dy = v * 2 - 1;
        const spec = Math.exp(-Math.pow((dy + 0.28 + dx * 0.2) * 3.8, 2));
        return 36 + spec * 195 + seamlessFbm(u, v, 4, 3, seed + 2020) * 24;
      }),
    },
    { id: TextureSlotId.MenuMaru, name: "TEXCMARU", width: 64, height: 64, mipCount: 1, base: buildMenuMaruIcons() },
    { id: TextureSlotId.GlyphAtlas, name: "GlyphAtlas", width: 256, height: 128, mipCount: 1, base: buildGlyphAtlas() },
  ];

  return specs.map((s) => {
    const mipLevels = buildRgbaMipChain(s.base, s.width, s.height, s.mipCount);
    return {
      id: s.id,
      name: s.name,
      width: s.width,
      height: s.height,
      mipLevels,
      provenance: "procedural-clean-room",
      fnv1aHash: computeFnv1aHex(s.base),
    };
  });
}
