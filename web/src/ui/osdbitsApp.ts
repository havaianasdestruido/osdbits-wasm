/**
 * OSDBITS application controller (UILess presentation).
 *
 * The page is only the application: one fullscreen WebGPU/Canvas 2D canvas. Every piece of
 * UI — the title/start screen, the Mode D OSDSYS menu, the Mode C warning line, the Mode E
 * options screen, diagnostics, the error state and the single corner pop-up — is drawn into an
 * in-app pixel raster (`inAppScreens.ts`) and composited over the scene. Pointer input is
 * resolved against the regions that were drawn for the current frame.
 *
 * Orchestrates:
 * - Single activation (pointer or key) that unlocks Web Audio synchronously and starts the
 *   deterministic WASM simulation + multi-pass WebGPU renderer.
 * - Mode D keyboard & pointer menu/clock navigation with provenance tags.
 * - Mode C localized warning text with provenance disclosure.
 * - Mode E options: display, towers, seed, speed, passes, audio mixer, local resource import,
 *   JSON export/import and a two-step reset.
 * - Live percentile telemetry and provenance on the diagnostics screen, plus non-destructive
 *   scene recovery.
 */

import {
  synthesizeProceduralTextures,
  type ProceduralTextureAsset,
} from "../assets/proceduralTextures";
import { importUserResourceArchive } from "../assets/userResourceImporter";
import { AudioEngine } from "../audio/audioEngine";
import { Canvas2DRenderer, compositeUiRasterOnCanvas2D } from "../renderer/canvas2dRenderer";
import { WebGpuRenderer, type AspectPresentationMode } from "../renderer/webgpuRenderer";
import {
  DEFAULT_FAITHFUL_SETTINGS,
  RESET_CONFIRMATION_TOKEN,
  StorageManager,
  sanitizeSettings,
  type OsdbitsSettings,
} from "../storage/storageManager";
import {
  WasmSimulationBridge,
  type FrameInspectionSnapshot,
  type MenuActionName,
  type MenuUiSnapshot,
  type OsdLanguageCode,
  type TowerPresetSpec,
  type VisualModeName,
} from "../wasm/bridge";
import {
  composeInAppUi,
  firstSelectable,
  nextSelectable,
  provenanceTag,
  type ListItem,
  type MenuModel,
  type OverlayModel,
  type PopupButton,
  type RowItem,
  type SceneModel,
  type TitleModel,
  type UiRegion,
} from "./inAppScreens";
import { logicalSizeForCanvas, PixelRaster, type UiOverlayFrame } from "./pixelRaster";

type ComposeInput = Parameters<typeof composeInAppUi>[1];

const MODE_PROVENANCE_DESCRIPTION: Record<VisualModeName, string> = {
  opening:
    "Sourced (opening.c + vucode_1.vsm + HeightGrid); clean-room procedural textures unless user TEXIMAGE imported.",
  idle:
    "Sourced tower/fog/cube math (opening.c) held at z = 16 for continuous atmospheric drift.",
  "illegal-disc":
    "Sourced DrawIllegalScene (openingType = 1); retail hardware osdBootParam value unverified (opening.c:4262).",
  menu:
    "Sourced Module U (menu.c / menuback.c / menuconfig.c / menutext.c); Browser & Version subscreens approximated.",
};

const MODE_ORDER: readonly VisualModeName[] = ["opening", "idle", "illegal-disc", "menu"];
const MODE_SHORT: Record<VisualModeName, string> = {
  opening: "A OPENING",
  idle: "B TOWER IDLE",
  "illegal-disc": "C RED DISC",
  menu: "D MENU & CONFIG",
};
const MODE_TAB: Record<VisualModeName, string> = {
  opening: "OPEN",
  idle: "IDLE",
  "illegal-disc": "RED",
  menu: "MENU",
};
const ASPECT_OPTIONS: readonly AspectPresentationMode[] = ["4:3", "16:9", "fit"];
const ASPECT_LABEL: Record<AspectPresentationMode, string> = {
  "4:3": "4:3 AUTHENTIC",
  "16:9": "16:9 WIDESCREEN",
  fit: "FILL VIEWPORT",
};
const RES_OPTIONS: readonly number[] = [0.5, 1, 1.5, 2];
const RES_LABEL: Record<string, string> = {
  "0.5": "0.5X PERFORMANCE",
  "1": "1.0X NATIVE",
  "1.5": "1.5X CRISP HD",
  "2": "2.0X SUPERSAMPLED",
};
const LANGUAGE_LABEL: readonly string[] = [
  "JAPANESE (CLEAN-ROOM)",
  "ENGLISH",
  "FRENCH",
  "SPANISH",
  "GERMAN",
  "ITALIAN",
  "DUTCH",
  "PORTUGUESE",
];
const TOWER_OPTIONS: readonly TowerPresetSpec["kind"][] = ["captured", "full", "simulated"];
const TOWER_LABEL: Record<TowerPresetSpec["kind"], string> = {
  captured: "SAVE CARD",
  full: "FULL 126 GRID",
  simulated: "SIMULATED CARD",
};
const MAX_SEED = 4294967295;
const MAX_GAMES = 126;
const MAX_SPEED = 3;
const PAINT_INTERVAL_MS = 50;
const POPUP_IDLE_MS = 2500;
const CURSOR_IDLE_MS = 3000;
const TOAST_MS = 2200;
const MENU_KEY_ACTIONS: Record<string, MenuActionName> = {
  ArrowUp: "up",
  w: "up",
  W: "up",
  ArrowDown: "down",
  s: "down",
  S: "down",
  ArrowLeft: "left",
  a: "left",
  A: "left",
  ArrowRight: "right",
  d: "right",
  D: "right",
  Enter: "confirm",
  " ": "confirm",
  z: "confirm",
  Z: "confirm",
  Escape: "cancel",
  Backspace: "cancel",
  x: "cancel",
  X: "cancel",
  o: "options",
  O: "options",
};

type OverlayKind = "options" | "diagnostics";

function percentile(sorted: readonly number[], p: number): number {
  if (sorted.length === 0) return 0;
  const idx = Math.min(sorted.length - 1, Math.max(0, Math.floor((sorted.length - 1) * p)));
  return sorted[idx] ?? 0;
}

function cycle<T>(options: readonly T[], current: T, dir: -1 | 1): T {
  const idx = Math.max(0, options.indexOf(current));
  return options[(idx + dir + options.length) % options.length] as T;
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function ensureSelection(items: readonly ListItem[], current: number): number {
  const at = items[current];
  if (at && at.kind === "row" && at.enabled) return current;
  return firstSelectable(items);
}

export class OsdbitsApp {
  readonly bridge: WasmSimulationBridge;
  readonly storage: StorageManager;
  readonly audio: AudioEngine;
  renderer: WebGpuRenderer | Canvas2DRenderer | null = null;

  private settings: OsdbitsSettings = sanitizeSettings(DEFAULT_FAITHFUL_SETTINGS);
  /** Scene has been started by the user (title screen dismissed). */
  private started = false;
  /** Simulation is advancing; false while faulted. */
  private running = false;
  private rafId = 0;
  private lastTickTimeMs = 0;
  private simAccumMs = 0;
  private lastSnapshot: FrameInspectionSnapshot | null = null;

  private frameDeltasMs: number[] = [];
  private wasmStepTimesMs: number[] = [];
  private awaitingResetConfirm = false;
  private webGpuErrorReason: string | null = null;
  private textureAssets: ProceduralTextureAsset[] | null = null;

  /** Simulated tower count kept while the preset is not "simulated". */
  private pendingGames = 36;
  private titleSelected = -1;
  private titleScroll = { top: 0 };
  private overlay: OverlayKind | null = null;
  private overlaySelected = -1;
  private overlayScroll = { top: 0 };
  private popupPinned = false;
  private popupUntilMs = 0;
  private toast: { text: string; untilMs: number } | null = null;
  private runtimeError: string | null = null;
  private persistenceStatus = "Ready.";
  private resourceStatus = "Using 100% clean-room procedural assets.";

  /** Set when the scene must be fully re-rendered (not just the UI layer re-presented). */
  private sceneDirty = true;
  private lastPaintMs = -Infinity;
  private lastPointerMoveMs = 0;
  private hoverRegion: UiRegion | null = null;
  private regions: UiRegion[] = [];
  private raster: PixelRaster | null = null;
  private srSignature = "";
  private pickerInput: HTMLInputElement | null = null;

  private readonly rootEl: HTMLElement;
  private canvasEl: HTMLCanvasElement;
  private srEl: HTMLElement | null = null;

  constructor(
    rootEl: HTMLElement,
    canvasEl: HTMLCanvasElement,
    bridge: WasmSimulationBridge,
    storage: StorageManager,
    audio: AudioEngine,
  ) {
    this.rootEl = rootEl;
    this.canvasEl = canvasEl;
    this.bridge = bridge;
    this.storage = storage;
    this.audio = audio;
  }

  async initialize(): Promise<void> {
    // Respect OS prefers-reduced-motion on first visit before user saves explicit settings
    const prefersReducedMotion =
      typeof window !== "undefined" &&
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches;

    const loaded = await this.storage.init();
    this.settings = loaded.settings;
    if (loaded.revision === 0 && prefersReducedMotion) {
      this.settings = { ...this.settings, reducedMotion: true };
    }
    if (this.settings.towerPreset.kind === "simulated") {
      this.pendingGames = this.settings.towerPreset.ngames;
    }

    this.audio.setMixer(this.mixerSnapshot());

    this.applySettingsToSimulation(true);
    this.lastSnapshot = this.bridge.stepFrame();

    // Restore any persisted user-imported resource bundle
    let initialTextures = synthesizeProceduralTextures();
    const persistedAsset = await this.storage.loadImportedAsset();
    if (persistedAsset) {
      const imp = await importUserResourceArchive(persistedAsset.bytes, this.bridge);
      if (imp.ok) {
        initialTextures = imp.textures;
      }
    }
    this.textureAssets = initialTextures;

    try {
      this.renderer = await WebGpuRenderer.create({
        canvas: this.canvasEl,
        aspectMode: this.settings.aspectMode,
        resolutionScale: this.settings.resolutionScale,
        initialTextures,
        onDeviceLost: (info) => {
          this.showRuntimeError(
            `WebGPU device lost (${info.reason}). Click RECOVER SCENE or press R to restore.`,
          );
        },
        onDeviceRecovered: () => {
          this.hideRuntimeError();
        },
      });
      this.renderer.setDisplayConfig({
        aspectMode: this.settings.aspectMode,
        resolutionScale: this.settings.resolutionScale,
        pal: this.settings.pal,
      });
      this.renderer.syncCanvasSize();
      // Atmospheric preview frame behind the title screen.
      this.renderer.renderFrame(
        this.bridge.getVertexView(),
        this.bridge.getCommandView(),
        this.lastSnapshot.mode,
      );
    } catch (err) {
      this.renderer = null;
      this.webGpuErrorReason = err instanceof Error ? err.message : String(err);
      console.error("[osdbits] WebGPU renderer unavailable:", err);
      // A failed WebGPU attempt can leave the canvas bound to a WebGPU context; a fresh
      // element keeps the reduced Canvas 2D path (title screen and fallback) available.
      if (this.canvasEl.getContext("2d") === null && this.canvasEl.parentNode) {
        const replacement = this.canvasEl.cloneNode(false) as HTMLCanvasElement;
        this.canvasEl.replaceWith(replacement);
        this.canvasEl = replacement;
      }
    }

    this.mountCanvasUi();
    this.refreshBackendDataset();
    this.titleSelected = firstSelectable(this.buildTitleItems());
    this.rafId = requestAnimationFrame(this.onAnimationFrame);
  }

  /**
   * Synchronously unlocks Web Audio inside the user's single activation gesture and starts
   * the live OSDSYS simulation + renderer loop with no second click required.
   */
  startFromUserGesture(): void {
    // CRITICAL: Must run synchronously before any async work!
    void this.audio.activateFromUserGesture();

    if (!this.renderer || this.started) {
      return;
    }

    this.applySettingsToSimulation(true);
    this.lastSnapshot = this.bridge.stepFrame();
    this.audio.dispatchCues(this.lastSnapshot.audioCues);

    this.started = true;
    this.running = true;
    this.lastTickTimeMs = performance.now();
    this.simAccumMs = 0;
    this.rootEl.dataset.sceneActive = "true";
    this.rootEl.dataset.activeMode = this.lastSnapshot.mode;
    this.touchPopup(performance.now());
    this.markSceneDirty();
  }

  /** Starts the explicitly requested reduced Canvas 2D backend without another user gesture. */
  proceedWithCanvasFallback(): void {
    if (this.renderer) {
      this.startFromUserGesture();
      return;
    }

    try {
      // A failed WebGPU initialization may already have claimed the canvas context type.
      // In that case, swap in an equivalent fresh canvas before requesting Canvas 2D.
      let hasCanvas2D = false;
      try {
        hasCanvas2D = this.canvasEl.getContext("2d") !== null;
      } catch {
        hasCanvas2D = false;
      }
      if (!hasCanvas2D) {
        if (!this.canvasEl.parentNode) {
          throw new Error("Cannot replace a detached canvas");
        }
        const replacement = this.canvasEl.cloneNode(false) as HTMLCanvasElement;
        this.canvasEl.replaceWith(replacement);
        this.canvasEl = replacement;
        this.bindCanvasEvents();
      }

      this.renderer = Canvas2DRenderer.create({
        canvas: this.canvasEl,
        aspectMode: this.settings.aspectMode,
        resolutionScale: this.settings.resolutionScale,
        initialTextures: this.textureAssets ?? undefined,
      });
      this.renderer.setDisplayConfig({
        aspectMode: this.settings.aspectMode,
        resolutionScale: this.settings.resolutionScale,
        pal: this.settings.pal,
      });
      this.renderer.syncCanvasSize();
      this.refreshBackendDataset();
      this.startFromUserGesture();
    } catch (err) {
      this.showToast(
        `CANVAS 2D FALLBACK COULD NOT START: ${err instanceof Error ? err.message : String(err)}`,
        6000,
      );
    }
  }

  async switchMode(mode: VisualModeName): Promise<void> {
    this.settings = { ...this.settings, mode };
    this.applySettingsToSimulation(true);
    this.lastSnapshot = this.bridge.stepFrame();
    this.audio.dispatchCues(this.lastSnapshot.audioCues);
    this.rootEl.dataset.activeMode = this.lastSnapshot.mode;
    this.markSceneDirty();
    this.showToast(`SCENE: ${MODE_SHORT[mode]}`);
    await this.persistCurrentSettings();
  }

  /** Triggers a recoverable scene error without crashing the page (used by fault-recovery tests & the error box). */
  triggerSimulatedFault(message = "Simulated runtime scene fault"): void {
    this.running = false;
    this.showRuntimeError(message);
  }

  async recoverSceneWithoutReload(): Promise<boolean> {
    try {
      if (this.renderer) {
        await this.renderer.simulateDeviceLossAndRecover();
      } else {
        this.renderer = await WebGpuRenderer.create({
          canvas: this.canvasEl,
          aspectMode: this.settings.aspectMode,
          resolutionScale: this.settings.resolutionScale,
        });
      }
      this.applySettingsToSimulation(true);
      this.lastSnapshot = this.bridge.stepFrame();
      this.renderer.syncCanvasSize();
      this.refreshBackendDataset();
      this.hideRuntimeError();
      this.markSceneDirty();

      if (this.started && !this.running) {
        this.running = true;
        this.lastTickTimeMs = performance.now();
        this.simAccumMs = 0;
      }
      return true;
    } catch (err) {
      this.showRuntimeError(
        `Recovery failed: ${err instanceof Error ? err.message : String(err)}`,
      );
      return false;
    }
  }

  async importResourceBytes(bytes: Uint8Array): Promise<boolean> {
    try {
      const report = await importUserResourceArchive(bytes, this.bridge);
      if (!report.ok) {
        this.resourceStatus = `Import rejected: ${report.errorMessage ?? "Invalid resource container"}`;
        return false;
      }
      this.textureAssets = report.textures;
      this.renderer?.updateTextures(report.textures);
      await this.storage.saveImportedAsset(bytes, report.importedSlots);
      this.resourceStatus = `Imported ${report.importedSlots.length} verified slots (SHA-256 ${report.archiveSha256.slice(0, 12)})`;
      this.markSceneDirty();
      return true;
    } catch (err) {
      this.resourceStatus = `Import rejected: ${err instanceof Error ? err.message : String(err)}`;
      return false;
    }
  }

  getSettings(): Readonly<OsdbitsSettings> {
    return this.settings;
  }

  getLatestSnapshot(): FrameInspectionSnapshot | null {
    return this.lastSnapshot;
  }

  /** Visible UI text drawn for the current frame (test and accessibility hook). */
  getDrawnUiText(): string[] {
    return this.raster ? [...this.raster.drawnText] : [];
  }

  /**
   * Client-space centre of a drawn UI region, or null when no such region is on screen.
   * Lets tests and automation click in-app controls through the real canvas input path.
   */
  uiHitCenter(id: string): { x: number; y: number; enabled: boolean } | null {
    const region = this.regions.find((r) => r.id === id);
    const raster = this.raster;
    if (!region || !raster) return null;
    const rect = this.canvasEl.getBoundingClientRect();
    const sx = rect.width / raster.width;
    const sy = rect.height / raster.height;
    return {
      x: rect.left + (region.x + region.w / 2) * sx,
      y: rect.top + (region.y + region.h / 2) * sy,
      enabled: region.enabled,
    };
  }

  /** All regions drawn for the current frame, in hit-test order (lowest first). */
  getUiRegions(): { id: string; enabled: boolean }[] {
    return this.regions.map((r) => ({ id: r.id, enabled: r.enabled }));
  }

  /** Last persistence and resource-import status lines shown on the options screen. */
  getPanelStatus(): { persistence: string; resource: string } {
    return { persistence: this.persistenceStatus, resource: this.resourceStatus };
  }

  dispose(): void {
    this.running = false;
    if (this.rafId) {
      cancelAnimationFrame(this.rafId);
      this.rafId = 0;
    }
    window.removeEventListener("resize", this.onWindowResize);
    window.removeEventListener("keydown", this.onGlobalKeyDown);
    this.canvasEl.removeEventListener("pointerdown", this.onPointerDown);
    this.canvasEl.removeEventListener("pointermove", this.onPointerMove);
    this.canvasEl.removeEventListener("wheel", this.onWheel);
    this.pickerInput?.remove();
    this.pickerInput = null;
    this.renderer?.dispose();
    this.audio.dispose();
    this.storage.close();
  }

  // -------------------------------------------------------------------------
  // Frame loop
  // -------------------------------------------------------------------------

  private readonly onAnimationFrame = (nowMs: number): void => {
    this.rafId = requestAnimationFrame(this.onAnimationFrame);
    if (this.running) {
      this.stepSimulation(nowMs);
    }
    this.presentFrame(nowMs);
  };

  private stepSimulation(nowMs: number): void {
    const rawDt = Math.max(0, Math.min(100, nowMs - this.lastTickTimeMs));
    this.lastTickTimeMs = nowMs;

    this.frameDeltasMs.push(rawDt);
    if (this.frameDeltasMs.length > 120) {
      this.frameDeltasMs.shift();
    }

    const targetFrameMs = this.settings.pal ? 1000 / 50 : 1000 / 60;
    this.simAccumMs += rawDt;
    let steps = 0;
    while (this.simAccumMs >= targetFrameMs && steps < 3) {
      const t0 = performance.now();
      const snap = this.bridge.stepFrame();
      const t1 = performance.now();
      this.wasmStepTimesMs.push(t1 - t0);
      if (this.wasmStepTimesMs.length > 120) {
        this.wasmStepTimesMs.shift();
      }
      this.lastSnapshot = snap;
      this.audio.dispatchCues(snap.audioCues);
      if (snap.mode !== this.settings.mode) {
        // Automatic Mode A -> Mode D transition completed!
        this.settings = { ...this.settings, mode: snap.mode };
        this.rootEl.dataset.activeMode = snap.mode;
      }
      this.simAccumMs -= targetFrameMs;
      steps++;
    }
  }

  /**
   * Composes this frame's UI and presents scene + UI. The WebGPU path re-presents only the
   * UI layer over the last scene when nothing animates; Canvas 2D paints in full at a capped rate
   * while paused because it has no retained scene target.
   */
  private presentFrame(nowMs: number): void {
    this.syncCanvasBuffer();
    const raster = this.ensureRaster();
    const input = this.buildComposeInput(nowMs);
    this.regions = composeInAppUi(raster, input);
    this.updateDatasetFromFrame(input);
    this.updateScreenReaderText();
    this.updateCursor(nowMs);

    const ui: UiOverlayFrame = { width: raster.width, height: raster.height, rgba: raster.rgba };
    const renderer = this.renderer;
    const snap = this.lastSnapshot;

    if (renderer && snap) {
      if (renderer.backend === "webgpu") {
        if (this.running || this.sceneDirty) {
          renderer.renderFrame(
            this.bridge.getVertexView(),
            this.bridge.getCommandView(),
            snap.mode,
            ui,
          );
          this.sceneDirty = false;
        } else if (!renderer.presentUi(ui)) {
          renderer.renderFrame(
            this.bridge.getVertexView(),
            this.bridge.getCommandView(),
            snap.mode,
            ui,
          );
        }
      } else {
        if (this.running || this.sceneDirty || nowMs - this.lastPaintMs >= PAINT_INTERVAL_MS) {
          renderer.renderFrame(
            this.bridge.getVertexView(),
            this.bridge.getCommandView(),
            snap.mode,
            ui,
          );
          this.sceneDirty = false;
          this.lastPaintMs = nowMs;
        }
      }
      return;
    }

    // No renderer yet (WebGPU unavailable): the title screen still draws through the 2D canvas.
    if (nowMs - this.lastPaintMs < PAINT_INTERVAL_MS) return;
    const ctx = this.canvasEl.getContext("2d");
    if (!ctx) return;
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.fillStyle = "#000";
    ctx.fillRect(0, 0, this.canvasEl.width, this.canvasEl.height);
    ctx.restore();
    compositeUiRasterOnCanvas2D(ctx, ui);
    this.lastPaintMs = nowMs;
  }

  private syncCanvasBuffer(): void {
    if (this.renderer) {
      this.renderer.syncCanvasSize();
      return;
    }
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    const w = Math.max(1, Math.round(this.canvasEl.clientWidth * dpr));
    const h = Math.max(1, Math.round(this.canvasEl.clientHeight * dpr));
    if (w > 1 && h > 1 && (this.canvasEl.width !== w || this.canvasEl.height !== h)) {
      this.canvasEl.width = w;
      this.canvasEl.height = h;
      this.markSceneDirty();
    }
  }

  private ensureRaster(): PixelRaster {
    const size = logicalSizeForCanvas(this.canvasEl.width, this.canvasEl.height);
    if (!this.raster || this.raster.width !== size.width || this.raster.height !== size.height) {
      this.raster = new PixelRaster(size.width, size.height);
    }
    return this.raster;
  }

  private markSceneDirty(): void {
    this.sceneDirty = true;
  }

  // -------------------------------------------------------------------------
  // Composition: view models for the in-app screens
  // -------------------------------------------------------------------------

  private buildComposeInput(nowMs: number): ComposeInput {
    if (!this.started) {
      return { nowMs, started: false, title: this.buildTitleModel() };
    }
    if (this.toast && nowMs > this.toast.untilMs) {
      this.toast = null;
    }
    const overlay = this.overlay ? this.buildOverlayModel(this.overlay) : undefined;
    return {
      nowMs,
      started: true,
      scene: this.buildSceneModel(nowMs, overlay !== undefined),
      overlay,
    };
  }

  private buildTitleModel(): TitleModel {
    const backend = this.renderer?.backend ?? null;
    const hasWebGpu = backend === "webgpu";
    const rendererLabel = hasWebGpu
      ? "WEBGPU READY"
      : backend === "canvas2d"
        ? "CANVAS 2D (REDUCED)"
        : "WEBGPU UNAVAILABLE";
    const storageLabel =
      this.storage.getBackendStatus() === "indexeddb" ? "INDEXEDDB VERIFIED" : "SESSION MEMORY FALLBACK";
    const items = this.buildTitleItems();
    this.titleSelected = ensureSelection(items, this.titleSelected);
    return {
      subtitle:
        "DETERMINISTIC RUST/WEBASSEMBLY SIMULATION AND MULTI-PASS PS2-GS WEBGPU RENDERER. 100% CLEAN-ROOM, ZERO PROPRIETARY BIOS ASSETS.",
      statusLines: [
        { text: `RENDERER: ${rendererLabel}`, tone: hasWebGpu ? "cyan" : "crimson" },
        { text: `WASM ABI V${this.bridge.abiVersion}  STORAGE: ${storageLabel}`, tone: "muted" },
      ],
      reason: hasWebGpu
        ? undefined
        : `WEBGPU ADAPTER UNAVAILABLE: ${this.webGpuErrorReason ?? "navigator.gpu missing"}. THE FULL RENDERER NEEDS WEBGPU. YOU CAN PROCEED WITH THE REDUCED CANVAS 2D BACKEND BELOW.`,
      list: {
        items,
        selected: this.titleSelected,
        onSelect: (idx) => {
          this.titleSelected = idx;
        },
        scroll: this.titleScroll,
      },
      startEnabled: hasWebGpu,
      onBackdrop: () => {
        if (hasWebGpu) this.startFromUserGesture();
      },
      legal:
        "CLEAN-ROOM RECONSTRUCTION BASED ON AAP/OSDBITS. NO SONY BIOS, ROM, OR COPYRIGHTED AUDIO/TEXTURES INCLUDED.",
      prompt: hasWebGpu ? "PRESS ENTER OR CLICK TO START" : "CHOOSE AN OPTION ABOVE",
    };
  }

  private buildTitleItems(): ListItem[] {
    const hasWebGpu = this.renderer?.backend === "webgpu";
    const items: ListItem[] = [
      {
        kind: "row",
        id: "title-start",
        label: "START OSDSYS",
        value: "",
        enabled: hasWebGpu,
        valueTone: "gold",
        onActivate: () => this.startFromUserGesture(),
      },
      {
        kind: "row",
        id: "title-scene",
        label: "INITIAL SCENE",
        value: `< ${MODE_SHORT[this.settings.mode]} >`,
        enabled: true,
        valueTone: "cyan",
        onActivate: () => this.cycleInitialScene(1),
        onAdjust: (dir) => this.cycleInitialScene(dir),
      },
    ];
    if (!hasWebGpu) {
      items.push({
        kind: "row",
        id: "title-canvas2d",
        label: "PROCEED WITH CANVAS 2D",
        value: "REDUCED",
        enabled: true,
        valueTone: "crimson",
        onActivate: () => this.proceedWithCanvasFallback(),
      });
    }
    return items;
  }

  private cycleInitialScene(dir: -1 | 1): void {
    const mode = cycle(MODE_ORDER, this.settings.mode, dir);
    this.settings = { ...this.settings, mode };
    void this.persistCurrentSettings();
  }

  private buildSceneModel(nowMs: number, overlayOpen: boolean): SceneModel {
    const snap = this.lastSnapshot;
    const model: SceneModel = {};
    if (snap && snap.mode === "illegal-disc" && snap.illegalTextAlpha > 16) {
      model.illegal = {
        text: snap.illegalMsg,
        alpha: snap.illegalTextAlpha / 255,
        note: `PROVENANCE: SOURCED DRAWILLEGALSCENE (OPENINGTYPE = 1). RETAIL OSDBOOTPARAM VALUE UNVERIFIED (OPENING.C:4262).`,
      };
    }
    if (snap && snap.mode === "menu" && snap.menuUi) {
      model.menu = this.buildMenuModel(snap, snap.menuUi);
    }
    if (this.runtimeError) {
      model.error = { message: this.runtimeError, onRecover: () => void this.recoverSceneWithoutReload() };
    }
    if (this.toast && nowMs <= this.toast.untilMs) {
      model.toast = { text: this.toast.text, alpha: 1 };
    }
    if (!overlayOpen && (this.popupPinned || nowMs < this.popupUntilMs)) {
      model.popup = { buttons: this.buildPopupButtons() };
    }
    return model;
  }

  private buildPopupButtons(): PopupButton[] {
    const buttons: PopupButton[] = MODE_ORDER.map((mode) => ({
      id: `mode-${mode}`,
      label: MODE_TAB[mode],
      active: this.settings.mode === mode,
      onPress: () => {
        void this.switchMode(mode);
        this.touchPopup(performance.now());
      },
    }));
    buttons.push({
      id: "options",
      label: "OPTIONS",
      active: this.overlay === "options",
      onPress: () => this.openOverlay("options"),
    });
    return buttons;
  }

  private buildMenuModel(snap: FrameInspectionSnapshot, ui: MenuUiSnapshot): MenuModel {
    const interactive = ui.interactive;
    let note: string | undefined;
    if (ui.screen === "browser-approx") {
      note = `MEMORY CARD / DISC BROWSER (CLEAN-ROOM INSPECTOR): ${snap.orbs} ACTIVE RTC ORBS. PRESS ESC TO RETURN.`;
    } else if (ui.screen === "version-approx") {
      note = `OSDBITS-WASM CLEAN-ROOM BUILD  |  WASM ABI V${this.bridge.abiVersion}  |  ZERO SONY ROM BYTES.`;
    }
    const showItems = ui.screen !== "browser-approx" && ui.screen !== "version-approx";
    const items = showItems
      ? ui.items.map((item) => ({
          label: item.label.toUpperCase(),
          value: item.value.toUpperCase(),
          selected: item.selected,
          tag: provenanceTag(item.provenance),
        }))
      : [];
    const hints: MenuModel["hints"] = [
      { id: "confirm", label: "ENTER CONFIRM", onPress: () => this.handleMenuAction("confirm") },
      { id: "cancel", label: "ESC BACK", onPress: () => this.handleMenuAction("cancel") },
    ];
    if (ui.showOptions) {
      hints.push({ id: "options", label: "O VERSION", onPress: () => this.handleMenuAction("options") });
    }
    const model: MenuModel = {
      title: ui.title.toUpperCase(),
      clock: `${ui.date}  ${ui.time}`,
      interactive,
      note,
      items,
      onItem: (idx) => this.confirmMenuItem(idx),
      hints,
    };
    if (ui.clockFields && ui.screen === "config-edit-clock") {
      const fields = ui.clockFields;
      model.clockEditor = {
        values: fields,
        cursor: ui.clockCursor,
        onField: (fIdx) => this.moveClockCursor(fIdx),
      };
    }
    return model;
  }

  private buildOverlayModel(kind: OverlayKind): OverlayModel {
    if (kind === "diagnostics") {
      const items = this.buildDiagnosticsItems();
      this.overlaySelected = ensureSelection(items, this.overlaySelected);
      return {
        title: "DIAGNOSTICS",
        subtitle: "LIVE TELEMETRY & PROVENANCE INSPECTOR",
        list: {
          items,
          selected: this.overlaySelected,
          onSelect: (idx) => {
            this.overlaySelected = idx;
          },
          scroll: this.overlayScroll,
        },
        footer: [{ text: `PERSISTENCE: ${this.persistenceStatus}`, tone: "muted" }],
        onDismiss: () => this.closeOverlay(),
      };
    }

    const items = this.buildOptionsItems();
    this.overlaySelected = ensureSelection(items, this.overlaySelected);
    return {
      title: "OPTIONS",
      subtitle: "MODE E  LIVE CONFIGURATION",
      list: {
        items,
        selected: this.overlaySelected,
        onSelect: (idx) => {
          this.overlaySelected = idx;
        },
        scroll: this.overlayScroll,
      },
      footer: [
        { text: `PERSISTENCE: ${this.persistenceStatus}`, tone: "muted" },
        { text: `RESOURCES: ${this.resourceStatus}`, tone: "muted" },
      ],
      onDismiss: () => this.closeOverlay(),
    };
  }

  private buildOptionsItems(): ListItem[] {
    const s = this.settings;
    const rows: ListItem[] = [];
    const row = (item: RowItem): void => {
      rows.push(item);
    };
    const header = (label: string): void => {
      rows.push({ kind: "header", label });
    };
    const toggle = (
      id: string,
      label: string,
      on: boolean,
      apply: () => void,
    ): RowItem => ({
      kind: "row",
      id,
      label,
      value: on ? "ON" : "OFF",
      enabled: true,
      valueTone: on ? "gold" : "dim",
      onActivate: apply,
    });

    header("DISPLAY & TIMING");
    row({
      kind: "row",
      id: "opt-video",
      label: "VIDEO STANDARD",
      value: this.settings.pal ? "PAL 50 HZ" : "NTSC 60 HZ",
      enabled: true,
      onActivate: () => this.commitSettings({ pal: !this.settings.pal }, "reinit"),
    });
    row({
      kind: "row",
      id: "opt-aspect",
      label: "ASPECT FRAMING",
      value: ASPECT_LABEL[s.aspectMode],
      enabled: true,
      onActivate: () => this.stepSetting("aspectMode", ASPECT_OPTIONS, 1, "reinit"),
      onAdjust: (dir) => this.stepSetting("aspectMode", ASPECT_OPTIONS, dir, "reinit"),
    });
    row({
      kind: "row",
      id: "opt-res",
      label: "RESOLUTION SCALE",
      value: RES_LABEL[String(s.resolutionScale)] ?? String(s.resolutionScale),
      enabled: true,
      onActivate: () => this.stepSetting("resolutionScale", RES_OPTIONS, 1, "reinit"),
      onAdjust: (dir) => this.stepSetting("resolutionScale", RES_OPTIONS, dir, "reinit"),
    });
    row({
      kind: "row",
      id: "opt-lang",
      label: "OSD LANGUAGE",
      value: LANGUAGE_LABEL[s.language] ?? "ENGLISH",
      enabled: true,
      onActivate: () => this.cycleLanguage(1),
      onAdjust: (dir) => this.cycleLanguage(dir),
    });
    row({
      kind: "row",
      id: "opt-mode",
      label: "ACTIVE SCENE",
      value: MODE_SHORT[s.mode],
      enabled: true,
      valueTone: "cyan",
      onActivate: () => void this.switchMode(cycle(MODE_ORDER, s.mode, 1)),
      onAdjust: (dir) => void this.switchMode(cycle(MODE_ORDER, s.mode, dir)),
    });

    header("TOWERS, CAMERA & SEED");
    row({
      kind: "row",
      id: "opt-tower",
      label: "TOWER FIELD",
      value: TOWER_LABEL[s.towerPreset.kind],
      enabled: true,
      onActivate: () => this.setTowerKind(cycle(TOWER_OPTIONS, s.towerPreset.kind, 1)),
      onAdjust: (dir) => this.setTowerKind(cycle(TOWER_OPTIONS, s.towerPreset.kind, dir)),
    });
    row({
      kind: "row",
      id: "opt-games",
      label: "SIMULATED GAMES",
      value: String(this.currentGames()),
      enabled: s.towerPreset.kind === "simulated",
      onActivate: () => this.adjustGames(1),
      onAdjust: (dir, big) => this.adjustGames(big ? dir * 10 : dir),
    });
    row({
      kind: "row",
      id: "opt-seed",
      label: "DETERMINISTIC SEED",
      value: String(s.rngSeed),
      enabled: true,
      onActivate: () => this.adjustSeed(1),
      onAdjust: (dir, big) => this.adjustSeed(big ? dir * 1000 : dir),
    });
    row({
      kind: "row",
      id: "opt-speed",
      label: "ANIMATION SPEED",
      value: `${s.speedMultiplier.toFixed(1)}X`,
      enabled: true,
      slider: { ratio: s.speedMultiplier / MAX_SPEED },
      onActivate: () => this.setSpeed(1),
      onAdjust: (dir, big) => this.setSpeed(s.speedMultiplier + dir * (big ? 0.5 : 0.1)),
      onRatio: (ratio) => this.setSpeed(Math.round(ratio * MAX_SPEED * 10) / 10),
    });

    header("VISUAL PASSES & ACCESSIBILITY");
    row(toggle("opt-fog", "FOG CLOUDS", s.enableFog, () => this.commitSettings({ enableFog: !this.settings.enableFog }, "live")));
    row(toggle("opt-lights", "LIGHT TRAILS", s.enableLights, () => this.commitSettings({ enableLights: !this.settings.enableLights }, "live")));
    row(toggle("opt-cubes", "GLASS CUBES", s.enableCubes, () => this.commitSettings({ enableCubes: !this.settings.enableCubes }, "live")));
    row(toggle("opt-glow", "EXTRABUF GLOW", s.enableGlow, () => this.commitSettings({ enableGlow: !this.settings.enableGlow }, "live")));
    row(toggle("opt-blur", "MOTION/ZOOM BLUR", s.enableMotionBlur, () => this.commitSettings({ enableMotionBlur: !this.settings.enableMotionBlur }, "live")));
    row(toggle("opt-reduced", "REDUCED MOTION", s.reducedMotion, () => this.commitSettings({ reducedMotion: !this.settings.reducedMotion }, "live")));
    row(toggle("opt-auto-menu", "OPENING TO MENU", s.autoTransitionToMenu, () => this.commitSettings({ autoTransitionToMenu: !this.settings.autoTransitionToMenu }, "reinit")));
    row(toggle("opt-letterbox", "16:9 LETTERBOX", s.letterbox, () => this.commitSettings({ letterbox: !this.settings.letterbox }, "live")));

    header("AUDIO MIXER");
    row(this.volumeRow("opt-master", "MASTER VOLUME", "masterVolume"));
    row(this.volumeRow("opt-ambient", "AMBIENT VOLUME", "ambientVolume"));
    row(this.volumeRow("opt-sfx", "UI FOLEY VOLUME", "sfxVolume"));
    row({
      kind: "row",
      id: "opt-mute",
      label: "AUDIO OUTPUT",
      value: s.audioMuted ? "MUTED" : "ON",
      enabled: true,
      valueTone: s.audioMuted ? "crimson" : "gold",
      onActivate: () => this.toggleMute(),
    });

    header("RESOURCES & SAVE DATA");
    row({
      kind: "row",
      id: "opt-import-res",
      label: "IMPORT RESOURCE FILE",
      value: "OPTIONAL",
      enabled: true,
      valueTone: "muted",
      onActivate: () => this.openFilePicker("", (file) => void this.importResourceFile(file)),
    });
    row({
      kind: "row",
      id: "opt-export",
      label: "EXPORT SETTINGS JSON",
      value: "",
      enabled: true,
      onActivate: () => void this.exportSettings(),
    });
    row({
      kind: "row",
      id: "opt-import-json",
      label: "IMPORT SETTINGS JSON",
      value: "",
      enabled: true,
      onActivate: () => this.openFilePicker("application/json,.json", (file) => void this.importSettingsFile(file)),
    });
    if (this.awaitingResetConfirm) {
      row({
        kind: "row",
        id: "opt-reset-confirm",
        label: "CONFIRM RESET TO DEFAULTS",
        value: "",
        enabled: true,
        valueTone: "crimson",
        tone: "crimson",
        onActivate: () => this.performReset(),
      });
      row({
        kind: "row",
        id: "opt-reset-cancel",
        label: "CANCEL RESET",
        value: "",
        enabled: true,
        valueTone: "muted",
        onActivate: () => {
          this.awaitingResetConfirm = false;
        },
      });
    } else {
      row({
        kind: "row",
        id: "opt-reset",
        label: "RESET TO DEFAULTS",
        value: "",
        enabled: true,
        valueTone: "crimson",
        onActivate: () => {
          this.awaitingResetConfirm = true;
        },
      });
    }

    header("DIAGNOSTICS");
    row({
      kind: "row",
      id: "opt-diagnostics",
      label: "OPEN DIAGNOSTICS",
      value: "",
      enabled: true,
      onActivate: () => this.openOverlay("diagnostics"),
    });

    header("CONTROLS");
    for (const line of [
      "1-4 SWITCH SCENE    TAB POP-UP    C OPTIONS",
      "I DIAGNOSTICS    M MUTE    R RECOVER SCENE",
      "MENU: ARROWS/WASD MOVE, ENTER CONFIRM, ESC BACK, O VERSION",
      "CLICK ANY ITEM OR USE KEYS. ESC CLOSES THIS SCREEN.",
    ]) {
      rows.push({ kind: "info", text: line, tone: "muted" });
    }
    return rows;
  }

  private buildDiagnosticsItems(): ListItem[] {
    const items: ListItem[] = [];
    const s = this.lastSnapshot;
    const sortedFrames = [...this.frameDeltasMs].sort((a, b) => a - b);
    const p50 = percentile(sortedFrames, 0.5);
    const p95 = percentile(sortedFrames, 0.95);
    const fps = p50 > 0 ? Math.min(240, 1000 / p50) : 0;
    const wasmP50 = percentile([...this.wasmStepTimesMs].sort((a, b) => a - b), 0.5);
    const gpuStats = this.renderer?.getResourceStats();
    const rendererBackend =
      this.renderer?.backend === "webgpu"
        ? "WEBGPU"
        : this.renderer?.backend === "canvas2d"
          ? "CANVAS 2D (REDUCED)"
          : "UNAVAILABLE";

    items.push({ kind: "header", label: "LIVE TELEMETRY" });
    items.push({ kind: "info", key: "MODE", text: s ? `${s.mode} (FRAME #${s.frame})` : "NOT STARTED", tone: "text" });
    items.push({
      kind: "info",
      key: "FRAME TIME",
      text: `${fps.toFixed(1)} FPS (P50 ${p50.toFixed(1)} MS / P95 ${p95.toFixed(1)} MS)`,
      tone: "text",
    });
    items.push({
      kind: "info",
      key: "WASM SIM",
      text: `${wasmP50.toFixed(2)} MS/STEP (${(this.bridge.memoryByteLength / 1024).toFixed(0)} KIB)`,
      tone: "text",
    });
    if (s) {
      items.push({ kind: "info", key: "GEOMETRY", text: `${s.commands} CMDS / ${s.vertices} VERTS / ${s.towers} TOWERS`, tone: "text" });
      items.push({ kind: "info", key: "OBJECTS", text: `${s.cubes} CUBES / ${s.orbs} ORBS / ${s.rods} RODS`, tone: "text" });
      items.push({
        kind: "info",
        key: "CAMERA",
        text: `(${s.camPos[0].toFixed(1)}, ${s.camPos[1].toFixed(1)}, ${s.camPos[2].toFixed(1)})`,
        tone: "text",
      });
    }
    items.push({ kind: "info", key: "RENDERER", text: rendererBackend, tone: "text" });
    items.push({
      kind: "info",
      key: "TARGETS",
      text: gpuStats
        ? `${gpuStats.activeRenderTargetWidth}X${gpuStats.activeRenderTargetHeight} (RECOVERIES ${gpuStats.deviceLostRecoveries})`
        : "NONE",
      tone: "text",
    });
    items.push({
      kind: "info",
      key: "AUDIO / DB",
      text: `${String(this.audio.getStatus()).toUpperCase()} / ${this.storage.getBackendStatus().toUpperCase()} (R${this.storage.getKnownRevision()})`,
      tone: "text",
    });

    items.push({ kind: "header", label: "PROVENANCE" });
    const mode = s?.mode ?? this.settings.mode;
    items.push({ kind: "info", text: MODE_PROVENANCE_DESCRIPTION[mode].toUpperCase(), tone: "muted" });
    items.push({
      kind: "info",
      text: "ALL SCENE, MENU AND TEXT GEOMETRY IS CLEAN-ROOM. NO SONY BIOS OR ROM BYTES ARE BUNDLED.",
      tone: "muted",
    });

    items.push({ kind: "header", label: "NAVIGATION" });
    items.push({
      kind: "row",
      id: "diag-back",
      label: "BACK TO OPTIONS",
      value: "",
      enabled: true,
      onActivate: () => this.openOverlay("options"),
    });
    return items;
  }

  private volumeRow(
    id: string,
    label: string,
    key: "masterVolume" | "ambientVolume" | "sfxVolume",
  ): RowItem {
    const current = this.settings[key];
    const setLevel = (value: number): void => {
      const next = Math.round(clamp(value, 0, 1) * 20) / 20;
      this.commitSettings({ [key]: next } as Partial<OsdbitsSettings>, "live");
    };
    return {
      kind: "row",
      id,
      label,
      value: `${Math.round(current * 100)}%`,
      enabled: true,
      slider: { ratio: current },
      onActivate: () => setLevel(current >= 1 ? 0 : current + 0.1),
      onAdjust: (dir, big) => setLevel(this.settings[key] + dir * (big ? 0.25 : 0.05)),
      onRatio: (ratio) => setLevel(ratio),
    };
  }

  // -------------------------------------------------------------------------
  // Settings mutations
  // -------------------------------------------------------------------------

  private stepSetting<K extends "aspectMode" | "resolutionScale">(
    key: K,
    options: readonly (OsdbitsSettings[K])[],
    dir: -1 | 1,
    mode: "reinit" | "live",
  ): void {
    const next = cycle(options, this.settings[key], dir);
    this.commitSettings({ [key]: next } as Partial<OsdbitsSettings>, mode);
  }

  private cycleLanguage(dir: -1 | 1): void {
    const count = LANGUAGE_LABEL.length;
    const next = (this.settings.language + dir + count) % count;
    this.commitSettings({ language: next as OsdLanguageCode }, "reinit");
  }

  private setTowerKind(kind: TowerPresetSpec["kind"]): void {
    const seed = this.settings.rngSeed;
    const towerPreset = this.towerPresetFor(kind, seed, this.currentGames());
    this.commitSettings({ towerPreset }, "reinit");
  }

  private currentGames(): number {
    const preset = this.settings.towerPreset;
    return preset.kind === "simulated" ? preset.ngames : this.pendingGames;
  }

  private towerPresetFor(kind: TowerPresetSpec["kind"], seed: number, ngames: number): TowerPresetSpec {
    if (kind === "full") return { kind: "full" };
    if (kind === "simulated") return { kind: "simulated", seed, ngames: clamp(Math.round(ngames), 0, MAX_GAMES) };
    return { kind: "captured" };
  }

  private adjustGames(delta: number): void {
    const next = clamp(this.currentGames() + delta, 0, MAX_GAMES);
    this.pendingGames = next;
    if (this.settings.towerPreset.kind === "simulated") {
      this.commitSettings({ towerPreset: this.towerPresetFor("simulated", this.settings.rngSeed, next) }, "reinit");
    }
  }

  private adjustSeed(delta: number): void {
    const seed = clamp(Math.round(this.settings.rngSeed + delta), 0, MAX_SEED) >>> 0;
    const preset =
      this.settings.towerPreset.kind === "simulated"
        ? this.towerPresetFor("simulated", seed, this.settings.towerPreset.ngames)
        : this.settings.towerPreset;
    this.commitSettings({ rngSeed: seed, towerPreset: preset }, "reinit");
  }

  private setSpeed(value: number): void {
    const speed = Math.round(clamp(value, 0, MAX_SPEED) * 10) / 10;
    this.commitSettings({ speedMultiplier: speed }, "live");
  }

  private mixerSnapshot(): {
    muted: boolean;
    masterVolume: number;
    ambientVolume: number;
    sfxVolume: number;
    reducedMotion: boolean;
  } {
    return {
      muted: this.settings.audioMuted,
      masterVolume: this.settings.masterVolume,
      ambientVolume: this.settings.ambientVolume,
      sfxVolume: this.settings.sfxVolume,
      reducedMotion: this.settings.reducedMotion,
    };
  }

  /**
   * Applies a settings patch. `reinit` rebuilds the scene (a full core re-init); `live` only
   * updates runtime overrides. Both persist the result.
   */
  private commitSettings(patch: Partial<OsdbitsSettings>, mode: "reinit" | "live"): void {
    this.settings = sanitizeSettings({ ...this.settings, ...patch });
    this.audio.setMixer(this.mixerSnapshot());
    if (mode === "reinit") {
      this.applySettingsToSimulation(true);
      this.lastSnapshot = this.bridge.stepFrame();
      this.rootEl.dataset.activeMode = this.lastSnapshot.mode;
    } else {
      this.applySettingsToSimulation(false);
    }
    this.markSceneDirty();
    void this.persistCurrentSettings();
  }

  private applySettingsToSimulation(fullReinit: boolean): void {
    if (fullReinit) {
      this.bridge.initScene({
        mode: this.settings.mode,
        bootToGame: this.settings.bootToGame,
        autoTransitionToMenu: this.settings.autoTransitionToMenu,
        pal: this.settings.pal,
        language: this.settings.language,
        rngSeed: this.settings.rngSeed,
        towerPreset: this.settings.towerPreset,
        reducedMotion: this.settings.reducedMotion,
        enableFog: this.settings.enableFog,
        enableLights: this.settings.enableLights,
        enableCubes: this.settings.enableCubes,
        enableGlow: this.settings.enableGlow,
        enableMotionBlur: this.settings.enableMotionBlur,
        letterbox: this.settings.letterbox,
        confirmCancelSwap: this.settings.confirmCancelSwap,
      });
    }
    this.bridge.setLiveOverrides({
      speedMultiplier: this.settings.speedMultiplier,
      camOffsetX: this.settings.camOffsetX,
      camOffsetY: this.settings.camOffsetY,
      reducedMotion: this.settings.reducedMotion,
      enableFog: this.settings.enableFog,
      enableLights: this.settings.enableLights,
      enableCubes: this.settings.enableCubes,
      enableGlow: this.settings.enableGlow,
      enableMotionBlur: this.settings.enableMotionBlur,
      letterbox: this.settings.letterbox,
      language: this.settings.language,
      confirmCancelSwap: this.settings.confirmCancelSwap,
    });
    this.renderer?.setDisplayConfig({
      aspectMode: this.settings.aspectMode,
      resolutionScale: this.settings.resolutionScale,
      pal: this.settings.pal,
    });
  }

  private async persistCurrentSettings(): Promise<void> {
    const res = await this.storage.saveSettings(this.settings);
    this.persistenceStatus = res.ok
      ? `Saved & read-back verified (${res.backend}, rev #${res.revision})`
      : (res.warning ?? "Save warning");
  }

  private toggleMute(): void {
    const muted = !this.settings.audioMuted;
    this.commitSettings({ audioMuted: muted }, "live");
    this.showToast(muted ? "AUDIO MUTED" : "AUDIO ON");
  }

  private async exportSettings(): Promise<void> {
    const json = await this.storage.exportSettingsJson();
    const blob = new Blob([json], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "osdbits-settings.json";
    a.click();
    URL.revokeObjectURL(url);
    this.persistenceStatus = "Exported settings JSON.";
  }

  private async importSettingsFile(file: File): Promise<void> {
    try {
      const text = await file.text();
      const res = await this.storage.importSettingsJson(text);
      const loaded = await this.storage.loadSettings();
      this.settings = loaded.settings;
      if (this.settings.towerPreset.kind === "simulated") {
        this.pendingGames = this.settings.towerPreset.ngames;
      }
      this.applySettingsToSimulation(true);
      this.lastSnapshot = this.bridge.stepFrame();
      this.persistenceStatus = `Imported & verified JSON (rev #${res.revision})`;
      this.markSceneDirty();
    } catch (err) {
      this.persistenceStatus = `Import error: ${err instanceof Error ? err.message : String(err)}`;
    }
  }

  private async importResourceFile(file: File): Promise<void> {
    const bytes = new Uint8Array(await file.arrayBuffer());
    await this.importResourceBytes(bytes);
  }

  private performReset(): void {
    this.awaitingResetConfirm = false;
    void (async () => {
      const res = await this.storage.resetToFaithfulDefaults(RESET_CONFIRMATION_TOKEN);
      this.settings = sanitizeSettings(DEFAULT_FAITHFUL_SETTINGS);
      this.pendingGames = 36;
      this.textureAssets = synthesizeProceduralTextures();
      this.renderer?.updateTextures(this.textureAssets);
      this.audio.setMixer(this.mixerSnapshot());
      this.applySettingsToSimulation(true);
      this.lastSnapshot = this.bridge.stepFrame();
      this.resourceStatus = "Using 100% clean-room procedural assets.";
      this.persistenceStatus = `Restored faithful OSDSYS defaults (verified rev #${res.revision})`;
      this.markSceneDirty();
    })();
  }

  /** Opens a transient hidden file input; it is removed as soon as a file is chosen or replaced. */
  private openFilePicker(accept: string, onFile: (file: File) => void): void {
    this.pickerInput?.remove();
    const input = document.createElement("input");
    input.type = "file";
    input.accept = accept;
    input.hidden = true;
    input.addEventListener("change", () => {
      const file = input.files?.[0];
      input.remove();
      if (this.pickerInput === input) this.pickerInput = null;
      if (file) onFile(file);
    });
    this.rootEl.appendChild(input);
    this.pickerInput = input;
    input.click();
  }

  // -------------------------------------------------------------------------
  // Scene / overlay state
  // -------------------------------------------------------------------------

  private openOverlay(kind: OverlayKind): void {
    this.overlay = kind;
    this.overlaySelected = -1;
    this.overlayScroll = { top: 0 };
    this.popupPinned = false;
    this.popupUntilMs = 0;
  }

  private closeOverlay(): void {
    if (this.overlay === "diagnostics") {
      this.openOverlay("options");
      return;
    }
    this.overlay = null;
    this.awaitingResetConfirm = false;
    this.overlaySelected = -1;
    this.touchPopup(performance.now());
  }

  private touchPopup(nowMs: number): void {
    if (this.started && this.overlay === null) {
      this.popupUntilMs = nowMs + POPUP_IDLE_MS;
    }
  }

  private showToast(text: string, durationMs = TOAST_MS): void {
    this.toast = { text, untilMs: performance.now() + durationMs };
  }

  private showRuntimeError(msg: string): void {
    this.runtimeError = msg;
    this.rootEl.dataset.runtimeError = "true";
    console.error(`[osdbits] ${msg}`);
  }

  private hideRuntimeError(): void {
    this.runtimeError = null;
    delete this.rootEl.dataset.runtimeError;
  }

  private handleMenuAction(action: MenuActionName): void {
    // Scene Customization (Main Menu item 2) opens the in-app options screen.
    const ui = this.lastSnapshot?.menuUi;
    if (action === "confirm" && ui?.screen === "main" && ui.items[2]?.selected) {
      this.openOverlay("options");
      return;
    }

    this.bridge.dispatchMenuAction(action);
    this.lastSnapshot = this.bridge.stepFrame();
    this.audio.dispatchCues(this.lastSnapshot.audioCues);
    this.markSceneDirty();
  }

  private confirmMenuItem(idx: number): void {
    const ui = this.lastSnapshot?.menuUi;
    if (!ui || !ui.interactive) {
      // Rows are disabled while the core ignores input; purely defensive.
      return;
    }
    const curIdx = ui.items.findIndex((x) => x.selected);
    if (curIdx >= 0 && curIdx !== idx) {
      const dir: MenuActionName = idx > curIdx ? "down" : "up";
      const steps = Math.abs(idx - curIdx);
      for (let step = 0; step < steps; step++) {
        this.bridge.dispatchMenuAction(dir);
      }
    }
    this.handleMenuAction("confirm");
  }

  private moveClockCursor(fieldIdx: number): void {
    const cur = this.lastSnapshot?.menuUi;
    if (!cur || !cur.interactive) return;
    const diff = fieldIdx - cur.clockCursor;
    const dir: MenuActionName = diff > 0 ? "right" : "left";
    for (let k = 0; k < Math.abs(diff); k++) {
      this.bridge.dispatchMenuAction(dir);
    }
    this.lastSnapshot = this.bridge.stepFrame();
    this.markSceneDirty();
  }

  // -------------------------------------------------------------------------
  // Input
  // -------------------------------------------------------------------------

  private mountCanvasUi(): void {
    window.addEventListener("resize", this.onWindowResize);
    window.addEventListener("keydown", this.onGlobalKeyDown);
    this.bindCanvasEvents();

    this.canvasEl.tabIndex = 0;
    this.canvasEl.setAttribute("role", "application");
    this.canvasEl.setAttribute(
      "aria-label",
      "OSDSYS 3D scene. Press Enter to start, number keys 1 to 4 switch scenes, C opens options, Tab opens the corner pop-up.",
    );

    let sr = document.getElementById("osdbits-sr-status");
    if (!sr) {
      sr = document.createElement("div");
      sr.id = "osdbits-sr-status";
      sr.className = "sr-only";
      sr.setAttribute("role", "status");
      sr.setAttribute("aria-live", "polite");
      this.rootEl.appendChild(sr);
    }
    this.srEl = sr;
  }

  private bindCanvasEvents(): void {
    this.canvasEl.addEventListener("pointerdown", this.onPointerDown);
    this.canvasEl.addEventListener("pointermove", this.onPointerMove);
    this.canvasEl.addEventListener("wheel", this.onWheel, { passive: false });
  }

  /** Wheel scrolls the open options/diagnostics list; the view clamps the offset when drawn. */
  private readonly onWheel = (ev: WheelEvent): void => {
    if (!this.started || this.overlay === null) return;
    const raster = this.raster;
    const rect = this.canvasEl.getBoundingClientRect();
    if (!raster || rect.height <= 0) return;
    ev.preventDefault();
    const pixels = ev.deltaMode === 1 ? ev.deltaY * 16 : ev.deltaY;
    const logicalDelta = (pixels * raster.height) / rect.height;
    this.overlayScroll.top = Math.max(0, this.overlayScroll.top + logicalDelta);
    this.markSceneDirty();
  };

  private toLogical(ev: PointerEvent): { x: number; y: number } | null {
    const raster = this.raster;
    const rect = this.canvasEl.getBoundingClientRect();
    if (!raster || rect.width <= 0 || rect.height <= 0) return null;
    return {
      x: ((ev.clientX - rect.left) / rect.width) * raster.width,
      y: ((ev.clientY - rect.top) / rect.height) * raster.height,
    };
  }

  /** Topmost region under the point (later-drawn regions sit on top). */
  private hitTest(x: number, y: number): UiRegion | null {
    for (let i = this.regions.length - 1; i >= 0; i--) {
      const r = this.regions[i];
      if (r && x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h) return r;
    }
    return null;
  }

  private readonly onPointerDown = (ev: PointerEvent): void => {
    if (ev.button !== 0) return;
    const p = this.toLogical(ev);
    if (!p) return;
    const now = performance.now();
    this.touchPopup(now);
    const region = this.hitTest(p.x, p.y);
    if (!region || !region.enabled) return;
    region.onPress(p.x, p.y);
    this.markSceneDirty();
  };

  private readonly onPointerMove = (ev: PointerEvent): void => {
    const now = performance.now();
    this.lastPointerMoveMs = now;
    this.touchPopup(now);
    const p = this.toLogical(ev);
    this.hoverRegion = p ? this.hitTest(p.x, p.y) : null;
  };

  private updateCursor(nowMs: number): void {
    const idle = this.started && this.overlay === null && nowMs - this.lastPointerMoveMs > CURSOR_IDLE_MS;
    const cursor = idle ? "none" : this.hoverRegion?.enabled ? "pointer" : "default";
    if (this.canvasEl.style.cursor !== cursor) {
      this.canvasEl.style.cursor = cursor;
    }
  }

  private readonly onWindowResize = (): void => {
    this.renderer?.syncCanvasSize();
    this.markSceneDirty();
  };

  private readonly onGlobalKeyDown = (ev: KeyboardEvent): void => {
    const target = ev.target as HTMLElement | null;
    if (target && (target.tagName === "INPUT" || target.tagName === "SELECT" || target.tagName === "TEXTAREA")) {
      return;
    }
    const now = performance.now();

    if (!this.started) {
      this.handleTitleKey(ev);
      return;
    }

    if (this.overlay !== null) {
      this.handleOverlayKey(ev);
      return;
    }

    this.touchPopup(now);

    if (ev.key === "Tab") {
      ev.preventDefault();
      this.popupPinned = !this.popupPinned;
      if (!this.popupPinned) this.popupUntilMs = 0;
      return;
    }
    if (ev.key === "m" || ev.key === "M") {
      ev.preventDefault();
      this.toggleMute();
      return;
    }
    if (ev.key === "i" || ev.key === "I") {
      ev.preventDefault();
      this.openOverlay("diagnostics");
      return;
    }
    if (ev.key === "c" || ev.key === "C") {
      ev.preventDefault();
      this.openOverlay("options");
      return;
    }
    if (ev.key === "r" || ev.key === "R") {
      if (this.runtimeError) {
        ev.preventDefault();
        void this.recoverSceneWithoutReload();
      }
      return;
    }
    if (/^[1-4]$/.test(ev.key)) {
      ev.preventDefault();
      void this.switchMode(MODE_ORDER[Number(ev.key) - 1] as VisualModeName);
      return;
    }

    if (this.lastSnapshot?.mode === "menu") {
      const action = MENU_KEY_ACTIONS[ev.key];
      if (action) {
        ev.preventDefault();
        this.handleMenuAction(action);
      }
    }
  };

  private handleTitleKey(ev: KeyboardEvent): void {
    const items = this.buildTitleItems();
    this.titleSelected = ensureSelection(items, this.titleSelected);
    switch (ev.key) {
      case "ArrowUp":
      case "w":
      case "W":
        ev.preventDefault();
        this.titleSelected = nextSelectable(items, this.titleSelected, -1);
        break;
      case "ArrowDown":
      case "s":
      case "S":
        ev.preventDefault();
        this.titleSelected = nextSelectable(items, this.titleSelected, 1);
        break;
      case "ArrowLeft":
      case "a":
      case "A":
        ev.preventDefault();
        this.adjustTitleRow(items, -1);
        break;
      case "ArrowRight":
      case "d":
      case "D":
        ev.preventDefault();
        this.adjustTitleRow(items, 1);
        break;
      case "Enter":
      case " ":
      case "z":
      case "Z": {
        ev.preventDefault();
        const item = items[this.titleSelected];
        if (item && item.kind === "row" && item.enabled) item.onActivate();
        break;
      }
      default:
        break;
    }
  }

  private adjustTitleRow(items: readonly ListItem[], dir: -1 | 1): void {
    const item = items[this.titleSelected];
    if (item && item.kind === "row" && item.enabled && item.onAdjust) {
      item.onAdjust(dir, false);
    }
  }

  private handleOverlayKey(ev: KeyboardEvent): void {
    const kind = this.overlay;
    if (kind === null) return;
    const model = this.buildOverlayModel(kind);
    const items = model.list.items;
    const big = ev.shiftKey;
    switch (ev.key) {
      case "ArrowUp":
      case "w":
      case "W":
        ev.preventDefault();
        this.overlaySelected = nextSelectable(items, this.overlaySelected, -1);
        break;
      case "ArrowDown":
      case "s":
      case "S":
        ev.preventDefault();
        this.overlaySelected = nextSelectable(items, this.overlaySelected, 1);
        break;
      case "ArrowLeft":
      case "a":
      case "A":
      case "ArrowRight":
      case "d":
      case "D": {
        ev.preventDefault();
        const item = items[this.overlaySelected];
        const dir: -1 | 1 = ev.key === "ArrowLeft" || ev.key === "a" || ev.key === "A" ? -1 : 1;
        if (item && item.kind === "row" && item.enabled && item.onAdjust) {
          item.onAdjust(dir, big);
        }
        break;
      }
      case "Enter":
      case " ":
      case "z":
      case "Z": {
        ev.preventDefault();
        const item = items[this.overlaySelected];
        if (item && item.kind === "row" && item.enabled) item.onActivate();
        break;
      }
      case "Escape":
      case "Backspace":
      case "x":
      case "X":
        ev.preventDefault();
        if (this.awaitingResetConfirm) {
          this.awaitingResetConfirm = false;
        } else {
          model.onDismiss();
        }
        break;
      case "c":
      case "C":
        ev.preventDefault();
        this.closeOverlay();
        break;
      case "i":
      case "I":
        ev.preventDefault();
        this.openOverlay(kind === "diagnostics" ? "options" : "diagnostics");
        break;
      default:
        break;
    }
  }

  // -------------------------------------------------------------------------
  // DOM surface: metadata-only attributes, a visually hidden status line and backend flags
  // -------------------------------------------------------------------------

  private refreshBackendDataset(): void {
    const backend = this.renderer?.backend ?? null;
    this.rootEl.dataset.rendererBackend = backend ?? "unavailable";
    if (backend === "webgpu") {
      delete this.rootEl.dataset.webgpuFallback;
    } else {
      this.rootEl.dataset.webgpuFallback = "true";
    }
    this.rootEl.dataset.abiVersion = String(this.bridge.abiVersion);
  }

  private updateDatasetFromFrame(input: ComposeInput): void {
    const root = this.rootEl;
    const scene = input.scene;
    const screen = !input.started
      ? "title"
      : input.overlay
        ? this.overlay ?? "options"
        : runtimeScreen(scene);
    root.dataset.uiScreen = screen;

    const menu = input.scene?.menu;
    if (menu) {
      root.dataset.menuInteractive = String(menu.interactive);
    } else {
      delete root.dataset.menuInteractive;
    }

    const illegal = input.scene?.illegal;
    if (illegal) {
      root.dataset.illegalText = this.lastSnapshot?.illegalMsg ?? "";
    } else {
      delete root.dataset.illegalText;
    }

    if (input.scene?.popup) {
      root.dataset.popupVisible = "true";
    } else {
      delete root.dataset.popupVisible;
    }
  }

  private updateScreenReaderText(): void {
    if (!this.srEl) return;
    const parts: string[] = [];
    if (!this.started) {
      const item = this.buildTitleItems()[this.titleSelected];
      parts.push("OSDBITS title screen.");
      if (item && item.kind === "row") parts.push(`Selected ${item.label}${item.value ? ` ${item.value}` : ""}.`);
    } else {
      parts.push(`Scene ${MODE_SHORT[this.lastSnapshot?.mode ?? this.settings.mode]}.`);
      if (this.overlay) parts.push(`${this.overlay === "options" ? "Options" : "Diagnostics"} open.`);
      const menu = this.lastSnapshot?.menuUi;
      if (menu && this.lastSnapshot?.mode === "menu") {
        const selected = menu.items.find((x) => x.selected);
        if (selected) parts.push(`${menu.title}, ${selected.label} ${selected.value}`.trim() + ".");
      }
      if (this.runtimeError) parts.push(`Error: ${this.runtimeError}`);
    }
    const text = parts.join(" ");
    if (text !== this.srSignature) {
      this.srSignature = text;
      this.srEl.textContent = text;
    }
  }
}

function runtimeScreen(scene: SceneModel | undefined): string {
  if (!scene) return "scene";
  if (scene.error) return "error";
  if (scene.menu) return "menu";
  if (scene.illegal) return "illegal";
  return "scene";
}
