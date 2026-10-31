/**
 * Accessible PS2-Aesthetic Browser Shell (`P09-001`, plus Mode C & D DOM HUDs `P06-001` & `P06-002`).
 *
 * Orchestrates:
 * - Single-click `Start OSDSYS` activation with synchronous `AudioContext.resume()`
 * - Fixed-timestep deterministic WASM simulation + multi-pass WebGPU rendering
 * - Mode D interactive keyboard & pointer menu/clock navigation with provenance badges
 * - Mode C localized warning text & explicit `osdBootParam` provenance disclosure
 * - Mode E customization drawer, local `BIOS`/`TEXIMAGE` import, JSON export/import, two-step reset
 * - Live percentile (`p50`/`p95`) telemetry & provenance inspector + non-destructive scene recovery
 */

import {
  synthesizeProceduralTextures,
  type ProceduralTextureAsset,
} from "../assets/proceduralTextures";
import { importUserResourceArchive } from "../assets/userResourceImporter";
import { AudioEngine } from "../audio/audioEngine";
import { Canvas2DRenderer } from "../renderer/canvas2dRenderer";
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
  type VisualModeName,
} from "../wasm/bridge";

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

/** Stable DOM row for one Mode D menu entry (reused instead of re-created). */
interface MenuItemNode {
  button: HTMLButtonElement;
  label: HTMLSpanElement;
  value: HTMLSpanElement;
  provenance: HTMLSpanElement;
}

function percentile(sorted: readonly number[], p: number): number {
  if (sorted.length === 0) return 0;
  const idx = Math.min(sorted.length - 1, Math.max(0, Math.floor((sorted.length - 1) * p)));
  return sorted[idx] ?? 0;
}

export class OsdbitsApp {
  readonly bridge: WasmSimulationBridge;
  readonly storage: StorageManager;
  readonly audio: AudioEngine;
  renderer: WebGpuRenderer | Canvas2DRenderer | null = null;

  private settings: OsdbitsSettings = sanitizeSettings(DEFAULT_FAITHFUL_SETTINGS);
  private running = false;
  private rafId = 0;
  private lastTickTimeMs = 0;
  private simAccumMs = 0;
  private lastSnapshot: FrameInspectionSnapshot | null = null;

  private frameDeltasMs: number[] = [];
  private wasmStepTimesMs: number[] = [];
  private awaitingResetConfirm = false;
  private activeMenuSignature = "";
  private menuItemNodes: MenuItemNode[] = [];
  private webGpuErrorReason: string | null = null;
  private textureAssets: ProceduralTextureAsset[] | null = null;

  private readonly rootEl: HTMLElement;
  private canvasEl: HTMLCanvasElement;

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

    this.audio.setMixer({
      muted: this.settings.audioMuted,
      masterVolume: this.settings.masterVolume,
      ambientVolume: this.settings.ambientVolume,
      sfxVolume: this.settings.sfxVolume,
      reducedMotion: this.settings.reducedMotion,
    });

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
          this.showRuntimeError(`WebGPU device lost (${info.reason}). Click Recover Scene to restore.`);
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
      // Render an atmospheric preview frame behind the glass launch card!
      this.renderer.renderFrame(
        this.bridge.getVertexView(),
        this.bridge.getCommandView(),
        this.lastSnapshot.mode,
      );
    } catch (err) {
      this.renderer = null;
      this.webGpuErrorReason = err instanceof Error ? err.message : String(err);
    }

    this.mountDomUi();
    this.syncControlsFromSettings();
    this.updateCapabilityBadges();
    this.updateTelemetryDom();
  }

  /**
   * Synchronously unlocks Web Audio inside the user's single activation gesture and starts
   * the live OSDSYS simulation + WebGPU loop with ZERO second click required.
   */
  startFromUserGesture(): void {
    // CRITICAL: Must run synchronously on line 1 before any async work!
    void this.audio.activateFromUserGesture();

    if (!this.renderer) {
      return;
    }

    const launchModeSelect = document.getElementById("launch-mode-select") as HTMLSelectElement | null;
    if (launchModeSelect) {
      const chosen = launchModeSelect.value as VisualModeName;
      if (chosen !== this.settings.mode) {
        this.settings = { ...this.settings, mode: chosen };
        void this.persistCurrentSettings();
      }
    }

    this.applySettingsToSimulation(true);
    this.lastSnapshot = this.bridge.stepFrame();
    this.audio.dispatchCues(this.lastSnapshot.audioCues);

    const launchOverlay = document.getElementById("launch-overlay");
    launchOverlay?.classList.add("hidden");
    document.getElementById("hud-toolbar")?.classList.remove("hidden");

    this.rootEl.dataset.sceneActive = "true";
    this.rootEl.dataset.activeMode = this.lastSnapshot.mode;

    if (!this.running) {
      this.running = true;
      this.lastTickTimeMs = performance.now();
      this.simAccumMs = 0;
      this.rafId = requestAnimationFrame(this.onAnimationFrame);
    }

    this.renderModeOverlays();
    this.updateTelemetryDom();
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
      if (this.lastSnapshot) {
        this.renderer.renderFrame(
          this.bridge.getVertexView(),
          this.bridge.getCommandView(),
          this.lastSnapshot.mode,
        );
      }
      this.updateCapabilityBadges();
      this.startFromUserGesture();
    } catch (err) {
      const reasonEl = document.getElementById("webgpu-fallback-reason");
      if (reasonEl) {
        reasonEl.textContent = `Canvas 2D fallback could not start: ${err instanceof Error ? err.message : String(err)}`;
      }
    }
  }

  async switchMode(mode: VisualModeName): Promise<void> {
    this.settings = { ...this.settings, mode };
    this.applySettingsToSimulation(true);
    this.lastSnapshot = this.bridge.stepFrame();
    this.audio.dispatchCues(this.lastSnapshot.audioCues);
    this.rootEl.dataset.activeMode = this.lastSnapshot.mode;
    this.syncControlsFromSettings();
    this.renderModeOverlays();
    await this.persistCurrentSettings();
  }

  /** Triggers a recoverable scene error without crashing the page (used by fault-recovery tests & button). */
  triggerSimulatedFault(message = "Simulated runtime scene fault"): void {
    this.running = false;
    if (this.rafId) {
      cancelAnimationFrame(this.rafId);
      this.rafId = 0;
    }
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
      this.renderer.renderFrame(
        this.bridge.getVertexView(),
        this.bridge.getCommandView(),
        this.lastSnapshot.mode,
      );
      this.updateCapabilityBadges();
      this.hideRuntimeError();

      if (this.rootEl.dataset.sceneActive === "true" && !this.running) {
        this.running = true;
        this.lastTickTimeMs = performance.now();
        this.rafId = requestAnimationFrame(this.onAnimationFrame);
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
    const statusEl = document.getElementById("resource-import-status");
    try {
      const report = await importUserResourceArchive(bytes, this.bridge);
      if (!report.ok) {
        if (statusEl) {
          statusEl.textContent = `Import rejected: ${report.errorMessage ?? "Invalid resource container"}`;
        }
        return false;
      }
      this.textureAssets = report.textures;
      this.renderer?.updateTextures(report.textures);
      await this.storage.saveImportedAsset(bytes, report.importedSlots);
      if (statusEl) {
        statusEl.textContent = `Imported ${report.importedSlots.length} verified slots (SHA-256 ${report.archiveSha256.slice(0, 12)}…)`;
      }
      return true;
    } catch (err) {
      if (statusEl) {
        statusEl.textContent = `Import rejected: ${err instanceof Error ? err.message : String(err)}`;
      }
      return false;
    }
  }

  getSettings(): Readonly<OsdbitsSettings> {
    return this.settings;
  }

  getLatestSnapshot(): FrameInspectionSnapshot | null {
    return this.lastSnapshot;
  }

  dispose(): void {
    this.running = false;
    if (this.rafId) {
      cancelAnimationFrame(this.rafId);
    }
    window.removeEventListener("resize", this.onWindowResize);
    window.removeEventListener("keydown", this.onGlobalKeyDown);
    this.renderer?.dispose();
    this.audio.dispose();
    this.storage.close();
  }

  private readonly onAnimationFrame = (nowMs: number): void => {
    if (!this.running) return;
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
        this.syncControlsFromSettings();
      }
      this.simAccumMs -= targetFrameMs;
      steps++;
    }

    if (this.renderer && this.lastSnapshot) {
      this.renderer.syncCanvasSize();
      this.renderer.renderFrame(
        this.bridge.getVertexView(),
        this.bridge.getCommandView(),
        this.lastSnapshot.mode,
      );
      this.renderModeOverlays();
      if ((this.lastSnapshot.frame & 7) === 0) {
        this.updateTelemetryDom();
      }
    }

    this.rafId = requestAnimationFrame(this.onAnimationFrame);
  };

  private readonly onWindowResize = (): void => {
    this.renderer?.syncCanvasSize();
  };

  private readonly onGlobalKeyDown = (ev: KeyboardEvent): void => {
    const target = ev.target as HTMLElement | null;
    if (
      target &&
      (target.tagName === "INPUT" || target.tagName === "SELECT" || target.tagName === "TEXTAREA")
    ) {
      return;
    }

    if (this.rootEl.dataset.sceneActive !== "true") {
      return;
    }

    // Global HUD shortcuts
    if (ev.key === "m" || ev.key === "M") {
      ev.preventDefault();
      this.toggleMute();
      return;
    }
    if (ev.key === "i" || ev.key === "I") {
      ev.preventDefault();
      this.toggleInspector();
      return;
    }
    if (ev.key === "c" || ev.key === "C") {
      ev.preventDefault();
      this.toggleCustomizeDrawer();
      return;
    }

    // Mode D navigation keys
    if (this.lastSnapshot?.mode === "menu") {
      let action: MenuActionName | null = null;
      switch (ev.key) {
        case "ArrowUp":
        case "w":
        case "W":
          action = "up";
          break;
        case "ArrowDown":
        case "s":
        case "S":
          action = "down";
          break;
        case "ArrowLeft":
        case "a":
        case "A":
          action = "left";
          break;
        case "ArrowRight":
        case "d":
        case "D":
          action = "right";
          break;
        case "Enter":
        case " ":
        case "z":
        case "Z":
          action = "confirm";
          break;
        case "Escape":
        case "Backspace":
        case "x":
        case "X":
          action = "cancel";
          break;
        case "o":
        case "O":
          action = "options";
          break;
      }
      if (action) {
        ev.preventDefault();
        this.handleMenuAction(action);
      }
    }
  };

  private handleMenuAction(action: MenuActionName): void {
    // Check if user confirmed "Scene Customization" (clean-room extension item 2 on Main Menu)
    const ui = this.lastSnapshot?.menuUi;
    if (
      action === "confirm" &&
      ui?.screen === "main" &&
      ui.items[2]?.selected
    ) {
      this.openCustomizeDrawer();
      return;
    }

    this.bridge.dispatchMenuAction(action);
    this.lastSnapshot = this.bridge.stepFrame();
    this.audio.dispatchCues(this.lastSnapshot.audioCues);
    this.renderModeOverlays();
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
    const statusEl = document.getElementById("persistence-status");
    if (statusEl) {
      statusEl.textContent = res.ok
        ? `Saved & read-back verified (${res.backend}, rev #${res.revision})`
        : (res.warning ?? "Save warning");
    }
  }

  private toggleMute(): void {
    this.settings = { ...this.settings, audioMuted: !this.settings.audioMuted };
    this.audio.setMixer({ muted: this.settings.audioMuted });
    this.syncControlsFromSettings();
    void this.persistCurrentSettings();
  }

  private toggleInspector(): void {
    const el = document.getElementById("telemetry-panel");
    const btn = document.getElementById("toggle-inspector-btn");
    if (!el) return;
    const hidden = el.classList.toggle("hidden");
    btn?.setAttribute("aria-pressed", String(!hidden));
    this.updateTelemetryDom();
  }

  private toggleCustomizeDrawer(): void {
    const el = document.getElementById("customize-drawer");
    const btn = document.getElementById("toggle-customize-btn");
    if (!el) return;
    const hidden = el.classList.toggle("hidden");
    btn?.setAttribute("aria-pressed", String(!hidden));
  }

  private openCustomizeDrawer(): void {
    const el = document.getElementById("customize-drawer");
    const btn = document.getElementById("toggle-customize-btn");
    el?.classList.remove("hidden");
    btn?.setAttribute("aria-pressed", "true");
  }

  private showRuntimeError(msg: string): void {
    this.rootEl.dataset.runtimeError = "true";
    const banner = document.getElementById("error-banner");
    const text = document.getElementById("error-banner-text");
    if (text) text.textContent = msg;
    banner?.classList.remove("hidden");
  }

  private hideRuntimeError(): void {
    delete this.rootEl.dataset.runtimeError;
    document.getElementById("error-banner")?.classList.add("hidden");
  }

  private mountDomUi(): void {
    window.addEventListener("resize", this.onWindowResize);
    window.addEventListener("keydown", this.onGlobalKeyDown);

    let uiContainer = document.getElementById("osdbits-ui-layer");
    if (!uiContainer) {
      uiContainer = document.createElement("div");
      uiContainer.id = "osdbits-ui-layer";
      this.rootEl.appendChild(uiContainer);
    }

    uiContainer.innerHTML = `
      <!-- Pre-Activation Launch Overlay -->
      <section id="launch-overlay" class="launch-overlay" role="dialog" aria-labelledby="launch-title">
        <div class="launch-card">
          <div class="brand-kicker">PlayStation 2 OSDSYS Visual Reconstruction</div>
          <h1 id="launch-title" class="brand-title">OSDBITS // WASM</h1>
          <p class="brand-subtitle">
            Deterministic Rust/WebAssembly simulation &amp; multi-pass PS2-GS WebGPU renderer.
            100% clean-room out-of-the-box with zero proprietary BIOS assets.
          </p>
          <div class="badge-row" id="capability-badges"></div>

          <div class="launch-controls">
            <label class="field-row" for="launch-mode-select">
              <span>Initial Scene Mode</span>
              <select id="launch-mode-select" aria-label="Initial Scene Mode">
                <option value="opening">Mode A — Opening &amp; Towers</option>
                <option value="idle">Mode B — Tower Field Idle</option>
                <option value="illegal-disc">Mode C — Illegal Disc (Red)</option>
                <option value="menu">Mode D — Main Menu &amp; Config</option>
              </select>
            </label>
            <button id="start-osdsys-btn" type="button" class="primary-cta">
              Start OSDSYS
            </button>
          </div>

          <div id="webgpu-fallback-panel" class="fallback-panel hidden" role="alert">
            <div id="webgpu-fallback-reason"></div>
            <button id="proceed-canvas-fallback-btn" type="button" class="primary-cta fallback-cta">
              Proceed anyway (Canvas 2D)
            </button>
            <p class="fallback-note">
              Reduced fidelity: geometry and animation run, but GPU refraction, glow, motion blur, and texture detail are simplified.
            </p>
          </div>

          <p class="legal-footnote">
            Clean-room reconstruction based on <code>aap/osdbits</code>. No Sony BIOS, ROM, or copyrighted audio/textures included.
          </p>
        </div>
      </section>

      <!-- Top Floating Quick Toolbar -->
      <nav id="hud-toolbar" class="hud-toolbar hidden" aria-label="Scene Controls">
        <div class="hud-group" role="group" aria-label="Visual Mode Switcher">
          <button type="button" class="hud-btn mode-switch-btn" data-mode="opening">Mode A: Opening</button>
          <button type="button" class="hud-btn mode-switch-btn" data-mode="idle">Mode B: Idle</button>
          <button type="button" class="hud-btn mode-switch-btn" data-mode="illegal-disc">Mode C: Red Disc</button>
          <button type="button" class="hud-btn mode-switch-btn" data-mode="menu">Mode D: Menu</button>
        </div>
        <div class="hud-group">
          <button type="button" id="toggle-mute-btn" class="hud-btn" aria-pressed="false">Audio: On</button>
          <button type="button" id="toggle-customize-btn" class="hud-btn" aria-pressed="false">Customize (Mode E)</button>
          <button type="button" id="toggle-inspector-btn" class="hud-btn" aria-pressed="false">Inspector</button>
        </div>
      </nav>

      <!-- Runtime Error Recovery Banner -->
      <div id="error-banner" class="error-banner hidden" role="alert">
        <span id="error-banner-text">Scene error detected.</span>
        <button type="button" id="recover-scene-btn" class="secondary-btn">Recover Scene</button>
      </div>

      <!-- Mode D Interactive OSDSYS Menu & System Configuration Overlay -->
      <section id="menu-hud" class="menu-hud hidden" aria-label="OSDSYS Navigation Menu">
        <div class="menu-top-bar">
          <div id="menu-screen-title" class="menu-screen-title"></div>
          <div id="menu-clock-readout" class="menu-clock-readout"></div>
        </div>
        <div id="menu-items-panel" class="menu-items-panel"></div>
        <div id="clock-editor-bar" class="clock-editor-bar hidden"></div>
        <div class="menu-bottom-hints">
          <button type="button" id="menu-hint-confirm" class="hint-btn">Enter / Click: Confirm</button>
          <button type="button" id="menu-hint-cancel" class="hint-btn">Esc: Back</button>
          <button type="button" id="menu-hint-options" class="hint-btn">O: Version / Provenance</button>
        </div>
      </section>

      <!-- Mode C Localized Warning & Provenance Notice -->
      <section id="illegal-hud" class="illegal-hud hidden" aria-live="polite">
        <div id="illegal-msg-text" class="illegal-msg-text"></div>
        <div class="illegal-prov-note">
          Provenance: Sourced <code>DrawIllegalScene</code> (<code>openingType = 1</code>); retail hardware <code>osdBootParam</code> value unverified (<code>opening.c:4262</code>).
        </div>
      </section>

      <!-- Mode E Customization Drawer -->
      <aside id="customize-drawer" class="customize-drawer hidden" role="dialog" aria-label="Mode E Scene Customization">
        <div class="drawer-header">
          <h2>Mode E — Customization</h2>
          <button type="button" id="close-customize-btn" class="secondary-btn">Close</button>
        </div>

        <div class="drawer-section">
          <h3>Display &amp; Timing</h3>
          <label class="field-row">
            <span>Video Standard</span>
            <select id="cfg-video-standard">
              <option value="ntsc">NTSC (60 Hz)</option>
              <option value="pal">PAL (50 Hz)</option>
            </select>
          </label>
          <label class="field-row">
            <span>Aspect Framing</span>
            <select id="cfg-aspect-mode">
              <option value="4:3">4:3 Authentic</option>
              <option value="16:9">16:9 Widescreen</option>
              <option value="fit">Fill Viewport</option>
            </select>
          </label>
          <label class="field-row">
            <span>Resolution Scale</span>
            <select id="cfg-res-scale">
              <option value="0.5">0.5x (Performance)</option>
              <option value="1">1.0x (Native Progressive)</option>
              <option value="1.5">1.5x (Crisp HD)</option>
              <option value="2">2.0x (Supersampled)</option>
            </select>
          </label>
          <label class="field-row">
            <span>OSD Language</span>
            <select id="cfg-language">
              <option value="0">Japanese (Clean-Room)</option>
              <option value="1">English</option>
              <option value="2">French</option>
              <option value="3">Spanish</option>
              <option value="4">German</option>
              <option value="5">Italian</option>
              <option value="6">Dutch</option>
              <option value="7">Portuguese</option>
            </select>
          </label>
        </div>

        <div class="drawer-section">
          <h3>Towers, Camera &amp; Deterministic Seed</h3>
          <label class="field-row">
            <span>Tower Field Preset</span>
            <select id="cfg-tower-preset">
              <option value="captured">Captured Retail Save Card</option>
              <option value="full">Full 126-Tower Monolith Grid</option>
              <option value="simulated">Simulated Memory Card</option>
            </select>
          </label>
          <label class="field-row">
            <span>Simulated Games (0..126)</span>
            <input id="cfg-tower-ngames" type="number" min="0" max="126" value="42" />
          </label>
          <label class="field-row">
            <span>Deterministic Seed</span>
            <input id="cfg-rng-seed" type="number" min="0" max="4294967295" value="1" />
          </label>
          <label class="field-row">
            <span>Animation Speed (<span id="cfg-speed-val">1.0x</span>)</span>
            <input id="cfg-speed" type="range" min="0" max="3" step="0.1" value="1" />
          </label>
        </div>

        <div class="drawer-section">
          <h3>Visual Passes &amp; Accessibility</h3>
          <div class="toggle-grid">
            <label><input type="checkbox" id="cfg-fog" checked /> Fog Clouds</label>
            <label><input type="checkbox" id="cfg-lights" checked /> Light Trails</label>
            <label><input type="checkbox" id="cfg-cubes" checked /> Glass Cubes</label>
            <label><input type="checkbox" id="cfg-glow" checked /> ExtraBuf Glow</label>
            <label><input type="checkbox" id="cfg-blur" checked /> Motion/Zoom Blur</label>
            <label><input type="checkbox" id="cfg-reduced-motion" /> Reduced Motion</label>
            <label><input type="checkbox" id="cfg-auto-menu" checked /> Opening -&gt; Menu</label>
            <label><input type="checkbox" id="cfg-letterbox" /> 16:9 Letterbox</label>
          </div>
        </div>

        <div class="drawer-section">
          <h3>Audio Mixer</h3>
          <label class="field-row">
            <span>Master Volume</span>
            <input id="cfg-master-vol" type="range" min="0" max="1" step="0.05" value="0.75" />
          </label>
          <label class="field-row">
            <span>Ambient Volume</span>
            <input id="cfg-ambient-vol" type="range" min="0" max="1" step="0.05" value="0.7" />
          </label>
          <label class="field-row">
            <span>UI Foley Volume</span>
            <input id="cfg-sfx-vol" type="range" min="0" max="1" step="0.05" value="0.8" />
          </label>
        </div>

        <div class="drawer-section">
          <h3>Optional Authorized Resource Import</h3>
          <input type="file" id="resource-file-input" aria-label="Import local PS2 BIOS or TEXIMAGE file" />
          <div id="resource-import-status" class="status-line">Using 100% clean-room procedural assets.</div>
        </div>

        <div class="drawer-section">
          <h3>Persistence &amp; Reset</h3>
          <div id="persistence-status" class="status-line">Ready.</div>
          <div class="action-row">
            <button type="button" id="export-json-btn" class="secondary-btn">Export JSON</button>
            <label class="secondary-btn" for="import-json-input" style="cursor:pointer;">
              Import JSON
              <input type="file" id="import-json-input" accept="application/json,.json" class="hidden" />
            </label>
            <button type="button" id="reset-defaults-btn" class="secondary-btn danger-btn">Reset to Defaults</button>
            <button type="button" id="confirm-reset-btn" class="secondary-btn danger-btn hidden">Confirm Reset</button>
          </div>
        </div>
      </aside>

      <!-- Collapsible Live Telemetry & Provenance Inspector -->
      <section id="telemetry-panel" class="telemetry-panel hidden" aria-label="Telemetry and Provenance Inspector">
        <div class="telemetry-grid" id="telemetry-grid"></div>
      </section>
    `;

    this.bindUiEvents();
  }

  private bindUiEvents(): void {
    // CRITICAL: Single-click activation calls startFromUserGesture() synchronously!
    document.getElementById("start-osdsys-btn")?.addEventListener("click", () => {
      this.startFromUserGesture();
    });
    document.getElementById("proceed-canvas-fallback-btn")?.addEventListener("click", () => {
      this.proceedWithCanvasFallback();
    });

    for (const btn of Array.from(document.querySelectorAll<HTMLButtonElement>(".mode-switch-btn"))) {
      btn.addEventListener("click", () => {
        const m = btn.dataset.mode as VisualModeName | undefined;
        if (m) void this.switchMode(m);
      });
    }

    document.getElementById("toggle-mute-btn")?.addEventListener("click", () => this.toggleMute());
    document
      .getElementById("toggle-customize-btn")
      ?.addEventListener("click", () => this.toggleCustomizeDrawer());
    document
      .getElementById("close-customize-btn")
      ?.addEventListener("click", () => this.toggleCustomizeDrawer());
    document
      .getElementById("toggle-inspector-btn")
      ?.addEventListener("click", () => this.toggleInspector());

    document.getElementById("recover-scene-btn")?.addEventListener("click", () => {
      void this.recoverSceneWithoutReload();
    });

    document.getElementById("menu-hint-confirm")?.addEventListener("click", () => {
      this.handleMenuAction("confirm");
    });
    document.getElementById("menu-hint-cancel")?.addEventListener("click", () => {
      this.handleMenuAction("cancel");
    });
    document.getElementById("menu-hint-options")?.addEventListener("click", () => {
      this.handleMenuAction("options");
    });

    // Mode E Controls
    const onReinitSettingChange = (): void => {
      const std = (document.getElementById("cfg-video-standard") as HTMLSelectElement).value;
      const aspect = (document.getElementById("cfg-aspect-mode") as HTMLSelectElement)
        .value as AspectPresentationMode;
      const resScale = Number(
        (document.getElementById("cfg-res-scale") as HTMLSelectElement).value,
      );
      const lang = Number(
        (document.getElementById("cfg-language") as HTMLSelectElement).value,
      ) as OsdLanguageCode;
      const presetKind = (document.getElementById("cfg-tower-preset") as HTMLSelectElement).value;
      const ngames = Number((document.getElementById("cfg-tower-ngames") as HTMLInputElement).value);
      const seed = Number((document.getElementById("cfg-rng-seed") as HTMLInputElement).value) >>> 0;

      const towerPreset =
        presetKind === "full"
          ? ({ kind: "full" } as const)
          : presetKind === "simulated"
            ? ({ kind: "simulated", seed, ngames: Math.max(0, Math.min(126, ngames)) } as const)
            : ({ kind: "captured" } as const);

      this.settings = sanitizeSettings({
        ...this.settings,
        pal: std === "pal",
        aspectMode: aspect,
        resolutionScale: resScale,
        language: lang,
        rngSeed: seed,
        towerPreset,
        autoTransitionToMenu: (document.getElementById("cfg-auto-menu") as HTMLInputElement).checked,
      });

      this.applySettingsToSimulation(true);
      this.lastSnapshot = this.bridge.stepFrame();
      this.renderModeOverlays();
      void this.persistCurrentSettings();
    };

    for (const id of [
      "cfg-video-standard",
      "cfg-aspect-mode",
      "cfg-res-scale",
      "cfg-language",
      "cfg-tower-preset",
      "cfg-tower-ngames",
      "cfg-rng-seed",
      "cfg-auto-menu",
    ]) {
      document.getElementById(id)?.addEventListener("change", onReinitSettingChange);
    }

    const onLiveOverrideChange = (): void => {
      const speed = Number((document.getElementById("cfg-speed") as HTMLInputElement).value);
      const speedLabel = document.getElementById("cfg-speed-val");
      if (speedLabel) speedLabel.textContent = `${speed.toFixed(1)}x`;

      this.settings = sanitizeSettings({
        ...this.settings,
        speedMultiplier: speed,
        enableFog: (document.getElementById("cfg-fog") as HTMLInputElement).checked,
        enableLights: (document.getElementById("cfg-lights") as HTMLInputElement).checked,
        enableCubes: (document.getElementById("cfg-cubes") as HTMLInputElement).checked,
        enableGlow: (document.getElementById("cfg-glow") as HTMLInputElement).checked,
        enableMotionBlur: (document.getElementById("cfg-blur") as HTMLInputElement).checked,
        reducedMotion: (document.getElementById("cfg-reduced-motion") as HTMLInputElement).checked,
        letterbox: (document.getElementById("cfg-letterbox") as HTMLInputElement).checked,
        masterVolume: Number((document.getElementById("cfg-master-vol") as HTMLInputElement).value),
        ambientVolume: Number((document.getElementById("cfg-ambient-vol") as HTMLInputElement).value),
        sfxVolume: Number((document.getElementById("cfg-sfx-vol") as HTMLInputElement).value),
      });

      this.audio.setMixer({
        muted: this.settings.audioMuted,
        masterVolume: this.settings.masterVolume,
        ambientVolume: this.settings.ambientVolume,
        sfxVolume: this.settings.sfxVolume,
        reducedMotion: this.settings.reducedMotion,
      });
      this.applySettingsToSimulation(false);
      void this.persistCurrentSettings();
    };

    for (const id of [
      "cfg-speed",
      "cfg-fog",
      "cfg-lights",
      "cfg-cubes",
      "cfg-glow",
      "cfg-blur",
      "cfg-reduced-motion",
      "cfg-letterbox",
      "cfg-master-vol",
      "cfg-ambient-vol",
      "cfg-sfx-vol",
    ]) {
      document.getElementById(id)?.addEventListener("input", onLiveOverrideChange);
      document.getElementById(id)?.addEventListener("change", onLiveOverrideChange);
    }

    // Optional local resource archive importer
    document.getElementById("resource-file-input")?.addEventListener("change", (ev) => {
      const input = ev.target as HTMLInputElement;
      const file = input.files?.[0];
      if (!file) return;
      void (async () => {
        const bytes = new Uint8Array(await file.arrayBuffer());
        await this.importResourceBytes(bytes);
      })();
    });

    // Export JSON
    document.getElementById("export-json-btn")?.addEventListener("click", () => {
      void (async () => {
        const json = await this.storage.exportSettingsJson();
        const blob = new Blob([json], { type: "application/json" });
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = "osdbits-settings.json";
        a.click();
        URL.revokeObjectURL(url);
      })();
    });

    // Import JSON
    document.getElementById("import-json-input")?.addEventListener("change", (ev) => {
      const input = ev.target as HTMLInputElement;
      const file = input.files?.[0];
      if (!file) return;
      void (async () => {
        const statusEl = document.getElementById("persistence-status");
        try {
          const text = await file.text();
          const res = await this.storage.importSettingsJson(text);
          const loaded = await this.storage.loadSettings();
          this.settings = loaded.settings;
          this.applySettingsToSimulation(true);
          this.syncControlsFromSettings();
          if (statusEl) {
            statusEl.textContent = `Imported & verified JSON (rev #${res.revision})`;
          }
        } catch (err) {
          if (statusEl) {
            statusEl.textContent = `Import error: ${err instanceof Error ? err.message : String(err)}`;
          }
        }
      })();
    });

    // Two-step confirmed Reset to Faithful Defaults
    document.getElementById("reset-defaults-btn")?.addEventListener("click", () => {
      this.awaitingResetConfirm = true;
      document.getElementById("confirm-reset-btn")?.classList.remove("hidden");
    });

    document.getElementById("confirm-reset-btn")?.addEventListener("click", () => {
      if (!this.awaitingResetConfirm) return;
      this.awaitingResetConfirm = false;
      document.getElementById("confirm-reset-btn")?.classList.add("hidden");
      void (async () => {
        const res = await this.storage.resetToFaithfulDefaults(RESET_CONFIRMATION_TOKEN);
        this.settings = sanitizeSettings(DEFAULT_FAITHFUL_SETTINGS);
        this.textureAssets = synthesizeProceduralTextures();
        this.renderer?.updateTextures(this.textureAssets);
        this.audio.setMixer({
          muted: this.settings.audioMuted,
          masterVolume: this.settings.masterVolume,
          ambientVolume: this.settings.ambientVolume,
          sfxVolume: this.settings.sfxVolume,
          reducedMotion: this.settings.reducedMotion,
        });
        this.applySettingsToSimulation(true);
        this.lastSnapshot = this.bridge.stepFrame();
        this.syncControlsFromSettings();
        this.renderModeOverlays();
        const statusEl = document.getElementById("persistence-status");
        if (statusEl) {
          statusEl.textContent = `Restored faithful OSDSYS defaults (verified rev #${res.revision})`;
        }
      })();
    });
  }

  private updateCapabilityBadges(): void {
    const badgesEl = document.getElementById("capability-badges");
    const fallbackEl = document.getElementById("webgpu-fallback-panel");
    const startBtn = document.getElementById("start-osdsys-btn") as HTMLButtonElement | null;
    if (!badgesEl) return;

    const backend = this.renderer?.backend ?? null;
    const hasWebGpu = backend === "webgpu";
    const storageBackend = this.storage.getBackendStatus();
    const rendererLabel =
      backend === "webgpu"
        ? "WebGPU Ready"
        : backend === "canvas2d"
          ? "Canvas 2D Reduced"
          : "WebGPU Unavailable";
    this.rootEl.dataset.rendererBackend = backend ?? "unavailable";

    badgesEl.innerHTML = `
      <span class="cap-badge ${hasWebGpu ? "" : "warn"}">${rendererLabel}</span>
      <span class="cap-badge">WASM ABI v${this.bridge.abiVersion}</span>
      <span class="cap-badge">${storageBackend === "indexeddb" ? "IndexedDB Verified" : "Session Memory Fallback"}</span>
    `;

    if (hasWebGpu) {
      delete this.rootEl.dataset.webgpuFallback;
      fallbackEl?.classList.add("hidden");
      if (startBtn) startBtn.disabled = false;
      return;
    }

    this.rootEl.dataset.webgpuFallback = "true";
    fallbackEl?.classList.remove("hidden");
    const reasonEl = document.getElementById("webgpu-fallback-reason");
    if (reasonEl) {
      reasonEl.textContent = `WebGPU Adapter Unavailable: ${this.webGpuErrorReason ?? "navigator.gpu missing"}. The full renderer needs WebGPU, or you can proceed with the reduced Canvas 2D backend below.`;
    }
    if (startBtn) startBtn.disabled = true;
  }

  private syncControlsFromSettings(): void {
    const setVal = (id: string, v: string): void => {
      const el = document.getElementById(id) as HTMLInputElement | HTMLSelectElement | null;
      if (el) el.value = v;
    };
    const setChecked = (id: string, c: boolean): void => {
      const el = document.getElementById(id) as HTMLInputElement | null;
      if (el) el.checked = c;
    };

    setVal("launch-mode-select", this.settings.mode);
    setVal("cfg-video-standard", this.settings.pal ? "pal" : "ntsc");
    setVal("cfg-aspect-mode", this.settings.aspectMode);
    setVal("cfg-res-scale", String(this.settings.resolutionScale));
    setVal("cfg-language", String(this.settings.language));
    setVal("cfg-tower-preset", this.settings.towerPreset.kind);
    if (this.settings.towerPreset.kind === "simulated") {
      setVal("cfg-tower-ngames", String(this.settings.towerPreset.ngames));
    }
    setVal("cfg-rng-seed", String(this.settings.rngSeed));
    setVal("cfg-speed", String(this.settings.speedMultiplier));
    const speedVal = document.getElementById("cfg-speed-val");
    if (speedVal) speedVal.textContent = `${this.settings.speedMultiplier.toFixed(1)}x`;

    setChecked("cfg-fog", this.settings.enableFog);
    setChecked("cfg-lights", this.settings.enableLights);
    setChecked("cfg-cubes", this.settings.enableCubes);
    setChecked("cfg-glow", this.settings.enableGlow);
    setChecked("cfg-blur", this.settings.enableMotionBlur);
    setChecked("cfg-reduced-motion", this.settings.reducedMotion);
    setChecked("cfg-auto-menu", this.settings.autoTransitionToMenu);
    setChecked("cfg-letterbox", this.settings.letterbox);

    setVal("cfg-master-vol", String(this.settings.masterVolume));
    setVal("cfg-ambient-vol", String(this.settings.ambientVolume));
    setVal("cfg-sfx-vol", String(this.settings.sfxVolume));

    const muteBtn = document.getElementById("toggle-mute-btn");
    if (muteBtn) {
      muteBtn.textContent = this.settings.audioMuted ? "Audio: Muted" : "Audio: On";
      muteBtn.setAttribute("aria-pressed", String(this.settings.audioMuted));
    }

    for (const btn of Array.from(document.querySelectorAll<HTMLButtonElement>(".mode-switch-btn"))) {
      const isCurrent = btn.dataset.mode === this.settings.mode;
      btn.classList.toggle("active", isCurrent);
      btn.setAttribute("aria-pressed", String(isCurrent));
    }
  }

  private renderModeOverlays(): void {
    const snap = this.lastSnapshot;
    const menuHud = document.getElementById("menu-hud");
    const illegalHud = document.getElementById("illegal-hud");
    if (!snap || this.rootEl.dataset.sceneActive !== "true") {
      menuHud?.classList.add("hidden");
      illegalHud?.classList.add("hidden");
      return;
    }

    // Mode C HUD
    if (snap.mode === "illegal-disc" && snap.illegalTextAlpha > 16) {
      illegalHud?.classList.remove("hidden");
      const msgEl = document.getElementById("illegal-msg-text");
      if (msgEl) msgEl.textContent = snap.illegalMsg;
    } else {
      illegalHud?.classList.add("hidden");
    }

    // Mode D HUD
    if (snap.mode === "menu" && snap.menuUi) {
      menuHud?.classList.remove("hidden");
      const ui = snap.menuUi;
      // The core swallows menu input while its fade-in / 90-frame System
      // Configuration choreography plays (`menu.c:0x227D08`). Mirror that here so
      // pointer clicks are never silently dropped: rows only become clickable
      // once the core would actually accept the confirm.
      const interactive = ui.interactive;
      const titleEl = document.getElementById("menu-screen-title");
      const clockEl = document.getElementById("menu-clock-readout");
      if (titleEl) titleEl.textContent = ui.title;
      if (clockEl) clockEl.textContent = `${ui.date}  ${ui.time}`;
      if (menuHud) menuHud.dataset.menuInteractive = String(interactive);
      for (const id of ["menu-hint-confirm", "menu-hint-cancel", "menu-hint-options"]) {
        const hint = document.getElementById(id) as HTMLButtonElement | null;
        if (hint) {
          hint.disabled = !interactive;
          hint.setAttribute("aria-disabled", String(!interactive));
        }
      }

      // Avoid touching the DOM every frame unless screen/selection/values changed
      const sig = JSON.stringify({
        s: ui.screen,
        i: interactive,
        items: ui.items.map((x) => [x.label, x.value, x.selected, x.provenance]),
        cf: ui.clockFields,
        cc: ui.clockCursor,
      });
      if (sig !== this.activeMenuSignature) {
        this.activeMenuSignature = sig;
        const itemsPanel = document.getElementById("menu-items-panel");
        const clockBar = document.getElementById("clock-editor-bar");

        if (itemsPanel) {
          itemsPanel.dataset.menuInteractive = String(interactive);
          if (ui.screen === "browser-approx") {
            this.menuItemNodes = [];
            itemsPanel.innerHTML = `
              <div class="menu-item-btn selected">
                <span>Memory Card / Disc Browser (Clean-Room Inspector): ${snap.orbs} active RTC orbs. Press Esc / Back to return.</span>
                <span class="prov-pill approximate">Approximate</span>
              </div>
            `;
          } else if (ui.screen === "version-approx") {
            this.menuItemNodes = [];
            itemsPanel.innerHTML = `
              <div class="menu-item-btn selected">
                <span>osdbits-wasm Clean-Room Build • WASM ABI v${this.bridge.abiVersion} • Zero Sony ROM bytes</span>
                <span class="prov-pill approximate">Approximate</span>
              </div>
            `;
          } else {
            this.syncMenuItemNodes(itemsPanel, ui.items, interactive);
          }
        }

        if (clockBar) {
          if (ui.clockFields && ui.screen === "config-edit-clock") {
            clockBar.classList.remove("hidden");
            clockBar.innerHTML = "";
            ui.clockFields.forEach((val, fIdx) => {
              const chip = document.createElement("button");
              chip.type = "button";
              chip.className = `clock-field-chip ${ui.clockCursor === fIdx ? "active" : ""}`;
              chip.textContent = val;
              chip.disabled = !interactive;
              chip.addEventListener("click", () => {
                const cur = this.lastSnapshot?.menuUi;
                if (!cur || !cur.interactive) {
                  return;
                }
                const diff = fIdx - cur.clockCursor;
                const dir: MenuActionName = diff > 0 ? "right" : "left";
                for (let k = 0; k < Math.abs(diff); k++) {
                  this.bridge.dispatchMenuAction(dir);
                }
                this.lastSnapshot = this.bridge.stepFrame();
                this.renderModeOverlays();
              });
              clockBar.appendChild(chip);
            });
          } else {
            clockBar.classList.add("hidden");
          }
        }
      }
    } else {
      menuHud?.classList.add("hidden");
    }
  }

  /**
   * Reuses one button per Mode D menu row instead of re-creating the whole list.
   *
   * The list used to be rebuilt whenever any value changed (the clock readout
   * ticks once per second), which meant a mousedown/mouseup pair could straddle a
   * re-render: the click then lands on the panel instead of the button and is
   * silently dropped. Stable nodes also let us mirror the core's input gate via
   * the `disabled` attribute, so controls are only clickable once the core will
   * actually accept the confirm.
   */
  private syncMenuItemNodes(
    panel: HTMLElement,
    items: MenuUiSnapshot["items"],
    interactive: boolean,
  ): void {
    if (this.menuItemNodes.length !== items.length) {
      panel.innerHTML = "";
      this.menuItemNodes = items.map((_, idx) => this.createMenuItemNode(idx));
      for (const node of this.menuItemNodes) {
        panel.appendChild(node.button);
      }
    }
    items.forEach((item, idx) => {
      const node = this.menuItemNodes[idx];
      if (!node) {
        return;
      }
      node.label.textContent = item.label;
      node.value.textContent = item.value;
      node.value.hidden = item.value.length === 0;
      node.provenance.textContent = item.provenance;
      node.provenance.className = `prov-pill ${item.provenance}`;
      node.button.classList.toggle("selected", item.selected);
      node.button.setAttribute("aria-current", String(item.selected));
      node.button.disabled = !interactive;
      node.button.setAttribute("aria-disabled", String(!interactive));
    });
  }

  private createMenuItemNode(idx: number): MenuItemNode {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "menu-item-btn";
    button.dataset.menuIndex = String(idx);
    const label = document.createElement("span");
    const right = document.createElement("span");
    right.className = "menu-item-right";
    const value = document.createElement("span");
    value.className = "menu-item-val";
    const provenance = document.createElement("span");
    provenance.className = "prov-pill";
    right.append(value, provenance);
    button.append(label, right);
    button.addEventListener("click", () => this.confirmMenuItem(idx));
    return { button, label, value, provenance };
  }

  /** Moves the core cursor to `idx` and confirms, using the freshest snapshot. */
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

  private updateTelemetryDom(): void {
    const grid = document.getElementById("telemetry-grid");
    if (!grid || !this.lastSnapshot) return;

    const sortedFrames = [...this.frameDeltasMs].sort((a, b) => a - b);
    const p50 = percentile(sortedFrames, 0.5);
    const p95 = percentile(sortedFrames, 0.95);
    const fps = p50 > 0 ? Math.min(240, 1000 / p50) : 0;

    const sortedWasm = [...this.wasmStepTimesMs].sort((a, b) => a - b);
    const wasmP50 = percentile(sortedWasm, 0.5);

    const s = this.lastSnapshot;
    const gpuStats = this.renderer?.getResourceStats();
    const rendererBackend =
      this.renderer?.backend === "webgpu"
        ? "WebGPU"
        : this.renderer?.backend === "canvas2d"
          ? "Canvas 2D (reduced)"
          : "Unavailable";

    grid.innerHTML = `
      <span class="telemetry-key">Mode</span><span>${s.mode} (frame #${s.frame})</span>
      <span class="telemetry-key">Frame Time</span><span>${fps.toFixed(1)} fps (p50 ${p50.toFixed(1)}ms / p95 ${p95.toFixed(1)}ms)</span>
      <span class="telemetry-key">WASM Sim</span><span>${wasmP50.toFixed(2)} ms/step (${(this.bridge.memoryByteLength / 1024).toFixed(0)} KiB)</span>
      <span class="telemetry-key">Geometry</span><span>${s.commands} cmds / ${s.vertices} verts / ${s.towers} towers</span>
      <span class="telemetry-key">Scene Objects</span><span>${s.cubes} cubes / ${s.orbs} orbs / ${s.rods} rods</span>
      <span class="telemetry-key">Camera</span><span>(${s.camPos[0].toFixed(1)}, ${s.camPos[1].toFixed(1)}, ${s.camPos[2].toFixed(1)})</span>
      <span class="telemetry-key">Renderer</span><span>${rendererBackend}</span>
      <span class="telemetry-key">Render Targets</span><span>${gpuStats ? `${gpuStats.activeRenderTargetWidth}x${gpuStats.activeRenderTargetHeight} (recoveries: ${gpuStats.deviceLostRecoveries})` : "Fallback UI"}</span>
      <span class="telemetry-key">Audio / DB</span><span>${this.audio.getStatus()} / ${this.storage.getBackendStatus()} (r${this.storage.getKnownRevision()})</span>
      <span class="telemetry-key">Provenance</span><span>${MODE_PROVENANCE_DESCRIPTION[s.mode]}</span>
    `;
  }
}
