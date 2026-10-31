/**
 * WebGPU execution of the WASM render command stream.
 *
 * The Rust core produces one flat list of render commands per frame plus an interleaved vertex
 * buffer. This renderer turns that into WebGPU render passes:
 *
 *  - `GsBlendMode` selects the blend state (the GS `ALPHA_1` equation), so the same triangles
 *    the PS2 would send composite the same way.
 *  - `DepthMode` selects the depth compare/write pair (the GS `TEST_1` / `ZBUF_1` bits).
 *  - `SamplerWrap` picks a CLAMP or REPEAT sampler (the per-primitive `CLAMP_1` register).
 *  - Render targets are the screen attachment and the two work buffers; `copyTextureToTexture`
 *    performs the capture passes (`CubeCaptureBuffer`, `DrawToExtraBuf2`).
 */

import {
  GsBlendMode,
  RenderCmdKind,
  RenderTargetId,
  SamplerWrap,
  STATIC_TEXTURE_COUNT,
  TextureId,
  VERTEX_BYTES,
  decodeCommands,
  type WasmCore,
} from '../wasm/bridge';
import { GpuContext, LOGICAL_HEIGHT, LOGICAL_WIDTH } from './gpu-context';
import { blitShader, sceneShader } from './shaders';

const UNIFORM_STRIDE = 256;
const MAX_DRAWS_PER_FRAME = 8192;

interface TextureEntry {
  texture: GPUTexture;
  view: GPUTextureView;
  width: number;
  height: number;
}

function blendState(mode: GsBlendMode): GPUBlendState {
  const alphaOver: GPUBlendComponent = {
    srcFactor: 'one',
    dstFactor: 'one-minus-src-alpha',
    operation: 'add',
  };
  switch (mode) {
    case GsBlendMode.Opaque:
      return {
        color: { srcFactor: 'one', dstFactor: 'zero', operation: 'add' },
        alpha: alphaOver,
      };
    case GsBlendMode.SourceAlpha:
      return {
        color: { srcFactor: 'src-alpha', dstFactor: 'one-minus-src-alpha', operation: 'add' },
        alpha: alphaOver,
      };
    case GsBlendMode.Additive:
      return {
        color: { srcFactor: 'src-alpha', dstFactor: 'one', operation: 'add' },
        alpha: { srcFactor: 'one', dstFactor: 'one', operation: 'add' },
      };
    case GsBlendMode.Subtractive:
      return {
        color: { srcFactor: 'src-alpha', dstFactor: 'one', operation: 'reverse-subtract' },
        alpha: alphaOver,
      };
    case GsBlendMode.FixedAdditive:
      return {
        color: { srcFactor: 'one', dstFactor: 'one', operation: 'add' },
        alpha: { srcFactor: 'one', dstFactor: 'one', operation: 'add' },
      };
    case GsBlendMode.FixedBlend:
      return {
        color: { srcFactor: 'src-alpha', dstFactor: 'one-minus-src-alpha', operation: 'add' },
        alpha: alphaOver,
      };
    case GsBlendMode.DestAlphaAdditive:
      return {
        color: { srcFactor: 'one', dstFactor: 'one', operation: 'add' },
        alpha: { srcFactor: 'one', dstFactor: 'one', operation: 'add' },
      };
    default:
      return {
        color: { srcFactor: 'src-alpha', dstFactor: 'one-minus-src-alpha', operation: 'add' },
        alpha: alphaOver,
      };
  }
}

function depthState(mode: number): { depthWriteEnabled: boolean; depthCompare: GPUCompareFunction } {
  switch (mode) {
    case 1:
      return { depthWriteEnabled: true, depthCompare: 'always' };
    case 2:
      return { depthWriteEnabled: true, depthCompare: 'less-equal' };
    case 3:
      return { depthWriteEnabled: false, depthCompare: 'less-equal' };
    case 4:
      return { depthWriteEnabled: true, depthCompare: 'greater' };
    default:
      return { depthWriteEnabled: false, depthCompare: 'always' };
  }
}

function rgbaToColor(rgba: number): GPUColor {
  return {
    r: (rgba & 0xff) / 255,
    g: ((rgba >> 8) & 0xff) / 255,
    b: ((rgba >> 16) & 0xff) / 255,
    a: ((rgba >> 24) & 0xff) / 255,
  };
}

export interface RenderStats {
  draws: number;
  passes: number;
  vertices: number;
}

export class FrameRenderer {
  private readonly gpu: GpuContext;
  private readonly sceneBindGroupLayout: GPUBindGroupLayout;
  private readonly scenePipelineLayout: GPUPipelineLayout;
  private readonly blitPipelineLayout: GPUPipelineLayout;
  private textures: TextureEntry[] = [];
  private readonly copyTextures = new Map<number, TextureEntry>();
  private readonly clampSampler: GPUSampler;
  private readonly repeatSampler: GPUSampler;
  private vertexBuffer: GPUBuffer;
  private vertexCapacity: number;
  private readonly uniformBuffer: GPUBuffer;
  private readonly blitUniformBuffer: GPUBuffer;
  private readonly presentPipelines = new Map<string, GPURenderPipeline>();
  private readonly pipelines = new Map<string, GPURenderPipeline>();
  private readonly bindGroups = new Map<string, GPUBindGroup>();
  private readonly uniformScratch = new Float32Array(MAX_DRAWS_PER_FRAME * (UNIFORM_STRIDE / 4));
  private lastStats: RenderStats = { draws: 0, passes: 0, vertices: 0 };

  constructor(gpu: GpuContext, core: WasmCore) {
    this.gpu = gpu;
    const device = gpu.device;

    this.sceneBindGroupLayout = device.createBindGroupLayout({
      label: 'osd-scene-bgl',
      entries: [
        {
          binding: 0,
          visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT,
          buffer: { type: 'uniform', hasDynamicOffset: true, minBindingSize: 32 },
        },
        { binding: 1, visibility: GPUShaderStage.FRAGMENT, sampler: { type: 'filtering' } },
        { binding: 2, visibility: GPUShaderStage.FRAGMENT, texture: { sampleType: 'float' } },
      ],
    });
    this.scenePipelineLayout = device.createPipelineLayout({
      label: 'osd-scene-layout',
      bindGroupLayouts: [this.sceneBindGroupLayout],
    });
    const blitBindGroupLayout = device.createBindGroupLayout({
      label: 'osd-blit-bgl',
      entries: [
        { binding: 0, visibility: GPUShaderStage.FRAGMENT, buffer: { type: 'uniform' } },
        { binding: 1, visibility: GPUShaderStage.FRAGMENT, sampler: { type: 'filtering' } },
        { binding: 2, visibility: GPUShaderStage.FRAGMENT, texture: { sampleType: 'float' } },
      ],
    });
    this.blitPipelineLayout = device.createPipelineLayout({
      label: 'osd-blit-layout',
      bindGroupLayouts: [blitBindGroupLayout],
    });

    this.clampSampler = device.createSampler({
      addressModeU: 'clamp-to-edge',
      addressModeV: 'clamp-to-edge',
      magFilter: 'linear',
      minFilter: 'linear',
      label: 'clamp-linear',
    });
    this.repeatSampler = device.createSampler({
      addressModeU: 'repeat',
      addressModeV: 'repeat',
      magFilter: 'linear',
      minFilter: 'linear',
      label: 'repeat-linear',
    });

    this.vertexCapacity = 8 * 1024 * 1024;
    this.vertexBuffer = device.createBuffer({
      label: 'osd-vertices',
      size: this.vertexCapacity,
      usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
    });
    this.uniformBuffer = device.createBuffer({
      label: 'osd-uniforms',
      size: UNIFORM_STRIDE * MAX_DRAWS_PER_FRAME,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    this.blitUniformBuffer = device.createBuffer({
      label: 'osd-blit-uniforms',
      size: 256,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });

    this.uploadStaticTextures(core);
    this.createDynamicTargets();
    this.createPresentBindGroup(blitBindGroupLayout);
  }

  private presentBindGroups = new Map<string, GPUBindGroup>();
  private blitLayout: GPUBindGroupLayout | null = null;

  private createPresentBindGroup(layout: GPUBindGroupLayout): void {
    this.blitLayout = layout;
  }

  private uploadStaticTextures(core: WasmCore): void {
    const device = this.gpu.device;
    const count = Math.min(core.textureCount(), STATIC_TEXTURE_COUNT);
    for (let id = 0; id < count; id += 1) {
      const { width, height, rgba } = core.texture(id);
      const texture = device.createTexture({
        label: `osd-tex-${id}`,
        size: { width, height, depthOrArrayLayers: 1 },
        format: 'rgba8unorm',
        usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
      });
      device.queue.writeTexture(
        { texture },
        rgba as unknown as ArrayBufferView<ArrayBuffer>,
        { bytesPerRow: width * 4, rowsPerImage: height },
        { width, height, depthOrArrayLayers: 1 },
      );
      this.textures.push({ texture, view: texture.createView(), width, height });
    }
  }

  private createDynamicTargets(): void {
    const device = this.gpu.device;
    const make = (label: string): TextureEntry => {
      const texture = device.createTexture({
        label,
        size: { width: LOGICAL_WIDTH, height: LOGICAL_HEIGHT, depthOrArrayLayers: 1 },
        format: 'rgba8unorm',
        usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST | GPUTextureUsage.COPY_SRC,
      });
      return { texture, view: texture.createView(), width: LOGICAL_WIDTH, height: LOGICAL_HEIGHT };
    };
    this.copyTextures.set(TextureId.ScreenCopy, make('osd-copy-screen'));
    this.copyTextures.set(TextureId.WorkBuf3, make('osd-copy-wb3'));
    this.copyTextures.set(TextureId.WorkBuf4, make('osd-copy-wb4'));
  }

  /** Re-uploads all static textures (after importing a BIOS/ROMDIR asset pack). */
  reloadTextures(core: WasmCore): void {
    for (const entry of this.textures) entry.texture.destroy();
    this.textures = [];
    this.bindGroups.clear();
    this.uploadStaticTextures(core);
  }

  private textureEntry(id: number): TextureEntry {
    const entry = this.textures[id];
    if (entry) return entry;
    const copy = this.copyTextures.get(id);
    if (copy) return copy;
    return this.textures[TextureId.WhiteDummy]!;
  }

  private targetView(target: number): GPUTextureView {
    switch (target) {
      case RenderTargetId.WorkBuf3:
        return this.gpu.workBuf3.view;
      case RenderTargetId.WorkBuf4:
        return this.gpu.workBuf4.view;
      default:
        return this.gpu.screen.view;
    }
  }

  private targetTexture(target: number): GPUTexture {
    switch (target) {
      case RenderTargetId.WorkBuf3:
        return this.gpu.workBuf3.texture;
      case RenderTargetId.WorkBuf4:
        return this.gpu.workBuf4.texture;
      default:
        return this.gpu.screen.texture;
    }
  }

  private pipelineFor(blend: GsBlendMode, depth: number, untextured: boolean): GPURenderPipeline {
    const key = `${blend}|${depth}|${untextured ? 'flat' : 'tex'}`;
    const cached = this.pipelines.get(key);
    if (cached) return cached;
    const device = this.gpu.device;
    const module = device.createShaderModule({ label: `osd-scene-${key}`, code: sceneShader });
    const ds = depthState(depth);
    const pipeline = device.createRenderPipeline({
      label: `osd-pipeline-${key}`,
      layout: this.scenePipelineLayout,
      vertex: {
        module,
        entryPoint: 'vs_main',
        buffers: [
          {
            arrayStride: VERTEX_BYTES,
            attributes: [
              { shaderLocation: 0, offset: 0, format: 'float32x4' },
              { shaderLocation: 1, offset: 16, format: 'float32x2' },
              { shaderLocation: 2, offset: 24, format: 'float32x4' },
            ],
          },
        ],
      },
      fragment: {
        module,
        entryPoint: untextured ? 'fs_flat' : 'fs_modulate',
        targets: [{ format: 'rgba8unorm', blend: blendState(blend) }],
      },
      primitive: { topology: 'triangle-list', cullMode: 'none', frontFace: 'ccw' },
      depthStencil: {
        format: 'depth24plus',
        depthWriteEnabled: ds.depthWriteEnabled,
        depthCompare: ds.depthCompare,
      },
    });
    this.pipelines.set(key, pipeline);
    return pipeline;
  }

  private bindGroupFor(textureId: number, wrap: SamplerWrap): GPUBindGroup {
    const key = `${textureId}|${wrap}`;
    const cached = this.bindGroups.get(key);
    if (cached) return cached;
    const group = this.gpu.device.createBindGroup({
      label: `osd-bg-${key}`,
      layout: this.sceneBindGroupLayout,
      entries: [
        { binding: 0, resource: { buffer: this.uniformBuffer, offset: 0, size: 32 } },
        {
          binding: 1,
          resource: wrap === SamplerWrap.Repeat ? this.repeatSampler : this.clampSampler,
        },
        { binding: 2, resource: this.textureEntry(textureId).view },
      ],
    });
    this.bindGroups.set(key, group);
    return group;
  }

  private presentPipeline(tint: boolean): GPURenderPipeline {
    const key = tint ? 'tint' : 'present';
    const cached = this.presentPipelines.get(key);
    if (cached) return cached;
    const device = this.gpu.device;
    const module = device.createShaderModule({ label: `osd-blit-${key}`, code: blitShader });
    const pipeline = device.createRenderPipeline({
      label: `osd-blit-pipeline-${key}`,
      layout: this.blitPipelineLayout,
      vertex: { module, entryPoint: 'vs_main' },
      fragment: {
        module,
        entryPoint: tint ? 'fs_main' : 'fs_present',
        targets: [{ format: this.gpu.format }],
      },
      primitive: { topology: 'triangle-list' },
    });
    this.presentPipelines.set(key, pipeline);
    return pipeline;
  }

  private presentBindGroup(texture: GPUTextureView, alpha: number, tint: [number, number, number]): GPUBindGroup {
    const device = this.gpu.device;
    // One small uniform buffer per present is cheap and keeps the state obvious.
    const data = new Float32Array(8);
    data[0] = LOGICAL_WIDTH;
    data[1] = LOGICAL_HEIGHT;
    data[2] = alpha;
    data[3] = tint[0];
    data[4] = tint[1];
    data[5] = tint[2];
    device.queue.writeBuffer(this.blitUniformBuffer, 0, data);
    if (!this.blitLayout) throw new Error('blit layout missing');
    return device.createBindGroup({
      label: 'osd-present-bg',
      layout: this.blitLayout,
      entries: [
        { binding: 0, resource: { buffer: this.blitUniformBuffer, offset: 0, size: 32 } },
        { binding: 1, resource: this.clampSampler },
        { binding: 2, resource: texture },
      ],
    });
  }

  /** Uploads this frame's buffers and records the whole frame, including presentation. */
  render(core: WasmCore): RenderStats {
    const device = this.gpu.device;
    const vertexView = core.vertexView();
    if (vertexView.byteLength > this.vertexCapacity) {
      this.vertexBuffer.destroy();
      this.vertexCapacity = Math.ceil(vertexView.byteLength / (1024 * 1024)) * 1024 * 1024;
      this.vertexBuffer = device.createBuffer({
        label: 'osd-vertices',
        size: this.vertexCapacity,
        usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
      });
    }
    if (vertexView.byteLength > 0) {
      device.queue.writeBuffer(
        this.vertexBuffer,
        0,
        vertexView as unknown as ArrayBufferView<ArrayBuffer>,
      );
    }

    const commands = decodeCommands(core.commandView());
    const encoder = device.createCommandEncoder({ label: 'osd-frame' });
    const uniformData = this.uniformScratch;
    uniformData.fill(0);

    let drawIndex = 0;
    let pass: GPURenderPassEncoder | null = null;
    let passTarget = -1;
    let passes = 0;

    const endPass = (): void => {
      if (pass) {
        pass.end();
        pass = null;
        passTarget = -1;
      }
    };

    const beginPass = (target: number, clearRgba: number | null): GPURenderPassEncoder => {
      const encoderPass = encoder.beginRenderPass({
        label: `osd-pass-${passes}`,
        colorAttachments: [
          {
            view: this.targetView(target),
            clearValue: clearRgba === null ? { r: 0, g: 0, b: 0, a: 0 } : rgbaToColor(clearRgba),
            loadOp: clearRgba === null ? 'load' : 'clear',
            storeOp: 'store',
          },
        ],
        depthStencilAttachment: {
          view: this.gpu.depthView,
          depthClearValue: 1,
          depthLoadOp: clearRgba === null ? 'load' : 'clear',
          depthStoreOp: 'store',
        },
      });
      passTarget = target;
      passes += 1;
      return encoderPass;
    };

    for (const cmd of commands) {
      if (cmd.kind === RenderCmdKind.ClearTarget) {
        endPass();
        pass = beginPass(cmd.target, cmd.clearRgba8);
        continue;
      }
      if (cmd.kind === RenderCmdKind.CopyTargetToTexture) {
        endPass();
        const dest = this.copyTextures.get(cmd.textureId);
        if (dest) {
          encoder.copyTextureToTexture(
            { texture: this.targetTexture(cmd.target) },
            { texture: dest.texture },
            { width: LOGICAL_WIDTH, height: LOGICAL_HEIGHT, depthOrArrayLayers: 1 },
          );
        }
        continue;
      }
      if (cmd.kind !== RenderCmdKind.DrawTriangles || cmd.vertexCount === 0) continue;
      if (drawIndex >= MAX_DRAWS_PER_FRAME) break;

      if (pass === null || passTarget !== cmd.target) {
        endPass();
        pass = beginPass(cmd.target, null);
      }
      const current: GPURenderPassEncoder = pass;

      const slot = drawIndex * (UNIFORM_STRIDE / 4);
      uniformData[slot] = LOGICAL_WIDTH;
      uniformData[slot + 1] = LOGICAL_HEIGHT;
      uniformData[slot + 2] = 1 / LOGICAL_WIDTH;
      uniformData[slot + 3] = 1 / LOGICAL_HEIGHT;
      uniformData[slot + 4] = cmd.blendFix;

      const untextured = cmd.textureId === TextureId.WhiteDummy;
      current.setPipeline(this.pipelineFor(cmd.blendMode, cmd.depthMode, untextured));
      current.setBindGroup(0, this.bindGroupFor(cmd.textureId, cmd.samplerWrap), [
        drawIndex * UNIFORM_STRIDE,
      ]);
      current.setVertexBuffer(0, this.vertexBuffer);
      current.draw(cmd.vertexCount, 1, cmd.firstVertex, 0);
      drawIndex += 1;
    }
    endPass();

    if (drawIndex > 0) {
      device.queue.writeBuffer(
        this.uniformBuffer,
        0,
        uniformData.buffer as ArrayBuffer,
        0,
        drawIndex * (UNIFORM_STRIDE / 4) * 4,
      );
    }

    // Presentation: a full-screen triangle resampling the 640x448 screen to the swap chain.
    const presentPass = encoder.beginRenderPass({
      label: 'osd-present',
      colorAttachments: [
        {
          view: this.gpu.context.getCurrentTexture().createView(),
          clearValue: { r: 0, g: 0, b: 0, a: 1 },
          loadOp: 'clear',
          storeOp: 'store',
        },
      ],
    });
    presentPass.setPipeline(this.presentPipeline(false));
    presentPass.setBindGroup(
      0,
      this.presentBindGroup(this.gpu.screen.view, 1, [1, 1, 1]),
    );
    presentPass.draw(3, 1, 0, 0);
    presentPass.end();
    passes += 1;

    device.queue.submit([encoder.finish()]);

    this.lastStats = {
      draws: drawIndex,
      passes,
      vertices: vertexView.byteLength / VERTEX_BYTES,
    };
    return this.lastStats;
  }

  stats(): RenderStats {
    return this.lastStats;
  }

  dispose(): void {
    for (const entry of this.textures) entry.texture.destroy();
    for (const entry of this.copyTextures.values()) entry.texture.destroy();
    this.vertexBuffer.destroy();
    this.uniformBuffer.destroy();
    this.blitUniformBuffer.destroy();
  }
}
