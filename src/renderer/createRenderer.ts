import { FallbackCanvasRenderer } from './fallbackCanvasRenderer';
import type { OsdbitsRenderer } from './types';
import { WebGpuRenderer } from './webgpuRenderer';

export interface CreateRendererOptions {
  canvas: HTMLCanvasElement;
  forceCanvasFallback?: boolean;
  onDeviceLost?: (reason: string, message: string) => void;
}

export interface CreateRendererResult {
  renderer: OsdbitsRenderer;
  /** Active canvas element (may be a clean replacement node if context type switched). */
  canvas: HTMLCanvasElement;
}

function ensureFresh2dCapableCanvas(canvas: HTMLCanvasElement): HTMLCanvasElement {
  const probe = canvas.getContext('2d', { alpha: false });
  if (probe) {
    return canvas;
  }
  const replacement = canvas.cloneNode(false) as HTMLCanvasElement;
  replacement.width = canvas.width;
  replacement.height = canvas.height;
  canvas.replaceWith(replacement);
  return replacement;
}

/**
 * Initialize WebGPU when available, or gracefully degrade to `FallbackCanvasRenderer`
 * without throwing uncaught errors or leaving a locked `<canvas>` context.
 */
export async function createOsdbitsRenderer(
  options: CreateRendererOptions,
): Promise<CreateRendererResult> {
  if (options.forceCanvasFallback) {
    const freshCanvas = ensureFresh2dCapableCanvas(options.canvas);
    return {
      renderer: new FallbackCanvasRenderer(
        freshCanvas,
        'Canvas 2D compatibility mode explicitly enabled by user or URL flag.',
      ),
      canvas: freshCanvas,
    };
  }

  try {
    const webgpu = await WebGpuRenderer.create({
      canvas: options.canvas,
      onDeviceLost: options.onDeviceLost,
    });
    return { renderer: webgpu, canvas: options.canvas };
  } catch (err) {
    const reason =
      err instanceof Error ? err.message : 'WebGPU initialization failed.';
    const freshCanvas = ensureFresh2dCapableCanvas(options.canvas);
    return {
      renderer: new FallbackCanvasRenderer(
        freshCanvas,
        `WebGPU unavailable (${reason}); using Canvas 2D compatibility renderer.`,
      ),
      canvas: freshCanvas,
    };
  }
}
