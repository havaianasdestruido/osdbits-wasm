/**
 * PS2-GS-Faithful Multi-Pass WebGPU Renderer (`P05-001`..`P05-003`, `P06-001`..`P06-002`).
 *
 * Single Documented Rendering Ownership Model (`RENDERING_OWNERSHIP_CONTRACT`):
 * - `osdbits-core` / `osdbits-wasm` (Rust -> WASM) owns 100% of deterministic timeline & state
 *   machines, PRNG streams, VU1 tower lighting, camera/view-screen projection, guard-band culling,
 *   painter's/bucket sorting, and flat `12-float` vertex + `8-word` command buffer generation.
 * - `WebGpuRenderer` (TypeScript + 7 WGSL modules) owns `GPUAdapter`/`GPUDevice` negotiation,
 *   procedural/imported `GPUTexture` upload (including 6-level `WAL0` mipmaps), offscreen ping-pong
 *   render targets (`sceneColor`, `scratchColor`, `capturedScreen`, `workBuf`, `extraBufA/B`, `depthTex`),
 *   PS2 GS `BlendModes[0..10]` (`0x80 = 1.0` overbright & destination-alpha `Ad`), reverse-Z `[0, 1]`,
 *   aspect-ratio framing, VRAM cleanup on resize, and `device.lost` recovery without full page reload.
 */

import {
  PROCEDURAL_SLOT_COUNT,
  synthesizeProceduralTextures,
  type ProceduralTextureAsset,
} from "../assets/proceduralTextures";
import type { UiOverlayFrame } from "../ui/pixelRaster";
import {
  DrawCommandKindId,
  EXPECTED_CMD_WORDS,
  EXPECTED_VERTEX_FLOATS,
  decodePipelineFlags,
  type DecodedDrawCommand,
  type VisualModeName,
} from "../wasm/bridge";
import { SHADER_SOURCES, type ShaderModuleName } from "./shaders";

export const RENDERING_OWNERSHIP_CONTRACT = {
  simulationOwner: "rust-wasm-osdbits-core",
  rasterizationOwner: "typescript-webgpu-renderer",
  vertexStrideFloats: EXPECTED_VERTEX_FLOATS,
  commandStrideWords: EXPECTED_CMD_WORDS,
  depthConvention: "reverse-z-0-to-1-greater-equal",
  ps2UnityAlpha: 128,
} as const;

export type AspectPresentationMode = "4:3" | "16:9" | "fit";

export interface RendererOptions {
  canvas: HTMLCanvasElement;
  aspectMode?: AspectPresentationMode;
  resolutionScale?: number;
  maxDevicePixelRatio?: number;
  initialTextures?: ProceduralTextureAsset[];
  onDeviceLost?: (info: { reason: string; message: string }) => void;
  onDeviceRecovered?: () => void;
}

export interface RendererResourceStats {
  createdTextures: number;
  destroyedTextures: number;
  activeRenderTargetWidth: number;
  activeRenderTargetHeight: number;
  deviceLostRecoveries: number;
  pipelineCount: number;
}

interface RenderTargetGraph {
  width: number;
  height: number;
  halfWidth: number;
  sceneColor: GPUTexture;
  sceneColorView: GPUTextureView;
  scratchColor: GPUTexture;
  scratchColorView: GPUTextureView;
  capturedScreen: GPUTexture;
  capturedScreenView: GPUTextureView;
  workBuf: GPUTexture;
  workBufView: GPUTextureView;
  extraBufA: GPUTexture;
  extraBufAView: GPUTextureView;
  extraBufB: GPUTexture;
  extraBufBView: GPUTextureView;
  depthTex: GPUTexture;
  depthView: GPUTextureView;
}

type SceneShaderFamily = "tower" | "fogSprite" | "lineTrail" | "glassCube" | "menuRodCube";

const UNIFORM_STRIDE = 256;
const INITIAL_MAX_VERTICES = 65_536;
const INITIAL_MAX_DRAW_SLOTS = 512;

export function ps2BlendStateForMode(abe: boolean, blendMode: number): GPUBlendState | undefined {
  if (!abe || blendMode === 255) {
    return undefined;
  }
  // PS2 ALPHA_1 updates RGB according to BlendModes[0..10] while writing fragment As into Ad!
  const alphaWrite: GPUBlendComponent = {
    srcFactor: "one",
    dstFactor: "zero",
    operation: "add",
  };

  switch (blendMode) {
    case 0: // (Cs - 0) * FIX + Cd
      return {
        color: { srcFactor: "constant", dstFactor: "one", operation: "add" },
        alpha: alphaWrite,
      };
    case 1: // (0 - Cs) * FIX + Cd = Cd - Cs * FIX
      return {
        color: { srcFactor: "constant", dstFactor: "one", operation: "reverse-subtract" },
        alpha: alphaWrite,
      };
    case 2: // (Cs - Cd) * FIX + Cd
      return {
        color: { srcFactor: "constant", dstFactor: "one-minus-constant", operation: "add" },
        alpha: alphaWrite,
      };
    case 3: // (Cd - 0) * FIX + Cs
      return {
        color: { srcFactor: "one", dstFactor: "constant", operation: "add" },
        alpha: alphaWrite,
      };
    case 4:
    case 7: // (Cs - Cd) * As + Cd
      return {
        color: { srcFactor: "src-alpha", dstFactor: "one-minus-src-alpha", operation: "add" },
        alpha: alphaWrite,
      };
    case 5: // (Cs - 0) * As + Cd
      return {
        color: { srcFactor: "src-alpha", dstFactor: "one", operation: "add" },
        alpha: alphaWrite,
      };
    case 6: // (0 - Cs) * As + Cd = Cd - Cs * As
      return {
        color: { srcFactor: "src-alpha", dstFactor: "one", operation: "reverse-subtract" },
        alpha: alphaWrite,
      };
    case 8: // (Cs - 0) * Ad + Cd
      return {
        color: { srcFactor: "dst-alpha", dstFactor: "one", operation: "add" },
        alpha: alphaWrite,
      };
    case 9: // (0 - Cs) * Ad + Cd = Cd - Cs * Ad
      return {
        color: { srcFactor: "dst-alpha", dstFactor: "one", operation: "reverse-subtract" },
        alpha: alphaWrite,
      };
    case 10: // (Cs - Cd) * Ad + Cd
      return {
        color: { srcFactor: "dst-alpha", dstFactor: "one-minus-dst-alpha", operation: "add" },
        alpha: alphaWrite,
      };
    default:
      return {
        color: { srcFactor: "src-alpha", dstFactor: "one-minus-src-alpha", operation: "add" },
        alpha: alphaWrite,
      };
  }
}

export function computeAspectViewport(
  canvasWidth: number,
  canvasHeight: number,
  mode: AspectPresentationMode,
): { x: number; y: number; width: number; height: number } {
  if (mode === "fit" || canvasWidth <= 0 || canvasHeight <= 0) {
    return { x: 0, y: 0, width: Math.max(1, canvasWidth), height: Math.max(1, canvasHeight) };
  }
  const targetAspect = mode === "16:9" ? 16 / 9 : 4 / 3;
  const currentAspect = canvasWidth / canvasHeight;
  if (currentAspect > targetAspect) {
    const fittedWidth = Math.max(1, Math.round(canvasHeight * targetAspect));
    const x = Math.floor((canvasWidth - fittedWidth) * 0.5);
    return { x, y: 0, width: fittedWidth, height: canvasHeight };
  }
  const fittedHeight = Math.max(1, Math.round(canvasWidth / targetAspect));
  const y = Math.floor((canvasHeight - fittedHeight) * 0.5);
  return { x: 0, y, width: canvasWidth, height: fittedHeight };
}

function uint32BitsToFloat32(bits: number): number {
  const buf = new ArrayBuffer(4);
  const dv = new DataView(buf);
  dv.setUint32(0, bits >>> 0, true);
  return dv.getFloat32(0, true);
}

export class WebGpuRenderer {
  readonly backend = "webgpu" as const;

  private readonly canvas: HTMLCanvasElement;
  private context: GPUCanvasContext;
  private presentationFormat: GPUTextureFormat;
  private device: GPUDevice;

  private aspectMode: AspectPresentationMode;
  private resolutionScale: number;
  private maxDpr: number;
  private palStandard = false;

  private textureAssets: ProceduralTextureAsset[];
  private staticTextures: GPUTexture[] = [];
  private staticViews: GPUTextureView[] = [];
  private white1x1Tex!: GPUTexture;
  private white1x1View!: GPUTextureView;

  private linearRepeatSampler!: GPUSampler;
  private linearClampSampler!: GPUSampler;

  private shaderModules!: Record<ShaderModuleName, GPUShaderModule>;
  private sceneBindGroupLayout!: GPUBindGroupLayout;
  private lineBindGroupLayout!: GPUBindGroupLayout;
  private blitBindGroupLayout!: GPUBindGroupLayout;
  private overlayBindGroupLayout!: GPUBindGroupLayout;

  private pipelineCache = new Map<string, GPURenderPipeline>();
  private blitCapturePipeline!: GPURenderPipeline;
  private blitModulateOpaquePipeline!: GPURenderPipeline;
  private blitModulateAlphaPipeline!: GPURenderPipeline;
  private blitPresentPipeline!: GPURenderPipeline;
  private overlayAlphaPipeline!: GPURenderPipeline;
  /** Presents the in-app UI raster over the swapchain (nearest-neighbour, alpha blended). */
  private uiPresentPipeline!: GPURenderPipeline;
  private uiUniformBuffer!: GPUBuffer;
  private uiTexture: GPUTexture | null = null;
  private uiTextureView: GPUTextureView | null = null;
  private uiTextureWidth = 0;
  private uiTextureHeight = 0;
  private nearestClampSampler!: GPUSampler;
  private readonly uiParamScratch = new Float32Array(16);

  private vertexBuffer!: GPUBuffer;
  private maxVertices = INITIAL_MAX_VERTICES;
  private uniformBuffer!: GPUBuffer;
  private maxUniformSlots = INITIAL_MAX_DRAW_SLOTS;
  private uniformScratch = new Float32Array((INITIAL_MAX_DRAW_SLOTS * UNIFORM_STRIDE) / 4);

  private targets: RenderTargetGraph | null = null;
  private extraBufFlip = false;
  private extraBufPrimed = false;

  private createdTextures = 0;
  private destroyedTextures = 0;
  private deviceLostRecoveries = 0;
  private isRecovering = false;
  private disposed = false;

  private readonly onDeviceLost?: (info: { reason: string; message: string }) => void;
  private readonly onDeviceRecovered?: () => void;

  private constructor(
    device: GPUDevice,
    context: GPUCanvasContext,
    format: GPUTextureFormat,
    options: RendererOptions,
  ) {
    this.canvas = options.canvas;
    this.device = device;
    this.context = context;
    this.presentationFormat = format;
    this.aspectMode = options.aspectMode ?? "4:3";
    this.resolutionScale = Math.max(0.5, Math.min(2.0, options.resolutionScale ?? 1.0));
    this.maxDpr = Math.max(1.0, Math.min(3.0, options.maxDevicePixelRatio ?? 2.0));
    this.textureAssets = options.initialTextures ?? synthesizeProceduralTextures();
    this.onDeviceLost = options.onDeviceLost;
    this.onDeviceRecovered = options.onDeviceRecovered;

    this.initGpuState();
    this.attachDeviceLossHandler(device);
  }

  static isWebGpuAvailable(): boolean {
    return typeof navigator !== "undefined" && typeof navigator.gpu?.requestAdapter === "function";
  }

  /**
   * Some runtimes expose WebGPU but reject a high-performance adapter request. Retry the
   * default adapter and then the explicit software fallback before reporting unavailability.
   */
  static async requestAdapterWithFallback(): Promise<GPUAdapter | null> {
    if (!WebGpuRenderer.isWebGpuAvailable()) return null;
    const attempts: GPURequestAdapterOptions[] = [
      { powerPreference: "high-performance" },
      {},
      { forceFallbackAdapter: true },
    ];
    for (const options of attempts) {
      try {
        const adapter = await navigator.gpu.requestAdapter(options);
        if (adapter) return adapter;
      } catch {
        // A rejected preference/fallback request does not rule out the next adapter class.
      }
    }
    return null;
  }

  static async create(options: RendererOptions): Promise<WebGpuRenderer> {
    if (!WebGpuRenderer.isWebGpuAvailable()) {
      throw new Error(
        "WebGPU is not available in this browser (navigator.gpu missing; HTTPS or localhost is required)",
      );
    }
    const adapter = await WebGpuRenderer.requestAdapterWithFallback();
    if (!adapter) {
      throw new Error(
        "No compatible WebGPU adapter could be acquired (hardware, default, and software attempts failed)",
      );
    }
    const device = await adapter.requestDevice();
    const context = options.canvas.getContext("webgpu");
    if (!context) {
      device.destroy();
      throw new Error("Failed to acquire WebGPU canvas context");
    }
    const format = navigator.gpu.getPreferredCanvasFormat();
    context.configure({
      device,
      format,
      alphaMode: "opaque",
    });

    return new WebGpuRenderer(device, context, format, options);
  }

  getResourceStats(): RendererResourceStats {
    return {
      createdTextures: this.createdTextures,
      destroyedTextures: this.destroyedTextures,
      activeRenderTargetWidth: this.targets?.width ?? 0,
      activeRenderTargetHeight: this.targets?.height ?? 0,
      deviceLostRecoveries: this.deviceLostRecoveries,
      pipelineCount: this.pipelineCache.size + 6,
    };
  }

  setDisplayConfig(opts: {
    aspectMode?: AspectPresentationMode;
    resolutionScale?: number;
    pal?: boolean;
  }): void {
    if (opts.aspectMode) {
      this.aspectMode = opts.aspectMode;
    }
    let rebuildTargets = false;
    if (opts.resolutionScale !== undefined) {
      const clamped = Math.max(0.5, Math.min(2.0, opts.resolutionScale));
      if (Math.abs(clamped - this.resolutionScale) > 1e-3) {
        this.resolutionScale = clamped;
        rebuildTargets = true;
      }
    }
    if (opts.pal !== undefined && opts.pal !== this.palStandard) {
      this.palStandard = opts.pal;
      rebuildTargets = true;
    }
    if (rebuildTargets) {
      this.ensureRenderTargets();
    }
  }

  updateTextures(assets: ProceduralTextureAsset[]): void {
    if (assets.length < PROCEDURAL_SLOT_COUNT) {
      throw new Error(`Expected ${PROCEDURAL_SLOT_COUNT} texture slots, got ${assets.length}`);
    }
    this.textureAssets = assets;
    if (!this.disposed && !this.isRecovering) {
      this.destroyStaticTextures();
      this.uploadStaticTextures();
    }
  }

  syncCanvasSize(cssWidth?: number, cssHeight?: number): void {
    if (this.disposed) return;
    const dpr = Math.min(
      this.maxDpr,
      typeof window !== "undefined" && window.devicePixelRatio ? window.devicePixelRatio : 1,
    );
    const w = Math.max(1, Math.floor((cssWidth ?? (this.canvas.clientWidth || 640)) * dpr));
    const h = Math.max(1, Math.floor((cssHeight ?? (this.canvas.clientHeight || 480)) * dpr));
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
  }

  /** Simulates device loss and verifies clean recovery without full page reload (for tests & fault drills). */
  async simulateDeviceLossAndRecover(): Promise<void> {
    this.device.destroy();
    await this.recoverFromDeviceLoss({ reason: "destroyed", message: "Simulated device loss" });
  }

  /**
   * Renders one scene frame into `sceneColor` and presents it. When `uiOverlay` is given, the
   * in-app UI raster is composited over the presented frame in a final swapchain pass.
   */
  renderFrame(
    vertexFloats: Float32Array,
    commandWords: Uint32Array,
    mode: VisualModeName,
    uiOverlay?: UiOverlayFrame,
  ): void {
    if (this.disposed || this.isRecovering) return;

    this.ensureRenderTargets();
    const targets = this.targets!;

    const vertexCount = Math.floor(vertexFloats.length / EXPECTED_VERTEX_FLOATS);
    this.ensureVertexCapacity(vertexCount);
    if (vertexCount > 0) {
      this.device.queue.writeBuffer(
        this.vertexBuffer,
        0,
        vertexFloats.buffer,
        vertexFloats.byteOffset,
        vertexCount * EXPECTED_VERTEX_FLOATS * 4,
      );
    }

    const cmdCount = Math.floor(commandWords.length / EXPECTED_CMD_WORDS);
    // Reserve generous uniform slots (commands + multi-pass post effects + present)
    this.ensureUniformSlots(cmdCount * 2 + 32);

    let uniformSlot = 0;
    const allocUniform = (floats16: readonly number[]): number => {
      if (uniformSlot >= this.maxUniformSlots) {
        this.ensureUniformSlots(this.maxUniformSlots * 2);
      }
      const byteOffset = uniformSlot * UNIFORM_STRIDE;
      const floatBase = byteOffset / 4;
      for (let i = 0; i < floats16.length && i < 16; i++) {
        this.uniformScratch[floatBase + i] = floats16[i] ?? 0;
      }
      uniformSlot++;
      return byteOffset;
    };

    // Pre-decode commands and stage uniforms in CPU scratch BEFORE recording GPU passes
    interface PreparedOp {
      cmd: DecodedDrawCommand;
      uniformOffsets: number[];
    }
    const ops: PreparedOp[] = [];

    for (let i = 0; i < cmdCount; i++) {
      const b = i * EXPECTED_CMD_WORDS;
      const cmd: DecodedDrawCommand = {
        kind: commandWords[b] as DrawCommandKindId,
        target: (commandWords[b + 1] === 1 ? 1 : 0) as 0 | 1,
        textureId: commandWords[b + 2] ?? 255,
        flags: decodePipelineFlags(commandWords[b + 3] ?? 0),
        firstVertex: commandWords[b + 4] ?? 0,
        vertexCount: commandWords[b + 5] ?? 0,
        aux0: commandWords[b + 6] ?? 0,
        aux1: commandWords[b + 7] ?? 0,
      };

      const offsets: number[] = [];
      switch (cmd.kind) {
        case DrawCommandKindId.Triangles:
        case DrawCommandKindId.Lines: {
          const isTextured = cmd.textureId !== 255 ? 1.0 : 0.0;
          const texAlphaScale = cmd.textureId < 100 ? 255.0 / 128.0 : 1.0;
          const uvClamp = cmd.flags.uvClamp ? 1.0 : 0.0;
          offsets.push(allocUniform([isTextured, texAlphaScale, uvClamp, 0.0]));
          break;
        }
        case DrawCommandKindId.CaptureScreenToWork: {
          offsets.push(allocUniform([0, 0, 1, 1, 128, 128, 128, 128, 0, 0, 0, 0]));
          break;
        }
        case DrawCommandKindId.ApplyExtraBufFeedback: {
          const fix = Math.min(128, cmd.aux0 & 0xff);
          // Exact PS2 GS `DrawExtraBuf2(fix)` (`opening.c:2045`, `BlendModes[2] = {0,1,2,1}`:
          // `(Cs - Cd) * (fix/128) + Cd` with +0.5 texel `pktSetTexRect` UV bias)
          const halfW = Math.max(1, Math.floor(targets.width / 2));
          const du = 0.5 / halfW;
          const dv = 0.5 / targets.height;
          offsets.push(allocUniform([du, dv, 1 - du, 1 - dv, 128, 128, 128, fix, 1, 0, 0, 0]));
          // Exact PS2 GS `DrawToExtraBuf2()` (`opening.c:1993`, `abe = 0` squeeze of `sceneColor` into half-width `extraBuf`)
          offsets.push(allocUniform([0, 0, 1, 1, 128, 128, 128, 128, 1, 0, 0, 0]));
          break;
        }
        case DrawCommandKindId.ApplyMotionBlur: {
          const passes = Math.min(3, Math.max(1, cmd.aux0));
          for (let p = 0; p < passes; p++) {
            const inset = 0.018 * (p + 1);
            offsets.push(
              allocUniform([
                inset,
                inset,
                1 - inset * 2,
                1 - inset * 2,
                128,
                128,
                128,
                46,
                1,
                0,
                0,
                0,
              ]),
            );
          }
          break;
        }
        case DrawCommandKindId.ApplyMenuBackdropTint: {
          const r = (cmd.aux0 >>> 24) & 0xff;
          const g = (cmd.aux0 >>> 16) & 0xff;
          const bl = (cmd.aux0 >>> 8) & 0xff;
          offsets.push(allocUniform([0, 0, 1, 1, r, g, bl, 128, 1, 0, 0, 0]));
          break;
        }
        case DrawCommandKindId.ApplyMenuZoomBlur: {
          const passes = Math.min(5, Math.max(1, cmd.aux0));
          const alphas = [0x60, 0x58, 0x50, 0x48, 0x40];
          for (let p = 0; p < passes; p++) {
            const ix = (4 * (p + 1)) / 640;
            const iy = (4 * (p + 1)) / (this.palStandard ? 256 : 224);
            offsets.push(
              allocUniform([
                ix,
                iy,
                1 - ix * 2,
                1 - iy * 2,
                128,
                128,
                128,
                alphas[p] ?? 0x40,
                1,
                0,
                0,
                0,
              ]),
            );
          }
          break;
        }
        case DrawCommandKindId.DrawFullScreenFade: {
          const r = ((cmd.aux0 >>> 24) & 0xff) / 255;
          const g = ((cmd.aux0 >>> 16) & 0xff) / 255;
          const bl = ((cmd.aux0 >>> 8) & 0xff) / 255;
          const a = Math.min(1, (cmd.aux0 & 0xff) / 128);
          offsets.push(allocUniform([-1, -1, 2, 2, 0, 0, 1, 1, r, g, bl, a, 0, 1, 0, 0]));
          break;
        }
        case DrawCommandKindId.DrawBannerOverlay: {
          const a = Math.min(1, (cmd.aux0 & 0xff) / 128);
          offsets.push(
            allocUniform([-0.42, -0.15, 0.84, 0.3, 0, 0, 1, 1, 1, 1, 1, a, 1, 255 / 128, 0, 0]),
          );
          break;
        }
        case DrawCommandKindId.DrawIllegalTextOverlay: {
          const a = Math.min(1, (cmd.aux0 & 0xff) / 128);
          offsets.push(
            allocUniform([-0.68, -0.22, 1.36, 0.44, 0, 0, 1, 1, 1, 1, 1, a, 1, 255 / 128, 0, 0]),
          );
          break;
        }
        case DrawCommandKindId.DrawLetterboxBars: {
          const ratio = Math.max(0, Math.min(0.45, uint32BitsToFloat32(cmd.aux0)));
          const barH = ratio * 2.0;
          offsets.push(allocUniform([-1, 1 - barH, 2, barH, 0, 0, 1, 1, 0, 0, 0, 1, 0, 1, 0, 0]));
          offsets.push(allocUniform([-1, -1, 2, barH, 0, 0, 1, 1, 0, 0, 0, 1, 0, 1, 0, 0]));
          break;
        }
      }

      ops.push({ cmd, uniformOffsets: offsets });
    }

    const presentUniformOffset = allocUniform([0, 0, 1, 1, 128, 128, 128, 128, 2, 0, 0, 0]);

    // Upload all staged uniform slots in one single writeBuffer call!
    this.device.queue.writeBuffer(
      this.uniformBuffer,
      0,
      this.uniformScratch.buffer,
      0,
      uniformSlot * UNIFORM_STRIDE,
    );

    const encoder = this.device.createCommandEncoder();

    // Clear `sceneColor` + reverse-Z `depthTex` (clearValue = 0.0 because reverse-Z near=1, far=0!)
    {
      const clearPass = encoder.beginRenderPass({
        colorAttachments: [
          {
            view: targets.sceneColorView,
            clearValue: { r: 0, g: 0, b: 0, a: 0 },
            loadOp: "clear",
            storeOp: "store",
          },
        ],
        depthStencilAttachment: {
          view: targets.depthView,
          depthClearValue: 0.0,
          depthLoadOp: "clear",
          depthStoreOp: "store",
        },
      });
      clearPass.end();
    }

    let activePass: GPURenderPassEncoder | null = null;
    let activeTarget: 0 | 1 | -1 = -1;
    let activeHasDepth = false;

    const closeActivePass = (): void => {
      if (activePass) {
        activePass.end();
        activePass = null;
        activeTarget = -1;
      }
    };

    const ensureSceneOrWorkPass = (target: 0 | 1, needDepth: boolean): GPURenderPassEncoder => {
      if (activePass && activeTarget === target && activeHasDepth === needDepth) {
        return activePass;
      }
      closeActivePass();
      const colorView = target === 1 ? targets.workBufView : targets.sceneColorView;
      activePass = encoder.beginRenderPass({
        colorAttachments: [
          {
            view: colorView,
            loadOp: "load",
            storeOp: "store",
          },
        ],
        depthStencilAttachment: needDepth
          ? {
              view: targets.depthView,
              depthLoadOp: "load",
              depthStoreOp: "store",
            }
          : undefined,
      });
      activeTarget = target;
      activeHasDepth = needDepth;
      return activePass;
    };

    for (const op of ops) {
      const { cmd, uniformOffsets } = op;

      switch (cmd.kind) {
        case DrawCommandKindId.Triangles: {
          if (cmd.vertexCount === 0) break;
          // Prevent self-sampling hazard if target == 1 and textureId == 101
          let sampleView = this.resolveTextureView(cmd.textureId, targets);
          if (cmd.target === 1 && cmd.textureId >= 101 && cmd.textureId <= 103) {
            closeActivePass();
            encoder.copyTextureToTexture(
              { texture: targets.workBuf },
              { texture: targets.scratchColor },
              { width: targets.width, height: targets.height },
            );
            sampleView = targets.scratchColorView;
          }

          const needDepth = cmd.flags.depthTest || cmd.flags.depthWrite;
          const pass = ensureSceneOrWorkPass(cmd.target, needDepth);
          const family = this.selectShaderFamily(cmd, mode);
          const pipeline = this.getOrCreateScenePipeline(
            family,
            "triangle-list",
            cmd.flags.abe,
            cmd.flags.blendMode,
            needDepth,
            cmd.flags.depthTest,
            cmd.flags.depthWrite,
          );
          pass.setPipeline(pipeline);
          if (cmd.flags.abe && cmd.flags.blendMode <= 3) {
            const f = Math.min(1.0, cmd.flags.blendFix / 128.0);
            pass.setBlendConstant({ r: f, g: f, b: f, a: f });
          }

          const sampler = cmd.flags.uvClamp ? this.linearClampSampler : this.linearRepeatSampler;
          const bg = this.device.createBindGroup({
            layout: this.sceneBindGroupLayout,
            entries: [
              {
                binding: 0,
                resource: {
                  buffer: this.uniformBuffer,
                  offset: uniformOffsets[0] ?? 0,
                  size: 16,
                },
              },
              { binding: 1, resource: sampleView },
              { binding: 2, resource: sampler },
            ],
          });
          pass.setBindGroup(0, bg);
          pass.setVertexBuffer(0, this.vertexBuffer);
          pass.draw(cmd.vertexCount, 1, cmd.firstVertex, 0);
          break;
        }

        case DrawCommandKindId.Lines: {
          if (cmd.vertexCount === 0) break;
          const needDepth = cmd.flags.depthTest || cmd.flags.depthWrite;
          const pass = ensureSceneOrWorkPass(cmd.target, needDepth);
          const pipeline = this.getOrCreateScenePipeline(
            "lineTrail",
            "line-list",
            cmd.flags.abe,
            cmd.flags.blendMode,
            needDepth,
            cmd.flags.depthTest,
            cmd.flags.depthWrite,
          );
          pass.setPipeline(pipeline);
          if (cmd.flags.abe && cmd.flags.blendMode <= 3) {
            const f = Math.min(1.0, cmd.flags.blendFix / 128.0);
            pass.setBlendConstant({ r: f, g: f, b: f, a: f });
          }
          const bg = this.device.createBindGroup({
            layout: this.lineBindGroupLayout,
            entries: [
              {
                binding: 0,
                resource: {
                  buffer: this.uniformBuffer,
                  offset: uniformOffsets[0] ?? 0,
                  size: 16,
                },
              },
            ],
          });
          pass.setBindGroup(0, bg);
          pass.setVertexBuffer(0, this.vertexBuffer);
          pass.draw(cmd.vertexCount, 1, cmd.firstVertex, 0);
          break;
        }

        case DrawCommandKindId.CaptureScreenToWork: {
          closeActivePass();
          const uOff = uniformOffsets[0] ?? 0;
          const bg = this.makeBlitBindGroup(uOff, targets.sceneColorView);
          for (const dstView of [targets.capturedScreenView, targets.workBufView]) {
            const p = encoder.beginRenderPass({
              colorAttachments: [
                {
                  view: dstView,
                  clearValue: { r: 0, g: 0, b: 0, a: 0 },
                  loadOp: "clear",
                  storeOp: "store",
                },
              ],
            });
            p.setPipeline(this.blitCapturePipeline);
            p.setBindGroup(0, bg);
            p.draw(3);
            p.end();
          }
          break;
        }

        case DrawCommandKindId.ApplyExtraBufFeedback: {
          closeActivePass();
          const extraReadView = this.extraBufFlip ? targets.extraBufBView : targets.extraBufAView;
          const extraWriteView = this.extraBufFlip ? targets.extraBufAView : targets.extraBufBView;

          // 1. `DrawExtraBuf2(fix)` (`opening.c:2045`): blend previous half-width buffer onto `sceneColor`
          //    via `BlendModes[2]` (`(Cs - Cd) * (fix/128) + Cd`)
          if (this.extraBufPrimed) {
            const feedbackPass = encoder.beginRenderPass({
              colorAttachments: [
                { view: targets.sceneColorView, loadOp: "load", storeOp: "store" },
              ],
            });
            feedbackPass.setPipeline(this.blitModulateAlphaPipeline);
            feedbackPass.setBindGroup(
              0,
              this.makeBlitBindGroup(uniformOffsets[0] ?? 0, extraReadView),
            );
            feedbackPass.draw(3);
            feedbackPass.end();
          }

          // 2. `DrawToExtraBuf2()` (`opening.c:1993`): squeeze current `sceneColor` into half-width `extraBuf`
          const savePass = encoder.beginRenderPass({
            colorAttachments: [
              {
                view: extraWriteView,
                clearValue: { r: 0, g: 0, b: 0, a: 0 },
                loadOp: "clear",
                storeOp: "store",
              },
            ],
          });
          savePass.setPipeline(this.blitModulateOpaquePipeline);
          savePass.setBindGroup(
            0,
            this.makeBlitBindGroup(uniformOffsets[1] ?? 0, targets.sceneColorView),
          );
          savePass.draw(3);
          savePass.end();

          this.extraBufFlip = !this.extraBufFlip;
          this.extraBufPrimed = true;
          break;
        }

        case DrawCommandKindId.ApplyMotionBlur:
        case DrawCommandKindId.ApplyMenuZoomBlur: {
          closeActivePass();
          encoder.copyTextureToTexture(
            { texture: targets.sceneColor },
            { texture: targets.scratchColor },
            { width: targets.width, height: targets.height },
          );
          const blurPass = encoder.beginRenderPass({
            colorAttachments: [{ view: targets.sceneColorView, loadOp: "load", storeOp: "store" }],
          });
          blurPass.setPipeline(this.blitModulateAlphaPipeline);
          for (const uOff of uniformOffsets) {
            blurPass.setBindGroup(0, this.makeBlitBindGroup(uOff, targets.scratchColorView));
            blurPass.draw(3);
          }
          blurPass.end();
          break;
        }

        case DrawCommandKindId.ApplyMenuBackdropTint: {
          closeActivePass();
          encoder.copyTextureToTexture(
            { texture: targets.sceneColor },
            { texture: targets.scratchColor },
            { width: targets.width, height: targets.height },
          );
          const tintPass = encoder.beginRenderPass({
            colorAttachments: [
              {
                view: targets.sceneColorView,
                clearValue: { r: 0, g: 0, b: 0, a: 0 },
                loadOp: "clear",
                storeOp: "store",
              },
            ],
          });
          tintPass.setPipeline(this.blitModulateOpaquePipeline);
          tintPass.setBindGroup(
            0,
            this.makeBlitBindGroup(uniformOffsets[0] ?? 0, targets.scratchColorView),
          );
          tintPass.draw(3);
          tintPass.end();
          break;
        }

        case DrawCommandKindId.DrawFullScreenFade:
        case DrawCommandKindId.DrawLetterboxBars: {
          const pass = ensureSceneOrWorkPass(0, false);
          pass.setPipeline(this.overlayAlphaPipeline);
          for (const uOff of uniformOffsets) {
            pass.setBindGroup(0, this.makeOverlayBindGroup(uOff, this.white1x1View));
            pass.draw(6);
          }
          break;
        }

        case DrawCommandKindId.DrawBannerOverlay: {
          const pass = ensureSceneOrWorkPass(0, false);
          pass.setPipeline(this.overlayAlphaPipeline);
          pass.setBindGroup(
            0,
            this.makeOverlayBindGroup(uniformOffsets[0] ?? 0, this.staticViews[0] ?? this.white1x1View),
          );
          pass.draw(6);
          break;
        }

        case DrawCommandKindId.DrawIllegalTextOverlay: {
          const pass = ensureSceneOrWorkPass(0, false);
          pass.setPipeline(this.overlayAlphaPipeline);
          pass.setBindGroup(
            0,
            this.makeOverlayBindGroup(uniformOffsets[0] ?? 0, this.staticViews[13] ?? this.white1x1View),
          );
          pass.draw(6);
          break;
        }
      }
    }

    closeActivePass();

    // Final presentation to canvas swapchain with aspect-ratio viewport framing
    const swapTex = this.context.getCurrentTexture();
    const swapView = swapTex.createView();
    this.encodePresentPass(encoder, swapView, swapTex.width, swapTex.height, presentUniformOffset);
    if (uiOverlay) {
      this.encodeUiPass(encoder, swapView, uiOverlay);
    }

    this.device.queue.submit([encoder.finish()]);
  }

  /**
   * Re-presents the most recently rendered scene (`sceneColor`) together with a new UI raster,
   * without running any simulation or scene passes. Used while the scene is paused (title screen)
   * so temporal feedback buffers do not advance. Returns false when no scene exists yet.
   */
  presentUi(uiOverlay: UiOverlayFrame): boolean {
    if (this.disposed || this.isRecovering || !this.targets) return false;
    const sourceView = this.targets.sceneColorView;
    const swapTex = this.context.getCurrentTexture();
    const swapView = swapTex.createView();
    const blitParams = new Float32Array(16);
    blitParams.set([0, 0, 1, 1, 128, 128, 128, 128, 2, 0, 0, 0]);
    this.device.queue.writeBuffer(this.uniformBuffer, 0, blitParams);

    const encoder = this.device.createCommandEncoder();
    this.encodePresentPass(encoder, swapView, swapTex.width, swapTex.height, 0, sourceView);
    this.encodeUiPass(encoder, swapView, uiOverlay);
    this.device.queue.submit([encoder.finish()]);
    return true;
  }

  private encodePresentPass(
    encoder: GPUCommandEncoder,
    swapView: GPUTextureView,
    swapWidth: number,
    swapHeight: number,
    uniformOffset: number,
    sourceView: GPUTextureView = this.targets!.sceneColorView,
  ): void {
    const vp = computeAspectViewport(swapWidth, swapHeight, this.aspectMode);
    const presentPass = encoder.beginRenderPass({
      colorAttachments: [
        {
          view: swapView,
          clearValue: { r: 0, g: 0, b: 0, a: 1 },
          loadOp: "clear",
          storeOp: "store",
        },
      ],
    });
    presentPass.setViewport(vp.x, vp.y, vp.width, vp.height, 0, 1);
    presentPass.setPipeline(this.blitPresentPipeline);
    presentPass.setBindGroup(0, this.makeBlitBindGroup(uniformOffset, sourceView));
    presentPass.draw(3);
    presentPass.end();
  }

  /** Full-canvas alpha-blended draw of the in-app UI raster (nearest-neighbour upscale). */
  private encodeUiPass(encoder: GPUCommandEncoder, swapView: GPUTextureView, ui: UiOverlayFrame): void {
    if (ui.width <= 0 || ui.height <= 0 || ui.rgba.length < ui.width * ui.height * 4) return;
    this.ensureUiTexture(ui.width, ui.height);
    const texture = this.uiTexture;
    const view = this.uiTextureView;
    if (!texture || !view) return;

    this.device.queue.writeTexture(
      { texture },
      ui.rgba,
      { bytesPerRow: ui.width * 4, rowsPerImage: ui.height },
      { width: ui.width, height: ui.height, depthOrArrayLayers: 1 },
    );
    // OverlayParams: ndc_rect (full screen), uv_rect (full texture), color (white), flags (textured, alpha 1).
    this.uiParamScratch.set([-1, -1, 2, 2, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0]);
    this.device.queue.writeBuffer(this.uiUniformBuffer, 0, this.uiParamScratch);

    const bindGroup = this.device.createBindGroup({
      layout: this.overlayBindGroupLayout,
      entries: [
        { binding: 0, resource: { buffer: this.uiUniformBuffer, size: 64 } },
        { binding: 1, resource: view },
        { binding: 2, resource: this.nearestClampSampler },
      ],
    });
    const pass = encoder.beginRenderPass({
      colorAttachments: [{ view: swapView, loadOp: "load", storeOp: "store" }],
    });
    pass.setPipeline(this.uiPresentPipeline);
    pass.setBindGroup(0, bindGroup);
    pass.draw(6);
    pass.end();
  }

  private ensureUiTexture(width: number, height: number): void {
    if (this.uiTexture && this.uiTextureWidth === width && this.uiTextureHeight === height) return;
    this.uiTexture?.destroy();
    if (this.uiTexture) this.destroyedTextures++;
    this.uiTexture = this.createTrackedTexture({
      size: { width, height, depthOrArrayLayers: 1 },
      format: "rgba8unorm",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
    });
    this.uiTextureView = this.uiTexture.createView();
    this.uiTextureWidth = width;
    this.uiTextureHeight = height;
  }

  /**
   * Executes `renderFrame()` inside a WebGPU `"validation"` error scope and returns `null` if
   * clean, or the `GPUError.message` string if Dawn/WebGPU raised any validation warning/error.
   */
  async renderFrameChecked(
    vertexFloats: Float32Array,
    commandWords: Uint32Array,
    mode: VisualModeName,
  ): Promise<string | null> {
    this.device.pushErrorScope("validation");
    this.renderFrame(vertexFloats, commandWords, mode);
    const err = await this.device.popErrorScope();
    return err ? err.message : null;
  }

  /**
   * Reads back the exact offscreen `sceneColor` `RGBA8` pixels (`width x height x 4`)
   * for deterministic browser WebGPU integration & visual regression tests!
   */
  async captureScenePixels(): Promise<{ width: number; height: number; rgba: Uint8Array }> {
    if (!this.targets) {
      throw new Error("No scene rendered yet — call renderFrame() before captureScenePixels()");
    }
    const { width, height, sceneColor } = this.targets;
    const unpaddedRow = width * 4;
    const paddedRow = Math.ceil(unpaddedRow / 256) * 256;
    const bufferSize = paddedRow * height;

    const readBuf = this.device.createBuffer({
      size: bufferSize,
      usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });

    const encoder = this.device.createCommandEncoder();
    encoder.copyTextureToBuffer(
      { texture: sceneColor },
      { buffer: readBuf, bytesPerRow: paddedRow, rowsPerImage: height },
      { width, height, depthOrArrayLayers: 1 },
    );
    this.device.queue.submit([encoder.finish()]);

    await readBuf.mapAsync(GPUMapMode.READ);
    const mapped = new Uint8Array(readBuf.getMappedRange());
    const rgba = new Uint8Array(width * height * 4);
    for (let y = 0; y < height; y++) {
      const srcStart = y * paddedRow;
      rgba.set(mapped.subarray(srcStart, srcStart + unpaddedRow), y * unpaddedRow);
    }
    readBuf.unmap();
    readBuf.destroy();
    return { width, height, rgba };
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.destroyRenderTargets();
    this.destroyStaticTextures();
    this.white1x1Tex.destroy();
    this.destroyedTextures++;
    this.uiTexture?.destroy();
    this.uiTexture = null;
    this.uiTextureView = null;
    this.uiUniformBuffer.destroy();
    this.vertexBuffer.destroy();
    this.uniformBuffer.destroy();
    this.device.destroy();
  }

  private initGpuState(): void {
    this.pipelineCache.clear();
    this.extraBufPrimed = false;
    this.extraBufFlip = false;

    this.linearRepeatSampler = this.device.createSampler({
      addressModeU: "repeat",
      addressModeV: "repeat",
      magFilter: "linear",
      minFilter: "linear",
      mipmapFilter: "linear",
    });
    this.linearClampSampler = this.device.createSampler({
      addressModeU: "clamp-to-edge",
      addressModeV: "clamp-to-edge",
      magFilter: "linear",
      minFilter: "linear",
      mipmapFilter: "linear",
    });

    this.shaderModules = {
      tower: this.device.createShaderModule({ label: "tower", code: SHADER_SOURCES.tower }),
      fogSprite: this.device.createShaderModule({ label: "fogSprite", code: SHADER_SOURCES.fogSprite }),
      lineTrail: this.device.createShaderModule({ label: "lineTrail", code: SHADER_SOURCES.lineTrail }),
      glassCube: this.device.createShaderModule({ label: "glassCube", code: SHADER_SOURCES.glassCube }),
      menuRodCube: this.device.createShaderModule({ label: "menuRodCube", code: SHADER_SOURCES.menuRodCube }),
      blitPost: this.device.createShaderModule({ label: "blitPost", code: SHADER_SOURCES.blitPost }),
      uiOverlay: this.device.createShaderModule({ label: "uiOverlay", code: SHADER_SOURCES.uiOverlay }),
    };

    this.sceneBindGroupLayout = this.device.createBindGroupLayout({
      entries: [
        { binding: 0, visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT, buffer: { type: "uniform" } },
        { binding: 1, visibility: GPUShaderStage.FRAGMENT, texture: { sampleType: "float" } },
        { binding: 2, visibility: GPUShaderStage.FRAGMENT, sampler: { type: "filtering" } },
      ],
    });

    this.lineBindGroupLayout = this.device.createBindGroupLayout({
      entries: [
        { binding: 0, visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT, buffer: { type: "uniform" } },
      ],
    });

    this.blitBindGroupLayout = this.device.createBindGroupLayout({
      entries: [
        { binding: 0, visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT, buffer: { type: "uniform" } },
        { binding: 1, visibility: GPUShaderStage.FRAGMENT, texture: { sampleType: "float" } },
        { binding: 2, visibility: GPUShaderStage.FRAGMENT, sampler: { type: "filtering" } },
      ],
    });

    this.overlayBindGroupLayout = this.device.createBindGroupLayout({
      entries: [
        { binding: 0, visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT, buffer: { type: "uniform" } },
        { binding: 1, visibility: GPUShaderStage.FRAGMENT, texture: { sampleType: "float" } },
        { binding: 2, visibility: GPUShaderStage.FRAGMENT, sampler: { type: "filtering" } },
      ],
    });

    const blitLayout = this.device.createPipelineLayout({ bindGroupLayouts: [this.blitBindGroupLayout] });
    this.blitCapturePipeline = this.device.createRenderPipeline({
      layout: blitLayout,
      vertex: { module: this.shaderModules.blitPost, entryPoint: "vs_main" },
      fragment: {
        module: this.shaderModules.blitPost,
        entryPoint: "fs_main",
        targets: [{ format: "rgba8unorm" }],
      },
      primitive: { topology: "triangle-list" },
    });

    this.blitModulateOpaquePipeline = this.device.createRenderPipeline({
      layout: blitLayout,
      vertex: { module: this.shaderModules.blitPost, entryPoint: "vs_main" },
      fragment: {
        module: this.shaderModules.blitPost,
        entryPoint: "fs_main",
        targets: [{ format: "rgba8unorm" }],
      },
      primitive: { topology: "triangle-list" },
    });

    this.blitModulateAlphaPipeline = this.device.createRenderPipeline({
      layout: blitLayout,
      vertex: { module: this.shaderModules.blitPost, entryPoint: "vs_main" },
      fragment: {
        module: this.shaderModules.blitPost,
        entryPoint: "fs_main",
        targets: [{ format: "rgba8unorm", blend: ps2BlendStateForMode(true, 4) }],
      },
      primitive: { topology: "triangle-list" },
    });

    this.blitPresentPipeline = this.device.createRenderPipeline({
      layout: blitLayout,
      vertex: { module: this.shaderModules.blitPost, entryPoint: "vs_main" },
      fragment: {
        module: this.shaderModules.blitPost,
        entryPoint: "fs_main",
        targets: [{ format: this.presentationFormat }],
      },
      primitive: { topology: "triangle-list" },
    });

    this.overlayAlphaPipeline = this.device.createRenderPipeline({
      layout: this.device.createPipelineLayout({ bindGroupLayouts: [this.overlayBindGroupLayout] }),
      vertex: { module: this.shaderModules.uiOverlay, entryPoint: "vs_main" },
      fragment: {
        module: this.shaderModules.uiOverlay,
        entryPoint: "fs_main",
        targets: [
          {
            format: "rgba8unorm",
            blend: {
              color: { srcFactor: "src-alpha", dstFactor: "one-minus-src-alpha", operation: "add" },
              alpha: { srcFactor: "one", dstFactor: "one-minus-src-alpha", operation: "add" },
            },
          },
        ],
      },
      primitive: { topology: "triangle-list" },
    });

    this.uiPresentPipeline = this.device.createRenderPipeline({
      layout: this.device.createPipelineLayout({ bindGroupLayouts: [this.overlayBindGroupLayout] }),
      vertex: { module: this.shaderModules.uiOverlay, entryPoint: "vs_main" },
      fragment: {
        module: this.shaderModules.uiOverlay,
        entryPoint: "fs_main",
        targets: [
          {
            format: this.presentationFormat,
            blend: {
              color: { srcFactor: "src-alpha", dstFactor: "one-minus-src-alpha", operation: "add" },
              alpha: { srcFactor: "one", dstFactor: "one-minus-src-alpha", operation: "add" },
            },
          },
        ],
      },
      primitive: { topology: "triangle-list" },
    });
    this.nearestClampSampler = this.device.createSampler({
      addressModeU: "clamp-to-edge",
      addressModeV: "clamp-to-edge",
      magFilter: "nearest",
      minFilter: "nearest",
    });
    this.uiUniformBuffer = this.device.createBuffer({
      size: 64,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    this.uiTexture = null;
    this.uiTextureView = null;
    this.uiTextureWidth = 0;
    this.uiTextureHeight = 0;

    this.white1x1Tex = this.createTrackedTexture({
      size: { width: 1, height: 1 },
      format: "rgba8unorm",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
    });
    this.device.queue.writeTexture(
      { texture: this.white1x1Tex },
      new Uint8Array([255, 255, 255, 128]),
      { bytesPerRow: 4 },
      { width: 1, height: 1 },
    );
    this.white1x1View = this.white1x1Tex.createView();

    this.vertexBuffer = this.device.createBuffer({
      size: this.maxVertices * EXPECTED_VERTEX_FLOATS * 4,
      usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
    });
    this.uniformBuffer = this.device.createBuffer({
      size: this.maxUniformSlots * UNIFORM_STRIDE,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });

    this.uploadStaticTextures();
    this.ensureRenderTargets();
  }

  private attachDeviceLossHandler(device: GPUDevice): void {
    void device.lost.then((info) => {
      if (this.disposed || this.isRecovering) return;
      this.onDeviceLost?.({ reason: String(info.reason ?? "unknown"), message: info.message });
      void this.recoverFromDeviceLoss({
        reason: String(info.reason ?? "unknown"),
        message: info.message,
      });
    });
  }

  private async recoverFromDeviceLoss(_info: { reason: string; message: string }): Promise<void> {
    if (this.disposed || this.isRecovering) return;
    this.isRecovering = true;
    try {
      const adapter = await WebGpuRenderer.requestAdapterWithFallback();
      if (!adapter) {
        throw new Error("Could not reacquire WebGPU adapter after device loss");
      }
      const nextDevice = await adapter.requestDevice();
      this.targets = null;
      this.staticTextures = [];
      this.staticViews = [];
      this.device = nextDevice;
      this.context.configure({
        device: nextDevice,
        format: this.presentationFormat,
        alphaMode: "opaque",
      });
      this.initGpuState();
      this.deviceLostRecoveries++;
      this.isRecovering = false;
      this.attachDeviceLossHandler(nextDevice);
      this.onDeviceRecovered?.();
    } catch {
      this.isRecovering = false;
    }
  }

  private createTrackedTexture(desc: GPUTextureDescriptor): GPUTexture {
    const tex = this.device.createTexture(desc);
    this.createdTextures++;
    return tex;
  }

  private uploadStaticTextures(): void {
    this.staticTextures = [];
    this.staticViews = [];

    for (const slot of this.textureAssets) {
      const mipCount = slot.mipLevels.length;
      const tex = this.createTrackedTexture({
        label: slot.name,
        size: { width: slot.width, height: slot.height },
        mipLevelCount: mipCount,
        format: "rgba8unorm",
        usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
      });

      for (let level = 0; level < mipCount; level++) {
        const mw = Math.max(1, slot.width >> level);
        const mh = Math.max(1, slot.height >> level);
        const data = slot.mipLevels[level]!;
        this.device.queue.writeTexture(
          { texture: tex, mipLevel: level },
          data.buffer,
          { offset: data.byteOffset, bytesPerRow: mw * 4, rowsPerImage: mh },
          { width: mw, height: mh },
        );
      }

      this.staticTextures.push(tex);
      this.staticViews.push(tex.createView());
    }
  }

  private destroyStaticTextures(): void {
    for (const tex of this.staticTextures) {
      tex.destroy();
      this.destroyedTextures++;
    }
    this.staticTextures = [];
    this.staticViews = [];
  }

  private ensureRenderTargets(): void {
    const baseH = this.palStandard ? 512 : 448;
    const width = Math.max(64, Math.round(640 * this.resolutionScale));
    const height = Math.max(64, Math.round(baseH * this.resolutionScale));
    const halfWidth = Math.max(32, Math.floor(width / 2));

    if (this.targets && this.targets.width === width && this.targets.height === height) {
      return;
    }

    this.destroyRenderTargets();

    const colorUsage =
      GPUTextureUsage.RENDER_ATTACHMENT |
      GPUTextureUsage.TEXTURE_BINDING |
      GPUTextureUsage.COPY_SRC |
      GPUTextureUsage.COPY_DST;

    const sceneColor = this.createTrackedTexture({ size: { width, height }, format: "rgba8unorm", usage: colorUsage });
    const scratchColor = this.createTrackedTexture({ size: { width, height }, format: "rgba8unorm", usage: colorUsage });
    const capturedScreen = this.createTrackedTexture({ size: { width, height }, format: "rgba8unorm", usage: colorUsage });
    const workBuf = this.createTrackedTexture({ size: { width, height }, format: "rgba8unorm", usage: colorUsage });
    const extraBufA = this.createTrackedTexture({ size: { width: halfWidth, height }, format: "rgba8unorm", usage: colorUsage });
    const extraBufB = this.createTrackedTexture({ size: { width: halfWidth, height }, format: "rgba8unorm", usage: colorUsage });
    const depthTex = this.createTrackedTexture({
      size: { width, height },
      format: "depth24plus",
      usage: GPUTextureUsage.RENDER_ATTACHMENT,
    });

    this.targets = {
      width,
      height,
      halfWidth,
      sceneColor,
      sceneColorView: sceneColor.createView(),
      scratchColor,
      scratchColorView: scratchColor.createView(),
      capturedScreen,
      capturedScreenView: capturedScreen.createView(),
      workBuf,
      workBufView: workBuf.createView(),
      extraBufA,
      extraBufAView: extraBufA.createView(),
      extraBufB,
      extraBufBView: extraBufB.createView(),
      depthTex,
      depthView: depthTex.createView(),
    };
    this.extraBufPrimed = false;
    this.extraBufFlip = false;
  }

  private destroyRenderTargets(): void {
    if (!this.targets) return;
    this.targets.sceneColor.destroy();
    this.targets.scratchColor.destroy();
    this.targets.capturedScreen.destroy();
    this.targets.workBuf.destroy();
    this.targets.extraBufA.destroy();
    this.targets.extraBufB.destroy();
    this.targets.depthTex.destroy();
    this.destroyedTextures += 7;
    this.targets = null;
  }

  private ensureVertexCapacity(neededVertices: number): void {
    if (neededVertices <= this.maxVertices) return;
    let next = this.maxVertices;
    while (next < neededVertices) {
      next *= 2;
    }
    this.vertexBuffer.destroy();
    this.maxVertices = next;
    this.vertexBuffer = this.device.createBuffer({
      size: this.maxVertices * EXPECTED_VERTEX_FLOATS * 4,
      usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
    });
  }

  private ensureUniformSlots(neededSlots: number): void {
    if (neededSlots <= this.maxUniformSlots) return;
    let next = this.maxUniformSlots;
    while (next < neededSlots) {
      next *= 2;
    }
    this.uniformBuffer.destroy();
    this.maxUniformSlots = next;
    this.uniformBuffer = this.device.createBuffer({
      size: this.maxUniformSlots * UNIFORM_STRIDE,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    const nextScratch = new Float32Array((this.maxUniformSlots * UNIFORM_STRIDE) / 4);
    nextScratch.set(this.uniformScratch);
    this.uniformScratch = nextScratch;
  }

  private resolveTextureView(textureId: number, targets: RenderTargetGraph): GPUTextureView {
    if (textureId >= 0 && textureId < this.staticViews.length) {
      return this.staticViews[textureId] ?? this.white1x1View;
    }
    if (textureId === 100) {
      return targets.capturedScreenView;
    }
    if (textureId >= 101 && textureId <= 103) {
      return targets.workBufView;
    }
    return this.white1x1View;
  }

  private selectShaderFamily(cmd: DecodedDrawCommand, mode: VisualModeName): SceneShaderFamily {
    if (cmd.textureId === 6) {
      return "tower";
    }
    if (
      mode === "menu" &&
      (cmd.textureId === 100 || (cmd.textureId >= 17 && cmd.textureId <= 20))
    ) {
      return "menuRodCube";
    }
    if (
      cmd.target === 1 ||
      cmd.textureId === 10 ||
      cmd.textureId === 11 ||
      cmd.textureId === 12 ||
      cmd.textureId === 100 ||
      cmd.textureId === 101
    ) {
      return "glassCube";
    }
    return "fogSprite";
  }

  private getOrCreateScenePipeline(
    family: SceneShaderFamily,
    topology: GPUPrimitiveTopology,
    abe: boolean,
    blendMode: number,
    hasDepthAttachment: boolean,
    depthTest: boolean,
    depthWrite: boolean,
  ): GPURenderPipeline {
    const key = `${family}:${topology}:${abe ? blendMode : 255}:${hasDepthAttachment ? 1 : 0}:${depthTest ? 1 : 0}:${depthWrite ? 1 : 0}`;
    const cached = this.pipelineCache.get(key);
    if (cached) return cached;

    const module = this.shaderModules[family];
    const bgl = family === "lineTrail" ? this.lineBindGroupLayout : this.sceneBindGroupLayout;
    const pipeline = this.device.createRenderPipeline({
      layout: this.device.createPipelineLayout({ bindGroupLayouts: [bgl] }),
      vertex: {
        module,
        entryPoint: "vs_main",
        buffers: [
          {
            arrayStride: EXPECTED_VERTEX_FLOATS * 4,
            attributes: [
              { shaderLocation: 0, offset: 0, format: "float32x4" },
              { shaderLocation: 1, offset: 16, format: "float32x4" },
              { shaderLocation: 2, offset: 32, format: "float32x4" },
            ],
          },
        ],
      },
      fragment: {
        module,
        entryPoint: "fs_main",
        targets: [
          {
            format: "rgba8unorm",
            blend: ps2BlendStateForMode(abe, blendMode),
          },
        ],
      },
      primitive: {
        topology,
        cullMode: "none",
      },
      depthStencil: hasDepthAttachment
        ? {
            format: "depth24plus",
            depthWriteEnabled: depthWrite,
            depthCompare: depthTest ? "greater-equal" : "always",
          }
        : undefined,
    });

    this.pipelineCache.set(key, pipeline);
    return pipeline;
  }

  private makeBlitBindGroup(uniformOffset: number, srcView: GPUTextureView): GPUBindGroup {
    return this.device.createBindGroup({
      layout: this.blitBindGroupLayout,
      entries: [
        { binding: 0, resource: { buffer: this.uniformBuffer, offset: uniformOffset, size: 48 } },
        { binding: 1, resource: srcView },
        { binding: 2, resource: this.linearClampSampler },
      ],
    });
  }

  private makeOverlayBindGroup(uniformOffset: number, texView: GPUTextureView): GPUBindGroup {
    return this.device.createBindGroup({
      layout: this.overlayBindGroupLayout,
      entries: [
        { binding: 0, resource: { buffer: this.uniformBuffer, offset: uniformOffset, size: 64 } },
        { binding: 1, resource: texView },
        { binding: 2, resource: this.linearClampSampler },
      ],
    });
  }
}
