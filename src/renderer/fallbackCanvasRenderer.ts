import {
  BlendMode,
  DepthMode,
  type FrameTelemetrySnapshot,
  type OsdbitsWasmEngine,
  TargetId,
  TextureSlot,
} from '../wasm/bridge';
import type {
  AdapterDiagnostics,
  AspectMode,
  OsdbitsRenderer,
  PixelReadback,
  ViewportLayout,
} from './types';
import { computeViewportLayout } from './viewport';

interface CachedSlotInfo {
  width: number;
  height: number;
  meanR: number;
  meanG: number;
  meanB: number;
  meanA: number;
  pixels: Uint8Array;
  bitmapCanvas: HTMLCanvasElement | null;
}

export class FallbackCanvasRenderer implements OsdbitsRenderer {
  public readonly backend = 'canvas2d' as const;
  public readonly diagnostics: AdapterDiagnostics;

  private readonly canvas: HTMLCanvasElement;
  private readonly presentCtx: CanvasRenderingContext2D;
  private readonly offscreenCanvas: HTMLCanvasElement;
  private readonly offscreenCtx: CanvasRenderingContext2D;
  private readonly slots = new Map<TextureSlot, CachedSlotInfo>();

  private currentLayout: ViewportLayout;
  private uploadedGen = 0;
  private disposed = false;

  public constructor(canvas: HTMLCanvasElement, degradedReason: string) {
    const presentCtx = canvas.getContext('2d', { alpha: false });
    if (!presentCtx) {
      throw new Error('Failed to acquire 2D fallback canvas context.');
    }
    this.canvas = canvas;
    this.presentCtx = presentCtx;

    this.offscreenCanvas = canvas.ownerDocument.createElement('canvas');
    const offCtx = this.offscreenCanvas.getContext('2d', { willReadFrequently: true });
    if (!offCtx) {
      throw new Error('Failed to acquire offscreen 2D canvas context.');
    }
    this.offscreenCtx = offCtx;

    this.diagnostics = {
      backend: 'canvas2d',
      isFallbackAdapter: true,
      vendor: 'canvas2d-software',
      architecture: 'cpu-2d',
      description: 'Canvas 2D software compatibility renderer',
      wgslCompilationClean: false,
      degradedReason,
    };

    this.slots.set(TextureSlot.Untextured, {
      width: 1,
      height: 1,
      meanR: 1,
      meanG: 1,
      meanB: 1,
      meanA: 1,
      pixels: new Uint8Array([255, 255, 255, 255]),
      bitmapCanvas: null,
    });

    this.currentLayout = computeViewportLayout(
      canvas.clientWidth || 640,
      canvas.clientHeight || 480,
      1.0,
      0.75,
      '4:3',
    );
    this.applyLayout(this.currentLayout);
  }

  public get layout(): ViewportLayout {
    return this.currentLayout;
  }

  private applyLayout(layout: ViewportLayout): void {
    this.canvas.width = layout.canvasWidth;
    this.canvas.height = layout.canvasHeight;
    this.offscreenCanvas.width = layout.renderWidth;
    this.offscreenCanvas.height = layout.renderHeight;
  }

  public syncTextures(engine: OsdbitsWasmEngine): void {
    if (this.disposed) return;
    const gen = engine.textureGeneration;
    if (gen === this.uploadedGen) return;

    for (const tex of engine.getAllTextures()) {
      let sumR = 0;
      let sumG = 0;
      let sumB = 0;
      let sumA = 0;
      const n = Math.max(1, tex.width * tex.height);
      for (let i = 0; i < tex.pixels.length; i += 4) {
        sumR += tex.pixels[i] ?? 0;
        sumG += tex.pixels[i + 1] ?? 0;
        sumB += tex.pixels[i + 2] ?? 0;
        sumA += tex.pixels[i + 3] ?? 0;
      }

      let bitmapCanvas: HTMLCanvasElement | null = null;
      if (
        tex.slot === TextureSlot.UiGlyphAtlas ||
        tex.slot === TextureSlot.OrbCore ||
        tex.slot === TextureSlot.OrbHalo ||
        tex.slot === TextureSlot.LensFlare
      ) {
        const c = this.canvas.ownerDocument.createElement('canvas');
        c.width = tex.width;
        c.height = tex.height;
        const cctx = c.getContext('2d');
        if (cctx && typeof ImageData !== 'undefined') {
          const copy = new Uint8ClampedArray(tex.pixels);
          cctx.putImageData(new ImageData(copy, tex.width, tex.height), 0, 0);
          bitmapCanvas = c;
        }
      }

      this.slots.set(tex.slot, {
        width: tex.width,
        height: tex.height,
        meanR: sumR / (n * 255),
        meanG: sumG / (n * 255),
        meanB: sumB / (n * 255),
        meanA: sumA / (n * 255),
        pixels: tex.pixels.slice(),
        bitmapCanvas,
      });
    }
    this.uploadedGen = gen;
  }

  public resize(
    cssWidth: number,
    cssHeight: number,
    dpr: number,
    resolutionScale: number,
    aspectMode: AspectMode,
  ): ViewportLayout {
    const next = computeViewportLayout(
      cssWidth,
      cssHeight,
      dpr,
      Math.min(resolutionScale, 1.0),
      aspectMode,
    );
    const changed =
      next.canvasWidth !== this.currentLayout.canvasWidth ||
      next.canvasHeight !== this.currentLayout.canvasHeight ||
      next.renderWidth !== this.currentLayout.renderWidth ||
      next.renderHeight !== this.currentLayout.renderHeight;
    this.currentLayout = next;
    if (changed && !this.disposed) {
      this.applyLayout(next);
    }
    return next;
  }

  public renderFrame(
    engine: OsdbitsWasmEngine,
    telemetry: FrameTelemetrySnapshot,
  ): void {
    if (this.disposed) return;
    this.syncTextures(engine);

    const ctx = this.offscreenCtx;
    const w = this.currentLayout.renderWidth;
    const h = this.currentLayout.renderHeight;
    const [cr, cg, cb] = telemetry.clearColor;

    ctx.globalCompositeOperation = 'source-over';
    ctx.globalAlpha = 1.0;
    ctx.fillStyle = `rgb(${Math.round(cr * 255)}, ${Math.round(cg * 255)}, ${Math.round(cb * 255)})`;
    ctx.fillRect(0, 0, w, h);

    const vertBytes = engine.getVertexBytes();
    const f32 = new Float32Array(
      vertBytes.buffer,
      vertBytes.byteOffset,
      vertBytes.byteLength / 4,
    );
    const u8 = vertBytes;
    const u32 = new Uint32Array(
      vertBytes.buffer,
      vertBytes.byteOffset,
      vertBytes.byteLength / 4,
    );

    const batches = engine.getBatches();
    const glyphSlot = this.slots.get(TextureSlot.UiGlyphAtlas);

    for (const batch of batches) {
      if (batch.vertexCount < 3 || batch.targetId === TargetId.ExtraBloom) {
        continue;
      }

      if (
        batch.blendMode === BlendMode.Additive ||
        batch.blendMode === BlendMode.AdditiveSrc
      ) {
        ctx.globalCompositeOperation = 'lighter';
      } else {
        ctx.globalCompositeOperation = 'source-over';
      }

      // Fast crisp glyph quad path for UI text & PlayStation pad icons
      if (
        batch.textureId === TextureSlot.UiGlyphAtlas &&
        glyphSlot?.bitmapCanvas &&
        batch.vertexCount % 6 === 0
      ) {
        for (let v = batch.firstVertex; v < batch.firstVertex + batch.vertexCount; v += 6) {
          const f0 = v * 8;
          const f2 = (v + 2) * 8;
          const b0 = v * 32;
          const w0 = Math.max(1e-4, f32[f0 + 3] ?? 1);
          const w2 = Math.max(1e-4, f32[f2 + 3] ?? 1);
          const x0 = (((f32[f0] ?? 0) / w0) * 0.5 + 0.5) * w;
          const y0 = (0.5 - ((f32[f0 + 1] ?? 0) / w0) * 0.5) * h;
          const x1 = (((f32[f2] ?? 0) / w2) * 0.5 + 0.5) * w;
          const y1 = (0.5 - ((f32[f2 + 1] ?? 0) / w2) * 0.5) * h;
          const u0 = (f32[f0 + 4] ?? 0) * glyphSlot.width;
          const v0 = (f32[f0 + 5] ?? 0) * glyphSlot.height;
          const u1 = (f32[f2 + 4] ?? 0) * glyphSlot.width;
          const v1 = (f32[f2 + 5] ?? 0) * glyphSlot.height;
          const alpha = (u8[b0 + 27] ?? 255) / 255;
          const sw = Math.max(1, u1 - u0);
          const sh = Math.max(1, v1 - v0);
          const dw = x1 - x0;
          const dh = y1 - y0;
          if (dw > 0.25 && dh > 0.25 && alpha > 0.01) {
            ctx.globalAlpha = alpha;
            ctx.drawImage(glyphSlot.bitmapCanvas, u0, v0, sw, sh, x0, y0, dw, dh);
          }
        }
        ctx.globalAlpha = 1.0;
        continue;
      }

      const slotInfo = this.slots.get(batch.textureId);
      const texR =
        batch.textureId === TextureSlot.BackdropUntinted ||
        batch.textureId === TextureSlot.BackdropTinted
          ? 0.38
          : (slotInfo?.meanR ?? 1.0);
      const texG =
        batch.textureId === TextureSlot.BackdropUntinted ||
        batch.textureId === TextureSlot.BackdropTinted
          ? 0.52
          : (slotInfo?.meanG ?? 1.0);
      const texB =
        batch.textureId === TextureSlot.BackdropUntinted ||
        batch.textureId === TextureSlot.BackdropTinted
          ? 0.78
          : (slotInfo?.meanB ?? 1.0);
      const texA =
        batch.textureId === TextureSlot.Untextured
          ? 1.0
          : Math.min(1.0, (slotInfo?.meanA ?? 0.85) * 1.35);

      const triCount = Math.floor(batch.vertexCount / 3);
      const order: number[] = new Array<number>(triCount);
      for (let t = 0; t < triCount; t++) {
        order[t] = t;
      }

      if (batch.depthMode === DepthMode.ReadWrite && triCount > 1 && triCount <= 2048) {
        const depths = new Float32Array(triCount);
        for (let t = 0; t < triCount; t++) {
          const vBase = batch.firstVertex + t * 3;
          let zSum = 0;
          for (let k = 0; k < 3; k++) {
            const fo = (vBase + k) * 8;
            const wk = Math.max(1e-4, f32[fo + 3] ?? 1);
            zSum += (f32[fo + 2] ?? 0) / wk;
          }
          depths[t] = zSum;
        }
        order.sort((a, b) => (depths[b] ?? 0) - (depths[a] ?? 0));
      }

      for (const t of order) {
        const v0 = batch.firstVertex + t * 3;
        const f0 = v0 * 8;
        const f1 = (v0 + 1) * 8;
        const f2 = (v0 + 2) * 8;

        const w0 = f32[f0 + 3] ?? 1;
        const w1 = f32[f1 + 3] ?? 1;
        const w2 = f32[f2 + 3] ?? 1;
        if (w0 <= 1e-4 || w1 <= 1e-4 || w2 <= 1e-4) continue;

        const x0 = (((f32[f0] ?? 0) / w0) * 0.5 + 0.5) * w;
        const y0 = (0.5 - ((f32[f0 + 1] ?? 0) / w0) * 0.5) * h;
        const x1 = (((f32[f1] ?? 0) / w1) * 0.5 + 0.5) * w;
        const y1 = (0.5 - ((f32[f1 + 1] ?? 0) / w1) * 0.5) * h;
        const x2 = (((f32[f2] ?? 0) / w2) * 0.5 + 0.5) * w;
        const y2 = (0.5 - ((f32[f2 + 1] ?? 0) / w2) * 0.5) * h;

        const b0 = v0 * 32;
        const b1 = (v0 + 1) * 32;
        const b2 = (v0 + 2) * 32;
        const gain = ((u32[f0 + 7] ?? 0) & 1) !== 0 ? 2.0 : 1.0;

        const rAvg =
          (((u8[b0 + 24] ?? 0) + (u8[b1 + 24] ?? 0) + (u8[b2 + 24] ?? 0)) / (3 * 255)) *
          texR *
          gain;
        const gAvg =
          (((u8[b0 + 25] ?? 0) + (u8[b1 + 25] ?? 0) + (u8[b2 + 25] ?? 0)) / (3 * 255)) *
          texG *
          gain;
        const bAvg =
          (((u8[b0 + 26] ?? 0) + (u8[b1 + 26] ?? 0) + (u8[b2 + 26] ?? 0)) / (3 * 255)) *
          texB *
          gain;
        const aAvg =
          (((u8[b0 + 27] ?? 0) + (u8[b1 + 27] ?? 0) + (u8[b2 + 27] ?? 0)) / (3 * 255)) *
          texA;

        if (aAvg <= 0.008) continue;

        const rr = Math.min(255, Math.max(0, Math.round(rAvg * 255)));
        const gg = Math.min(255, Math.max(0, Math.round(gAvg * 255)));
        const bb = Math.min(255, Math.max(0, Math.round(bAvg * 255)));
        const aa = Math.min(1, Math.max(0, batch.blendMode === BlendMode.Opaque ? 1 : aAvg));

        ctx.fillStyle =
          batch.blendMode === BlendMode.Subtractive
            ? `rgba(0, 0, 0, ${(aa * 0.35).toFixed(3)})`
            : `rgba(${rr}, ${gg}, ${bb}, ${aa.toFixed(3)})`;
        ctx.beginPath();
        ctx.moveTo(x0, y0);
        ctx.lineTo(x1, y1);
        ctx.lineTo(x2, y2);
        ctx.closePath();
        ctx.fill();
      }
    }

    ctx.globalCompositeOperation = 'source-over';
    ctx.globalAlpha = 1.0;

    // Present onto visible canvas with aspect letterboxing/pillarboxing
    this.presentCtx.fillStyle = '#000000';
    this.presentCtx.fillRect(
      0,
      0,
      this.currentLayout.canvasWidth,
      this.currentLayout.canvasHeight,
    );
    this.presentCtx.drawImage(
      this.offscreenCanvas,
      0,
      0,
      w,
      h,
      this.currentLayout.viewportX,
      this.currentLayout.viewportY,
      this.currentLayout.viewportWidth,
      this.currentLayout.viewportHeight,
    );
  }

  public async readPixels(): Promise<PixelReadback> {
    const w = this.currentLayout.renderWidth;
    const h = this.currentLayout.renderHeight;
    const img = this.offscreenCtx.getImageData(0, 0, w, h);
    return {
      width: w,
      height: h,
      rgba: new Uint8Array(img.data.buffer, img.data.byteOffset, img.data.byteLength).slice(),
    };
  }

  public dispose(): void {
    this.disposed = true;
    this.slots.clear();
  }
}
