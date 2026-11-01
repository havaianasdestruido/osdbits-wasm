/**
 * In-app screens for the UILess presentation layer.
 *
 * Everything the shell shows is drawn here onto a {@link PixelRaster} in the
 * application's own pixel style: the title/start screen, the Mode D OSDSYS
 * menu, the Mode C warning line, the corner pop-up (the only chrome, capped at
 * five buttons), the Mode E configuration list and the diagnostics page.
 *
 * Screens are pure functions of their view models. They also return hit
 * regions in logical raster coordinates, so pointer input is resolved against
 * exactly what was drawn this frame.
 */

import { GLYPH_HEIGHT } from "./pixelFont";
import { LINE_HEIGHT, PixelRaster, type RgbaColor } from "./pixelRaster";

export type Tone = "text" | "muted" | "gold" | "cyan" | "crimson" | "dim" | "accent";

export const UI_COLORS = {
  veil: [3, 6, 15, 150],
  veilStrong: [3, 6, 15, 214],
  panel: [7, 13, 28, 226],
  panelEdge: [94, 166, 255, 255],
  text: [230, 239, 253, 255],
  muted: [147, 168, 204, 255],
  dim: [96, 116, 150, 255],
  accent: [94, 166, 255, 255],
  cyan: [91, 226, 245, 255],
  gold: [245, 205, 121, 255],
  crimson: [255, 110, 130, 255],
  selectBg: [40, 84, 160, 210],
  disabled: [78, 94, 124, 255],
  warnPanel: [40, 10, 18, 232],
} as const satisfies Record<string, RgbaColor>;

const TONE_COLOR: Record<Tone, RgbaColor> = {
  text: UI_COLORS.text,
  muted: UI_COLORS.muted,
  gold: UI_COLORS.gold,
  cyan: UI_COLORS.cyan,
  crimson: UI_COLORS.crimson,
  dim: UI_COLORS.dim,
  accent: UI_COLORS.accent,
};

const ROW_HEIGHT = 11;
const HEADER_HEIGHT = 13;
const LIST_PAD = 4;
const SCROLLBAR_WIDTH = 2;
const SLIDER_WIDTH = 40;

/** A clickable area in logical raster coordinates. Later regions sit on top. */
export interface UiRegion {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
  enabled: boolean;
  onPress: (lx: number, ly: number) => void;
}

export interface RowItem {
  kind: "row";
  id: string;
  label: string;
  value: string;
  enabled: boolean;
  valueTone?: Tone;
  /** Slider rows draw a bar; `ratio` is the filled fraction in [0, 1]. */
  slider?: { ratio: number };
  onActivate: () => void;
  onAdjust?: (dir: -1 | 1, big: boolean) => void;
  /** Pointer placement on a slider bar (ratio in [0, 1]). */
  onRatio?: (ratio: number) => void;
  tone?: Tone;
}

export interface HeaderItem {
  kind: "header";
  label: string;
}

export interface InfoItem {
  kind: "info";
  key?: string;
  text: string;
  tone?: Tone;
}

export type ListItem = RowItem | HeaderItem | InfoItem;

export interface ListModel {
  items: readonly ListItem[];
  /** Flat index of the keyboard/pointer cursor inside `items`, or -1 when none. */
  selected: number;
  onSelect: (flatIndex: number) => void;
  /**
   * Scroll state owned by the caller so it persists between frames. `followed` remembers the
   * selection the view last scrolled to, so manual (wheel) scrolling is not undone every frame.
   */
  scroll: { top: number; followed?: number };
}

export interface ProvenanceTag {
  text: string;
  tone: Tone;
}

export interface MenuItemView {
  label: string;
  value: string;
  selected: boolean;
  tag: ProvenanceTag;
}

export interface MenuModel {
  title: string;
  clock: string;
  interactive: boolean;
  /** Approximate-screen note (Browser / Version) shown instead of entries. */
  note?: string;
  items: readonly MenuItemView[];
  onItem: (index: number) => void;
  clockEditor?: {
    values: readonly string[];
    cursor: number;
    onField: (index: number) => void;
  };
  hints: { id: string; label: string; onPress: () => void }[];
}

export interface PopupButton {
  id: string;
  label: string;
  active: boolean;
  onPress: () => void;
}

export interface SceneModel {
  illegal?: { text: string; alpha: number; note?: string };
  menu?: MenuModel;
  error?: { message: string; onRecover: () => void };
  popup?: { buttons: readonly PopupButton[] };
  toast?: { text: string; alpha: number };
}

export interface TitleModel {
  subtitle: string;
  statusLines: readonly { text: string; tone: Tone }[];
  reason?: string;
  list: ListModel;
  startEnabled: boolean;
  onBackdrop: () => void;
  legal: string;
  prompt: string;
}

export interface OverlayModel {
  title: string;
  subtitle: string;
  list: ListModel;
  footer: readonly { text: string; tone: Tone }[];
  onDismiss: () => void;
}

export interface UiComposeInput {
  nowMs: number;
  started: boolean;
  title?: TitleModel;
  scene?: SceneModel;
  overlay?: OverlayModel;
}

class RegionSink {
  readonly list: UiRegion[] = [];

  add(
    id: string,
    x: number,
    y: number,
    w: number,
    h: number,
    enabled: boolean,
    onPress: (lx: number, ly: number) => void,
  ): void {
    if (w <= 0 || h <= 0) return;
    this.list.push({ id, x, y, w, h, enabled, onPress });
  }
}

/** Draws one complete UI frame and returns its hit regions. */
export function composeInAppUi(raster: PixelRaster, input: UiComposeInput): UiRegion[] {
  raster.clear();
  const sink = new RegionSink();
  if (!input.started && input.title) {
    drawTitle(raster, sink, input.title, input.nowMs);
  } else if (input.scene) {
    drawScene(raster, sink, input.scene);
  }
  if (input.overlay) {
    drawOverlay(raster, sink, input.overlay);
  }
  return sink.list;
}

/** Index of the next selectable item from `from` in direction `dir`, or `from` when none. */
export function nextSelectable(items: readonly ListItem[], from: number, dir: -1 | 1): number {
  let idx = from;
  for (let step = 0; step < items.length; step++) {
    idx += dir;
    if (idx < 0 || idx >= items.length) return from;
    const item = items[idx];
    if (item && item.kind === "row" && item.enabled) return idx;
  }
  return from;
}

/** First selectable item index, or -1 when nothing is selectable. */
export function firstSelectable(items: readonly ListItem[]): number {
  return items.findIndex((item) => item.kind === "row" && item.enabled);
}

// ---------------------------------------------------------------------------
// Title / start screen
// ---------------------------------------------------------------------------

function drawTitle(r: PixelRaster, sink: RegionSink, t: TitleModel, nowMs: number): void {
  const W = r.width;
  const H = r.height;
  r.fillRect(0, 0, W, H, UI_COLORS.veil);
  sink.add("title-backdrop", 0, 0, W, H, t.startEnabled, () => t.onBackdrop());

  let y = 12;
  r.textCenter(W / 2, y, "PLAYSTATION 2 OSDSYS VISUAL RECONSTRUCTION", UI_COLORS.cyan, 1);
  y += LINE_HEIGHT + 2;

  const brand = "OSDBITS // WASM";
  const brandScale = r.textWidth(brand, 3) <= W - 24 ? 3 : 2;
  r.textCenter(W / 2, y, brand, UI_COLORS.text, brandScale);
  y += GLYPH_HEIGHT * brandScale + 6;

  for (const line of r.wrap(t.subtitle, W - 48)) {
    r.textCenter(W / 2, y, line, UI_COLORS.muted);
    y += LINE_HEIGHT;
  }
  y += 4;

  for (const status of t.statusLines) {
    for (const line of r.wrap(status.text, W - 40)) {
      r.textCenter(W / 2, y, line, TONE_COLOR[status.tone]);
      y += LINE_HEIGHT;
    }
  }
  y += 6;

  const listHeight = listContentHeight(t.list.items) + LIST_PAD * 2;
  const boxW = Math.min(W - 48, 260);
  drawList(r, sink, { x: Math.floor((W - boxW) / 2), y, w: boxW, h: listHeight }, t.list, {
    panel: true,
  });
  y += listHeight + 6;

  if (t.reason) {
    for (const line of r.wrap(t.reason, W - 44)) {
      r.textCenter(W / 2, y, line, UI_COLORS.crimson);
      y += LINE_HEIGHT;
    }
  }

  const legalLines = r.wrap(t.legal, W - 40);
  let legalY = H - 8 - legalLines.length * LINE_HEIGHT;
  for (const line of legalLines) {
    r.textCenter(W / 2, legalY, line, UI_COLORS.dim);
    legalY += LINE_HEIGHT;
  }
  const blinkOn = Math.floor(nowMs / 500) % 2 === 0;
  if (t.prompt && blinkOn) {
    r.textCenter(W / 2, legalY - LINE_HEIGHT - 6, t.prompt, UI_COLORS.gold);
  }
}

// ---------------------------------------------------------------------------
// Scene HUD: Mode C text, Mode D menu, error box, toast and corner pop-up
// ---------------------------------------------------------------------------

function drawScene(r: PixelRaster, sink: RegionSink, s: SceneModel): void {
  const W = r.width;
  const H = r.height;

  if (s.illegal && s.illegal.alpha > 0) {
    const alpha = Math.max(0, Math.min(1, s.illegal.alpha));
    const lines = r.wrap(s.illegal.text, W - 40, 2);
    let y = Math.round(H * 0.78);
    for (const line of lines) {
      r.textCenter(W / 2, y, line, [255, 178, 188, 255], 2, alpha);
      y += (GLYPH_HEIGHT + 2) * 2;
    }
    if (s.illegal.note) {
      const noteLines = r.wrap(s.illegal.note, W - 24);
      let ny = H - 12 - noteLines.length * LINE_HEIGHT;
      for (const line of noteLines) {
        r.textCenter(W / 2, ny, line, UI_COLORS.dim, 1, alpha);
        ny += LINE_HEIGHT;
      }
    }
  }

  if (s.menu) {
    drawMenu(r, sink, s.menu);
  }

  if (s.error) {
    drawErrorBox(r, sink, s.error);
  }

  const toast = s.toast;
  if (toast && toast.alpha > 0) {
    const lines = r.wrap(toast.text, 112);
    const y = H - 10 - (lines.length - 1) * LINE_HEIGHT;
    lines.forEach((line, idx) => {
      const width = r.textWidth(line) + 8;
      r.fillRect(6, y + idx * LINE_HEIGHT - 3, width, LINE_HEIGHT + 2, UI_COLORS.panel, toast.alpha * 0.9);
      r.text(10, y + idx * LINE_HEIGHT, line, UI_COLORS.gold, 1, toast.alpha);
    });
  }

  if (s.popup) {
    drawPopup(r, sink, s.popup.buttons);
  }
}

function drawMenu(r: PixelRaster, sink: RegionSink, m: MenuModel): void {
  const W = r.width;
  const H = r.height;
  // The core ignores menu input during its fade-in and choreography; rows only accept clicks when it does.
  const interactive = m.interactive;

  r.text(8, 6, m.title, UI_COLORS.gold, 1);
  r.textRight(W - 8, 6, m.clock, UI_COLORS.muted, 1);
  r.fillRect(8, 17, W - 16, 1, UI_COLORS.accent, 0.5);

  // Left column: controls hints, anchored to the bottom-left corner.
  const hintGap = 10;
  let hy = H - 12 - (m.hints.length - 1) * hintGap - LINE_HEIGHT + 2;
  for (const hint of m.hints) {
    const text = `[${hint.label}]`;
    const w = r.textWidth(text);
    r.text(8, hy, text, interactive ? UI_COLORS.cyan : UI_COLORS.disabled);
    sink.add(`menu-hint-${hint.id}`, 6, hy - 2, w + 4, LINE_HEIGHT + 1, interactive, () => {
      if (interactive) hint.onPress();
    });
    hy += hintGap;
  }

  // Right column: entries (or an approximate-screen note) and the clock editor.
  const colW = Math.min(188, W - 16);
  const x0 = W - 8 - colW;
  let y = 24;

  if (m.note) {
    r.text(x0, y, "[APPROXIMATE]", UI_COLORS.gold);
    y += LINE_HEIGHT + 1;
    for (const line of r.wrap(m.note, colW)) {
      r.text(x0, y, line, UI_COLORS.text);
      y += LINE_HEIGHT;
    }
  }

  m.items.forEach((item, idx) => {
    const rowH = item.value.length > 0 ? 21 : 12;
    if (item.selected) {
      r.fillRect(x0 - 2, y - 2, colW + 4, rowH, UI_COLORS.selectBg);
      r.fillRect(x0 - 2, y - 2, 2, rowH, UI_COLORS.gold);
    }
    const labelColor = !interactive ? UI_COLORS.disabled : item.selected ? UI_COLORS.gold : UI_COLORS.text;
    r.text(x0 + 4, y, item.label, labelColor);
    const tagWidth = r.textWidth(item.tag.text);
    r.text(x0 + colW - tagWidth - 4, y, item.tag.text, TONE_COLOR[item.tag.tone]);
    if (item.value.length > 0) {
      r.text(x0 + 4, y + LINE_HEIGHT, item.value, UI_COLORS.muted);
    }
    sink.add(`menu-item-${idx}`, x0 - 2, y - 2, colW + 4, rowH, interactive, () => {
      if (interactive) m.onItem(idx);
    });
    y += rowH + 2;
  });

  if (m.clockEditor) {
    const clock = m.clockEditor;
    y += 4;
    r.text(x0 + 4, y, "SET CLOCK  LEFT/RIGHT FIELD  UP/DOWN VALUE", UI_COLORS.dim);
    y += LINE_HEIGHT + 1;
    const separators = ["/", "/", " ", ":", ":"];
    let cx = x0 + 4;
    clock.values.forEach((value, idx) => {
      const active = idx === clock.cursor;
      const w = r.textWidth(value) + 6;
      if (active) {
        r.fillRect(cx, y - 2, w, LINE_HEIGHT + 2, UI_COLORS.selectBg);
        r.strokeRect(cx, y - 2, w, LINE_HEIGHT + 2, UI_COLORS.gold);
      }
      r.text(cx + 3, y, value, active ? UI_COLORS.gold : UI_COLORS.text);
      sink.add(`clock-field-${idx}`, cx, y - 2, w, LINE_HEIGHT + 2, interactive, () => {
        if (interactive) clock.onField(idx);
      });
      cx += w;
      const sep = separators[idx];
      if (sep !== undefined && idx < clock.values.length - 1) {
        r.text(cx + 1, y, sep, UI_COLORS.muted);
        cx += r.textWidth(sep) + 3;
      }
    });
  }
}

function drawErrorBox(r: PixelRaster, sink: RegionSink, e: { message: string; onRecover: () => void }): void {
  const W = r.width;
  const H = r.height;
  const boxW = Math.min(W - 40, 240);
  const lines = r.wrap(e.message, boxW - 16);
  const boxH = 14 + LINE_HEIGHT + lines.length * LINE_HEIGHT + 16;
  const x = Math.floor((W - boxW) / 2);
  const y = Math.floor(H / 2 - boxH / 2);
  r.fillRect(x, y, boxW, boxH, UI_COLORS.warnPanel);
  r.strokeRect(x, y, boxW, boxH, UI_COLORS.crimson);
  r.text(x + 8, y + 6, "SCENE ERROR", UI_COLORS.crimson);
  let ly = y + 6 + LINE_HEIGHT + 2;
  for (const line of lines) {
    r.text(x + 8, ly, line, UI_COLORS.text);
    ly += LINE_HEIGHT;
  }
  const label = "[ RECOVER SCENE ]";
  const bw = r.textWidth(label);
  const bx = Math.floor(x + boxW / 2 - bw / 2);
  const by = y + boxH - LINE_HEIGHT - 6;
  r.text(bx, by, label, UI_COLORS.gold);
  sink.add("error-recover", bx - 4, by - 3, bw + 8, LINE_HEIGHT + 4, true, () => e.onRecover());
}

function drawPopup(r: PixelRaster, sink: RegionSink, buttons: readonly PopupButton[]): void {
  const W = r.width;
  const H = r.height;
  const pad = 3;
  const gap = 3;
  const btnH = LINE_HEIGHT + 2;
  const widths = buttons.map((b) => r.textWidth(b.label) + pad * 2);
  const total = widths.reduce((a, b) => a + b, 0) + gap * (buttons.length - 1);
  const x0 = W - 6 - total;
  const y0 = H - 6 - btnH;
  r.fillRect(x0 - 4, y0 - 3, total + 8, btnH + 6, UI_COLORS.panel, 0.92);
  r.strokeRect(x0 - 4, y0 - 3, total + 8, btnH + 6, UI_COLORS.panelEdge, 0.7);
  let x = x0;
  buttons.forEach((btn, idx) => {
    const w = widths[idx] ?? 0;
    if (btn.active) {
      r.fillRect(x, y0, w, btnH, UI_COLORS.selectBg);
      r.strokeRect(x, y0, w, btnH, UI_COLORS.gold);
    }
    r.text(x + pad, y0 + 2, btn.label, btn.active ? UI_COLORS.gold : UI_COLORS.text);
    sink.add(`popup-${btn.id}`, x, y0, w, btnH, true, () => btn.onPress());
    x += w + gap;
  });
}

// ---------------------------------------------------------------------------
// Full-panel overlay: Mode E configuration and diagnostics
// ---------------------------------------------------------------------------

function drawOverlay(r: PixelRaster, sink: RegionSink, o: OverlayModel): void {
  const W = r.width;
  const H = r.height;
  r.fillRect(0, 0, W, H, UI_COLORS.veilStrong);
  sink.add("overlay-dismiss", 0, 0, W, H, true, () => o.onDismiss());

  const box = { x: 6, y: 5, w: W - 12, h: H - 10 };
  r.fillRect(box.x, box.y, box.w, box.h, UI_COLORS.panel);
  r.strokeRect(box.x, box.y, box.w, box.h, UI_COLORS.panelEdge);
  sink.add("overlay-panel", box.x, box.y, box.w, box.h, true, () => undefined);

  r.text(box.x + 8, box.y + 6, o.title, UI_COLORS.gold);
  r.text(box.x + 8, box.y + 6 + LINE_HEIGHT, o.subtitle, UI_COLORS.muted);
  r.fillRect(box.x + 8, box.y + 6 + LINE_HEIGHT * 2 + 1, box.w - 16, 1, UI_COLORS.accent, 0.6);

  const footerLines: { text: string; tone: Tone }[] = [];
  for (const entry of o.footer) {
    for (const line of r.wrap(entry.text, box.w - 20)) {
      footerLines.push({ text: line, tone: entry.tone });
    }
  }
  // Footer = status lines + one key-hint line at the bottom; the list gets whatever height remains.
  const footerH = footerLines.length * LINE_HEIGHT + LINE_HEIGHT + 14;
  const listTop = box.y + 6 + LINE_HEIGHT * 2 + 6;
  const listBox = { x: box.x + 4, y: listTop, w: box.w - 8, h: box.y + box.h - footerH - listTop - 2 };
  drawList(r, sink, listBox, o.list, { panel: false });

  const footerTop = box.y + box.h - footerH;
  r.fillRect(box.x + 8, footerTop, box.w - 16, 1, UI_COLORS.accent, 0.4);
  let fy = footerTop + 4;
  for (const line of footerLines) {
    r.text(box.x + 10, fy, line.text, TONE_COLOR[line.tone]);
    fy += LINE_HEIGHT;
  }
  r.text(box.x + 10, box.y + box.h - LINE_HEIGHT - 4, "ARROWS MOVE/CHANGE  ENTER SELECT  ESC BACK", UI_COLORS.dim);
}

// ---------------------------------------------------------------------------
// Scrolling list
// ---------------------------------------------------------------------------

interface ListLayoutOptions {
  /** Draw a framed panel behind the list (title screen rows). */
  panel: boolean;
}

interface InfoLine {
  key?: string;
  text: string;
  tone: Tone;
}

/** Splits an info item into drawable lines (key on the same line when it fits). */
function infoLines(r: PixelRaster, item: InfoItem, width: number): InfoLine[] {
  const tone = item.tone ?? "muted";
  if (item.key === undefined) {
    return r.wrap(item.text, width).map((text) => ({ text, tone }));
  }
  const inline = `${item.key}  ${item.text}`;
  if (r.textWidth(inline) <= width) {
    return [{ key: item.key, text: item.text, tone }];
  }
  return [
    { text: item.key, tone: "gold" },
    ...r.wrap(item.text, width).map((text) => ({ text, tone })),
  ];
}

function itemHeight(r: PixelRaster, item: ListItem, width: number): number {
  if (item.kind === "header") return HEADER_HEIGHT;
  if (item.kind === "row") return ROW_HEIGHT;
  const lines = infoLines(r, item, width);
  return Math.max(1, lines.length) * LINE_HEIGHT + 3;
}

function listContentHeight(items: readonly ListItem[]): number {
  // Only used for title layout where info lines are short; measured with a generous width.
  let total = 0;
  for (const item of items) {
    total += item.kind === "header" ? HEADER_HEIGHT : item.kind === "row" ? ROW_HEIGHT : LINE_HEIGHT + 3;
  }
  return total;
}

function drawList(
  r: PixelRaster,
  sink: RegionSink,
  box: { x: number; y: number; w: number; h: number },
  list: ListModel,
  opts: ListLayoutOptions,
): void {
  if (opts.panel) {
    r.fillRect(box.x, box.y, box.w, box.h, UI_COLORS.panel, 0.94);
    r.strokeRect(box.x, box.y, box.w, box.h, UI_COLORS.panelEdge, 0.8);
  }
  const innerX = box.x + LIST_PAD;
  const innerW = box.w - LIST_PAD * 2 - (opts.panel ? 0 : SCROLLBAR_WIDTH + 2);
  const viewTop = box.y + LIST_PAD;
  const viewH = box.h - LIST_PAD * 2;

  const heights = list.items.map((item) => itemHeight(r, item, innerW - 12));
  const offsets: number[] = [];
  let content = 0;
  for (const h of heights) {
    offsets.push(content);
    content += h;
  }
  const maxScroll = Math.max(0, content - viewH);
  const scroll = list.scroll;
  scroll.top = Math.max(0, Math.min(scroll.top, maxScroll));
  if (list.selected !== scroll.followed) {
    scroll.followed = list.selected;
    if (list.selected >= 0 && list.selected < list.items.length) {
      const top = offsets[list.selected] ?? 0;
      const bottom = top + (heights[list.selected] ?? 0);
      if (top < scroll.top) scroll.top = top;
      if (bottom > scroll.top + viewH) scroll.top = Math.max(0, bottom - viewH);
    }
  }

  r.setClip(innerX - 2, viewTop, innerW + 4, viewH);
  list.items.forEach((item, idx) => {
    const top = (offsets[idx] ?? 0) - scroll.top;
    const h = heights[idx] ?? 0;
    if (top + h <= 0 || top >= viewH) return;
    const y = viewTop + top;
    if (item.kind === "header") {
      r.text(innerX, y + 2, item.label, UI_COLORS.cyan);
      r.fillRect(innerX, y + HEADER_HEIGHT - 3, innerW, 1, UI_COLORS.accent, 0.45);
      return;
    }
    if (item.kind === "info") {
      let ly = y + 1;
      for (const line of infoLines(r, item, innerW - 12)) {
        if (line.key !== undefined) {
          r.text(innerX + 6, ly, line.key, UI_COLORS.gold);
          r.text(innerX + 6 + r.textWidth(line.key) + 6, ly, line.text, TONE_COLOR[line.tone]);
        } else {
          r.text(innerX + 6, ly, line.text, TONE_COLOR[line.tone]);
        }
        ly += LINE_HEIGHT;
      }
      return;
    }
    drawRow(r, sink, innerX, y, innerW, item, idx === list.selected, list, viewTop, viewTop + viewH);
  });
  r.clearClip();

  if (maxScroll > 0) {
    const trackX = box.x + box.w - SCROLLBAR_WIDTH - 2;
    r.fillRect(trackX, viewTop, SCROLLBAR_WIDTH, viewH, UI_COLORS.dim, 0.4);
    const thumbH = Math.max(6, Math.round((viewH * viewH) / content));
    const thumbY = viewTop + Math.round((viewH - thumbH) * (scroll.top / maxScroll));
    r.fillRect(trackX, thumbY, SCROLLBAR_WIDTH, thumbH, UI_COLORS.gold);
  }
}

function drawRow(
  r: PixelRaster,
  sink: RegionSink,
  x: number,
  y: number,
  w: number,
  item: RowItem,
  selected: boolean,
  list: ListModel,
  visibleTop: number,
  visibleBottom: number,
): void {
  if (selected) {
    r.fillRect(x - 2, y - 1, w + 4, ROW_HEIGHT, UI_COLORS.selectBg);
    r.text(x - 1, y + 1, ">", UI_COLORS.gold);
  }
  const labelColor = !item.enabled
    ? UI_COLORS.disabled
    : selected
      ? UI_COLORS.gold
      : item.tone
        ? TONE_COLOR[item.tone]
        : UI_COLORS.text;
  r.text(x + 6, y + 1, item.label, labelColor);

  const valueTone = item.valueTone ? TONE_COLOR[item.valueTone] : selected ? UI_COLORS.text : UI_COLORS.muted;
  const valueColor = item.enabled ? valueTone : UI_COLORS.disabled;
  const valueRight = x + w - 6;
  const top = Math.max(y - 1, visibleTop);
  const bottom = Math.min(y + ROW_HEIGHT - 1, visibleBottom);

  // The row region is registered first so slider bars (registered below) sit on top of it.
  if (item.enabled && bottom > top) {
    sink.add(item.id, x - 2, top, w + 4, bottom - top, true, () => {
      const idx = list.items.indexOf(item);
      if (idx >= 0) list.onSelect(idx);
      item.onActivate();
    });
  }

  if (item.slider) {
    const valueWidth = r.textWidth(item.value);
    const barRight = valueRight - valueWidth - 8;
    const barX = barRight - SLIDER_WIDTH;
    r.fillRect(barX, y + 3, SLIDER_WIDTH, 4, UI_COLORS.dim, 0.5);
    const ratio = Math.max(0, Math.min(1, item.slider.ratio));
    r.fillRect(barX, y + 3, Math.round(SLIDER_WIDTH * ratio), 4, item.enabled ? UI_COLORS.accent : UI_COLORS.disabled);
    r.text(valueRight - valueWidth, y + 1, item.value, valueColor);
    if (item.enabled && item.onRatio && bottom > top) {
      const onRatio = item.onRatio;
      sink.add(`${item.id}-bar`, barX - 2, top, SLIDER_WIDTH + 4, bottom - top, true, (lx) => {
        onRatio(Math.max(0, Math.min(1, (lx - barX) / SLIDER_WIDTH)));
      });
    }
  } else {
    r.textRight(valueRight, y + 1, item.value, valueColor);
  }
}

/** Screen-space pill used by menus to label provenance (SOURCED / APPROX / EXT). */
export function provenanceTag(tag: string): ProvenanceTag {
  switch (tag) {
    case "sourced":
      return { text: "[SOURCED]", tone: "cyan" };
    case "approximate":
      return { text: "[APPROX]", tone: "gold" };
    default:
      return { text: "[EXT]", tone: "accent" };
  }
}
