/**
 * Logical pixel raster for the in-app presentation layer.
 *
 * The shell draws every visible UI element into a small straight-alpha RGBA
 * buffer (height fixed at 240 logical pixels, width derived from the canvas
 * aspect). The renderer uploads that buffer as a texture and composites it
 * over the scene with nearest-neighbour sampling, so text stays crisp at any
 * window size and the look stays pixel-art rather than browser-rendered.
 */

import { GLYPH_ADVANCE, GLYPH_HEIGHT, GLYPH_WIDTH, glyphFor } from "./pixelFont";

export type RgbaColor = readonly [number, number, number, number];

/** Logical height of the in-app raster. Width follows the canvas aspect ratio. */
export const UI_LOGICAL_HEIGHT = 240;
export const UI_MIN_LOGICAL_WIDTH = 240;
export const UI_MAX_LOGICAL_WIDTH = 640;
/** Vertical distance between two text lines at scale 1. */
export const LINE_HEIGHT = GLYPH_HEIGHT + 2;

/** Straight-alpha RGBA snapshot handed to the renderers. */
export interface UiOverlayFrame {
  width: number;
  height: number;
  rgba: Uint8Array<ArrayBuffer>;
}

export function logicalSizeForCanvas(
  canvasWidth: number,
  canvasHeight: number,
): { width: number; height: number } {
  const aspect = canvasHeight > 0 ? canvasWidth / canvasHeight : 4 / 3;
  const width = Math.round(UI_LOGICAL_HEIGHT * aspect);
  return {
    width: Math.max(UI_MIN_LOGICAL_WIDTH, Math.min(UI_MAX_LOGICAL_WIDTH, width)),
    height: UI_LOGICAL_HEIGHT,
  };
}

export class PixelRaster {
  readonly width: number;
  readonly height: number;
  readonly rgba: Uint8Array<ArrayBuffer>;
  private drawn: string[] = [];
  private clip = { x0: 0, y0: 0, x1: 0, y1: 0 };

  constructor(width: number, height: number) {
    this.width = Math.max(1, Math.floor(width));
    this.height = Math.max(1, Math.floor(height));
    this.rgba = new Uint8Array(this.width * this.height * 4);
    this.clearClip();
  }

  /** Clears the buffer to fully transparent (the scene shows through). */
  clear(): void {
    this.rgba.fill(0);
    this.drawn = [];
    this.clearClip();
  }

  /** Restricts subsequent drawing to the given rectangle (used by scrolling lists). */
  setClip(x: number, y: number, w: number, h: number): void {
    this.clip = {
      x0: Math.max(0, Math.floor(x)),
      y0: Math.max(0, Math.floor(y)),
      x1: Math.min(this.width, Math.floor(x + w)),
      y1: Math.min(this.height, Math.floor(y + h)),
    };
  }

  clearClip(): void {
    this.clip = { x0: 0, y0: 0, x1: this.width, y1: this.height };
  }

  /** Every string drawn since the last `clear()`, in draw order. */
  get drawnText(): readonly string[] {
    return this.drawn;
  }

  /** Source-over blend of one straight-alpha colour into pixel (x, y). */
  private blendPixel(x: number, y: number, color: RgbaColor, alphaScale: number): void {
    if (x < this.clip.x0 || y < this.clip.y0 || x >= this.clip.x1 || y >= this.clip.y1) return;
    const srcA = (color[3] / 255) * alphaScale;
    if (srcA <= 0) return;
    const o = (y * this.width + x) * 4;
    const dstA = (this.rgba[o + 3] ?? 0) / 255;
    const outA = srcA + dstA * (1 - srcA);
    if (outA <= 0) return;
    const mix = (src: number, dst: number): number =>
      Math.round((src * srcA + dst * dstA * (1 - srcA)) / outA);
    this.rgba[o] = mix(color[0], this.rgba[o] ?? 0);
    this.rgba[o + 1] = mix(color[1], this.rgba[o + 1] ?? 0);
    this.rgba[o + 2] = mix(color[2], this.rgba[o + 2] ?? 0);
    this.rgba[o + 3] = Math.round(outA * 255);
  }

  fillRect(x: number, y: number, w: number, h: number, color: RgbaColor, alphaScale = 1): void {
    const x0 = Math.max(0, Math.floor(x));
    const y0 = Math.max(0, Math.floor(y));
    const x1 = Math.min(this.width, Math.floor(x + w));
    const y1 = Math.min(this.height, Math.floor(y + h));
    for (let py = y0; py < y1; py++) {
      for (let px = x0; px < x1; px++) {
        this.blendPixel(px, py, color, alphaScale);
      }
    }
  }

  /** One-pixel outline drawn just inside the given rectangle. */
  strokeRect(x: number, y: number, w: number, h: number, color: RgbaColor, alphaScale = 1): void {
    this.fillRect(x, y, w, 1, color, alphaScale);
    this.fillRect(x, y + h - 1, w, 1, color, alphaScale);
    this.fillRect(x, y, 1, h, color, alphaScale);
    this.fillRect(x + w - 1, y, 1, h, color, alphaScale);
  }

  /** Width in logical pixels of `text` at integer `scale` (no trailing tracking). */
  textWidth(text: string, scale = 1): number {
    if (text.length === 0) return 0;
    return (text.length * GLYPH_ADVANCE - 1) * scale;
  }

  /** Draws `text` with its top-left corner at (x, y). Returns the drawn width. */
  text(x: number, y: number, text: string, color: RgbaColor, scale = 1, alphaScale = 1): number {
    const s = Math.max(1, Math.floor(scale));
    let cursorX = Math.floor(x);
    const top = Math.floor(y);
    for (const ch of text) {
      const rows = glyphFor(ch);
      for (let row = 0; row < GLYPH_HEIGHT; row++) {
        const bits = rows[row] ?? 0;
        if (bits === 0) continue;
        for (let col = 0; col < GLYPH_WIDTH; col++) {
          if ((bits >> (GLYPH_WIDTH - 1 - col)) & 1) {
            this.fillRect(
              cursorX + col * s,
              top + row * s,
              s,
              s,
              color,
              alphaScale,
            );
          }
        }
      }
      cursorX += GLYPH_ADVANCE * s;
    }
    if (text.length > 0) {
      this.drawn.push(text);
    }
    return this.textWidth(text, s);
  }

  textRight(rightX: number, y: number, text: string, color: RgbaColor, scale = 1, alphaScale = 1): void {
    this.text(rightX - this.textWidth(text, scale), y, text, color, scale, alphaScale);
  }

  textCenter(centerX: number, y: number, text: string, color: RgbaColor, scale = 1, alphaScale = 1): void {
    this.text(centerX - this.textWidth(text, scale) / 2, y, text, color, scale, alphaScale);
  }

  /** Greedy word wrap that never splits a word unless it alone exceeds `maxWidth`. */
  wrap(text: string, maxWidth: number, scale = 1): string[] {
    const words = text.trim().split(/\s+/).filter((w) => w.length > 0);
    const lines: string[] = [];
    let line = "";
    const fits = (candidate: string): boolean => this.textWidth(candidate, scale) <= maxWidth;
    for (const word of words) {
      const candidate = line.length === 0 ? word : `${line} ${word}`;
      if (fits(candidate)) {
        line = candidate;
        continue;
      }
      if (line.length > 0) lines.push(line);
      line = word;
      // Hard-break words that cannot fit on a line by themselves.
      while (!fits(line) && line.length > 1) {
        let cut = line.length - 1;
        while (cut > 1 && !fits(line.slice(0, cut))) cut--;
        lines.push(line.slice(0, cut));
        line = line.slice(cut);
      }
    }
    if (line.length > 0) lines.push(line);
    return lines;
  }
}
