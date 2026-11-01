/**
 * Reduced-fidelity Canvas 2D fallback for browsers without a usable WebGPU adapter.
 *
 * The Rust/WASM core already emits backend-neutral clip-space vertices and draw commands.
 * This renderer intentionally supports only the inexpensive subset of that stream: flat
 * triangle/line colors (modulated by average texture color), fades, tinting, letterboxing,
 * and the clean-room banner. Multi-pass capture, refraction, feedback, and blur commands
 * are skipped rather than approximated as full WebGPU parity.
 */

import {
  PROCEDURAL_SLOT_COUNT,
  synthesizeProceduralTextures,
  type ProceduralTextureAsset,
} from "../assets/proceduralTextures";
import {
  DrawCommandKindId,
  EXPECTED_CMD_WORDS,
  EXPECTED_VERTEX_FLOATS,
  decodePipelineFlags,
  type VisualModeName,
} from "../wasm/bridge";
import {
  computeAspectViewport,
  type AspectPresentationMode,
  type RendererResourceStats,
} from "./webgpuRenderer";
import type { UiOverlayFrame } from "../ui/pixelRaster";

let uiLayerCanvas: HTMLCanvasElement | null = null;
let uiLayerContext: CanvasRenderingContext2D | null = null;

/**
 * Draws an in-app UI raster over the full target canvas with nearest-neighbour scaling
 * and source-over alpha. Shared by the Canvas 2D renderer and the no-GPU title screen.
 */
export function compositeUiRasterOnCanvas2D(target: CanvasRenderingContext2D, ui: UiOverlayFrame): void {
  if (ui.width <= 0 || ui.height <= 0 || ui.rgba.length < ui.width * ui.height * 4) return;
  if (!uiLayerCanvas || uiLayerCanvas.width !== ui.width || uiLayerCanvas.height !== ui.height) {
    uiLayerCanvas = document.createElement("canvas");
    uiLayerCanvas.width = ui.width;
    uiLayerCanvas.height = ui.height;
    uiLayerContext = uiLayerCanvas.getContext("2d");
  }
  if (!uiLayerContext || !uiLayerCanvas) return;
  uiLayerContext.putImageData(new ImageData(new Uint8ClampedArray(ui.rgba), ui.width, ui.height), 0, 0);

  const canvas = target.canvas;
  target.save();
  target.setTransform(1, 0, 0, 1, 0, 0);
  target.globalAlpha = 1;
  target.globalCompositeOperation = "source-over";
  target.imageSmoothingEnabled = false;
  target.drawImage(uiLayerCanvas, 0, 0, canvas.width, canvas.height);
  target.restore();
}

export interface Canvas2DRendererOptions {
  canvas: HTMLCanvasElement;
  aspectMode?: AspectPresentationMode;
  resolutionScale?: number;
  maxDevicePixelRatio?: number;
  initialTextures?: ProceduralTextureAsset[];
}

interface TextureTint {
  r: number;
  g: number;
  b: number;
  a: number;
}

const WHITE_TINT: TextureTint = { r: 1, g: 1, b: 1, a: 1 };

function clamp01(value: number): number {
  return Math.max(0, Math.min(1, value));
}

function averageTextureTint(asset: ProceduralTextureAsset): TextureTint {
  const pixels = asset.mipLevels[0];
  if (!pixels || pixels.length < 4) return WHITE_TINT;

  // Sampling keeps imported, high-resolution textures inexpensive to summarize.
  const pixelCount = Math.floor(pixels.length / 4);
  const stride = Math.max(1, Math.floor(pixelCount / 4096));
  let r = 0;
  let g = 0;
  let b = 0;
  let a = 0;
  let samples = 0;
  for (let pixel = 0; pixel < pixelCount; pixel += stride) {
    const offset = pixel * 4;
    r += pixels[offset] ?? 0;
    g += pixels[offset + 1] ?? 0;
    b += pixels[offset + 2] ?? 0;
    a += pixels[offset + 3] ?? 0;
    samples++;
  }
  const divisor = Math.max(1, samples) * 255;
  return { r: r / divisor, g: g / divisor, b: b / divisor, a: a / divisor };
}

function uint32BitsToFloat32(bits: number): number {
  const buffer = new ArrayBuffer(4);
  const view = new DataView(buffer);
  view.setUint32(0, bits >>> 0, true);
  return view.getFloat32(0, true);
}

export class Canvas2DRenderer {
  readonly backend = "canvas2d" as const;

  private readonly canvas: HTMLCanvasElement;
  private readonly context: CanvasRenderingContext2D;
  private aspectMode: AspectPresentationMode;
  private resolutionScale: number;
  private readonly maxDpr: number;
  private textureTints = new Map<number, TextureTint>();
  private disposed = false;

  private constructor(
    canvas: HTMLCanvasElement,
    context: CanvasRenderingContext2D,
    options: Canvas2DRendererOptions,
  ) {
    this.canvas = canvas;
    this.context = context;
    this.aspectMode = options.aspectMode ?? "4:3";
    this.resolutionScale = Math.max(0.5, Math.min(2, options.resolutionScale ?? 1));
    this.maxDpr = Math.max(1, Math.min(2, options.maxDevicePixelRatio ?? 1));
    this.updateTextures(options.initialTextures ?? synthesizeProceduralTextures());
  }

  static create(options: Canvas2DRendererOptions): Canvas2DRenderer {
    const context = options.canvas.getContext("2d", { alpha: false });
    if (!context) {
      throw new Error("Canvas 2D is not available in this browser");
    }
    return new Canvas2DRenderer(options.canvas, context, options);
  }

  setDisplayConfig(options: {
    aspectMode?: AspectPresentationMode;
    resolutionScale?: number;
    pal?: boolean;
  }): void {
    if (options.aspectMode) this.aspectMode = options.aspectMode;
    if (options.resolutionScale !== undefined) {
      this.resolutionScale = Math.max(0.5, Math.min(2, options.resolutionScale));
    }
  }

  updateTextures(assets: ProceduralTextureAsset[]): void {
    if (assets.length < PROCEDURAL_SLOT_COUNT) {
      throw new Error(`Expected ${PROCEDURAL_SLOT_COUNT} texture slots, got ${assets.length}`);
    }
    this.textureTints.clear();
    for (const asset of assets) {
      this.textureTints.set(asset.id, averageTextureTint(asset));
    }
  }

  syncCanvasSize(cssWidth?: number, cssHeight?: number): void {
    if (this.disposed) return;
    const dpr = Math.min(
      this.maxDpr,
      typeof window !== "undefined" && window.devicePixelRatio ? window.devicePixelRatio : 1,
    );
    const scale = dpr * this.resolutionScale;
    const width = Math.max(
      1,
      Math.floor((cssWidth ?? (this.canvas.clientWidth || 640)) * scale),
    );
    const height = Math.max(
      1,
      Math.floor((cssHeight ?? (this.canvas.clientHeight || 480)) * scale),
    );
    if (this.canvas.width !== width) this.canvas.width = width;
    if (this.canvas.height !== height) this.canvas.height = height;
  }

  /**
   * Draws one scene frame. The optional in-app UI raster is composited last with smoothing
   * disabled so the pixel font keeps its hard edges.
   */
  renderFrame(
    vertexFloats: Float32Array,
    commandWords: Uint32Array,
    _mode: VisualModeName,
    uiOverlay?: UiOverlayFrame,
  ): void {
    if (this.disposed) return;

    const viewport = computeAspectViewport(
      this.canvas.width,
      this.canvas.height,
      this.aspectMode,
    );
    const ctx = this.context;
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.globalAlpha = 1;
    ctx.globalCompositeOperation = "source-over";
    ctx.fillStyle = "rgb(0, 0, 0)";
    ctx.fillRect(0, 0, this.canvas.width, this.canvas.height);
    ctx.restore();

    const commandCount = Math.floor(commandWords.length / EXPECTED_CMD_WORDS);
    for (let index = 0; index < commandCount; index++) {
      const base = index * EXPECTED_CMD_WORDS;
      const kind = commandWords[base] as DrawCommandKindId;
      const target = commandWords[base + 1] ?? 0;
      const textureId = commandWords[base + 2] ?? 255;
      const packedFlags = commandWords[base + 3] ?? 0;
      const firstVertex = commandWords[base + 4] ?? 0;
      const vertexCount = commandWords[base + 5] ?? 0;
      const aux0 = commandWords[base + 6] ?? 0;

      switch (kind) {
        case DrawCommandKindId.Triangles:
          this.drawTriangles(
            vertexFloats,
            firstVertex,
            vertexCount,
            target,
            textureId,
            packedFlags,
            viewport,
          );
          break;
        case DrawCommandKindId.Lines:
          this.drawLines(
            vertexFloats,
            firstVertex,
            vertexCount,
            target,
            packedFlags,
            viewport,
          );
          break;
        case DrawCommandKindId.ApplyMenuBackdropTint:
          this.applyBackdropTint(aux0, viewport);
          break;
        case DrawCommandKindId.DrawFullScreenFade:
          this.drawFade(aux0, viewport);
          break;
        case DrawCommandKindId.DrawBannerOverlay:
          this.drawBanner(aux0, viewport);
          break;
        case DrawCommandKindId.DrawLetterboxBars:
          this.drawLetterbox(aux0, viewport);
          break;
        case DrawCommandKindId.CaptureScreenToWork:
        case DrawCommandKindId.ApplyExtraBufFeedback:
        case DrawCommandKindId.ApplyMotionBlur:
        case DrawCommandKindId.ApplyMenuZoomBlur:
        case DrawCommandKindId.DrawIllegalTextOverlay:
          // Localized illegal-disc text is drawn by the in-app UI layer; GPU-only multi-pass effects are omitted.
          break;
      }
    }

    if (uiOverlay) {
      this.compositeUiOverlay(uiOverlay);
    }
  }

  /** Scales the in-app UI raster over the whole canvas with nearest-neighbour sampling. */
  private compositeUiOverlay(ui: UiOverlayFrame): void {
    compositeUiRasterOnCanvas2D(this.context, ui);
  }

  async captureScenePixels(): Promise<{ width: number; height: number; rgba: Uint8Array }> {
    const width = this.canvas.width;
    const height = this.canvas.height;
    const pixels = this.context.getImageData(0, 0, width, height).data;
    const rgba = new Uint8Array(width * height * 4);
    rgba.set(pixels);
    return { width, height, rgba };
  }

  getResourceStats(): RendererResourceStats {
    return {
      createdTextures: 0,
      destroyedTextures: 0,
      activeRenderTargetWidth: this.canvas.width,
      activeRenderTargetHeight: this.canvas.height,
      deviceLostRecoveries: 0,
      pipelineCount: 0,
    };
  }

  /** Canvas 2D has no GPU device-loss lifecycle; recovery simply verifies the live context. */
  async simulateDeviceLossAndRecover(): Promise<void> {
    if (this.disposed || this.context.isContextLost?.() === true) {
      throw new Error("Canvas 2D context is unavailable");
    }
  }

  dispose(): void {
    this.disposed = true;
    this.textureTints.clear();
  }

  private drawTriangles(
    vertices: Float32Array,
    firstVertex: number,
    vertexCount: number,
    target: number,
    textureId: number,
    packedFlags: number,
    viewport: { x: number; y: number; width: number; height: number },
  ): void {
    // Work-buffer captures/refraction passes have no Canvas 2D equivalent.
    if (target !== 0) return;
    const flags = decodePipelineFlags(packedFlags);
    const points = new Float64Array(6);
    const ctx = this.context;
    ctx.save();
    ctx.globalCompositeOperation =
      flags.abe && (flags.blendMode === 0 || flags.blendMode === 3 || flags.blendMode === 5)
        ? "lighter"
        : "source-over";

    const availableVertices = Math.floor(vertices.length / EXPECTED_VERTEX_FLOATS);
    const end = Math.min(availableVertices, firstVertex + vertexCount);
    for (let first = firstVertex; first + 2 < end; first += 3) {
      if (!this.projectTriangle(vertices, first, viewport, points)) continue;
      this.setGeometryStyle(vertices, first, 3, textureId, flags.abe);
      ctx.beginPath();
      ctx.moveTo(points[0] ?? 0, points[1] ?? 0);
      ctx.lineTo(points[2] ?? 0, points[3] ?? 0);
      ctx.lineTo(points[4] ?? 0, points[5] ?? 0);
      ctx.closePath();
      ctx.fill();
    }
    ctx.restore();
  }

  private drawLines(
    vertices: Float32Array,
    firstVertex: number,
    vertexCount: number,
    target: number,
    packedFlags: number,
    viewport: { x: number; y: number; width: number; height: number },
  ): void {
    if (target !== 0) return;
    const flags = decodePipelineFlags(packedFlags);
    const points = new Float64Array(4);
    const ctx = this.context;
    ctx.save();
    ctx.globalCompositeOperation =
      flags.abe && (flags.blendMode === 0 || flags.blendMode === 3 || flags.blendMode === 5)
        ? "lighter"
        : "source-over";
    ctx.lineCap = "round";
    ctx.lineWidth = Math.max(1, viewport.width / 640);

    const availableVertices = Math.floor(vertices.length / EXPECTED_VERTEX_FLOATS);
    const end = Math.min(availableVertices, firstVertex + vertexCount);
    for (let first = firstVertex; first + 1 < end; first += 2) {
      if (
        !this.projectVertex(vertices, first, viewport, points, 0) ||
        !this.projectVertex(vertices, first + 1, viewport, points, 2)
      ) {
        continue;
      }
      this.setGeometryStyle(vertices, first, 2, 255, flags.abe);
      ctx.beginPath();
      ctx.moveTo(points[0] ?? 0, points[1] ?? 0);
      ctx.lineTo(points[2] ?? 0, points[3] ?? 0);
      ctx.stroke();
    }
    ctx.restore();
  }

  private projectTriangle(
    vertices: Float32Array,
    first: number,
    viewport: { x: number; y: number; width: number; height: number },
    output: Float64Array,
  ): boolean {
    return (
      this.projectVertex(vertices, first, viewport, output, 0) &&
      this.projectVertex(vertices, first + 1, viewport, output, 2) &&
      this.projectVertex(vertices, first + 2, viewport, output, 4)
    );
  }

  private projectVertex(
    vertices: Float32Array,
    vertex: number,
    viewport: { x: number; y: number; width: number; height: number },
    output: Float64Array,
    outputOffset: number,
  ): boolean {
    const base = vertex * EXPECTED_VERTEX_FLOATS;
    const clipX = vertices[base] ?? Number.NaN;
    const clipY = vertices[base + 1] ?? Number.NaN;
    const clipW = vertices[base + 3] ?? Number.NaN;
    if (!Number.isFinite(clipX) || !Number.isFinite(clipY) || !(clipW > 0.000001)) {
      return false;
    }
    const x = viewport.x + ((clipX / clipW + 1) * 0.5) * viewport.width;
    const y = viewport.y + ((1 - clipY / clipW) * 0.5) * viewport.height;
    if (!Number.isFinite(x) || !Number.isFinite(y)) return false;
    output[outputOffset] = x;
    output[outputOffset + 1] = y;
    return true;
  }

  private setGeometryStyle(
    vertices: Float32Array,
    first: number,
    count: number,
    textureId: number,
    blended: boolean,
  ): void {
    let r = 0;
    let g = 0;
    let b = 0;
    let a = 0;
    for (let index = 0; index < count; index++) {
      const colorBase = (first + index) * EXPECTED_VERTEX_FLOATS + 8;
      r += vertices[colorBase] ?? 0;
      g += vertices[colorBase + 1] ?? 0;
      b += vertices[colorBase + 2] ?? 0;
      a += vertices[colorBase + 3] ?? 0;
    }
    const tint = this.textureTints.get(textureId) ?? WHITE_TINT;
    const divisor = count * 128;
    const red = Math.round(clamp01((r / divisor) * tint.r) * 255);
    const green = Math.round(clamp01((g / divisor) * tint.g) * 255);
    const blue = Math.round(clamp01((b / divisor) * tint.b) * 255);
    this.context.fillStyle = `rgb(${red}, ${green}, ${blue})`;
    this.context.strokeStyle = this.context.fillStyle;
    this.context.globalAlpha = blended ? clamp01((a / divisor) * tint.a) : 1;
  }

  private applyBackdropTint(
    packed: number,
    viewport: { x: number; y: number; width: number; height: number },
  ): void {
    // WebGPU multiplies by GS colors where 128 is unity; reproduce that inexpensive part.
    const r = Math.round((((packed >>> 24) & 0xff) / 128) * 255);
    const g = Math.round((((packed >>> 16) & 0xff) / 128) * 255);
    const b = Math.round((((packed >>> 8) & 0xff) / 128) * 255);
    const ctx = this.context;
    ctx.save();
    ctx.globalCompositeOperation = "multiply";
    ctx.fillStyle = `rgb(${Math.min(255, r)}, ${Math.min(255, g)}, ${Math.min(255, b)})`;
    ctx.fillRect(viewport.x, viewport.y, viewport.width, viewport.height);
    ctx.restore();
  }

  private drawFade(
    packed: number,
    viewport: { x: number; y: number; width: number; height: number },
  ): void {
    const ctx = this.context;
    ctx.save();
    ctx.globalAlpha = clamp01((packed & 0xff) / 128);
    ctx.fillStyle = `rgb(${(packed >>> 24) & 0xff}, ${(packed >>> 16) & 0xff}, ${(packed >>> 8) & 0xff})`;
    ctx.fillRect(viewport.x, viewport.y, viewport.width, viewport.height);
    ctx.restore();
  }

  private drawBanner(
    packedAlpha: number,
    viewport: { x: number; y: number; width: number; height: number },
  ): void {
    const ctx = this.context;
    ctx.save();
    ctx.globalAlpha = clamp01((packedAlpha & 0xff) / 128);
    ctx.fillStyle = "rgb(230, 239, 253)";
    ctx.font = `300 ${Math.max(14, viewport.height * 0.055)}px Segoe UI, sans-serif`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(
      "O S D B I T S",
      viewport.x + viewport.width / 2,
      viewport.y + viewport.height / 2,
    );
    ctx.restore();
  }

  private drawLetterbox(
    packedRatio: number,
    viewport: { x: number; y: number; width: number; height: number },
  ): void {
    const ratio = Math.max(0, Math.min(0.45, uint32BitsToFloat32(packedRatio)));
    const barHeight = ratio * viewport.height;
    const ctx = this.context;
    ctx.save();
    ctx.globalAlpha = 1;
    ctx.fillStyle = "rgb(0, 0, 0)";
    ctx.fillRect(viewport.x, viewport.y, viewport.width, barHeight);
    ctx.fillRect(
      viewport.x,
      viewport.y + viewport.height - barHeight,
      viewport.width,
      barHeight,
    );
    ctx.restore();
  }
}
