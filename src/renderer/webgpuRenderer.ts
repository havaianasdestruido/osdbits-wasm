import {
  BlendMode,
  DepthMode,
  type FrameTelemetrySnapshot,
  type OsdbitsWasmEngine,
  PassActionBit,
  TargetId,
  TextureSlot,
} from '../wasm/bridge';
import { OSDBITS_WGSL_SOURCE } from './shaders/osdbitsShaderSource';
import type {
  AdapterDiagnostics,
  AspectMode,
  OsdbitsRenderer,
  PixelReadback,
  ViewportLayout,
} from './types';
import { computeViewportLayout } from './viewport';

export interface WebGpuRendererOptions {
  canvas: HTMLCanvasElement;
  onDeviceLost?: (reason: string, message: string) => void;
}

const BYTES_PER_ROW_ALIGN = 256;
const INITIAL_VERTEX_BUFFER_BYTES = 256 * 1024;
const TOTAL_TEXTURE_SLOTS = 19;

function makeBlendState(mode: BlendMode): GPUBlendState | undefined {
  switch (mode) {
    case BlendMode.Opaque:
      return undefined;
    case BlendMode.SrcAlpha:
      return {
        color: {
          srcFactor: 'src-alpha',
          dstFactor: 'one-minus-src-alpha',
          operation: 'add',
        },
        alpha: {
          srcFactor: 'one',
          dstFactor: 'one-minus-src-alpha',
          operation: 'add',
        },
      };
    case BlendMode.Additive:
      return {
        color: {
          srcFactor: 'src-alpha',
          dstFactor: 'one',
          operation: 'add',
        },
        alpha: {
          srcFactor: 'src-alpha',
          dstFactor: 'one',
          operation: 'add',
        },
      };
    case BlendMode.AdditiveSrc:
      return {
        color: {
          srcFactor: 'one',
          dstFactor: 'one',
          operation: 'add',
        },
        alpha: {
          srcFactor: 'one',
          dstFactor: 'one',
          operation: 'add',
        },
      };
    case BlendMode.Subtractive:
      // PS2 GS `(Zero - Cs)*As + Cd` == `Cd * 1 - Cs * As`
      return {
        color: {
          srcFactor: 'src-alpha',
          dstFactor: 'one',
          operation: 'reverse-subtract',
        },
        alpha: {
          srcFactor: 'zero',
          dstFactor: 'one',
          operation: 'add',
        },
      };
  }
}

function makeDepthStencilState(mode: DepthMode): GPUDepthStencilState {
  switch (mode) {
    case DepthMode.Disabled:
      return {
        format: 'depth24plus',
        depthWriteEnabled: false,
        depthCompare: 'always',
      };
    case DepthMode.ReadOnly:
      return {
        format: 'depth24plus',
        depthWriteEnabled: false,
        depthCompare: 'less-equal',
      };
    case DepthMode.ReadWrite:
      return {
        format: 'depth24plus',
        depthWriteEnabled: true,
        depthCompare: 'less-equal',
      };
  }
}

interface SlotGpuBinding {
  texture: GPUTexture;
  view: GPUTextureView;
  repeatGroup: GPUBindGroup;
  clampGroup: GPUBindGroup;
  ownedStatic: boolean;
}

export class WebGpuRenderer implements OsdbitsRenderer {
  public readonly backend = 'webgpu' as const;
  public readonly diagnostics: AdapterDiagnostics;

  private readonly canvas: HTMLCanvasElement;
  private readonly context: GPUCanvasContext | null;
  private readonly softwareBlitCtx: CanvasRenderingContext2D | null;
  private readonly softwareBlitOffscreen: HTMLCanvasElement | null;
  private readonly softwareBlitOffscreenCtx: CanvasRenderingContext2D | null;
  private softwarePresentBusy = false;

  private readonly device: GPUDevice;
  private readonly presentFormat: GPUTextureFormat;
  private readonly bindGroupLayout: GPUBindGroupLayout;
  private readonly samplerRepeat: GPUSampler;
  private readonly samplerClamp: GPUSampler;
  private readonly scenePipelines: Map<number, GPURenderPipeline> = new Map();
  private readonly downsamplePipeline: GPURenderPipeline;
  private readonly presentPipeline: GPURenderPipeline;

  private vertexBuffer: GPUBuffer;
  private vertexBufferCapacity: number;

  private mainColorTexture!: GPUTexture;
  private mainColorView!: GPUTextureView;
  private mainColorPresentGroup!: GPUBindGroup;
  private mainDepthTexture!: GPUTexture;
  private mainDepthView!: GPUTextureView;

  private backdropUntintedTex!: GPUTexture;
  private backdropTintedTex!: GPUTexture;
  private extraBloomTex!: GPUTexture;
  private extraBloomView!: GPUTextureView;

  private readonly slots: Array<SlotGpuBinding | null> = new Array<SlotGpuBinding | null>(
    TOTAL_TEXTURE_SLOTS,
  ).fill(null);

  private currentLayout: ViewportLayout;
  private uploadedTextureGen = 0;
  private disposed = false;

  private constructor(
    canvas: HTMLCanvasElement,
    context: GPUCanvasContext | null,
    softwareBlitCtx: CanvasRenderingContext2D | null,
    device: GPUDevice,
    presentFormat: GPUTextureFormat,
    diagnostics: AdapterDiagnostics,
    shaderModule: GPUShaderModule,
    onDeviceLost?: (reason: string, message: string) => void,
  ) {
    this.canvas = canvas;
    this.context = context;
    this.softwareBlitCtx = softwareBlitCtx;
    if (softwareBlitCtx) {
      this.softwareBlitOffscreen = canvas.ownerDocument.createElement('canvas');
      this.softwareBlitOffscreenCtx = this.softwareBlitOffscreen.getContext('2d');
    } else {
      this.softwareBlitOffscreen = null;
      this.softwareBlitOffscreenCtx = null;
    }

    this.device = device;
    this.presentFormat = presentFormat;
    this.diagnostics = diagnostics;

    void device.lost.then((info) => {
      if (!this.disposed) {
        onDeviceLost?.(info.reason ?? 'unknown', info.message ?? 'WebGPU device lost');
      }
    });

    this.bindGroupLayout = device.createBindGroupLayout({
      label: 'osdbits-diffuse-bgl',
      entries: [
        {
          binding: 0,
          visibility: GPUShaderStage.FRAGMENT,
          sampler: { type: 'filtering' },
        },
        {
          binding: 1,
          visibility: GPUShaderStage.FRAGMENT,
          texture: { sampleType: 'float' },
        },
      ],
    });

    const pipelineLayout = device.createPipelineLayout({
      label: 'osdbits-pipeline-layout',
      bindGroupLayouts: [this.bindGroupLayout],
    });

    this.samplerRepeat = device.createSampler({
      label: 'osdbits-sampler-repeat',
      addressModeU: 'repeat',
      addressModeV: 'repeat',
      magFilter: 'linear',
      minFilter: 'linear',
    });

    this.samplerClamp = device.createSampler({
      label: 'osdbits-sampler-clamp',
      addressModeU: 'clamp-to-edge',
      addressModeV: 'clamp-to-edge',
      magFilter: 'linear',
      minFilter: 'linear',
    });

    const vertexBufferLayout: GPUVertexBufferLayout = {
      arrayStride: 32,
      stepMode: 'vertex',
      attributes: [
        { shaderLocation: 0, offset: 0, format: 'float32x4' },
        { shaderLocation: 1, offset: 16, format: 'float32x2' },
        { shaderLocation: 2, offset: 24, format: 'unorm8x4' },
        { shaderLocation: 3, offset: 28, format: 'uint32' },
      ],
    };

    for (let blend = 0; blend <= 4; blend++) {
      for (let depth = 0; depth <= 2; depth++) {
        const key = blend * 8 + depth;
        const pipeline = device.createRenderPipeline({
          label: `osdbits-scene-b${blend}-d${depth}`,
          layout: pipelineLayout,
          vertex: {
            module: shaderModule,
            entryPoint: 'vs_main',
            buffers: [vertexBufferLayout],
          },
          fragment: {
            module: shaderModule,
            entryPoint: 'fs_main',
            targets: [
              {
                format: 'rgba8unorm',
                blend: makeBlendState(blend as BlendMode),
              },
            ],
          },
          primitive: {
            topology: 'triangle-list',
            cullMode: 'none',
          },
          depthStencil: makeDepthStencilState(depth as DepthMode),
        });
        this.scenePipelines.set(key, pipeline);
      }
    }

    this.downsamplePipeline = device.createRenderPipeline({
      label: 'osdbits-bloom-downsample',
      layout: pipelineLayout,
      vertex: { module: shaderModule, entryPoint: 'vs_blit' },
      fragment: {
        module: shaderModule,
        entryPoint: 'fs_downsample',
        targets: [{ format: 'rgba8unorm' }],
      },
      primitive: { topology: 'triangle-list' },
    });

    this.presentPipeline = device.createRenderPipeline({
      label: 'osdbits-present-blit',
      layout: pipelineLayout,
      vertex: { module: shaderModule, entryPoint: 'vs_blit' },
      fragment: {
        module: shaderModule,
        entryPoint: 'fs_blit',
        targets: [{ format: presentFormat }],
      },
      primitive: { topology: 'triangle-list' },
    });

    this.vertexBufferCapacity = INITIAL_VERTEX_BUFFER_BYTES;
    this.vertexBuffer = device.createBuffer({
      label: 'osdbits-vertices',
      size: this.vertexBufferCapacity,
      usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
    });

    // Slot 0: 1x1 opaque white Untextured fallback
    this.uploadRgbaSlot(
      TextureSlot.Untextured,
      1,
      1,
      new Uint8Array([255, 255, 255, 255]),
    );

    this.currentLayout = computeViewportLayout(
      canvas.clientWidth || 640,
      canvas.clientHeight || 480,
      1.0,
      1.0,
      '4:3',
    );
    this.recreateRenderTargets(this.currentLayout);
  }

  public static async create(options: WebGpuRendererOptions): Promise<WebGpuRenderer> {
    if (typeof navigator === 'undefined' || !('gpu' in navigator) || !navigator.gpu) {
      throw new Error('navigator.gpu is not available in this browser.');
    }

    const adapter = await navigator.gpu.requestAdapter();
    if (!adapter) {
      throw new Error('WebGPU requestAdapter() returned null.');
    }

    const info = adapter.info;
    const vendor = (info?.vendor ?? 'unknown').toLowerCase();
    const architecture = (info?.architecture ?? 'unknown').toLowerCase();
    const description = info?.description ?? '';
    const legacyFallbackFlag = Boolean(
      (adapter as unknown as { isFallbackAdapter?: boolean }).isFallbackAdapter ||
        (info as unknown as { isFallbackAdapter?: boolean })?.isFallbackAdapter,
    );
    const isSoftware =
      legacyFallbackFlag ||
      vendor.includes('swiftshader') ||
      architecture.includes('swiftshader') ||
      description.toLowerCase().includes('swiftshader') ||
      vendor.includes('llvmpipe');

    const device = await adapter.requestDevice();
    const presentFormat = navigator.gpu.getPreferredCanvasFormat();

    // On hardware GPUs (`!isSoftware`), use zero-copy `GPUCanvasContext` swapchain.
    // On software Vulkan adapters (e.g. headless SwiftShader where ANGLE GLES2 lacks
    // Vulkan SharedImage interop), render 100% on Dawn WebGPU offscreen targets and
    // present onto a 2D surface so `WebGPUSwapBufferProvider` never faults SharedImageStub.
    let gpuContext: GPUCanvasContext | null = null;
    let software2dCtx: CanvasRenderingContext2D | null = null;

    if (!isSoftware) {
      gpuContext = options.canvas.getContext('webgpu');
      if (!gpuContext) {
        device.destroy();
        throw new Error('Failed to acquire WebGPU canvas context.');
      }
      gpuContext.configure({
        device,
        format: presentFormat,
        alphaMode: 'opaque',
      });
    } else {
      software2dCtx = options.canvas.getContext('2d', { alpha: false });
      if (!software2dCtx) {
        gpuContext = options.canvas.getContext('webgpu');
        if (!gpuContext) {
          device.destroy();
          throw new Error('Failed to acquire presentation context for software WebGPU.');
        }
        gpuContext.configure({
          device,
          format: presentFormat,
          alphaMode: 'opaque',
        });
      }
    }

    const shaderModule = device.createShaderModule({
      label: 'osdbits-wgsl',
      code: OSDBITS_WGSL_SOURCE,
    });

    let wgslClean = true;
    if (typeof shaderModule.getCompilationInfo === 'function') {
      const comp = await shaderModule.getCompilationInfo();
      const errors = comp.messages.filter((m) => m.type === 'error');
      if (errors.length > 0) {
        const msg = errors
          .map((e) => `line ${e.lineNum}:${e.linePos} ${e.message}`)
          .join('; ');
        device.destroy();
        throw new Error(`WGSL compilation error: ${msg}`);
      }
      wgslClean = comp.messages.every((m) => m.type !== 'warning');
    }

    const diagnostics: AdapterDiagnostics = {
      backend: 'webgpu',
      isFallbackAdapter: isSoftware,
      vendor: info?.vendor || 'unknown',
      architecture: info?.architecture || 'unknown',
      description,
      wgslCompilationClean: wgslClean,
      degradedReason: isSoftware
        ? 'Running WebGPU on software fallback adapter (SwiftShader/CPU Vulkan).'
        : null,
    };

    return new WebGpuRenderer(
      options.canvas,
      gpuContext,
      software2dCtx,
      device,
      presentFormat,
      diagnostics,
      shaderModule,
      options.onDeviceLost,
    );
  }

  public get layout(): ViewportLayout {
    return this.currentLayout;
  }

  private createBindGroupsForView(view: GPUTextureView): {
    repeatGroup: GPUBindGroup;
    clampGroup: GPUBindGroup;
  } {
    const repeatGroup = this.device.createBindGroup({
      layout: this.bindGroupLayout,
      entries: [
        { binding: 0, resource: this.samplerRepeat },
        { binding: 1, resource: view },
      ],
    });
    const clampGroup = this.device.createBindGroup({
      layout: this.bindGroupLayout,
      entries: [
        { binding: 0, resource: this.samplerClamp },
        { binding: 1, resource: view },
      ],
    });
    return { repeatGroup, clampGroup };
  }

  private uploadRgbaSlot(
    slot: TextureSlot,
    width: number,
    height: number,
    pixels: Uint8Array,
  ): void {
    const prev = this.slots[slot];
    if (prev && prev.ownedStatic) {
      prev.texture.destroy();
    }

    const tex = this.device.createTexture({
      label: `osdbits-slot-${slot}`,
      size: { width, height, depthOrArrayLayers: 1 },
      format: 'rgba8unorm',
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
    });

    this.device.queue.writeTexture(
      { texture: tex },
      pixels.buffer,
      {
        offset: pixels.byteOffset,
        bytesPerRow: width * 4,
        rowsPerImage: height,
      },
      { width, height, depthOrArrayLayers: 1 },
    );

    const view = tex.createView();
    const { repeatGroup, clampGroup } = this.createBindGroupsForView(view);
    this.slots[slot] = {
      texture: tex,
      view,
      repeatGroup,
      clampGroup,
      ownedStatic: true,
    };
  }

  private recreateRenderTargets(layout: ViewportLayout): void {
    this.canvas.width = layout.canvasWidth;
    this.canvas.height = layout.canvasHeight;
    if (this.context) {
      this.context.configure({
        device: this.device,
        format: this.presentFormat,
        alphaMode: 'opaque',
      });
    }
    if (this.softwareBlitOffscreen) {
      this.softwareBlitOffscreen.width = layout.renderWidth;
      this.softwareBlitOffscreen.height = layout.renderHeight;
    }

    this.mainColorTexture?.destroy();
    this.mainDepthTexture?.destroy();
    this.backdropUntintedTex?.destroy();
    this.backdropTintedTex?.destroy();
    this.extraBloomTex?.destroy();

    const w = layout.renderWidth;
    const h = layout.renderHeight;
    const halfW = Math.max(1, w >> 1);
    const halfH = Math.max(1, h >> 1);

    this.mainColorTexture = this.device.createTexture({
      label: 'osdbits-main-color',
      size: { width: w, height: h, depthOrArrayLayers: 1 },
      format: 'rgba8unorm',
      usage:
        GPUTextureUsage.RENDER_ATTACHMENT |
        GPUTextureUsage.TEXTURE_BINDING |
        GPUTextureUsage.COPY_SRC |
        GPUTextureUsage.COPY_DST,
    });
    this.mainColorView = this.mainColorTexture.createView();
    this.mainColorPresentGroup = this.createBindGroupsForView(
      this.mainColorView,
    ).clampGroup;

    this.mainDepthTexture = this.device.createTexture({
      label: 'osdbits-main-depth',
      size: { width: w, height: h, depthOrArrayLayers: 1 },
      format: 'depth24plus',
      usage: GPUTextureUsage.RENDER_ATTACHMENT,
    });
    this.mainDepthView = this.mainDepthTexture.createView();

    this.backdropUntintedTex = this.device.createTexture({
      label: 'osdbits-backdrop-untinted',
      size: { width: w, height: h, depthOrArrayLayers: 1 },
      format: 'rgba8unorm',
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
    });
    const untintedView = this.backdropUntintedTex.createView();
    this.slots[TextureSlot.BackdropUntinted] = {
      texture: this.backdropUntintedTex,
      view: untintedView,
      ...this.createBindGroupsForView(untintedView),
      ownedStatic: false,
    };

    this.backdropTintedTex = this.device.createTexture({
      label: 'osdbits-backdrop-tinted',
      size: { width: w, height: h, depthOrArrayLayers: 1 },
      format: 'rgba8unorm',
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
    });
    const tintedView = this.backdropTintedTex.createView();
    this.slots[TextureSlot.BackdropTinted] = {
      texture: this.backdropTintedTex,
      view: tintedView,
      ...this.createBindGroupsForView(tintedView),
      ownedStatic: false,
    };

    this.extraBloomTex = this.device.createTexture({
      label: 'osdbits-extra-bloom',
      size: { width: halfW, height: halfH, depthOrArrayLayers: 1 },
      format: 'rgba8unorm',
      usage:
        GPUTextureUsage.RENDER_ATTACHMENT |
        GPUTextureUsage.TEXTURE_BINDING |
        GPUTextureUsage.COPY_DST,
    });
    this.extraBloomView = this.extraBloomTex.createView();
    this.slots[TextureSlot.ExtraBloom] = {
      texture: this.extraBloomTex,
      view: this.extraBloomView,
      ...this.createBindGroupsForView(this.extraBloomView),
      ownedStatic: false,
    };
  }

  public syncTextures(engine: OsdbitsWasmEngine): void {
    if (this.disposed) return;
    const gen = engine.textureGeneration;
    if (gen === this.uploadedTextureGen) return;

    for (const tex of engine.getAllTextures()) {
      this.uploadRgbaSlot(tex.slot, tex.width, tex.height, tex.pixels);
    }
    this.uploadedTextureGen = gen;
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
      resolutionScale,
      aspectMode,
    );
    const renderChanged =
      next.renderWidth !== this.currentLayout.renderWidth ||
      next.renderHeight !== this.currentLayout.renderHeight ||
      next.canvasWidth !== this.currentLayout.canvasWidth ||
      next.canvasHeight !== this.currentLayout.canvasHeight;
    this.currentLayout = next;
    if (renderChanged && !this.disposed) {
      this.recreateRenderTargets(next);
    }
    return next;
  }

  private ensureVertexCapacity(byteLength: number): void {
    if (byteLength <= this.vertexBufferCapacity) return;
    let nextCap = this.vertexBufferCapacity;
    while (nextCap < byteLength) {
      nextCap *= 2;
    }
    this.vertexBuffer.destroy();
    this.vertexBufferCapacity = nextCap;
    this.vertexBuffer = this.device.createBuffer({
      label: 'osdbits-vertices',
      size: nextCap,
      usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
    });
  }

  public renderFrame(
    engine: OsdbitsWasmEngine,
    telemetry: FrameTelemetrySnapshot,
  ): void {
    if (this.disposed) return;
    this.syncTextures(engine);

    const vertexBytes = engine.getVertexBytes();
    if (vertexBytes.byteLength > 0) {
      this.ensureVertexCapacity(vertexBytes.byteLength);
      this.device.queue.writeBuffer(
        this.vertexBuffer,
        0,
        vertexBytes.buffer,
        vertexBytes.byteOffset,
        vertexBytes.byteLength,
      );
    }

    const batches = engine.getBatches();
    const encoder = this.device.createCommandEncoder({ label: 'osdbits-frame' });

    const w = this.currentLayout.renderWidth;
    const h = this.currentLayout.renderHeight;
    const [cr, cg, cb, ca] = telemetry.clearColor;

    let mainCleared = false;
    let activePass: GPURenderPassEncoder | null = null;

    const openMainPass = (): GPURenderPassEncoder => {
      if (activePass) return activePass;
      const loadOp: GPULoadOp = mainCleared ? 'load' : 'clear';
      mainCleared = true;
      activePass = encoder.beginRenderPass({
        label: 'osdbits-main-pass',
        colorAttachments: [
          {
            view: this.mainColorView,
            clearValue: { r: cr, g: cg, b: cb, a: ca },
            loadOp,
            storeOp: 'store',
          },
        ],
        depthStencilAttachment: {
          view: this.mainDepthView,
          depthClearValue: 1.0,
          depthLoadOp: loadOp,
          depthStoreOp: 'store',
        },
      });
      activePass.setVertexBuffer(0, this.vertexBuffer);
      return activePass;
    };

    const closeMainPass = (): void => {
      if (activePass) {
        activePass.end();
        activePass = null;
      }
    };

    openMainPass();

    for (const batch of batches) {
      if (
        (batch.passAction &
          (PassActionBit.CaptureUntintedBackdrop |
            PassActionBit.CaptureTintedBackdrop |
            PassActionBit.CaptureBloomDownsample)) !==
        0
      ) {
        closeMainPass();
        if ((batch.passAction & PassActionBit.CaptureUntintedBackdrop) !== 0) {
          encoder.copyTextureToTexture(
            { texture: this.mainColorTexture },
            { texture: this.backdropUntintedTex },
            { width: w, height: h, depthOrArrayLayers: 1 },
          );
        }
        if ((batch.passAction & PassActionBit.CaptureTintedBackdrop) !== 0) {
          encoder.copyTextureToTexture(
            { texture: this.mainColorTexture },
            { texture: this.backdropTintedTex },
            { width: w, height: h, depthOrArrayLayers: 1 },
          );
        }
        if ((batch.passAction & PassActionBit.CaptureBloomDownsample) !== 0) {
          const bloomPass = encoder.beginRenderPass({
            label: 'osdbits-downsample-pass',
            colorAttachments: [
              {
                view: this.extraBloomView,
                clearValue: { r: 0, g: 0, b: 0, a: 1 },
                loadOp: 'clear',
                storeOp: 'store',
              },
            ],
          });
          bloomPass.setPipeline(this.downsamplePipeline);
          bloomPass.setBindGroup(0, this.mainColorPresentGroup);
          bloomPass.draw(3, 1, 0, 0);
          bloomPass.end();
        }
      }

      if (batch.vertexCount === 0) {
        continue;
      }

      if (batch.targetId === TargetId.ExtraBloom) {
        closeMainPass();
        const extraPass = encoder.beginRenderPass({
          label: 'osdbits-extrabloom-draw',
          colorAttachments: [
            {
              view: this.extraBloomView,
              loadOp: 'load',
              storeOp: 'store',
            },
          ],
        });
        const pipe =
          this.scenePipelines.get(batch.blendMode * 8 + DepthMode.Disabled) ??
          this.scenePipelines.get(0)!;
        const slot = this.slots[batch.textureId] ?? this.slots[TextureSlot.Untextured]!;
        extraPass.setPipeline(pipe);
        extraPass.setVertexBuffer(0, this.vertexBuffer);
        extraPass.setBindGroup(
          0,
          (batch.passAction & PassActionBit.ClampUv) !== 0
            ? slot.clampGroup
            : slot.repeatGroup,
        );
        extraPass.draw(batch.vertexCount, 1, batch.firstVertex, 0);
        extraPass.end();
        continue;
      }

      const pass = openMainPass();
      const pipeKey = batch.blendMode * 8 + batch.depthMode;
      const pipeline = this.scenePipelines.get(pipeKey) ?? this.scenePipelines.get(0)!;
      const slot = this.slots[batch.textureId] ?? this.slots[TextureSlot.Untextured]!;
      pass.setPipeline(pipeline);
      pass.setBindGroup(
        0,
        (batch.passAction & PassActionBit.ClampUv) !== 0
          ? slot.clampGroup
          : slot.repeatGroup,
      );
      pass.draw(batch.vertexCount, 1, batch.firstVertex, 0);
    }

    closeMainPass();

    if (this.context) {
      const swapTex = this.context.getCurrentTexture();
      const presentPass = encoder.beginRenderPass({
        label: 'osdbits-present-pass',
        colorAttachments: [
          {
            view: swapTex.createView(),
            clearValue: { r: 0, g: 0, b: 0, a: 1 },
            loadOp: 'clear',
            storeOp: 'store',
          },
        ],
      });
      presentPass.setViewport(
        this.currentLayout.viewportX,
        this.currentLayout.viewportY,
        this.currentLayout.viewportWidth,
        this.currentLayout.viewportHeight,
        0,
        1,
      );
      presentPass.setPipeline(this.presentPipeline);
      presentPass.setBindGroup(0, this.mainColorPresentGroup);
      presentPass.draw(3, 1, 0, 0);
      presentPass.end();
    }

    this.device.queue.submit([encoder.finish()]);

    if (this.softwareBlitCtx && !this.softwarePresentBusy) {
      this.softwarePresentBusy = true;
      void this.readPixels()
        .then((pb) => {
          if (
            this.disposed ||
            !this.softwareBlitCtx ||
            !this.softwareBlitOffscreen ||
            !this.softwareBlitOffscreenCtx
          ) {
            return;
          }
          if (
            this.softwareBlitOffscreen.width !== pb.width ||
            this.softwareBlitOffscreen.height !== pb.height
          ) {
            this.softwareBlitOffscreen.width = pb.width;
            this.softwareBlitOffscreen.height = pb.height;
          }
          const clamped = new Uint8ClampedArray(pb.rgba.byteLength);
          clamped.set(pb.rgba);
          this.softwareBlitOffscreenCtx.putImageData(
            new ImageData(clamped, pb.width, pb.height),
            0,
            0,
          );
          this.softwareBlitCtx.fillStyle = '#000000';
          this.softwareBlitCtx.fillRect(
            0,
            0,
            this.currentLayout.canvasWidth,
            this.currentLayout.canvasHeight,
          );
          this.softwareBlitCtx.drawImage(
            this.softwareBlitOffscreen,
            0,
            0,
            pb.width,
            pb.height,
            this.currentLayout.viewportX,
            this.currentLayout.viewportY,
            this.currentLayout.viewportWidth,
            this.currentLayout.viewportHeight,
          );
        })
        .catch(() => undefined)
        .finally(() => {
          this.softwarePresentBusy = false;
        });
    }
  }

  public async readPixels(): Promise<PixelReadback> {
    const w = this.currentLayout.renderWidth;
    const h = this.currentLayout.renderHeight;
    const bytesPerRow =
      Math.ceil((w * 4) / BYTES_PER_ROW_ALIGN) * BYTES_PER_ROW_ALIGN;
    const bufferSize = bytesPerRow * h;

    const staging = this.device.createBuffer({
      label: 'osdbits-pixel-readback',
      size: bufferSize,
      usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });

    const encoder = this.device.createCommandEncoder({ label: 'osdbits-readback' });
    encoder.copyTextureToBuffer(
      { texture: this.mainColorTexture },
      { buffer: staging, bytesPerRow, rowsPerImage: h },
      { width: w, height: h, depthOrArrayLayers: 1 },
    );
    this.device.queue.submit([encoder.finish()]);

    await staging.mapAsync(GPUMapMode.READ);
    try {
      const mapped = new Uint8Array(staging.getMappedRange());
      const rgba = new Uint8Array(w * h * 4);
      for (let row = 0; row < h; row++) {
        const srcOff = row * bytesPerRow;
        const dstOff = row * w * 4;
        rgba.set(mapped.subarray(srcOff, srcOff + w * 4), dstOff);
      }
      return { width: w, height: h, rgba };
    } finally {
      staging.unmap();
      staging.destroy();
    }
  }

  public dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    const releaseGpu = (): void => {
      try {
        this.vertexBuffer.destroy();
        this.mainColorTexture?.destroy();
        this.mainDepthTexture?.destroy();
        this.backdropUntintedTex?.destroy();
        this.backdropTintedTex?.destroy();
        this.extraBloomTex?.destroy();
        for (const s of this.slots) {
          if (s?.ownedStatic) {
            s.texture.destroy();
          }
        }
      } catch {
        // Ignore already-destroyed resources during page teardown
      }
    };
    if (this.softwarePresentBusy) {
      setTimeout(releaseGpu, 60);
    } else {
      releaseGpu();
    }
  }
}
