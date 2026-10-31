/**
 * Compatibility renderer (Canvas 2D).
 *
 * Used only when WebGPU is unavailable or fails to initialise. It consumes the
 * same draw list and the same procedural textures, so the reconstruction is
 * still recognisable — but it cannot reproduce the GS blend modes exactly, the
 * additive fog stack is drawn with `globalCompositeOperation = "lighter"`, and
 * textured triangles fall back to their average texel colour. Everything it
 * does differently is listed in docs/LIMITATIONS.md; it is never presented as
 * the faithful path.
 */

import { BLEND, PRIM, VERTEX_WORDS } from "./abi.js";
import type { Frame } from "./frame.js";
import { CUBE_FACE_NORMAL, CUBE_FACE_VERTS } from "./cubeGeometry.js";
import { textureDigest, type ProceduralTexture } from "./textures.js";
import type { RendererStats } from "./gpu/renderer.js";

interface Raster {
  canvas: HTMLCanvasElement;
  width: number;
  height: number;
  /** Average colour, used when a triangle samples a texture. */
  average: [number, number, number];
}

const MAX_TRIANGLES_PER_FRAME = 6000;

export class Canvas2dRenderer {
  private readonly ctx: CanvasRenderingContext2D;
  private readonly rasters = new Map<number, Raster>();
  readonly kind = "canvas2d" as const;
  readonly info = { kind: "canvas2d" as const };
  private trianglesDrawn = 0;

  constructor(
    private readonly canvas: HTMLCanvasElement,
    textures: readonly ProceduralTexture[],
    private readonly cubePasses: boolean,
  ) {
    const ctx = canvas.getContext("2d", { alpha: false });
    if (!ctx) throw new Error("Canvas 2D is not available");
    this.ctx = ctx;
    for (const t of textures) {
      const c = document.createElement("canvas");
      c.width = t.width;
      c.height = t.height;
      const cctx = c.getContext("2d");
      if (!cctx) continue;
      const image = cctx.createImageData(t.width, t.height);
      image.data.set(t.data);
      cctx.putImageData(image, 0, 0);
      let r = 0;
      let g = 0;
      let b = 0;
      let wsum = 0;
      for (let i = 0; i < t.data.length; i += 4) {
        const a = t.data[i + 3] / 255;
        r += t.data[i] * a;
        g += t.data[i + 1] * a;
        b += t.data[i + 2] * a;
        wsum += a;
      }
      const n = Math.max(1, wsum);
      this.rasters.set(t.id, {
        canvas: c,
        width: t.width,
        height: t.height,
        average: [r / n / 255, g / n / 255, b / n / 255],
      });
    }
    void textureDigest;
  }

  resize(width: number, height: number): void {
    if (this.canvas.width !== width || this.canvas.height !== height) {
      this.canvas.width = width;
      this.canvas.height = height;
    }
  }

  private project(frame: Frame, x: number, y: number, z: number): [number, number, number] | null {
    const v = frame.view;
    const vx = v[0] * x + v[1] * y + v[2] * z + v[3];
    const vy = v[4] * x + v[5] * y + v[6] * z + v[7];
    const vz = v[8] * x + v[9] * y + v[10] * z + v[11];
    if (vz <= 0.5) return null;
    const px = ((frame.ax * frame.scrz * vx) / vz + frame.width * 0.5) * this.scaleX(frame);
    const py = ((frame.ay * frame.scrz * vy) / vz + frame.height * 0.5) * this.scaleY(frame);
    return [px, py, vz];
  }

  private scaleX(frame: Frame): number {
    return this.canvas.width / frame.width;
  }

  private scaleY(frame: Frame): number {
    return this.canvas.height / frame.height;
  }

  draw(frame: Frame): RendererStats {
    const stats: RendererStats = { draws: 0, triangles: 0, batchesSkipped: 0, batchesDrawn: 0 };
    const ctx = this.ctx;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.globalAlpha = 1;
    ctx.globalCompositeOperation = "source-over";
    ctx.fillStyle = "#000";
    ctx.fillRect(0, 0, this.canvas.width, this.canvas.height);
    this.trianglesDrawn = 0;

    const drawBatch = (index: number): void => {
      const b = frame.batches[index];
      if (b.vertexCount === 0) return;
      switch (b.prim) {
        case PRIM.TRIANGLES:
          this.drawTriangles(frame, b.vertexOffset, b.vertexCount, b.tex, b.blend);
          break;
        case PRIM.SPRITES:
          this.drawSprites(frame, b.vertexOffset, b.vertexCount, b.tex, b.blend);
          break;
        case PRIM.SCREEN_SPRITES:
          this.drawScreenSprites(frame, b.vertexOffset, b.vertexCount, b.tex, b.blend);
          break;
        case PRIM.LINES:
          this.drawLines(frame, b.vertexOffset, b.vertexCount, b.blend);
          break;
        default:
          stats.batchesSkipped++;
          break;
      }
      stats.draws++;
      stats.batchesDrawn++;
    };

    for (let i = 0; i < frame.cubeAfterBatch; i++) drawBatch(i);
    if (this.cubePasses) this.drawCubes(frame, stats);
    for (let i = frame.cubeAfterBatch; i < frame.batches.length; i++) drawBatch(i);
    return stats;
  }

  private composite(blend: number): void {
    this.ctx.globalCompositeOperation =
      blend === BLEND.ADD || blend === BLEND.ADD_DEST_ALPHA || blend === BLEND.FIXALPHA
        ? "lighter"
        : "source-over";
  }

  destroy(): void {
    this.rasters.clear();
  }

  private drawTriangles(
    frame: Frame,
    offset: number,
    count: number,
    tex: number,
    blend: number,
  ): void {
    const ctx = this.ctx;
    this.composite(blend);
    const raster = this.rasters.get(tex);
    const w = frame.vertices;
    for (let i = 0; i + 2 < count; i += 3) {
      if (this.trianglesDrawn >= MAX_TRIANGLES_PER_FRAME) return;
      const idx = [offset + i, offset + i + 1, offset + i + 2].map((v) => v * VERTEX_WORDS);
      const pts: [number, number][] = [];
      let ok = true;
      for (const o of idx) {
        const p = this.project(frame, w[o], w[o + 1], w[o + 2]);
        if (!p) {
          ok = false;
          break;
        }
        pts.push([p[0], p[1]]);
      }
      if (!ok) continue;
      // Flat fill from the average vertex colour (times the texture average).
      let r = 0;
      let g = 0;
      let b = 0;
      let a = 0;
      for (const o of idx) {
        r += w[o + 5];
        g += w[o + 6];
        b += w[o + 7];
        a += w[o + 8];
      }
      r /= 3;
      g /= 3;
      b /= 3;
      a /= 3;
      if (raster) {
        r *= raster.average[0];
        g *= raster.average[1];
        b *= raster.average[2];
        a *= Math.max(0.15, raster.average[0] * 0.5 + raster.average[1] * 0.3 + raster.average[2] * 0.2);
      }
      if (a <= 0.01) continue;
      ctx.globalAlpha = Math.min(1, a);
      ctx.fillStyle = `rgb(${Math.min(255, r * 255) | 0},${Math.min(255, g * 255) | 0},${Math.min(255, b * 255) | 0})`;
      ctx.beginPath();
      ctx.moveTo(pts[0][0], pts[0][1]);
      ctx.lineTo(pts[1][0], pts[1][1]);
      ctx.lineTo(pts[2][0], pts[2][1]);
      ctx.closePath();
      ctx.fill();
      this.trianglesDrawn++;
    }
    ctx.globalCompositeOperation = "source-over";
    ctx.globalAlpha = 1;
  }

  private drawSprites(frame: Frame, offset: number, count: number, tex: number, blend: number): void {
    const ctx = this.ctx;
    this.composite(blend);
    const w = frame.vertices;
    const raster = this.rasters.get(tex);
    for (let i = 0; i < count; i++) {
      const o = (offset + i) * VERTEX_WORDS;
      const p = this.project(frame, w[o], w[o + 1], w[o + 2]);
      if (!p) continue;
      const halfW = (frame.ax * frame.scrz * w[o + 3]) / p[2] * this.scaleX(frame);
      const halfH = (frame.ay * frame.scrz * w[o + 4]) / p[2] * this.scaleY(frame);
      if (halfW < 0.5 || halfH < 0.5) continue;
      ctx.globalAlpha = Math.min(1, Math.max(0, w[o + 8]));
      if (raster) {
        ctx.drawImage(raster.canvas, p[0] - halfW, p[1] - halfH, halfW * 2, halfH * 2);
      } else {
        ctx.fillStyle = `rgb(${(w[o + 5] * 255) | 0},${(w[o + 6] * 255) | 0},${(w[o + 7] * 255) | 0})`;
        ctx.fillRect(p[0] - halfW, p[1] - halfH, halfW * 2, halfH * 2);
      }
    }
    ctx.globalCompositeOperation = "source-over";
    ctx.globalAlpha = 1;
  }

  private drawScreenSprites(
    frame: Frame,
    offset: number,
    count: number,
    tex: number,
    blend: number,
  ): void {
    const ctx = this.ctx;
    this.composite(blend);
    const w = frame.vertices;
    const raster = this.rasters.get(tex);
    const sx = this.scaleX(frame);
    const sy = this.scaleY(frame);
    for (let i = 0; i < count; i++) {
      const o = (offset + i) * VERTEX_WORDS;
      const cw = w[o + 3] * sx;
      const ch = w[o + 4] * sy;
      const x = w[o] * sx - cw * 0.5;
      const y = w[o + 1] * sy - ch * 0.5;
      ctx.globalAlpha = Math.min(1, Math.max(0, w[o + 8]));
      if (raster) {
        ctx.drawImage(raster.canvas, x, y, cw, ch);
      } else {
        ctx.fillStyle = `rgb(${(w[o + 5] * 255) | 0},${(w[o + 6] * 255) | 0},${(w[o + 7] * 255) | 0})`;
        ctx.fillRect(x, y, cw, ch);
      }
    }
    ctx.globalCompositeOperation = "source-over";
    ctx.globalAlpha = 1;
  }

  private drawLines(frame: Frame, offset: number, count: number, blend: number): void {
    const ctx = this.ctx;
    this.composite(blend);
    const w = frame.vertices;
    let prev: [number, number] | null = null;
    for (let i = 0; i < count; i++) {
      const o = (offset + i) * VERTEX_WORDS;
      const p = this.project(frame, w[o], w[o + 1], w[o + 2]);
      if (!p) {
        prev = null;
        continue;
      }
      if (prev) {
        ctx.globalAlpha = Math.min(1, Math.max(0, w[o + 8]));
        ctx.strokeStyle = `rgb(${(w[o + 5] * 255) | 0},${(w[o + 6] * 255) | 0},${(w[o + 7] * 255) | 0})`;
        ctx.lineWidth = Math.max(1, w[o + 3] * 2);
        ctx.beginPath();
        ctx.moveTo(prev[0], prev[1]);
        ctx.lineTo(p[0], p[1]);
        ctx.stroke();
      }
      prev = [p[0], p[1]];
    }
    ctx.globalCompositeOperation = "source-over";
    ctx.globalAlpha = 1;
  }

  private drawCubes(frame: Frame, stats: RendererStats): void {
    const ctx = this.ctx;
    for (let c = 0; c < frame.stats.cubes; c++) {
      const base = c * 128;
      const cube = frame.cubes.subarray(base, base + 128);
      const colour: [number, number, number] = [cube[8], cube[9], cube[10]];
      for (let f = 0; f < 6; f++) {
        if (cube[2 + f] < 0) continue;
        const verts = CUBE_FACE_VERTS[f];
        const pts: [number, number][] = [];
        for (const vi of verts) {
          const p = this.project(
            frame,
            cube[21 + vi * 4],
            cube[22 + vi * 4],
            cube[23 + vi * 4],
          );
          if (!p) {
            pts.length = 0;
            break;
          }
          pts.push([p[0], p[1]]);
        }
        if (pts.length !== 4) continue;
        const light = cube[101 + f * 4] + 0.35;
        ctx.globalCompositeOperation = "lighter";
        ctx.globalAlpha = 0.45;
        ctx.fillStyle = `rgb(${Math.min(255, colour[0] * light) | 0},${Math.min(255, colour[1] * light) | 0},${Math.min(255, colour[2] * light) | 0})`;
        ctx.beginPath();
        ctx.moveTo(pts[0][0], pts[0][1]);
        for (const p of pts.slice(1)) ctx.lineTo(p[0], p[1]);
        ctx.closePath();
        ctx.fill();
        void CUBE_FACE_NORMAL;
        stats.triangles += 2;
        stats.draws++;
      }
    }
    ctx.globalCompositeOperation = "source-over";
    ctx.globalAlpha = 1;
  }
}
