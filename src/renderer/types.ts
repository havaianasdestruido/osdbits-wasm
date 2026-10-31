import type { FrameTelemetrySnapshot, OsdbitsWasmEngine } from '../wasm/bridge';

export type RendererBackendKind = 'webgpu' | 'canvas2d';

/**
 * Aspect ratio display modes matching OSDSYS System Configuration (`4:3`, `Full`, `16:9`).
 */
export type AspectMode = '4:3' | 'full' | '16:9';

export function screenSizeIndexToAspect(screenSize: number): AspectMode {
  if (screenSize === 1) return 'full';
  if (screenSize === 2) return '16:9';
  return '4:3';
}

export interface AdapterDiagnostics {
  backend: RendererBackendKind;
  /**
   * True when WebGPU is backed by a software fallback adapter (e.g., Dawn SwiftShader)
   * or when running on Canvas 2D compatibility mode.
   */
  isFallbackAdapter: boolean;
  vendor: string;
  architecture: string;
  description: string;
  wgslCompilationClean: boolean;
  degradedReason: string | null;
}

export interface ViewportLayout {
  /** Physical swapchain canvas width in device pixels */
  canvasWidth: number;
  /** Physical swapchain canvas height in device pixels */
  canvasHeight: number;
  /** Internal offscreen scene render target width */
  renderWidth: number;
  /** Internal offscreen scene render target height */
  renderHeight: number;
  /** Letterboxed/pillarboxed blit X in canvas device pixels */
  viewportX: number;
  /** Letterboxed/pillarboxed blit Y in canvas device pixels */
  viewportY: number;
  /** Letterboxed/pillarboxed blit width in canvas device pixels */
  viewportWidth: number;
  /** Letterboxed/pillarboxed blit height in canvas device pixels */
  viewportHeight: number;
}

export interface PixelReadback {
  width: number;
  height: number;
  rgba: Uint8Array;
}

export interface OsdbitsRenderer {
  readonly backend: RendererBackendKind;
  readonly diagnostics: AdapterDiagnostics;
  readonly layout: ViewportLayout;

  /** Upload procedural or user-imported textures when `engine.textureGeneration` changes. */
  syncTextures(engine: OsdbitsWasmEngine): void;

  /** Recompute render target and letterbox/pillarbox viewport dimensions. */
  resize(
    cssWidth: number,
    cssHeight: number,
    dpr: number,
    resolutionScale: number,
    aspectMode: AspectMode,
  ): ViewportLayout;

  /** Execute all render batches and backdrop captures for the current WASM frame. */
  renderFrame(engine: OsdbitsWasmEngine, telemetry: FrameTelemetrySnapshot): void;

  /** Read back RGBA8 pixels from the active offscreen color target for deterministic verification. */
  readPixels(): Promise<PixelReadback>;

  /** Release all GPU or offscreen resources owned by this renderer. */
  dispose(): void;
}
