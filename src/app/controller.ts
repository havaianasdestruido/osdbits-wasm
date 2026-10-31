import { AudioEngine, type AudioEngineSnapshot } from '../audio/audioEngine';
import {
  FACTORY_RESET_TOKEN,
  FAITHFUL_DEFAULT_SETTINGS,
  type PersistedUserSettings,
  PersistenceStore,
  type StorageDurabilityReport,
} from '../persistence/storage';
import { createOsdbitsRenderer } from '../renderer/createRenderer';
import {
  type AdapterDiagnostics,
  type OsdbitsRenderer,
  type PixelReadback,
  screenSizeIndexToAspect,
} from '../renderer/types';
import {
  AppMode,
  type BootCompletionPolicy,
  type FrameTelemetrySnapshot,
  loadOsdbitsWasm,
  MenuAction,
  MenuPage,
  OsdbitsWasmEngine,
} from '../wasm/bridge';

export interface FramePerfSummary {
  sampleCount: number;
  meanMs: number;
  p95Ms: number;
  maxMs: number;
}

export interface AppDiagnosticsSnapshot {
  started: boolean;
  paused: boolean;
  renderer: AdapterDiagnostics | null;
  audio: AudioEngineSnapshot;
  durability: StorageDurabilityReport | null;
  persistenceRevision: number;
  lastSaveReadBackVerified: boolean;
  usingImportedAssets: boolean;
  importedSha256Hex: string | null;
  settings: PersistedUserSettings;
  telemetry: FrameTelemetrySnapshot | null;
  perf: FramePerfSummary;
}

export interface DeterministicCaptureRequest {
  mode: AppMode;
  seed: number;
  framesToAdvance: number;
  rtcIso?: string;
}

const MODE_LABELS: Record<AppMode, string> = {
  [AppMode.Opening]: 'Mode A · Opening',
  [AppMode.Idle]: 'Mode B · Idle Towers',
  [AppMode.Illegal]: 'Mode C · Illegal Disc',
  [AppMode.Menu]: 'Mode D · OSDSYS Menu',
};

function byId<T extends HTMLElement>(doc: Document, id: string): T {
  const el = doc.getElementById(id);
  if (!el) {
    throw new Error(`Missing required DOM element #${id}`);
  }
  return el as T;
}

export class OsdbitsAppController {
  private readonly doc: Document;
  private canvas: HTMLCanvasElement;
  private readonly store: PersistenceStore;
  private readonly audio: AudioEngine;

  private engine: OsdbitsWasmEngine | null = null;
  private renderer: OsdbitsRenderer | null = null;
  private durability: StorageDurabilityReport | null = null;
  private settings: PersistedUserSettings = structuredClone(FAITHFUL_DEFAULT_SETTINGS);
  private lastTelemetry: FrameTelemetrySnapshot | null = null;
  private lastSaveVerified = true;
  private importedSha256Hex: string | null = null;

  private started = false;
  private starting = false;
  private paused = false;
  private rafId = 0;
  private lastRafTimeMs = 0;
  private lastClockSyncMs = 0;
  private clockLockedForTest = false;
  private resizeObserver: ResizeObserver | null = null;

  private preflightPromise: Promise<void> | null = null;
  private uiBound = false;
  private readonly frameTimesMs: number[] = [];
  private disposed = false;

  public constructor(doc: Document = document) {
    this.doc = doc;
    this.canvas = byId<HTMLCanvasElement>(doc, 'osdbits-canvas');
    this.audio = new AudioEngine();
    this.store = new PersistenceStore({
      onRemoteSettingsUpdate: (envelope) => {
        this.settings = envelope.settings;
        this.applySettingsToRuntime(false);
        this.syncHudControlsFromSettings();
        this.updateStorageStatusBanner('Synced live update from sibling tab.');
      },
      onRemoteReset: () => {
        this.settings = structuredClone(FAITHFUL_DEFAULT_SETTINGS);
        this.engine?.resetProceduralTextures();
        this.importedSha256Hex = null;
        this.applySettingsToRuntime(true);
        this.syncHudControlsFromSettings();
        this.updateAssetBanner();
        this.updateStorageStatusBanner('Factory reset synced from sibling tab.');
      },
    });
  }

  public bootstrapPreflight(): Promise<void> {
    if (!this.uiBound) {
      this.uiBound = true;
      this.bindUiEvents();
    }
    if (!this.preflightPromise) {
      this.preflightPromise = this.runPreflightAsync();
    }
    return this.preflightPromise;
  }

  private async runPreflightAsync(): Promise<void> {
    const gpuBadge = byId<HTMLElement>(this.doc, 'gate-badge-gpu');
    const storageBadge = byId<HTMLElement>(this.doc, 'gate-badge-storage');
    const assetBadge = byId<HTMLElement>(this.doc, 'gate-badge-assets');

    // Load persisted preferences before the user even clicks Start!
    const envelope = await this.store.loadSettings();
    if (envelope.revision > 0) {
      this.settings = envelope.settings;
    } else if (
      typeof window !== 'undefined' &&
      typeof window.matchMedia === 'function' &&
      window.matchMedia('(prefers-reduced-motion: reduce)').matches
    ) {
      this.settings.tuning.reducedMotion = true;
    }

    // Apply URL query overrides (`?renderer=canvas2d`, `?mode=0..3`, `?seed=...`)
    this.applyUrlQueryOverrides();
    this.audio.setPreferences(this.settings.audio);
    this.syncHudControlsFromSettings();

    this.durability = await this.store.queryDurability(false);
    storageBadge.textContent = this.durability.indexedDbAvailable
      ? `IndexedDB Ready${envelope.revision > 0 ? ` (Saved r${envelope.revision})` : ''}`
      : 'Memory Storage Fallback';

    const assetLoad = await this.store.loadImportedAsset();
    if (assetLoad.record) {
      assetBadge.textContent = `Saved Asset (${assetLoad.record.importedTextureCount} tex)`;
    } else {
      assetBadge.textContent = 'Procedural Clean-Room Assets';
    }

    if (this.settings.forceCanvasFallback) {
      gpuBadge.textContent = 'Canvas 2D Mode Selected';
      gpuBadge.className = 'badge badge-approx';
    } else if (typeof navigator !== 'undefined' && 'gpu' in navigator && navigator.gpu) {
      gpuBadge.textContent = 'WebGPU API Detected';
      gpuBadge.className = 'badge badge-verified';
    } else {
      gpuBadge.textContent = 'Canvas 2D Fallback Ready';
      gpuBadge.className = 'badge badge-approx';
    }
  }

  private applyUrlQueryOverrides(): void {
    if (typeof window === 'undefined') return;
    const params = new URLSearchParams(window.location.search);
    if (params.get('renderer') === 'canvas2d') {
      this.settings.forceCanvasFallback = true;
    }
    const modeParam = params.get('mode');
    if (modeParam !== null) {
      const m = Number(modeParam);
      if (Number.isInteger(m) && m >= 0 && m <= 3) {
        this.settings.preferredMode = m as AppMode;
      }
    }
    const seedParam = params.get('seed');
    if (seedParam !== null) {
      const s = Number(seedParam);
      if (Number.isFinite(s) && s >= 0) {
        this.settings.seed = (Math.floor(s) >>> 0);
      }
    }
  }

  /**
   * Single-gesture startup entrypoint bound to `#start-osdsys-btn`.
   * Calls `audio.unlockFromUserGesture()` SYNCHRONOUSLY before any `await`.
   */
  public startFromUserGesture(): Promise<void> {
    if (this.started || this.starting) {
      return Promise.resolve();
    }
    this.starting = true;

    // 1. Synchronous Web Audio unlock in the exact user-gesture call stack!
    const audioUnlock = this.audio.unlockFromUserGesture();

    const startBtn = byId<HTMLButtonElement>(this.doc, 'start-osdsys-btn');
    startBtn.disabled = true;
    startBtn.textContent = 'Initializing OSDSYS…';

    return this.finishAsyncStartup(audioUnlock.settled).finally(() => {
      this.starting = false;
    });
  }

  private async finishAsyncStartup(
    audioSettled: Promise<AudioEngineSnapshot>,
  ): Promise<void> {
    if (this.preflightPromise) {
      await this.preflightPromise;
    }
    const [, rawWasm, rendererRes, persistedAsset] = await Promise.all([
      audioSettled,
      loadOsdbitsWasm('./wasm/osdbits_core.wasm'),
      createOsdbitsRenderer({
        canvas: this.canvas,
        forceCanvasFallback: this.settings.forceCanvasFallback,
        onDeviceLost: (reason, message) => {
          void this.handleGpuDeviceLost(reason, message);
        },
      }),
      this.store.loadImportedAsset(),
    ]);

    this.canvas = rendererRes.canvas;
    this.bindCanvasPointerInput(this.canvas);
    this.renderer = rendererRes.renderer;
    this.engine = new OsdbitsWasmEngine(rawWasm, this.settings.seed);

    if (persistedAsset.record) {
      const res = this.engine.importContainer(persistedAsset.record.bytes);
      if (res.ok) {
        this.importedSha256Hex = res.sha256Hex;
      }
    }

    this.applySettingsToRuntime(true);
    this.syncViewportSize();

    // Hide startup gate, reveal status bar & HUD drawer (unless collapsed)
    byId<HTMLElement>(this.doc, 'startup-gate').hidden = true;
    byId<HTMLElement>(this.doc, 'top-bar').hidden = false;
    this.setHudCollapsed(this.settings.hudCollapsed, false);

    this.updateTopBadges();
    this.updateAssetBanner();
    this.updateStorageStatusBanner();

    this.started = true;
    // Render immediate first frame before next RAF tick
    this.stepOneFrame(1.0);

    this.lastRafTimeMs = performance.now();
    const tick = (nowMs: number): void => {
      if (this.disposed) return;
      const elapsedMs = Math.max(0, nowMs - this.lastRafTimeMs);
      this.lastRafTimeMs = nowMs;

      if (!this.paused) {
        const nominalHz = this.settings.tuning.isPal ? 50 : 60;
        const dtFrames = Math.min(3.0, Math.max(0.25, (elapsedMs * nominalHz) / 1000));
        const t0 = performance.now();
        this.stepOneFrame(dtFrames);
        const costMs = performance.now() - t0;
        this.recordFrameCost(costMs);
      }
      this.rafId = requestAnimationFrame(tick);
    };
    this.rafId = requestAnimationFrame(tick);
  }

  private async handleGpuDeviceLost(reason: string, message: string): Promise<void> {
    if (this.disposed || !this.started) return;
    this.renderer?.dispose();
    const fallbackRes = await createOsdbitsRenderer({
      canvas: this.canvas,
      forceCanvasFallback: true,
    });
    this.canvas = fallbackRes.canvas;
    this.bindCanvasPointerInput(this.canvas);
    this.renderer = fallbackRes.renderer;
    this.syncViewportSize();
    this.updateTopBadges();
    this.updateStorageStatusBanner(
      `Recovered from GPU device loss (${reason}: ${message}) via Canvas 2D.`,
    );
  }

  private recordFrameCost(ms: number): void {
    this.frameTimesMs.push(ms);
    if (this.frameTimesMs.length > 240) {
      this.frameTimesMs.shift();
    }
  }

  private stepOneFrame(dtFrames: number): void {
    if (!this.engine || !this.renderer) return;

    const now = Date.now();
    if (!this.clockLockedForTest && now - this.lastClockSyncMs >= 500) {
      this.engine.setRtc(new Date(now), false);
      this.lastClockSyncMs = now;
    }

    const prevMode = this.lastTelemetry?.mode ?? null;
    const tel = this.engine.stepAndRender(dtFrames);
    this.lastTelemetry = tel;

    if (prevMode !== tel.mode) {
      this.audio.setSceneMode(tel.mode);
      this.updateTopBadges();
      this.highlightActiveModeButton(tel.mode);
    } else if (tel.mode === AppMode.Menu) {
      this.updateTopBadges();
    }

    if (tel.audioCues !== 0) {
      this.audio.triggerCues(tel.audioCues);
    }

    this.renderer.renderFrame(this.engine, tel);
    this.updateTelemetryDom(tel);
  }

  private applySettingsToRuntime(resetTimeline: boolean): void {
    this.audio.setPreferences(this.settings.audio);
    if (!this.engine) return;

    this.engine.setSeed(this.settings.seed);
    this.engine.setBootCompletionPolicy(this.settings.bootCompletionPolicy);
    this.engine.setTuning(this.settings.tuning);
    if (resetTimeline) {
      this.engine.setMode(this.settings.preferredMode, true);
      this.audio.setSceneMode(this.settings.preferredMode);
    }
    this.syncViewportSize();
  }

  private syncViewportSize(): void {
    if (!this.renderer) return;
    const stage = byId<HTMLElement>(this.doc, 'viewport-stage');
    const cssW = stage.clientWidth || window.innerWidth || 640;
    const cssH = stage.clientHeight || window.innerHeight || 480;
    const dpr = typeof window !== 'undefined' ? window.devicePixelRatio || 1 : 1;
    this.renderer.resize(
      cssW,
      cssH,
      dpr,
      this.settings.resolutionScale,
      screenSizeIndexToAspect(this.settings.tuning.screenSize),
    );
  }

  private persistSettings(): void {
    void this.store.saveSettings(this.settings).then((res) => {
      this.lastSaveVerified = res.ok && res.readBackVerified;
      this.updateStorageStatusBanner(
        res.ok
          ? `Saved r${res.revision} · Read-back verified`
          : `Save warning: ${res.error ?? 'unknown'}`,
      );
    });
  }

  private setHudCollapsed(collapsed: boolean, persist = true): void {
    this.settings.hudCollapsed = collapsed;
    const drawer = byId<HTMLElement>(this.doc, 'hud-drawer');
    const toggleBtn = byId<HTMLButtonElement>(this.doc, 'hud-toggle-btn');
    drawer.hidden = collapsed;
    toggleBtn.setAttribute('aria-expanded', collapsed ? 'false' : 'true');
    if (persist) {
      this.persistSettings();
    }
  }

  private bindCanvasPointerInput(canvas: HTMLCanvasElement): void {
    canvas.onclick = (ev: MouseEvent) => {
      if (!this.engine || !this.renderer || !this.lastTelemetry) return;
      if (this.lastTelemetry.mode !== AppMode.Menu) return;

      const rect = canvas.getBoundingClientRect();
      const dprX = this.renderer.layout.canvasWidth / Math.max(1, rect.width);
      const dprY = this.renderer.layout.canvasHeight / Math.max(1, rect.height);
      const px = (ev.clientX - rect.left) * dprX;
      const py = (ev.clientY - rect.top) * dprY;

      const { viewportX, viewportY, viewportWidth, viewportHeight } =
        this.renderer.layout;
      const nx = (px - viewportX) / Math.max(1, viewportWidth);
      const ny = (py - viewportY) / Math.max(1, viewportHeight);
      if (nx < 0 || nx > 1 || ny < 0 || ny > 1) return;

      const cues = this.engine.dispatchPointerClick(nx, ny);
      if (cues !== 0) {
        this.audio.triggerCues(cues);
      }
      if (this.paused) {
        this.stepOneFrame(0.0);
      }
    };
  }

  private bindUiEvents(): void {
    const startBtn = byId<HTMLButtonElement>(this.doc, 'start-osdsys-btn');
    startBtn.addEventListener('click', () => {
      void this.startFromUserGesture();
    });

    byId<HTMLButtonElement>(this.doc, 'hud-toggle-btn').addEventListener(
      'click',
      () => {
        this.setHudCollapsed(!this.settings.hudCollapsed, true);
      },
    );

    // Mode buttons
    this.doc.querySelectorAll<HTMLButtonElement>('.mode-btn').forEach((btn) => {
      btn.addEventListener('click', () => {
        const mode = Number(btn.dataset['mode'] ?? 0) as AppMode;
        this.settings.preferredMode = mode;
        this.engine?.setMode(mode, true);
        this.audio.setSceneMode(mode);
        this.highlightActiveModeButton(mode);
        this.updateTopBadges();
        this.persistSettings();
        if (this.paused) {
          this.stepOneFrame(1.0);
        }
      });
    });

    // Pad cluster buttons
    this.doc.querySelectorAll<HTMLButtonElement>('.pad-btn').forEach((btn) => {
      btn.addEventListener('click', () => {
        const act = Number(btn.dataset['menuAction'] ?? 0) as MenuAction;
        this.triggerMenuAction(act);
      });
    });

    byId<HTMLButtonElement>(this.doc, 'replay-scene-btn').addEventListener(
      'click',
      () => {
        const activeMode = this.lastTelemetry?.mode ?? this.settings.preferredMode;
        this.engine?.setMode(activeMode, true);
        if (this.paused) {
          this.stepOneFrame(1.0);
        }
      },
    );

    byId<HTMLSelectElement>(this.doc, 'boot-policy-select').addEventListener(
      'change',
      (ev) => {
        const val = Number(
          (ev.currentTarget as HTMLSelectElement).value,
        ) as BootCompletionPolicy;
        this.settings.bootCompletionPolicy = val;
        this.engine?.setBootCompletionPolicy(val);
        this.persistSettings();
      },
    );

    byId<HTMLButtonElement>(this.doc, 'apply-seed-btn').addEventListener(
      'click',
      () => {
        const raw = Number(byId<HTMLInputElement>(this.doc, 'seed-input').value);
        const seed = Number.isFinite(raw) ? (Math.max(0, Math.floor(raw)) >>> 0) : 0;
        this.settings.seed = seed;
        this.engine?.setSeed(seed);
        this.persistSettings();
        if (this.paused) {
          this.stepOneFrame(1.0);
        }
      },
    );

    const pauseBtn = byId<HTMLButtonElement>(this.doc, 'pause-btn');
    pauseBtn.addEventListener('click', () => {
      this.paused = !this.paused;
      pauseBtn.textContent = this.paused ? 'Resume' : 'Pause';
      pauseBtn.setAttribute('aria-pressed', this.paused ? 'true' : 'false');
    });

    byId<HTMLButtonElement>(this.doc, 'step-frame-btn').addEventListener(
      'click',
      () => {
        this.paused = true;
        pauseBtn.textContent = 'Resume';
        pauseBtn.setAttribute('aria-pressed', 'true');
        this.stepOneFrame(1.0);
      },
    );

    // Customization sliders & selects
    const bindTuningControl = (id: string, fn: () => void): void => {
      byId<HTMLElement>(this.doc, id).addEventListener('input', () => {
        fn();
        this.applySettingsToRuntime(false);
        this.persistSettings();
        if (this.paused) this.stepOneFrame(0.0);
      });
    };

    bindTuningControl('speed-range', () => {
      const v = Number(byId<HTMLInputElement>(this.doc, 'speed-range').value);
      this.settings.tuning.speedScale = v;
      byId<HTMLElement>(this.doc, 'speed-val').textContent = v.toFixed(2);
    });

    bindTuningControl('drift-range', () => {
      const v = Number(byId<HTMLInputElement>(this.doc, 'drift-range').value);
      this.settings.tuning.idleDrift = v;
      byId<HTMLElement>(this.doc, 'drift-val').textContent = v.toFixed(2);
    });

    bindTuningControl('towers-range', () => {
      const v = Math.round(
        Number(byId<HTMLInputElement>(this.doc, 'towers-range').value),
      );
      this.settings.tuning.towerGames = v;
      byId<HTMLElement>(this.doc, 'towers-val').textContent = String(v);
    });

    bindTuningControl('res-scale-range', () => {
      const v = Number(byId<HTMLInputElement>(this.doc, 'res-scale-range').value);
      this.settings.resolutionScale = v;
      byId<HTMLElement>(this.doc, 'res-val').textContent = v.toFixed(2);
    });

    bindTuningControl('aspect-select', () => {
      this.settings.tuning.screenSize = Number(
        byId<HTMLSelectElement>(this.doc, 'aspect-select').value,
      );
    });

    bindTuningControl('video-std-select', () => {
      this.settings.tuning.isPal =
        byId<HTMLSelectElement>(this.doc, 'video-std-select').value === 'pal';
    });

    bindTuningControl('language-select', () => {
      this.settings.tuning.language = Number(
        byId<HTMLSelectElement>(this.doc, 'language-select').value,
      );
    });

    const bindCheckbox = (id: string, apply: (checked: boolean) => void): void => {
      byId<HTMLInputElement>(this.doc, id).addEventListener('change', (ev) => {
        apply((ev.currentTarget as HTMLInputElement).checked);
        this.applySettingsToRuntime(false);
        this.persistSettings();
        if (this.paused) this.stepOneFrame(0.0);
      });
    };

    bindCheckbox('chk-fog', (c) => (this.settings.tuning.fogEnabled = c));
    bindCheckbox(
      'chk-refraction',
      (c) => (this.settings.tuning.refractionEnabled = c),
    );
    bindCheckbox('chk-bloom', (c) => (this.settings.tuning.bloomEnabled = c));
    bindCheckbox('chk-banner', (c) => (this.settings.tuning.showBanner = c));
    bindCheckbox(
      'chk-reduced-motion',
      (c) => (this.settings.tuning.reducedMotion = c),
    );
    bindCheckbox(
      'chk-confirm-circle',
      (c) => (this.settings.tuning.confirmWithCircle = c),
    );
    bindCheckbox('chk-mute', (c) => (this.settings.audio.muted = c));

    byId<HTMLInputElement>(this.doc, 'chk-canvas2d').addEventListener(
      'change',
      (ev) => {
        const force = (ev.currentTarget as HTMLInputElement).checked;
        this.settings.forceCanvasFallback = force;
        this.persistSettings();
        if (this.started) {
          void this.switchRendererLive(force);
        }
      },
    );

    bindTuningControl('master-vol-range', () => {
      this.settings.audio.masterVolume = Number(
        byId<HTMLInputElement>(this.doc, 'master-vol-range').value,
      );
    });

    byId<HTMLButtonElement>(this.doc, 'reset-defaults-btn').addEventListener(
      'click',
      () => {
        const hudWasCollapsed = this.settings.hudCollapsed;
        this.settings = structuredClone(FAITHFUL_DEFAULT_SETTINGS);
        this.settings.hudCollapsed = hudWasCollapsed;
        this.syncHudControlsFromSettings();
        this.applySettingsToRuntime(true);
        this.persistSettings();
      },
    );

    // Asset import & procedural restore
    byId<HTMLInputElement>(this.doc, 'import-asset-input').addEventListener(
      'change',
      (ev) => {
        const input = ev.currentTarget as HTMLInputElement;
        const file = input.files?.[0];
        if (!file) return;
        void file.arrayBuffer().then((ab) => {
          void this.importAssetBytes(file.name, new Uint8Array(ab));
        });
        input.value = '';
      },
    );

    byId<HTMLButtonElement>(this.doc, 'restore-procedural-btn').addEventListener(
      'click',
      () => {
        this.engine?.resetProceduralTextures();
        this.importedSha256Hex = null;
        void this.store.clearImportedAsset().then(() => {
          this.updateAssetBanner('Restored clean-room procedural assets.');
          if (this.paused) this.stepOneFrame(0.0);
        });
      },
    );

    // Export / Import JSON settings
    byId<HTMLButtonElement>(this.doc, 'export-settings-btn').addEventListener(
      'click',
      () => {
        const json = this.store.exportSettingsJson(this.settings);
        const blob = new Blob([json], { type: 'application/json' });
        const url = URL.createObjectURL(blob);
        const a = this.doc.createElement('a');
        a.href = url;
        a.download = 'osdbits-settings.json';
        a.click();
        URL.revokeObjectURL(url);
        this.updateStorageStatusBanner('Exported osdbits-settings.json.');
      },
    );

    byId<HTMLInputElement>(this.doc, 'import-settings-input').addEventListener(
      'change',
      (ev) => {
        const input = ev.currentTarget as HTMLInputElement;
        const file = input.files?.[0];
        if (!file) return;
        void file.text().then(async (txt) => {
          const res = await this.store.importSettingsJson(txt);
          if (res.ok) {
            this.settings = res.settings;
            this.syncHudControlsFromSettings();
            this.applySettingsToRuntime(true);
            this.updateStorageStatusBanner('Imported settings JSON & verified.');
          } else {
            this.updateStorageStatusBanner(`Import error: ${res.error}`);
          }
        });
        input.value = '';
      },
    );

    byId<HTMLButtonElement>(this.doc, 'persist-storage-btn').addEventListener(
      'click',
      () => {
        void this.store.queryDurability(true).then((rep) => {
          this.durability = rep;
          this.updateStorageStatusBanner(
            rep.persistedGranted
              ? 'Durable storage granted by browser.'
              : 'Browser declined durable storage upgrade (best-effort IndexedDB active).',
          );
        });
      },
    );

    // Two-step confirmed factory reset
    const resetBtn = byId<HTMLButtonElement>(this.doc, 'factory-reset-btn');
    const confirmResetBtn = byId<HTMLButtonElement>(
      this.doc,
      'confirm-factory-reset-btn',
    );
    resetBtn.addEventListener('click', () => {
      confirmResetBtn.hidden = !confirmResetBtn.hidden;
    });
    confirmResetBtn.addEventListener('click', () => {
      confirmResetBtn.hidden = true;
      void this.store.resetAllConfirmed(FACTORY_RESET_TOKEN).then((res) => {
        if (res.ok) {
          this.settings = structuredClone(FAITHFUL_DEFAULT_SETTINGS);
          this.engine?.resetProceduralTextures();
          this.importedSha256Hex = null;
          this.syncHudControlsFromSettings();
          this.applySettingsToRuntime(true);
          this.updateAssetBanner();
          this.updateStorageStatusBanner('Factory reset completed & verified.');
        }
      });
    });

    // Global keyboard controls
    this.doc.addEventListener('keydown', (ev: KeyboardEvent) => {
      const target = ev.target as HTMLElement | null;
      const inTextInput =
        target instanceof HTMLInputElement ||
        target instanceof HTMLSelectElement ||
        target instanceof HTMLTextAreaElement;
      if (inTextInput) return;

      if (!this.started) {
        if (ev.key === 'Enter' || ev.key === ' ') {
          ev.preventDefault();
          void this.startFromUserGesture();
        }
        return;
      }

      if (ev.key === 'h' || ev.key === 'H') {
        ev.preventDefault();
        this.setHudCollapsed(!this.settings.hudCollapsed, true);
        return;
      }

      const keyMap: Record<string, MenuAction | undefined> = {
        ArrowUp: MenuAction.Up,
        ArrowDown: MenuAction.Down,
        ArrowLeft: MenuAction.Left,
        ArrowRight: MenuAction.Right,
        Enter: MenuAction.Confirm,
        Escape: MenuAction.Cancel,
        Backspace: MenuAction.Cancel,
        v: MenuAction.VersionInfo,
        V: MenuAction.VersionInfo,
      };
      const act = keyMap[ev.key];
      if (act !== undefined) {
        ev.preventDefault();
        if (this.lastTelemetry?.mode !== AppMode.Menu) {
          this.settings.preferredMode = AppMode.Menu;
          this.engine?.setMode(AppMode.Menu, true);
          this.audio.setSceneMode(AppMode.Menu);
        }
        this.triggerMenuAction(act);
      }
    });

    if (typeof ResizeObserver !== 'undefined') {
      this.resizeObserver = new ResizeObserver(() => {
        this.syncViewportSize();
        if (this.paused) this.stepOneFrame(0.0);
      });
      this.resizeObserver.observe(byId<HTMLElement>(this.doc, 'viewport-stage'));
    } else if (typeof window !== 'undefined') {
      window.addEventListener('resize', () => this.syncViewportSize());
    }
  }

  private async switchRendererLive(forceCanvas: boolean): Promise<void> {
    this.renderer?.dispose();
    const res = await createOsdbitsRenderer({
      canvas: this.canvas,
      forceCanvasFallback: forceCanvas,
      onDeviceLost: (reason, message) => {
        void this.handleGpuDeviceLost(reason, message);
      },
    });
    this.canvas = res.canvas;
    this.bindCanvasPointerInput(this.canvas);
    this.renderer = res.renderer;
    this.syncViewportSize();
    this.updateTopBadges();
    if (this.paused) {
      this.stepOneFrame(0.0);
    }
  }

  public async importAssetBytes(
    fileName: string,
    bytes: Uint8Array,
  ): Promise<{ ok: boolean; message: string }> {
    if (!this.engine) {
      return { ok: false, message: 'Start OSDSYS before importing assets.' };
    }
    const res = this.engine.importContainer(bytes);
    if (!res.ok) {
      this.updateAssetBanner(`Import rejected: ${res.message}`);
      return { ok: false, message: res.message };
    }
    this.importedSha256Hex = res.sha256Hex;
    const saved = await this.store.saveImportedAsset({
      fileName,
      bytes,
      sha256Hex: res.sha256Hex,
      importedTextureCount: res.importedTextureCount,
    });
    const msg = saved.ok
      ? `Imported ${res.importedTextureCount} textures (${res.sha256Hex.slice(0, 12)}…) · Verified`
      : `Imported in session (${saved.error ?? 'storage unverified'})`;
    this.updateAssetBanner(msg);
    if (this.paused) this.stepOneFrame(0.0);
    return { ok: true, message: msg };
  }

  public triggerMenuAction(action: MenuAction): void {
    if (!this.engine) return;
    if (this.lastTelemetry?.mode !== AppMode.Menu) {
      this.engine.setMode(AppMode.Menu, true);
    }
    const cues = this.engine.dispatchMenuAction(action);
    if (cues !== 0) {
      this.audio.triggerCues(cues);
    }
    if (this.paused) {
      this.stepOneFrame(0.0);
    }
  }

  private syncHudControlsFromSettings(): void {
    const s = this.settings;
    this.highlightActiveModeButton(s.preferredMode);
    byId<HTMLSelectElement>(this.doc, 'boot-policy-select').value = String(
      s.bootCompletionPolicy,
    );
    byId<HTMLInputElement>(this.doc, 'seed-input').value = String(s.seed);
    byId<HTMLInputElement>(this.doc, 'speed-range').value = String(
      s.tuning.speedScale,
    );
    byId<HTMLElement>(this.doc, 'speed-val').textContent =
      s.tuning.speedScale.toFixed(2);
    byId<HTMLInputElement>(this.doc, 'drift-range').value = String(
      s.tuning.idleDrift,
    );
    byId<HTMLElement>(this.doc, 'drift-val').textContent =
      s.tuning.idleDrift.toFixed(2);
    byId<HTMLInputElement>(this.doc, 'towers-range').value = String(
      s.tuning.towerGames,
    );
    byId<HTMLElement>(this.doc, 'towers-val').textContent = String(
      s.tuning.towerGames,
    );
    byId<HTMLInputElement>(this.doc, 'res-scale-range').value = String(
      s.resolutionScale,
    );
    byId<HTMLElement>(this.doc, 'res-val').textContent =
      s.resolutionScale.toFixed(2);
    byId<HTMLSelectElement>(this.doc, 'aspect-select').value = String(
      s.tuning.screenSize,
    );
    byId<HTMLSelectElement>(this.doc, 'video-std-select').value = s.tuning.isPal
      ? 'pal'
      : 'ntsc';
    byId<HTMLSelectElement>(this.doc, 'language-select').value = String(
      s.tuning.language,
    );

    byId<HTMLInputElement>(this.doc, 'chk-fog').checked = s.tuning.fogEnabled;
    byId<HTMLInputElement>(this.doc, 'chk-refraction').checked =
      s.tuning.refractionEnabled;
    byId<HTMLInputElement>(this.doc, 'chk-bloom').checked = s.tuning.bloomEnabled;
    byId<HTMLInputElement>(this.doc, 'chk-banner').checked = s.tuning.showBanner;
    byId<HTMLInputElement>(this.doc, 'chk-reduced-motion').checked =
      s.tuning.reducedMotion;
    byId<HTMLInputElement>(this.doc, 'chk-confirm-circle').checked =
      s.tuning.confirmWithCircle;
    byId<HTMLInputElement>(this.doc, 'chk-mute').checked = s.audio.muted;
    byId<HTMLInputElement>(this.doc, 'chk-canvas2d').checked =
      s.forceCanvasFallback;
    byId<HTMLInputElement>(this.doc, 'master-vol-range').value = String(
      s.audio.masterVolume,
    );
  }

  private highlightActiveModeButton(mode: AppMode): void {
    this.doc.querySelectorAll<HTMLButtonElement>('.mode-btn').forEach((btn) => {
      const m = Number(btn.dataset['mode'] ?? -1);
      btn.setAttribute('aria-pressed', m === mode ? 'true' : 'false');
    });
  }

  private updateTopBadges(): void {
    const mode = this.lastTelemetry?.mode ?? this.settings.preferredMode;
    byId<HTMLElement>(this.doc, 'badge-active-mode').textContent =
      MODE_LABELS[mode];

    const fidelityEl = byId<HTMLElement>(this.doc, 'badge-fidelity');
    const isBrowserApprox =
      mode === AppMode.Menu &&
      this.lastTelemetry?.menuPage === MenuPage.BrowserShowcase;
    const isCanvasFallback = this.renderer?.backend === 'canvas2d';
    if (isBrowserApprox || isCanvasFallback) {
      fidelityEl.textContent = isCanvasFallback
        ? 'Approximate · Canvas 2D'
        : 'Approximate · Browser Grid';
      fidelityEl.className = 'badge badge-approx';
    } else {
      fidelityEl.textContent = 'Verified Reconstruction';
      fidelityEl.className = 'badge badge-verified';
    }

    const rendererEl = byId<HTMLElement>(this.doc, 'badge-renderer');
    if (!this.renderer) {
      rendererEl.textContent = 'Pending';
    } else if (this.renderer.backend === 'canvas2d') {
      rendererEl.textContent = 'Canvas 2D Software';
      rendererEl.className = 'badge badge-approx';
    } else if (this.renderer.diagnostics.isFallbackAdapter) {
      rendererEl.textContent = 'WebGPU Software (SwiftShader)';
      rendererEl.className = 'badge badge-approx';
    } else {
      rendererEl.textContent = 'WebGPU Hardware';
      rendererEl.className = 'badge badge-verified';
    }
  }

  private updateAssetBanner(customMessage?: string): void {
    const el = byId<HTMLElement>(this.doc, 'asset-status-banner');
    if (customMessage) {
      el.textContent = customMessage;
      return;
    }
    if (this.engine?.usingImportedAssets && this.importedSha256Hex) {
      el.textContent = `Assets: Imported Container (${this.importedSha256Hex.slice(0, 16)}…)`;
    } else {
      el.textContent = 'Assets: Clean-Room Procedural (15 slots)';
    }
  }

  private updateStorageStatusBanner(customMessage?: string): void {
    const el = byId<HTMLElement>(this.doc, 'storage-status-banner');
    if (customMessage) {
      el.textContent = `Storage: ${customMessage}`;
      return;
    }
    if (this.durability?.usingMemoryFallback) {
      el.textContent = `Storage: Session Memory (${this.durability.degradedReason ?? 'IndexedDB unavailable'})`;
    } else {
      el.textContent = `Storage: IndexedDB r${this.store.currentRevision}${
        this.durability?.persistedGranted ? ' · Durable' : ''
      }`;
    }
  }

  private updateTelemetryDom(tel: FrameTelemetrySnapshot): void {
    if (tel.frameCount % 3 !== 0 && !this.paused) return;
    byId<HTMLElement>(this.doc, 'tel-frame').textContent = String(tel.frameCount);
    byId<HTMLElement>(this.doc, 'tel-camz').textContent = tel.camPos[2].toFixed(1);
    byId<HTMLElement>(this.doc, 'tel-verts').textContent = String(tel.vertexCount);
    byId<HTMLElement>(this.doc, 'tel-batches').textContent = String(tel.batchCount);
    byId<HTMLElement>(this.doc, 'tel-audio').textContent =
      this.audio.snapshot().state;
    const perf = this.computePerfSummary();
    byId<HTMLElement>(this.doc, 'tel-ms').textContent = perf.meanMs.toFixed(2);
  }

  private computePerfSummary(): FramePerfSummary {
    const n = this.frameTimesMs.length;
    if (n === 0) {
      return { sampleCount: 0, meanMs: 0, p95Ms: 0, maxMs: 0 };
    }
    const sorted = [...this.frameTimesMs].sort((a, b) => a - b);
    const sum = sorted.reduce((acc, v) => acc + v, 0);
    const p95Idx = Math.min(n - 1, Math.floor(n * 0.95));
    return {
      sampleCount: n,
      meanMs: sum / n,
      p95Ms: sorted[p95Idx] ?? 0,
      maxMs: sorted[n - 1] ?? 0,
    };
  }

  // =========================================================================
  // Deterministic Inspection & Test Hooks (`window.__OSDBITS_APP__`)
  // =========================================================================

  public getDiagnostics(): AppDiagnosticsSnapshot {
    return {
      started: this.started,
      paused: this.paused,
      renderer: this.renderer?.diagnostics ?? null,
      audio: this.audio.snapshot(),
      durability: this.durability,
      persistenceRevision: this.store.currentRevision,
      lastSaveReadBackVerified: this.lastSaveVerified,
      usingImportedAssets: this.engine?.usingImportedAssets ?? false,
      importedSha256Hex: this.importedSha256Hex,
      settings: structuredClone(this.settings),
      telemetry: this.lastTelemetry ? structuredClone(this.lastTelemetry) : null,
      perf: this.computePerfSummary(),
    };
  }

  public async renderDeterministicFrame(
    req: DeterministicCaptureRequest,
  ): Promise<PixelReadback> {
    if (!this.engine || !this.renderer) {
      throw new Error('Application must be started before capturing frames.');
    }
    this.paused = true;
    this.clockLockedForTest = true;
    const fixedDate = req.rtcIso
      ? new Date(req.rtcIso)
      : new Date('2026-10-31T21:45:12.000Z');
    this.engine.setRtc(fixedDate, true);
    this.engine.setSeed(req.seed >>> 0);
    this.engine.setMode(req.mode, true);

    const steps = Math.max(1, Math.floor(req.framesToAdvance));
    let tel = this.engine.stepAndRender(1.0);
    for (let i = 1; i < steps; i++) {
      tel = this.engine.stepAndRender(1.0);
    }
    this.lastTelemetry = tel;
    this.renderer.renderFrame(this.engine, tel);
    return this.renderer.readPixels();
  }

  public async readCurrentPixels(): Promise<PixelReadback> {
    if (!this.renderer) {
      throw new Error('Renderer is not initialized.');
    }
    return this.renderer.readPixels();
  }

  public async importSyntheticTeximage(
    seed = 0xc0ffee,
  ): Promise<{ ok: boolean; message: string; sha256Hex: string | null }> {
    if (!this.engine) {
      throw new Error('Application must be started first.');
    }
    const syntheticBytes = this.engine.buildSyntheticTeximage(seed);
    const res = await this.importAssetBytes('SYNTHETIC_TEXIMAGE.BIN', syntheticBytes);
    return { ...res, sha256Hex: this.importedSha256Hex };
  }

  public async simulateDeviceLoss(): Promise<void> {
    await this.handleGpuDeviceLost('simulated', 'Simulated WebGPU device loss test');
  }

  public dispose(): void {
    this.disposed = true;
    if (this.rafId !== 0) {
      cancelAnimationFrame(this.rafId);
      this.rafId = 0;
    }
    this.resizeObserver?.disconnect();
    this.renderer?.dispose();
    this.engine?.dispose();
    this.audio.dispose();
    this.store.dispose();
  }
}
