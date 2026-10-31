/**
 * A small software rasterizer that consumes **the same command stream the WebGPU renderer
 * consumes**. It exists because this sandbox's Chromium destroys its WebGPU device after a few
 * frames, which makes pixel verification of the GPU path impossible here; this gives an
 * independent, deterministic check of the geometry, texture mapping and GS blend maths that the
 * Rust core emits — a different implementation from `src/renderer/frame-renderer.ts`, so the two
 * cross-check the command protocol rather than agreeing by construction.
 *
 * Supported: the command kinds the core actually emits (clear target, copy target to texture,
 * draw triangles), the GS blend modes, depth test/write modes, and CLAMP/REPEAT bilinear sampling.
 */

import {
  GsBlendMode,
  RenderCmdKind,
  RenderTargetId,
  SamplerWrap,
  TextureId,
  VERTEX_FLOATS,
  decodeCommands,
  type WasmCore,
} from '../../src/wasm/bridge';

export const W = 640;
export const H = 448;

export interface Surface {
  rgba: Float32Array; // linear 0..1, RGBA
  depth: Float32Array;
}

function newSurface(): Surface {
  const rgba = new Float32Array(W * H * 4);
  const depth = new Float32Array(W * H).fill(1);
  return { rgba, depth };
}

export class SoftwareRenderer {
  screen = newSurface();
  wb3 = newSurface();
  wb4 = newSurface();
  private textures = new Map<number, { w: number; h: number; data: Float32Array }>();

  constructor(core: WasmCore) {
    for (let id = 0; id < core.textureCount(); id += 1) {
      const { width, height, rgba } = core.texture(id);
      const data = new Float32Array(width * height * 4);
      for (let i = 0; i < data.length; i += 1) data[i] = (rgba[i] ?? 0) / 255;
      this.textures.set(id, { w: width, h: height, data });
    }
  }

  private surface(target: number): Surface {
    if (target === RenderTargetId.WorkBuf3) return this.wb3;
    if (target === RenderTargetId.WorkBuf4) return this.wb4;
    return this.screen;
  }

  private texture(id: number): { w: number; h: number; data: Float32Array } {
    const direct = this.textures.get(id);
    if (direct) return direct;
    const copyId =
      id === TextureId.ScreenCopy
        ? 'screen'
        : id === TextureId.WorkBuf3
          ? 'wb3'
          : id === TextureId.WorkBuf4
            ? 'wb4'
            : null;
    if (copyId) {
      // The offscreen copies are re-snapshotted from the target when the command arrives, so
      // reading the live target here is equivalent to sampling the copied texture.
      const surface = copyId === 'screen' ? this.screen : copyId === 'wb3' ? this.wb3 : this.wb4;
      return { w: W, h: H, data: surface.rgba };
    }
    return this.textures.get(TextureId.WhiteDummy)!;
  }

  private sample(
    id: number,
    u: number,
    v: number,
    wrap: SamplerWrap,
  ): [number, number, number, number] {
    const tex = this.texture(id);
    let uu = u;
    let vv = v;
    if (wrap === SamplerWrap.Repeat) {
      uu = uu - Math.floor(uu);
      vv = vv - Math.floor(vv);
    } else {
      uu = Math.min(1, Math.max(0, uu));
      vv = Math.min(1, Math.max(0, vv));
    }
    // Bilinear, matching the LINEAR sampler the GPU path uses.
    const x = uu * tex.w - 0.5;
    const y = vv * tex.h - 0.5;
    const x0 = Math.floor(x);
    const y0 = Math.floor(y);
    const fx = x - x0;
    const fy = y - y0;
    const fetch = (px: number, py: number): [number, number, number, number] => {
      let ix = px;
      let iy = py;
      if (wrap === SamplerWrap.Repeat) {
        ix = ((ix % tex.w) + tex.w) % tex.w;
        iy = ((iy % tex.h) + tex.h) % tex.h;
      } else {
        ix = Math.min(tex.w - 1, Math.max(0, ix));
        iy = Math.min(tex.h - 1, Math.max(0, iy));
      }
      const o = (iy * tex.w + ix) * 4;
      return [tex.data[o] ?? 0, tex.data[o + 1] ?? 0, tex.data[o + 2] ?? 0, tex.data[o + 3] ?? 0];
    };
    const c00 = fetch(x0, y0);
    const c10 = fetch(x0 + 1, y0);
    const c01 = fetch(x0, y0 + 1);
    const c11 = fetch(x0 + 1, y0 + 1);
    const out: [number, number, number, number] = [0, 0, 0, 0];
    for (let k = 0; k < 4; k += 1) {
      const a = (c00[k] ?? 0) * (1 - fx) + (c10[k] ?? 0) * fx;
      const b = (c01[k] ?? 0) * (1 - fx) + (c11[k] ?? 0) * fx;
      out[k] = a * (1 - fy) + b * fy;
    }
    return out;
  }

  render(core: WasmCore): void {
    const verts = core.vertexView();
    for (const cmd of decodeCommands(core.commandView())) {
      if (cmd.kind === RenderCmdKind.ClearTarget) {
        const surface = this.surface(cmd.target);
        const r = ((cmd.clearRgba8 & 0xff) / 255) | 0;
        const g = (((cmd.clearRgba8 >> 8) & 0xff) / 255) | 0;
        const b = (((cmd.clearRgba8 >> 16) & 0xff) / 255) | 0;
        const a = (((cmd.clearRgba8 >> 24) & 0xff) / 255) | 0;
        for (let i = 0; i < W * H; i += 1) {
          surface.rgba[i * 4] = r;
          surface.rgba[i * 4 + 1] = g;
          surface.rgba[i * 4 + 2] = b;
          surface.rgba[i * 4 + 3] = a;
        }
        surface.depth.fill(1);
        continue;
      }
      if (cmd.kind === RenderCmdKind.CopyTargetToTexture) {
        // Copies are resolved lazily by `texture()`; nothing to do for a CPU renderer whose
        // texture ids map onto the live surfaces.
        continue;
      }
      if (cmd.kind !== RenderCmdKind.DrawTriangles) continue;

      const surface = this.surface(cmd.target);
      const depthWrite = cmd.depthMode === 1 || cmd.depthMode === 2 || cmd.depthMode === 4;
      const compare = (z: number, existing: number): boolean => {
        switch (cmd.depthMode) {
          case 2:
          case 3:
            return z <= existing;
          case 4:
            return z >= existing;
          default:
            return true;
        }
      };

      for (let t = 0; t + 2 < cmd.vertexCount; t += 3) {
        const base = cmd.firstVertex + t;
        const p: Array<[number, number, number, number, number, number, number, number, number, number]> = [];
        for (let k = 0; k < 3; k += 1) {
          const o = (base + k) * VERTEX_FLOATS;
          p.push([
            verts[o] ?? 0,
            verts[o + 1] ?? 0,
            verts[o + 2] ?? 0,
            verts[o + 3] ?? 1,
            verts[o + 4] ?? 0,
            verts[o + 5] ?? 0,
            verts[o + 6] ?? 0,
            verts[o + 7] ?? 0,
            verts[o + 8] ?? 0,
            verts[o + 9] ?? 0,
          ]);
        }
        const [a, b, c] = p as [
          (typeof p)[number],
          (typeof p)[number],
          (typeof p)[number],
        ];
        const area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        if (Math.abs(area) < 1e-9) continue;

        const minX = Math.max(0, Math.floor(Math.min(a[0], b[0], c[0])));
        const maxX = Math.min(W - 1, Math.ceil(Math.max(a[0], b[0], c[0])));
        const minY = Math.max(0, Math.floor(Math.min(a[1], b[1], c[1])));
        const maxY = Math.min(H - 1, Math.ceil(Math.max(a[1], b[1], c[1])));
        if (maxX < minX || maxY < minY) continue;

        for (let y = minY; y <= maxY; y += 1) {
          const py = y + 0.5;
          for (let x = minX; x <= maxX; x += 1) {
            const px = x + 0.5;
            const w0 = ((b[0] - px) * (c[1] - py) - (b[1] - py) * (c[0] - px)) / area;
            const w1 = ((c[0] - px) * (a[1] - py) - (c[1] - py) * (a[0] - px)) / area;
            const w2 = 1 - w0 - w1;
            if (w0 < -1e-6 || w1 < -1e-6 || w2 < -1e-6) continue;

            const z = w0 * a[2] + w1 * b[2] + w2 * c[2];
            const idx = y * W + x;
            if (!compare(z, surface.depth[idx] ?? 1)) continue;

            const u = w0 * a[4] + w1 * b[4] + w2 * c[4];
            const v = w0 * a[5] + w1 * b[5] + w2 * c[5];
            const texel = this.sample(cmd.textureId, u, v, cmd.samplerWrap);

            const cr = w0 * a[6] + w1 * b[6] + w2 * c[6];
            const cg = w0 * a[7] + w1 * b[7] + w2 * c[7];
            const cb = w0 * a[8] + w1 * b[8] + w2 * c[8];
            const ca = w0 * a[9] + w1 * b[9] + w2 * c[9];

            // GS MODULATE; colours arrive as 0..2 with 0x80 = 1.0.
            let sr = texel[0] * cr;
            let sg = texel[1] * cg;
            let sb = texel[2] * cb;
            let sa = texel[3] * ca;
            sr = Math.min(1, sr);
            sg = Math.min(1, sg);
            sb = Math.min(1, sb);
            sa = Math.min(1, sa * 1);

            const dr = surface.rgba[idx * 4] ?? 0;
            const dg = surface.rgba[idx * 4 + 1] ?? 0;
            const db = surface.rgba[idx * 4 + 2] ?? 0;
            const da = surface.rgba[idx * 4 + 3] ?? 0;
            const fix = cmd.blendFix / 128;
            let or_: number;
            let og: number;
            let ob: number;
            let oa: number;
            switch (cmd.blendMode) {
              case GsBlendMode.Opaque:
                or_ = sr;
                og = sg;
                ob = sb;
                oa = sa;
                break;
              case GsBlendMode.Additive:
                or_ = sr * sa + dr;
                og = sg * sa + dg;
                ob = sb * sa + db;
                oa = sa + da;
                break;
              case GsBlendMode.Subtractive:
                or_ = Math.max(0, dr - sr * sa);
                og = Math.max(0, dg - sg * sa);
                ob = Math.max(0, db - sb * sa);
                oa = da;
                break;
              case GsBlendMode.FixedAdditive:
                or_ = sr * sa * fix + dr;
                og = sg * sa * fix + dg;
                ob = sb * sa * fix + db;
                oa = sa * fix + da;
                break;
              case GsBlendMode.DestAlphaAdditive:
                or_ = sr * da + dr;
                og = sg * da + dg;
                ob = sb * da + db;
                oa = da;
                break;
              case GsBlendMode.FixedBlend:
              case GsBlendMode.SourceAlpha:
              default: {
                const al = cmd.blendMode === GsBlendMode.FixedBlend ? sa * fix : sa;
                or_ = sr * al + dr * (1 - al);
                og = sg * al + dg * (1 - al);
                ob = sb * al + db * (1 - al);
                oa = al + da * (1 - al);
                break;
              }
            }
            surface.rgba[idx * 4] = Math.min(1, or_);
            surface.rgba[idx * 4 + 1] = Math.min(1, og);
            surface.rgba[idx * 4 + 2] = Math.min(1, ob);
            surface.rgba[idx * 4 + 3] = Math.min(1, oa);
            if (depthWrite) surface.depth[idx] = z;
          }
        }
      }
    }
  }
}

export interface ImageStats {
  coverage: number;
  meanLuma: number;
  red: number;
  green: number;
  blue: number;
  maxLuma: number;
  /** Mean luma of the central third of the frame. */
  centerLuma: number;
  /** Mean luma of the outer border band. */
  borderLuma: number;
}

export function statsOf(surface: Surface): ImageStats {
  let lit = 0;
  let luma = 0;
  let r = 0;
  let g = 0;
  let b = 0;
  let maxLuma = 0;
  let centerSum = 0;
  let centerCount = 0;
  let borderSum = 0;
  let borderCount = 0;
  const cx0 = W / 3;
  const cx1 = (W * 2) / 3;
  const cy0 = H / 3;
  const cy1 = (H * 2) / 3;
  for (let y = 0; y < H; y += 1) {
    for (let x = 0; x < W; x += 1) {
      const i = (y * W + x) * 4;
      const rr = surface.rgba[i] ?? 0;
      const gg = surface.rgba[i + 1] ?? 0;
      const bb = surface.rgba[i + 2] ?? 0;
      const l = 0.299 * rr + 0.587 * gg + 0.114 * bb;
      luma += l;
      r += rr;
      g += gg;
      b += bb;
      if (l > 0.08) lit += 1;
      if (l > maxLuma) maxLuma = l;
      if (x > cx0 && x < cx1 && y > cy0 && y < cy1) {
        centerSum += l;
        centerCount += 1;
      } else if (x < W * 0.08 || x > W * 0.92 || y < H * 0.08 || y > H * 0.92) {
        borderSum += l;
        borderCount += 1;
      }
    }
  }
  const n = W * H;
  return {
    coverage: lit / n,
    meanLuma: luma / n,
    red: r / n,
    green: g / n,
    blue: b / n,
    maxLuma,
    centerLuma: centerCount ? centerSum / centerCount : 0,
    borderLuma: borderCount ? borderSum / borderCount : 0,
  };
}

/** Minimal PNG encoder (8-bit RGBA, no filtering) so a rendered frame can be inspected by eye. */
export function encodePng(surface: Surface): Uint8Array {
  const raw = Buffer.alloc((W * 4 + 1) * H);
  for (let y = 0; y < H; y += 1) {
    raw[y * (W * 4 + 1)] = 0;
    for (let x = 0; x < W; x += 1) {
      const i = (y * W + x) * 4;
      const o = y * (W * 4 + 1) + 1 + x * 4;
      raw[o] = Math.round(Math.min(1, surface.rgba[i] ?? 0) * 255);
      raw[o + 1] = Math.round(Math.min(1, surface.rgba[i + 1] ?? 0) * 255);
      raw[o + 2] = Math.round(Math.min(1, surface.rgba[i + 2] ?? 0) * 255);
      raw[o + 3] = 255;
    }
  }
  const idat = deflate(raw);
  const chunks: Buffer[] = [Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])];
  const chunk = (type: string, data: Buffer): Buffer => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length, 0);
    const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(body) >>> 0, 0);
    return Buffer.concat([len, body, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(W, 0);
  ihdr.writeUInt32BE(H, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  chunks.push(chunk('IHDR', ihdr));
  chunks.push(chunk('IDAT', idat));
  chunks.push(chunk('IEND', Buffer.alloc(0)));
  return new Uint8Array(Buffer.concat(chunks));
}

// `zlib.deflateSync` is available in Node; importing lazily keeps the browser bundle untouched.
import { deflateSync as zlibDeflate } from 'node:zlib';
function deflate(data: Buffer): Buffer {
  return zlibDeflate(data);
}

let crcTable: Uint32Array | null = null;
function crc32(buf: Buffer): number {
  if (!crcTable) {
    crcTable = new Uint32Array(256);
    for (let n = 0; n < 256; n += 1) {
      let c = n;
      for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      crcTable[n] = c >>> 0;
    }
  }
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i += 1) {
    c = (crcTable[(c ^ buf[i]!) & 0xff] ?? 0) ^ (c >>> 8);
  }
  return (c ^ 0xffffffff) >>> 0;
}
