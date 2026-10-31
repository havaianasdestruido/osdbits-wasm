/**
 * WebGPU adapter/device acquisition, offscreen target allocation, resize handling,
 * and device-loss recovery.
 *
 * Targets mirror the PS2 GS framebuffer layout used by the original engine:
 *  - `screen`  : the double-buffered draw buffer the scene rasterizes into
 *  - `wb3`     : `extraBuf1` - the pre-object copy / work buffer 3
 *  - `wb4`     : `extraBuf2` - work buffer 4 / per-object refraction capture
 *  - `depth`   : the PSMZ32-equivalent depth attachment
 *
 * Every draw resolves against `rgba8unorm` with alpha blending done in linear space on
 * straight (non-premultiplied) alpha, which is what the GS blend equations operate on.
 */

export const LOGICAL_WIDTH = 640;
export const LOGICAL_HEIGHT = 448;

export interface GpuCapabilities {
  readonly available: boolean;
  readonly reason: string | null;
  readonly adapterName: string;
  readonly isFallbackAdapter: boolean;
  readonly maxTextureDimension2D: number;
}

export interface GpuTarget {
  texture: GPUTexture;
  view: GPUTextureView;
  width: number;
  height: number;
}

function createTarget(device: GPUDevice, width: number, height: number, label: string): GpuTarget {
  const texture = device.createTexture({
    label,
    size: { width, height, depthOrArrayLayers: 1 },
    format: 'rgba8unorm',
    usage:
      GPUTextureUsage.RENDER_ATTACHMENT |
      GPUTextureUsage.TEXTURE_BINDING |
      GPUTextureUsage.COPY_SRC |
      GPUTextureUsage.COPY_DST,
  });
  return { texture, view: texture.createView({ label: `${label}-view` }), width, height };
}

export class GpuContext {
  readonly canvas: HTMLCanvasElement;
  readonly device: GPUDevice;
  readonly context: GPUCanvasContext;
  readonly format: GPUTextureFormat;
  readonly capabilities: GpuCapabilities;

  screen: GpuTarget;
  workBuf3: GpuTarget;
  workBuf4: GpuTarget;
  depth: GPUTexture;
  depthView: GPUTextureView;
  /** A tiny copy the visual regression tests read back. */
  private readbackTexture: GPUTexture | null = null;

  private lost = false;
  private readonly deviceLostListeners: Array<(info: GPUDeviceLostInfo) => void> = [];

  private constructor(
    canvas: HTMLCanvasElement,
    device: GPUDevice,
    context: GPUCanvasContext,
    format: GPUTextureFormat,
    capabilities: GpuCapabilities,
  ) {
    this.canvas = canvas;
    this.device = device;
    this.context = context;
    this.format = format;
    this.capabilities = capabilities;

    this.screen = createTarget(device, LOGICAL_WIDTH, LOGICAL_HEIGHT, 'osd-screen');
    this.workBuf3 = createTarget(device, LOGICAL_WIDTH, LOGICAL_HEIGHT, 'osd-wb3');
    this.workBuf4 = createTarget(device, LOGICAL_WIDTH, LOGICAL_HEIGHT, 'osd-wb4');
    this.depth = this.createDepth(LOGICAL_WIDTH, LOGICAL_HEIGHT);
    this.depthView = this.depth.createView({ label: 'osd-depth-view' });

    void device.lost.then((info) => {
      this.lost = true;
      for (const listener of this.deviceLostListeners) listener(info);
    });
  }

  static async create(canvas: HTMLCanvasElement): Promise<GpuContext> {
    if (!('gpu' in navigator) || !navigator.gpu) {
      throw new Error(
        'WebGPU is not exposed by this browser. Use a recent Chrome/Edge (113+), or enable the WebGPU flag.',
      );
    }
    const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
    if (!adapter) {
      throw new Error(
        'No WebGPU adapter is available. The GPU may be blocklisted or software rendering is disabled.',
      );
    }
    const device = await adapter.requestDevice({ label: 'osdbits-device' });
    const context = canvas.getContext('webgpu');
    if (!context) {
      throw new Error('Failed to acquire a WebGPU canvas context');
    }
    const format = navigator.gpu.getPreferredCanvasFormat();
    context.configure({
      device,
      format,
      alphaMode: 'opaque',
      usage: GPUTextureUsage.RENDER_ATTACHMENT,
    });

    const info = adapter.info as { vendor?: string; architecture?: string; device?: string } | undefined;
    const adapterName = [info?.vendor, info?.device, info?.architecture]
      .filter((v): v is string => typeof v === 'string' && v.length > 0)
      .join(' ') || 'unknown adapter';

    return new GpuContext(canvas, device, context, format, {
      available: true,
      reason: null,
      adapterName,
      isFallbackAdapter: 'isFallbackAdapter' in adapter ? Boolean(adapter.isFallbackAdapter) : false,
      maxTextureDimension2D: device.limits.maxTextureDimension2D,
    });
  }

  onDeviceLost(listener: (info: GPUDeviceLostInfo) => void): void {
    this.deviceLostListeners.push(listener);
  }

  get isLost(): boolean {
    return this.lost;
  }

  private createDepth(width: number, height: number): GPUTexture {
    return this.device.createTexture({
      label: 'osd-depth',
      size: { width, height, depthOrArrayLayers: 1 },
      format: 'depth24plus',
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
    });
  }

  /**
   * Reconfigures the swap chain and depth attachment for a new device-pixel size.
   * Render targets stay at the fixed PS2 resolution: the GS never scaled with the display,
   * and the presentation pass resamples from 640x448 so the art keeps its aspect and DPI.
   */
  resize(devicePixelWidth: number, devicePixelHeight: number): void {
    const w = Math.max(1, Math.floor(devicePixelWidth));
    const h = Math.max(1, Math.floor(devicePixelHeight));
    if (this.canvas.width !== w) this.canvas.width = w;
    if (this.canvas.height !== h) this.canvas.height = h;
    const max = this.device.limits.maxTextureDimension2D;
    if (this.depth.width !== Math.min(w, max) || this.depth.height !== Math.min(h, max)) {
      this.depth.destroy();
      this.depth = this.createDepth(Math.min(w, max), Math.min(h, max));
      this.depthView = this.depth.createView({ label: 'osd-depth-view' });
    }
  }

  /** Reads the current screen target back as RGBA8 (used by visual regression tests). */
  async readScreenPixels(): Promise<{ width: number; height: number; data: Uint8Array }> {
    const { width, height } = this.screen;
    const bytesPerRow = Math.ceil((width * 4) / 256) * 256;
    if (!this.readbackTexture) {
      this.readbackTexture = this.device.createTexture({
        label: 'osd-readback',
        size: { width, height, depthOrArrayLayers: 1 },
        format: 'rgba8unorm',
        usage: GPUTextureUsage.COPY_DST | GPUTextureUsage.TEXTURE_BINDING,
      });
    }
    const buffer = this.device.createBuffer({
      label: 'osd-readback-buffer',
      size: bytesPerRow * height,
      usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });
    const encoder = this.device.createCommandEncoder({ label: 'osd-readback-encoder' });
    encoder.copyTextureToTexture(
      { texture: this.screen.texture },
      { texture: this.readbackTexture },
      { width, height, depthOrArrayLayers: 1 },
    );
    encoder.copyTextureToBuffer(
      { texture: this.readbackTexture },
      { buffer, bytesPerRow },
      { width, height, depthOrArrayLayers: 1 },
    );
    this.device.queue.submit([encoder.finish()]);
    await buffer.mapAsync(GPUMapMode.READ);
    const mapped = new Uint8Array(buffer.getMappedRange());
    const data = new Uint8Array(width * height * 4);
    for (let y = 0; y < height; y += 1) {
      data.set(mapped.subarray(y * bytesPerRow, y * bytesPerRow + width * 4), y * width * 4);
    }
    buffer.unmap();
    buffer.destroy();
    return { width, height, data };
  }

  dispose(): void {
    for (const target of [this.screen, this.workBuf3, this.workBuf4]) target.texture.destroy();
    this.depth.destroy();
    this.readbackTexture?.destroy();
    this.device.destroy();
  }
}

export function isWebGpuAvailable(): boolean {
  return typeof navigator !== 'undefined' && 'gpu' in navigator && Boolean(navigator.gpu);
}
