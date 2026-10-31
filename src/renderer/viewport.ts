import type { AspectMode, ViewportLayout } from './types';

function clamp(v: number, lo: number, hi: number): number {
  if (!Number.isFinite(v)) return lo;
  return Math.min(Math.max(v, lo), hi);
}

function alignEven(v: number, minEven: number, maxEven: number): number {
  const clamped = Math.round(clamp(v, minEven, maxEven));
  const even = clamped & ~1;
  return Math.max(minEven, even);
}

/**
 * Compute physical swapchain dimensions, letterboxed/pillarboxed presentation
 * viewport, and internal offscreen render target resolution without aspect distortion.
 */
export function computeViewportLayout(
  cssWidth: number,
  cssHeight: number,
  dpr: number,
  resolutionScale: number,
  aspectMode: AspectMode,
): ViewportLayout {
  const safeDpr = clamp(dpr, 1.0, 2.5);
  const safeScale = clamp(resolutionScale, 0.5, 2.0);

  const canvasWidth = alignEven(cssWidth * safeDpr, 64, 3840);
  const canvasHeight = alignEven(cssHeight * safeDpr, 64, 2160);

  const containerAspect = canvasWidth / canvasHeight;
  const targetAspect =
    aspectMode === '4:3' ? 4 / 3 : aspectMode === '16:9' ? 16 / 9 : containerAspect;

  let viewportWidth = canvasWidth;
  let viewportHeight = canvasHeight;

  if (Math.abs(containerAspect - targetAspect) > 1e-3) {
    if (containerAspect > targetAspect) {
      // Pillarbox left/right
      viewportHeight = canvasHeight;
      viewportWidth = Math.max(2, Math.round(canvasHeight * targetAspect));
    } else {
      // Letterbox top/bottom
      viewportWidth = canvasWidth;
      viewportHeight = Math.max(2, Math.round(canvasWidth / targetAspect));
    }
  }

  const viewportX = Math.max(0, Math.floor((canvasWidth - viewportWidth) * 0.5));
  const viewportY = Math.max(0, Math.floor((canvasHeight - viewportHeight) * 0.5));

  const renderWidth = alignEven((viewportWidth / safeDpr) * safeScale, 160, 1920);
  const renderHeight = alignEven((viewportHeight / safeDpr) * safeScale, 120, 1440);

  return {
    canvasWidth,
    canvasHeight,
    renderWidth,
    renderHeight,
    viewportX,
    viewportY,
    viewportWidth,
    viewportHeight,
  };
}
