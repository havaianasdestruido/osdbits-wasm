/**
 * WebGPU renderer for the core's draw list.
 *
 * Batch semantics come straight from the frame (`prim`, `tex`, `blend`); the
 * per-batch uniform carries the view matrix and the primitive mode. Pipelines
 * exist per (primitive mode, blend) pair because WebGPU bakes blend state into
 * the pipeline.
 *
 * Line primitives and cubes are expanded to triangles on the CPU into one
 * scratch buffer and drawn through the triangle pipelines: lines because
 * `line-list` cannot be thickened, cubes because their faces need per-face
 * colour and light.
 *
 * Known approximation (docs/LIMITATIONS.md): the GS draws each cube as a ten
 * pass ping-pong through a captured framebuffer. This renderer draws the lit,
 * front-facing faces once with alpha blending — no capture buffer, no
 * refraction passes — because reproducing it needs ten render targets and an
 * exact framebuffer copy per cube.
 */

import { BLEND, PRIM, VERTEX_WORDS } from "../abi.js";
import type { Frame } from "../frame.js";
import { CUBE_FACE_VERTS, CUBE_FACE_NORMAL } from "../cubeGeometry.js";
import { SHADER } from "./shaders.js";
import type { ProceduralTexture } from "../textures.js";

export interface RendererOptions {
  antialias: boolean;
  /** Draw the cube passes at all (quality preset). */
  cubePasses: boolean;
  /** Hard cap on batches per frame, for the low-quality preset. */
  maxBatches: number;
}

export interface RendererStats {
  draws: number;
  triangles: number;
  batchesSkipped: number;
  batchesDrawn: number;
}

/** Must be a multiple of the device's minUniformBufferOffsetAlignment (256). */
const UNIFORM_STRIDE = 256;
const MAX_BATCHES = 256;
/** Two slots after the batches, for the cube and line draws. */
const AUX_WORLD = 254;
const AUX_SCREEN = 255;
const FLOATS_PER_UNIFORM = UNIFORM_STRIDE / 4;
const VERTEX_FLOATS = VERTEX_WORDS;

/** Blend mapping; the GS ALPHA register semantics are in docs/WASM-ABI.md. */
export function blendState(blend: number): GPUBlendState | undefined {
  switch (blend) {
    case BLEND.NONE:
      return undefined;
    case BLEND.ALPHA:
      return {
        color: { srcFactor: "src-alpha", dstFactor: "one-minus-src-alpha", operation: "add" },
        alpha: { srcFactor: "one", dstFactor: "one-minus-src-alpha", operation: "add" },
      };
    case BLEND.ADD:
    case BLEND.ADD_DEST_ALPHA:
    case BLEND.FIXALPHA:
      // Cs*As + Cd: the GS mode 0/5/8 family. FIXALPHA and ADD_DEST_ALPHA are
      // approximated with the vertex alpha channel.
      return {
        color: { srcFactor: "src-alpha", dstFactor: "one", operation: "add" },
        alpha: { srcFactor: "one", dstFactor: "one", operation: "add" },
      };
    case BLEND.SUB:
      return {
        color: { srcFactor: "one", dstFactor: "one", operation: "reverse-subtract" },
        alpha: { srcFactor: "zero", dstFactor: "one", operation: "add" },
      };
    case BLEND.MUL:
      return {
        color: { srcFactor: "zero", dstFactor: "src", operation: "add" },
        alpha: { srcFactor: "zero", dstFactor: "one", operation: "add" },
      };
    default:
      return {
        color: { srcFactor: "src-alpha", dstFactor: "one-minus-src-alpha", operation: "add" },
        alpha: { srcFactor: "one", dstFactor: "one-minus-src-alpha", operation: "add" },
      };
  }
}

export class WebGpuRenderer {
  readonly kind = "webgpu" as const;
  readonly device: GPUDevice;
  readonly info: { format: string; textures: number };
  private readonly context: GPUCanvasContext;
  private readonly format: GPUTextureFormat;
  private readonly shader: GPUShaderModule;
  private readonly pipelines = new Map<string, GPURenderPipeline>();
  private readonly uniformBuffer: GPUBuffer;
  private readonly uniformData = new Float32Array(MAX_BATCHES * FLOATS_PER_UNIFORM);
  private readonly sampler: GPUSampler;
  private readonly textures = new Map<number, GPUTexture>();
  private readonly bindGroups = new Map<number, GPUBindGroup>();
  private readonly bindGroupLayout: GPUBindGroupLayout;
  private vertexBuffer: GPUBuffer | null = null;
  private vertexCapacity = 0;
  private depth: GPUTexture | null = null;
  private depthSize: [number, number] = [0, 0];
  private scratch = new Float32Array(64 * 1024);
  private scratchLen = 0;
  private readonly options: RendererOptions;
  private disposed = false;

  private constructor(
    device: GPUDevice,
    context: GPUCanvasContext,
    format: GPUTextureFormat,
    textures: readonly ProceduralTexture[],
    options: RendererOptions,
  ) {
    this.device = device;
    this.context = context;
    this.format = format;
    this.options = options;
    this.shader = device.createShaderModule({ code: SHADER, label: "osd-shader" });
    this.sampler = device.createSampler({
      magFilter: "linear",
      minFilter: "linear",
      addressModeU: "clamp-to-edge",
      addressModeV: "clamp-to-edge",
    });
    this.uniformBuffer = device.createBuffer({
      size: MAX_BATCHES * UNIFORM_STRIDE,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
      label: "osd-batch-uniforms",
    });
    this.bindGroupLayout = device.createBindGroupLayout({
      entries: [
        {
          binding: 0,
          visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT,
          buffer: { type: "uniform", hasDynamicOffset: true, minBindingSize: 96 },
        },
        { binding: 1, visibility: GPUShaderStage.FRAGMENT, texture: { sampleType: "float" } },
        { binding: 2, visibility: GPUShaderStage.FRAGMENT, sampler: { type: "filtering" } },
      ],
      label: "osd-bgl",
    });

    const upload = (id: number, name: string, width: number, height: number, data: Uint8Array): void => {
      const texture = device.createTexture({
        size: { width, height },
        format: "rgba8unorm",
        usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
        label: `osd-tex-${name}`,
      });
      device.queue.writeTexture(
        { texture },
        data as unknown as Uint8Array<ArrayBuffer>,
        { bytesPerRow: width * 4, rowsPerImage: height },
        { width, height },
      );
      this.textures.set(id, texture);
    };
    for (const t of textures) upload(t.id, t.name, t.width, t.height, t.data);
    // 1x1 white, used for every untextured batch.
    upload(0, "white", 1, 1, new Uint8Array([255, 255, 255, 255]));
    this.info = { format, textures: this.textures.size };
  }

  static async create(
    canvas: HTMLCanvasElement,
    textures: readonly ProceduralTexture[],
    options: RendererOptions,
  ): Promise<WebGpuRenderer> {
    const gpu = (navigator as Navigator & { gpu?: GPU }).gpu;
    if (!gpu) throw new Error("WebGPU is not available (navigator.gpu is undefined)");
    const adapter = await gpu.requestAdapter({ powerPreference: "high-performance" });
    if (!adapter) throw new Error("no WebGPU adapter: the GPU is blocked or unavailable");
    const device = await adapter.requestDevice({ label: "osd-device" });
    device.lost.then((info) => {
      // The engine listens through `onLost`.
      this.lostHandler?.(info.message || info.reason);
    });
    const context = canvas.getContext("webgpu");
    if (!context) throw new Error("could not obtain a WebGPU canvas context");
    const format = gpu.getPreferredCanvasFormat();
    context.configure({ device, format, alphaMode: "opaque" });
    return new WebGpuRenderer(device, context, format, textures, options);
  }

  /** Optional device-loss callback (set by the engine before the first frame). */
  static lostHandler: ((message: string) => void) | null = null;

  private pipeline(mode: 0 | 1 | 2, blend: number): GPURenderPipeline {
    const key = `${mode}:${blend}`;
    const existing = this.pipelines.get(key);
    if (existing) return existing;
    const opaque = blend === BLEND.NONE;
    const pipeline = this.device.createRenderPipeline({
      label: `osd-${key}`,
      layout: this.device.createPipelineLayout({ bindGroupLayouts: [this.bindGroupLayout] }),
      vertex: {
        module: this.shader,
        entryPoint: "vs_main",
        buffers: [
          {
            arrayStride: VERTEX_FLOATS * 4,
            stepMode: mode === 0 ? "vertex" : "instance",
            attributes: [
              { shaderLocation: 0, offset: 0, format: "float32x3" },
              { shaderLocation: 1, offset: 12, format: "float32x2" },
              { shaderLocation: 2, offset: 20, format: "float32x4" },
              { shaderLocation: 3, offset: 36, format: "float32" },
            ],
          },
        ],
      },
      fragment: {
        module: this.shader,
        entryPoint: "fs_main",
        targets: [{ format: this.format, blend: blendState(blend) }],
      },
      primitive: { topology: "triangle-list", cullMode: "none" },
      depthStencil: {
        format: "depth24plus",
        depthWriteEnabled: opaque,
        depthCompare: opaque ? "less" : "always",
      },
    });
    this.pipelines.set(key, pipeline);
    return pipeline;
  }

  private bindGroup(tex: number): GPUBindGroup {
    const existing = this.bindGroups.get(tex);
    if (existing) return existing;
    const texture = this.textures.get(tex) ?? this.textures.get(0);
    if (!texture) throw new Error("no fallback texture");
    const group = this.device.createBindGroup({
      layout: this.bindGroupLayout,
      entries: [
        { binding: 0, resource: { buffer: this.uniformBuffer, offset: 0, size: 96 } },
        { binding: 1, resource: texture.createView() },
        { binding: 2, resource: this.sampler },
      ],
    });
    this.bindGroups.set(tex, group);
    return group;
  }

  resize(width: number, height: number): void {
    if (this.disposed) return;
    const w = Math.max(1, Math.floor(width));
    const h = Math.max(1, Math.floor(height));
    if (this.depthSize[0] === w && this.depthSize[1] === h && this.depth) return;
    this.depth?.destroy();
    this.depth = this.device.createTexture({
      size: { width: w, height: h },
      format: "depth24plus",
      usage: GPUTextureUsage.RENDER_ATTACHMENT,
    });
    this.depthSize = [w, h];
  }

  private ensureVertexBuffer(floats: number): void {
    if (this.vertexBuffer && this.vertexCapacity >= floats) return;
    this.vertexBuffer?.destroy();
    const capacity = Math.max(floats, 1 << 16);
    this.vertexBuffer = this.device.createBuffer({
      size: capacity * 4,
      usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
      label: "osd-vertices",
    });
    this.vertexCapacity = capacity;
  }

  private pushVertex(
    x: number,
    y: number,
    z: number,
    u: number,
    v: number,
    r: number,
    g: number,
    b: number,
    a: number,
    aux: number,
  ): void {
    if (this.scratchLen + VERTEX_FLOATS > this.scratch.length) {
      const bigger = new Float32Array(this.scratch.length * 2);
      bigger.set(this.scratch);
      this.scratch = bigger;
    }
    const o = this.scratchLen;
    this.scratch[o] = x;
    this.scratch[o + 1] = y;
    this.scratch[o + 2] = z;
    this.scratch[o + 3] = u;
    this.scratch[o + 4] = v;
    this.scratch[o + 5] = r;
    this.scratch[o + 6] = g;
    this.scratch[o + 7] = b;
    this.scratch[o + 8] = a;
    this.scratch[o + 9] = aux;
    this.scratchLen += VERTEX_FLOATS;
  }

  /** Draw one frame; returns statistics for the debug overlay. */
  draw(frame: Frame): RendererStats {
    if (this.disposed) throw new Error("renderer disposed");
    const stats: RendererStats = { draws: 0, triangles: 0, batchesSkipped: 0, batchesDrawn: 0 };
    const canvas = this.context.canvas as HTMLCanvasElement;
    this.resize(canvas.width, canvas.height);

    // 1. Core vertices, then the expanded line quads and cube faces.
    if (this.scratch.length < frame.vertices.length + 4096) {
      this.scratch = new Float32Array(Math.max(frame.vertices.length + 4096, this.scratch.length * 2));
    }
    this.scratch.set(frame.vertices, 0);
    this.scratchLen = frame.vertices.length;
    const lineRange = this.appendLines(frame);
    const cubeRange = this.appendCubes(frame);

    // 2. Upload.
    this.ensureVertexBuffer(this.scratchLen);
    if (this.vertexBuffer && this.scratchLen > 0) {
      this.device.queue.writeBuffer(
        this.vertexBuffer,
        0,
        this.scratch.buffer,
        this.scratch.byteOffset,
        this.scratchLen,
      );
    }

    // 3. Uniforms: one per batch plus the two auxiliary slots.
    const slots = Math.min(frame.batches.length, this.options.maxBatches, AUX_WORLD);
    for (let i = 0; i < slots; i++) {
      const b = frame.batches[i];
      const mode = b.prim === PRIM.SPRITES ? 1 : b.prim === PRIM.SCREEN_SPRITES ? 2 : 0;
      this.writeUniform(i, frame, mode, b.tex !== 0);
    }
    this.writeUniform(AUX_WORLD, frame, 0, false);
    this.writeUniform(AUX_SCREEN, frame, 2, false);
    this.device.queue.writeBuffer(this.uniformBuffer, 0, this.uniformData);

    // 4. Record.
    const encoder = this.device.createCommandEncoder({ label: "osd-frame" });
    const pass = encoder.beginRenderPass({
      colorAttachments: [
        {
          view: this.context.getCurrentTexture().createView(),
          clearValue: { r: 0, g: 0, b: 0, a: 1 },
          loadOp: "clear",
          storeOp: "store",
        },
      ],
      depthStencilAttachment: this.depth
        ? {
            view: this.depth.createView(),
            depthClearValue: 1,
            depthLoadOp: "clear",
            depthStoreOp: "store",
          }
        : undefined,
    });
    if (this.vertexBuffer) pass.setVertexBuffer(0, this.vertexBuffer);

    const drawBatch = (i: number): void => {
      if (i >= slots) {
        stats.batchesSkipped++;
        return;
      }
      const b = frame.batches[i];
      if (b.prim === PRIM.LINES) return; // expanded in appendLines
      if (b.vertexCount === 0) return;
      const mode = b.prim === PRIM.SPRITES ? 1 : b.prim === PRIM.SCREEN_SPRITES ? 2 : 0;
      pass.setPipeline(this.pipeline(mode as 0 | 1 | 2, b.blend));
      pass.setBindGroup(0, this.bindGroup(b.tex), [i * UNIFORM_STRIDE]);
      if (mode === 0) {
        pass.draw(b.vertexCount, 1, b.vertexOffset);
        stats.triangles += b.vertexCount / 3;
      } else {
        pass.draw(4, b.vertexCount, 0, b.vertexOffset);
        stats.triangles += 2 * b.vertexCount;
      }
      stats.draws++;
      stats.batchesDrawn++;
    };

    const upTo = Math.min(frame.cubeAfterBatch, slots);
    for (let i = 0; i < upTo; i++) drawBatch(i);
    if (cubeRange.count > 0) {
      pass.setPipeline(this.pipeline(0, BLEND.ALPHA));
      pass.setBindGroup(0, this.bindGroup(0), [AUX_WORLD * UNIFORM_STRIDE]);
      pass.draw(cubeRange.count, 1, cubeRange.offset);
      stats.triangles += cubeRange.count / 3;
      stats.draws++;
    }
    for (let i = upTo; i < slots; i++) drawBatch(i);
    if (lineRange.count > 0) {
      pass.setPipeline(this.pipeline(2, BLEND.ADD));
      pass.setBindGroup(0, this.bindGroup(0), [AUX_SCREEN * UNIFORM_STRIDE]);
      // The screen mode reads x/y in pixels; the shader clamps to the viewport.
      pass.draw(lineRange.count, 1, lineRange.offset);
      stats.triangles += lineRange.count / 3;
      stats.draws++;
    }

    pass.end();
    this.device.queue.submit([encoder.finish()]);
    return stats;
  }

  private writeUniform(slot: number, frame: Frame, mode: 0 | 1 | 2, texOn: boolean): void {
    const o = slot * FLOATS_PER_UNIFORM;
    const view = frame.view;
    for (let i = 0; i < 16; i++) this.uniformData[o + i] = view[i];
    this.uniformData[o + 16] = frame.width;
    this.uniformData[o + 17] = frame.height;
    this.uniformData[o + 18] = frame.scrz;
    this.uniformData[o + 19] = frame.ax;
    this.uniformData[o + 20] = frame.ay;
    this.uniformData[o + 21] = mode;
    this.uniformData[o + 22] = texOn ? 1 : 0;
    this.uniformData[o + 23] = 0;
  }

  /** Project a world point to virtual-screen pixels (null when behind). */
  private project(
    frame: Frame,
    x: number,
    y: number,
    z: number,
  ): [number, number, number] | null {
    const v = frame.view;
    const vx = v[0] * x + v[1] * y + v[2] * z + v[3];
    const vy = v[4] * x + v[5] * y + v[6] * z + v[7];
    const vz = v[8] * x + v[9] * y + v[10] * z + v[11];
    if (vz <= 0.5) return null;
    return [
      (frame.ax * frame.scrz * vx) / vz + frame.width * 0.5,
      (frame.ay * frame.scrz * vy) / vz + frame.height * 0.5,
      vz,
    ];
  }

  /**
   * Expand LINES batches into screen-space quads. The core writes one record
   * per trail sample, so consecutive records are consecutive segments.
   */
  private appendLines(frame: Frame): { offset: number; count: number } {
    const first = this.scratchLen / VERTEX_FLOATS;
    let count = 0;
    for (const batch of frame.batches) {
      if (batch.prim !== PRIM.LINES) continue;
      for (let k = 0; k + 1 < batch.vertexCount; k++) {
        const a = (batch.vertexOffset + k) * VERTEX_FLOATS;
        const b = (batch.vertexOffset + k + 1) * VERTEX_FLOATS;
        const pa = this.project(frame, frame.vertices[a], frame.vertices[a + 1], frame.vertices[a + 2]);
        const pb = this.project(frame, frame.vertices[b], frame.vertices[b + 1], frame.vertices[b + 2]);
        if (!pa || !pb) continue;
        const width = Math.max(1, frame.vertices[a + 3]);
        const dx = pb[0] - pa[0];
        const dy = pb[1] - pa[1];
        const len = Math.hypot(dx, dy) || 1;
        const nx = (-dy / len) * width * 0.5;
        const ny = (dx / len) * width * 0.5;
        const ca = frame.vertices.subarray(a + 5, a + 9);
        const quad: [number, number][] = [
          [pa[0] + nx, pa[1] + ny],
          [pa[0] - nx, pa[1] - ny],
          [pb[0] + nx, pb[1] + ny],
          [pb[0] - nx, pb[1] - ny],
        ];
        for (const idx of [0, 1, 2, 2, 1, 3]) {
          this.pushVertex(quad[idx][0], quad[idx][1], 0, width, width, ca[0], ca[1], ca[2], ca[3], 0);
          count++;
        }
      }
    }
    return { offset: first, count };
  }

  /** Expand the cube records into lit triangles. */
  private appendCubes(frame: Frame): { offset: number; count: number } {
    const first = this.scratchLen / VERTEX_FLOATS;
    if (!this.options.cubePasses) return { offset: first, count: 0 };
    for (let c = 0; c < frame.stats.cubes; c++) {
      const base = c * 128;
      const cube = frame.cubes.subarray(base, base + 128);
      const baseColor: [number, number, number] = [cube[8], cube[9], cube[10]];
      for (let f = 0; f < 6; f++) {
        if (cube[2 + f] < 0) continue; // back-facing
        const verts = CUBE_FACE_VERTS[f];
        for (const k of [0, 1, 2, 2, 1, 3] as const) {
          const vi = verts[k];
          const p = cube.subarray(21 + vi * 4, 24 + vi * 4);
          const vd = cube.subarray(53 + vi * 3, 56 + vi * 3);
          const l = Math.min(1, Math.max(0, cube[101 + f * 4 + k]));
          const shade = 0.35 + 0.65 * l;
          this.pushVertex(
            p[0],
            p[1],
            p[2],
            vd[1] * 0.5 + 0.5,
            vd[0] * 0.5 + 0.5,
            (baseColor[0] / 128) * shade,
            (baseColor[1] / 128) * shade,
            (baseColor[2] / 128) * shade,
            0.55,
            CUBE_FACE_NORMAL[f][0],
          );
        }
      }
    }
    return { offset: first, count: this.scratchLen / VERTEX_FLOATS - first };
  }

  destroy(): void {
    this.disposed = true;
    this.depth?.destroy();
    for (const t of this.textures.values()) t.destroy();
    this.vertexBuffer?.destroy();
    this.uniformBuffer.destroy();
    this.device.destroy();
  }
}
