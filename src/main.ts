/**
 * osdbits-wasm browser shell.
 *
 * One activation click does everything: it unlocks audio inside the gesture, creates the WebGPU
 * device, instantiates the WASM core, restores persisted settings and starts the frame loop.
 * There is no second "initialise" step.
 */

import './style.css';
import { AudioEngine } from './audio/audio-engine';
import { ControlPanel, buildPreset } from './app/control-panel';
import { InputController } from './app/input';
import { FrameRenderer } from './renderer/frame-renderer';
import { GpuContext, isWebGpuAvailable } from './renderer/gpu-context';
import {
  Persistence,
  defaultSettings,
  exportSettings,
  importSettings,
  sanitizeSettings,
  type PersistedSettings,
} from './storage/persistence';
import { Pad, SceneMode, WasmCore, decodeAudio, type EngineState } from './wasm/bridge';

interface Elements {
  canvas: HTMLCanvasElement;
  overlay: HTMLElement;
  startButton: HTMLButtonElement;
  resumeCheckbox: HTMLInputElement;
  overlayStatus: HTMLElement;
  capabilities: HTMLElement;
  hud: HTMLElement;
  panel: HTMLElement;
  panelClose: HTMLButtonElement;
  importDialog: HTMLDialogElement;
  importFile: HTMLInputElement;
  importStatus: HTMLElement;
  importConfirm: HTMLButtonElement;
  importCancel: HTMLButtonElement;
}

function el<T extends Element>(selector: string): T {
  const node = document.querySelector(selector);
  if (!node) throw new Error(`Missing required element: ${selector}`);
  return node as T;
}

function gatherElements(): Elements {
  return {
    canvas: el<HTMLCanvasElement>('#osd-canvas'),
    overlay: el<HTMLElement>('#overlay'),
    startButton: el<HTMLButtonElement>('#start-btn'),
    resumeCheckbox: el<HTMLInputElement>('#resume-settings'),
    overlayStatus: el<HTMLElement>('#overlay-status'),
    capabilities: el<HTMLElement>('#capabilities'),
    hud: el<HTMLElement>('#hud'),
    panel: el<HTMLElement>('#panel'),
    panelClose: el<HTMLButtonElement>('#panel-close'),
    importDialog: el<HTMLDialogElement>('#import-dialog'),
    importFile: el<HTMLInputElement>('#import-file'),
    importStatus: el<HTMLElement>('#import-status'),
    importConfirm: el<HTMLButtonElement>('#import-confirm'),
    importCancel: el<HTMLButtonElement>('#import-cancel'),
  };
}

function setCapability(capabilities: HTMLElement, cap: string, state: 'ok' | 'warn' | 'fail', title: string): void {
  const node = capabilities.querySelector(`[data-cap="${cap}"]`);
  if (node instanceof HTMLElement) {
    node.dataset.state = state;
    node.title = title;
  }
}

class App {
  private readonly dom: Elements;
  private core: WasmCore | null = null;
  private gpu: GpuContext | null = null;
  private renderer: FrameRenderer | null = null;
  private readonly audio = new AudioEngine();
  private readonly persistence = new Persistence();
  private readonly input = new InputController();
  private panel: ControlPanel | null = null;
  private settings: PersistedSettings = defaultSettings();
  private running = false;
  private frameHandle = 0;
  private lastTimestamp = 0;
  private accumulator = 0;
  private paused = false;
  private fpsSamples: number[] = [];
  private lastHudUpdate = 0;
  private frameStats = { draws: 0, passes: 0, vertices: 0 };
  private started = false;
  private gpuError: string | null = null;
  private gpuReady = false;
  private gpuLossReason: string | null = null;
  private recoveryAttempts = 0;
  private readonly onResize = (): void => this.applyResolution();

  constructor(dom: Elements) {
    this.dom = dom;
  }

  /** Probes capabilities so the activation screen can state them honestly. */
  probe(): void {
    setCapability(
      this.dom.capabilities,
      'wasm',
      typeof WebAssembly === 'object' ? 'ok' : 'fail',
      typeof WebAssembly === 'object' ? 'WebAssembly is supported' : 'This browser has no WebAssembly',
    );
    setCapability(
      this.dom.capabilities,
      'webgpu',
      isWebGpuAvailable() ? 'ok' : 'fail',
      isWebGpuAvailable()
        ? 'WebGPU is exposed; the device is created when you start'
        : 'WebGPU is missing. Use Chrome/Edge 113+ or enable the WebGPU flag.',
    );
    setCapability(
      this.dom.capabilities,
      'storage',
      typeof indexedDB === 'object' ? 'ok' : 'warn',
      typeof indexedDB === 'object'
        ? 'IndexedDB is available for settings persistence'
        : 'IndexedDB is unavailable; settings will not persist',
    );
    setCapability(
      this.dom.capabilities,
      'audio',
      'AudioContext' in window || 'webkitAudioContext' in window ? 'ok' : 'warn',
      'AudioContext' in window || 'webkitAudioContext' in window
        ? 'Web Audio is available and will be unlocked by the start click'
        : 'Web Audio is missing; the scene will run silently',
    );
  }

  wireUi(): void {
    this.dom.startButton.addEventListener('click', () => {
      void this.start();
    });
    this.dom.panelClose.addEventListener('click', () => this.panel?.hide());
    this.dom.importCancel.addEventListener('click', () => this.dom.importDialog.close());
    this.dom.importConfirm.addEventListener('click', () => {
      void this.handleImport();
    });
    window.addEventListener('resize', this.onResize);
    window.addEventListener('keydown', (event) => {
      if (event.key === 'F2' || (event.key === 'p' && event.ctrlKey)) {
        event.preventDefault();
        this.panel?.toggle();
      }
      if (event.key === ' ' && event.shiftKey) {
        event.preventDefault();
        this.paused = !this.paused;
        this.core?.setPaused(this.paused);
      }
    });
    document.addEventListener('visibilitychange', () => {
      if (document.hidden) {
        void this.audio.suspend();
      } else if (this.running) {
        void this.audio.resume();
        this.lastTimestamp = 0;
      }
    });
  }

  private async start(): Promise<void> {
    if (this.started) return;
    this.started = true;
    this.dom.startButton.disabled = true;
    this.dom.overlayStatus.textContent = 'Starting…';

    // 1. Audio unlock MUST happen first, synchronously, while the gesture is still active.
    const audio = this.audio.unlock();
    if (!audio.ok) {
      this.dom.overlayStatus.textContent = `Audio unavailable (${audio.error ?? 'unknown'}). Continuing without sound.`;
    }

    try {
      // 2. Storage (non-fatal when unavailable).
      const opened = await this.persistence.open();
      if (!opened) {
        this.dom.overlayStatus.textContent = 'Storage unavailable — running without saved settings.';
      }
      void this.persistence.requestPersistent();
      const load = await this.persistence.load();
      if (load.settings && this.dom.resumeCheckbox.checked) {
        this.settings = load.settings;
      } else if (load.settings) {
        this.settings = { ...load.settings, revision: load.settings.revision };
        this.settings = sanitizeSettings({ ...defaultSettings(), revision: load.settings.revision });
      }
      if (load.migratedFrom !== null) {
        console.info(`[osdbits] migrated settings from schema v${load.migratedFrom}`);
      }
      if (load.recoveredFromCorruption) {
        console.warn('[osdbits] stored settings were corrupt; defaults restored', load.error);
      }

      // 3. WebGPU device + renderer.
      try {
        this.gpu = await GpuContext.create(this.dom.canvas);
      } catch (error) {
        this.gpuError = error instanceof Error ? error.message : String(error);
        throw new Error(`WebGPU initialisation failed: ${this.gpuError}`);
      }
      this.gpu.onDeviceLost((info) => {
        // A GPU reset (driver update, eGPU removal, sandboxed software renderer teardown) must
        // not throw the user back to the activation screen: keep the simulation running and
        // rebuild the device in the background.
        console.warn('[osdbits] WebGPU device lost:', info.reason, info.message);
        this.gpuReady = false;
        this.gpuLossReason = `${info.reason}: ${info.message}`;
        if (info.reason !== 'destroyed') void this.recoverGpu();
        else this.gpuLossReason = `${info.message} (the browser destroyed the device)`;
        this.updateHud(true);
      });

      // 4. WASM core.
      // Resolved against Vite's base so the app also works when deployed under a subpath.
      const wasmUrl = `${import.meta.env.BASE_URL}wasm/osdbits_core.wasm`;
      this.core = await WasmCore.load(wasmUrl);
      this.core.init(0x13579bdf, this.settings.regionJapan);
      this.applySettingsToCore();

      // 5. Restore an imported asset pack if one was persisted.
      await this.restoreImportedAssets();

      this.renderer = new FrameRenderer(this.gpu, this.core);
      this.gpuReady = true;
      this.applyResolution();
      this.buildPanel();

      this.core.setMode(this.settings.mode as SceneMode);
      this.core.step(0);
      this.frameStats = this.renderer.render(this.core);
      this.bootFrameStats = { ...this.frameStats };
      this.bootSignature = await this.measurePixels();

      this.dom.overlay.hidden = true;
      this.running = true;
      this.lastTimestamp = 0;
      this.frameHandle = requestAnimationFrame(this.frame);
      this.updateHud(true);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.error('[osdbits] start failed', error);
      this.dom.overlayStatus.textContent = `Could not start: ${message}`;
      this.dom.startButton.disabled = false;
      this.started = false;
      setCapability(this.dom.capabilities, 'webgpu', 'fail', message);
    }
  }

  private async restoreImportedAssets(): Promise<void> {
    if (this.settings.assetSource !== 'imported' || !this.core) return;
    const data = await this.persistence.loadAsset('osdbits-assets.bin');
    if (!data) return;
    const merged = this.core.importRomdir(data);
    if (merged > 0) {
      console.info(`[osdbits] restored ${merged} imported resources`);
      this.renderer?.reloadTextures(this.core);
    }
  }

  /**
   * Rebuilds the GPU device and the renderer after a device loss. Bounded: after three failed
   * attempts the experience stays in simulation-only mode and the HUD says so.
   */
  private async recoverGpu(): Promise<void> {
    if (this.recoveryAttempts >= 3 || !this.core) {
      this.gpuReady = false;
      this.updateHud(true);
      return;
    }
    this.recoveryAttempts += 1;
    await new Promise((resolve) => setTimeout(resolve, 1500 * this.recoveryAttempts));
    try {
      this.renderer?.dispose();
      this.gpu?.dispose();
      const gpu = await GpuContext.create(this.dom.canvas);
      gpu.onDeviceLost((info) => {
        console.warn('[osdbits] WebGPU device lost again:', info.reason, info.message);
        this.gpuReady = false;
        this.gpuLossReason = `${info.reason}: ${info.message}`;
        this.updateHud(true);
      });
      const renderer = new FrameRenderer(gpu, this.core);
      this.gpu = gpu;
      this.renderer = renderer;
      this.applyResolution();
      this.gpuReady = true;
      this.gpuLossReason = null;
      console.info('[osdbits] WebGPU device recovered');
    } catch (error) {
      this.gpuLossReason = error instanceof Error ? error.message : String(error);
    }
    this.updateHud(true);
  }

  private applySettingsToCore(): void {
    const core = this.core;
    if (!core) return;
    core.setSpeed(this.settings.speed);
    core.setReducedMotion(this.settings.reducedMotion);
    core.setFx(this.settings.fogIntensity, this.settings.refractionScale);
    core.setLanguage(this.settings.language);
    core.setConfig(this.settings.screenSize, this.settings.digitalOut, this.settings.componentOut);
    core.setRegionJapan(this.settings.regionJapan);
    core.setClock(this.settings.clock[0], this.settings.clock[1], this.settings.clock[2]);
    core.setBootHistory(this.settings.bootHistory);
    this.audio.setMasterVolume(this.settings.audioMaster);
    this.audio.setBgmVolume(this.settings.audioBgm);
    this.audio.setSeVolume(this.settings.audioSe);
    this.audio.setMuted(this.settings.muted);
  }

  private buildPanel(): void {
    this.panel = new ControlPanel(this.dom.panel, this.settings, {
      onChange: (next) => {
        this.settings = next;
        this.applySettingsToCore();
        this.scheduleSave();
      },
      onModeChange: (mode) => {
        this.core?.setMode(mode as SceneMode);
      },
      onResetSettings: () => {
        void this.resetEverything();
      },
      onResetBootHistory: () => {
        this.settings = { ...this.settings, bootHistory: new Array<number>(21).fill(0) };
        this.applySettingsToCore();
        this.panel?.setSettings(this.settings);
        this.scheduleSave();
      },
      onImport: () => {
        this.dom.importStatus.textContent = '';
        this.dom.importDialog.showModal();
      },
      onResetAssets: () => {
        this.core?.resetAssets();
        if (this.core) this.renderer?.reloadTextures(this.core);
        this.settings = { ...this.settings, assetSource: 'procedural' };
        this.scheduleSave();
      },
      onExport: () => exportSettings(this.settings),
      onImportJson: (json) => {
        const parsed = importSettings(json);
        if (!parsed.settings) return parsed.error;
        this.settings = parsed.settings;
        this.applySettingsToCore();
        this.panel?.setSettings(this.settings);
        this.core?.setMode(this.settings.mode as SceneMode);
        this.scheduleSave();
        return null;
      },
      getDiagnostics: () => this.diagnostics(),
    });
  }

  private saveTimer: number | null = null;

  private scheduleSave(): void {
    if (this.saveTimer !== null) window.clearTimeout(this.saveTimer);
    this.saveTimer = window.setTimeout(() => {
      this.saveTimer = null;
      void this.persist();
    }, 350);
  }

  private lastSaveState: 'idle' | 'saving' | 'ok' | 'failed' = 'idle';
  /** Signature of the very first rendered frame; evidence the GPU path really drew. */
  /** Draw counts of the first submitted frame, kept even if the device is later lost. */
  private bootFrameStats: { draws: number; passes: number; vertices: number } | null = null;

  private bootSignature: {
    lum: number;
    lit: number;
    total: number;
    width: number;
    height: number;
  } | null = null;

  private async persist(): Promise<void> {
    this.lastSaveState = 'saving';
    const result = await this.persistence.save(this.settings);
    this.lastSaveState = result.verified ? 'ok' : 'failed';
    if (result.verified) {
      this.settings = { ...this.settings, revision: result.revision };
    } else if (result.error) {
      console.warn('[osdbits] settings were not verified:', result.error);
    }
    this.updateHud(true);
  }

  private async resetEverything(): Promise<void> {
    await this.persistence.clear();
    this.settings = defaultSettings();
    this.applySettingsToCore();
    this.panel?.setSettings(this.settings);
    this.core?.setMode(SceneMode.Opening);
    this.core?.resetAssets();
    if (this.core) this.renderer?.reloadTextures(this.core);
    await this.persist();
  }

  private async handleImport(): Promise<void> {
    const file = this.dom.importFile.files?.[0];
    if (!file) {
      this.dom.importStatus.textContent = 'Choose a file first.';
      return;
    }
    if (!this.core) {
      this.dom.importStatus.textContent = 'Start the experience first, then import.';
      return;
    }
    this.dom.importConfirm.disabled = true;
    this.dom.importStatus.textContent = `Reading ${file.name} (${Math.round(file.size / 1024)} KiB)…`;
    try {
      const data = new Uint8Array(await file.arrayBuffer());
      const merged = this.core.importRomdir(data);
      if (merged <= 0) {
        this.dom.importStatus.textContent =
          'No OSDSYS resources were found in that file. Expected a PS2 boot ROM or a TEXIMAGE/FNTIMAGE container.';
        return;
      }
      this.renderer?.reloadTextures(this.core);
      const save = await this.persistence.saveAsset('osdbits-assets.bin', data);
      this.settings = { ...this.settings, assetSource: 'imported' };
      await this.persist();
      this.panel?.setSettings(this.settings);
      this.dom.importStatus.textContent = save.verified
        ? `Imported ${merged} resources and verified the stored copy.`
        : `Imported ${merged} resources for this session (storage verification failed: ${save.error ?? 'unknown'}).`;
      this.dom.importDialog.close();
    } catch (error) {
      this.dom.importStatus.textContent = `Import failed: ${error instanceof Error ? error.message : String(error)}`;
    } finally {
      this.dom.importConfirm.disabled = false;
    }
  }

  private bootSignatureCaptured = false;

  private async measurePixels(): Promise<{
    lum: number;
    lit: number;
    total: number;
    width: number;
    height: number;
  } | null> {
    if (!this.gpu) return null;
    try {
      const { data, width, height } = await this.gpu.readScreenPixels();
      let lum = 0;
      let lit = 0;
      for (let i = 0; i < data.length; i += 4) {
        const l = (data[i] ?? 0) + (data[i + 1] ?? 0) + (data[i + 2] ?? 0);
        lum += l;
        if (l > 30) lit += 1;
      }
      this.bootSignatureCaptured = true;
      return { lum, lit, total: data.length / 4, width, height };
    } catch (error) {
      // The device may already be gone; that is reported through the HUD, not a thrown test.
      console.warn('[osdbits] pixel readback unavailable:', error);
      return null;
    }
  }

  debugRenderInfo(): {
    gpuReady: boolean;
    adapter: string | null;
    draws: number;
    passes: number;
    vertices: number;
    bootSignature: boolean;
    lossReason: string | null;
  } {
    return {
      gpuReady: this.gpuReady,
      adapter: this.gpu?.capabilities.adapterName ?? null,
      draws: this.bootFrameStats?.draws ?? this.frameStats.draws,
      passes: this.bootFrameStats?.passes ?? this.frameStats.passes,
      vertices: this.bootFrameStats?.vertices ?? this.frameStats.vertices,
      bootSignature: this.bootSignature !== null,
      lossReason: this.gpuLossReason,
    };
  }

  debugBootSignature(): {
    lum: number;
    lit: number;
    total: number;
    width: number;
    height: number;
  } | null {
    return this.bootSignature;
  }

  private applyResolution(): void {
    if (!this.gpu) return;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const rect = this.dom.canvas.getBoundingClientRect();
    const scale = this.settings.resolutionScale;
    const width = Math.max(1, Math.round((rect.width || window.innerWidth) * dpr * scale));
    const height = Math.max(1, Math.round((rect.height || window.innerHeight) * dpr * scale));
    this.gpu.resize(width, height);
  }

  private stop(): void {
    this.running = false;
    if (this.frameHandle) cancelAnimationFrame(this.frameHandle);
    this.frameHandle = 0;
  }

  private frame = (timestamp: number): void => {
    if (!this.running || !this.core || !this.renderer) return;
    try {
      this.frameBody(timestamp);
    } catch (error) {
      console.error('[osdbits] frame error', error);
      this.stop();
      this.dom.overlay.hidden = false;
      this.dom.overlayStatus.textContent = `Rendering stopped: ${
        error instanceof Error ? error.message : String(error)
      }`;
      this.dom.startButton.disabled = false;
      this.started = false;
      return;
    }
  };

  private frameBody(timestamp: number): void {
    const core = this.core;
    if (!core) return;
    if (this.lastTimestamp === 0) this.lastTimestamp = timestamp;
    const delta = Math.min(0.1, (timestamp - this.lastTimestamp) / 1000);
    this.lastTimestamp = timestamp;

    const targetFps = 60;
    const stepSeconds = 1 / targetFps;
    this.accumulator += delta;
    // Fixed 60 Hz simulation, matching the console's NTSC field rate.
    let steps = 0;
    while (this.accumulator >= stepSeconds && steps < 4) {
      this.input.pollGamepads();
      core.step(this.input.word());
      this.audio.processMessages(decodeAudio(core.audioView()));
      core.drainAudio();
      this.accumulator -= stepSeconds;
      steps += 1;
    }
    if (steps === 0) {
      // Still present the last rendered frame so resizes and device changes show up.
      core.step(this.input.word());
      this.audio.processMessages(decodeAudio(core.audioView()));
      core.drainAudio();
    }

    if (this.renderer && this.gpuReady) {
      this.frameStats = this.renderer.render(core);
    }

    this.fpsSamples.push(delta);
    if (this.fpsSamples.length > 120) this.fpsSamples.shift();

    const now = performance.now();
    if (now - this.lastHudUpdate > 250) {
      this.lastHudUpdate = now;
      this.updateHud(false);
    }

    this.frameHandle = requestAnimationFrame(this.frame);
  }

  private updateHud(force: boolean): void {
    if (!this.core) return;
    const state: EngineState = this.core.state();
    const avg = this.fpsSamples.length
      ? this.fpsSamples.reduce((a, b) => a + b, 0) / this.fpsSamples.length
      : 0;
    const fps = avg > 0 ? 1 / avg : 0;
    const modeNames = ['Opening', 'Idle', 'Illegal disc', 'Menu'];
    const saveLabel =
      this.lastSaveState === 'ok'
        ? '<strong>saved &amp; verified</strong>'
        : this.lastSaveState === 'saving'
          ? 'saving…'
          : this.lastSaveState === 'failed'
            ? '<span class="warn">save unverified</span>'
            : '—';
    const audio = this.audio.status();
    this.dom.hud.innerHTML = [
      `<strong>${modeNames[state.mode] ?? 'unknown'}</strong> · frame ${state.frame}`,
      `${fps.toFixed(0)} fps · ${this.frameStats.draws} draws · ${this.frameStats.passes} passes · ${(this.frameStats.vertices / 1000).toFixed(1)}k verts`,
      `towers ${state.active_towers} · assets ${state.asset_imported ? 'imported' : 'procedural'}`,
      `audio ${audio.contextState}${audio.muted ? ' (muted)' : ''} · settings ${saveLabel}`,
      this.gpuReady && this.gpu
        ? `gpu ${this.gpu.capabilities.adapterName}`
        : `<span class="warn">gpu unavailable${this.gpuLossReason ? ` — ${this.gpuLossReason}` : ''} · simulation running</span>`,
      force ? 'F2 panel · Shift+Space pause' : '',
    ]
      .filter(Boolean)
      .join('<br>');
  }

  private diagnostics(): string {
    const gpu = this.gpu;
    const audio = this.audio.status();
    const lines = [
      `WebGPU: ${gpu ? gpu.capabilities.adapterName : (this.gpuError ?? 'not initialised')}`,
      gpu ? `canvas format: ${gpu.format}` : '',
      gpu ? `max texture 2D: ${gpu.capabilities.maxTextureDimension2D}` : '',
      `audio: ${audio.contextState} · unlocked=${String(audio.unlocked)} · beds=${audio.activeBeds.join(',') || 'none'}`,
      audio.error ? `audio error: ${audio.error}` : '',
      `storage: ${this.persistence.available ? 'available' : 'unavailable'}`,
      `settings revision: ${this.settings.revision}`,
      `assets: ${this.settings.assetSource}`,
    ];
    return lines.filter(Boolean).join('\n');
  }

  /** Test hook: steps N frames deterministically and returns the state. */
  debugStep(frames: number, buttons = 0): EngineState {
    if (!this.core) throw new Error('core not initialised');
    for (let i = 0; i < frames; i += 1) this.core.step(buttons);
    return this.core.state();
  }

  debugState(): EngineState | null {
    return this.core ? this.core.state() : null;
  }

  async debugPixels(): Promise<{ width: number; height: number; data: Uint8Array } | null> {
    if (!this.gpu) return null;
    try {
      return await this.gpu.readScreenPixels();
    } catch (error) {
      console.warn('[osdbits] pixel readback failed:', error);
      return null;
    }
  }

  /** Test hook: import a ROMDIR/BIOS blob and return the merged resource count (0 = rejected). */
  debugImportRomdir(data: Uint8Array): number {
    if (!this.core) throw new Error('core not initialised');
    const merged = this.core.importRomdir(data);
    if (merged > 0) this.renderer?.reloadTextures(this.core);
    return merged;
  }

  debugPause(paused: boolean): void {
    this.paused = paused;
    this.core?.setPaused(paused);
  }

  debugPreset(name: string): void {
    this.settings = { ...this.settings, bootHistory: buildPreset(name) };
    this.applySettingsToCore();
  }

  debugSetMode(mode: number): void {
    this.core?.setMode(mode as SceneMode);
  }

  get isRunning(): boolean {
    return this.running;
  }
}

const app = new App(gatherElements());
app.probe();
app.wireUi();

// Expose a narrow debug surface used by the Playwright suite.
declare global {
  interface Window {
    __osdbits?: {
      step: (frames: number, buttons?: number) => EngineState;
      state: () => EngineState | null;
      pixels: () => Promise<{ width: number; height: number; data: Uint8Array } | null>;
      signature: () => {
        lum: number;
        lit: number;
        total: number;
        width: number;
        height: number;
      } | null;
      pause: (paused: boolean) => void;
      importRomdir: (data: Uint8Array) => number;
      renderInfo: () => {
        gpuReady: boolean;
        adapter: string | null;
        draws: number;
        passes: number;
        vertices: number;
        bootSignature: boolean;
        lossReason: string | null;
      };
      preset: (name: string) => void;
      setMode: (mode: number) => void;
      running: () => boolean;
      pad: typeof Pad;
    };
  }
}

window.__osdbits = {
  step: (frames, buttons = 0) => app.debugStep(frames, buttons),
  state: () => app.debugState(),
  pixels: () => app.debugPixels(),
  signature: () => app.debugBootSignature(),
  pause: (paused: boolean) => app.debugPause(paused),
  renderInfo: () => app.debugRenderInfo(),
  importRomdir: (data: Uint8Array) => app.debugImportRomdir(data),
  preset: (name) => app.debugPreset(name),
  setMode: (mode) => app.debugSetMode(mode),
  running: () => app.isRunning,
  pad: Pad,
};
