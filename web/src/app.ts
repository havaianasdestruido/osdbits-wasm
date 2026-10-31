/**
 * The app layer: DOM, persistence and the single-activation boot flow.
 *
 * One click — "Start OSDSYS" — must be enough. Inside that gesture we resume
 * the `AudioContext` (browsers only allow this from a gesture), then
 * everything else follows without a second button: settings load, WebGPU
 * initialisation, the wasm core, and the opening scene.
 *
 * Nothing here fakes capability: if WebGPU is missing the compatibility
 * renderer takes over and the UI says so; if storage is unavailable it says
 * settings will not persist.
 */

import { AudioEngine } from "./audio.js";
import { Engine, type EngineStatus } from "./engine.js";
import { effectiveCameraMotion, normalizeSettings, type SceneMode, type Settings } from "./settings.js";
import { Repository, openStore, type KeyValueStore } from "./storage.js";
import { loadCore, type Core } from "./wasm.js";
import { describeFrame, type Frame } from "./frame.js";
import { STATE } from "./abi.js";
import { MENU_ITEMS, close as closeMenuState, move as moveMenu, open as openMenuState } from "./menu.js";

type FieldDef =
  | {
      key: keyof Settings;
      label: string;
      kind: "select";
      options: readonly { value: string; label: string }[];
      note?: string;
    }
  | { key: keyof Settings; label: string; kind: "checkbox"; note?: string }
  | {
      key: keyof Settings;
      label: string;
      kind: "range";
      min: number;
      max: number;
      step: number;
      note?: string;
    }
  | {
      key: keyof Settings;
      label: string;
      kind: "number";
      min: number;
      max: number;
      note?: string;
    };

const FIELDS: readonly FieldDef[] = [
  {
    key: "scene",
    label: "Scene",
    kind: "select",
    options: [
      { value: "boot", label: "Opening / towers (A)" },
      { value: "idle", label: "Idle field (B)" },
      { value: "illegal", label: "Illegal disc (C)" },
      { value: "menu", label: "OSDSYS-style menu (D)" },
    ],
    note: "The menu is a UI reconstruction, not a full OSDSYS browser.",
  },
  {
    key: "quality",
    label: "Render quality",
    kind: "select",
    options: [
      { value: "low", label: "Low" },
      { value: "medium", label: "Medium" },
      { value: "high", label: "High" },
    ],
  },
  { key: "renderScale", label: "Resolution scale", kind: "range", min: 0.5, max: 2, step: 0.05 },
  { key: "animationSpeed", label: "Animation speed", kind: "range", min: 0.25, max: 2, step: 0.05 },
  { key: "fpsCap", label: "Frame cap (0 = scene rate)", kind: "number", min: 0, max: 240 },
  { key: "seed", label: "Tower seed", kind: "number", min: 0, max: 4294967295 },
  { key: "games", label: "Simulated titles", kind: "number", min: 0, max: 64 },
  { key: "boots", label: "Simulated boots", kind: "number", min: 0, max: 1000 },
  { key: "pal", label: "PAL timings (50 Hz, 640x256)", kind: "checkbox" },
  { key: "fullField", label: "Light the whole field", kind: "checkbox" },
  { key: "fog", label: "Fog layers", kind: "checkbox" },
  { key: "lights", label: "Light streaks", kind: "checkbox" },
  { key: "cubes", label: "Refractive cubes", kind: "checkbox" },
  { key: "text", label: "Text overlays", kind: "checkbox" },
  { key: "blackBars", label: "16:9 letterbox bars", kind: "checkbox" },
  { key: "cameraMotion", label: "Camera motion", kind: "checkbox" },
  {
    key: "reducedMotion",
    label: "Reduced motion (hold the camera)",
    kind: "checkbox",
    note: "Honours the system preference as well.",
  },
  { key: "audio", label: "Audio", kind: "checkbox" },
  { key: "volume", label: "Volume", kind: "range", min: 0, max: 1, step: 0.05 },
  { key: "debugOverlay", label: "Show diagnostics while running", kind: "checkbox" },
];

const $ = <T extends HTMLElement>(id: string): T => {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing element #${id}`);
  return el as T;
};

export class App {
  private settings: Settings = normalizeSettings(null);
  private repository: Repository | null = null;
  private store: KeyValueStore | null = null;
  private core: Core | null = null;
  private engine: Engine | null = null;
  private readonly audio = new AudioEngine();
  private started = false;
  private storageNote = "";
  private lastFrame: Frame | null = null;
  private menuOpen = false;
  private menuIndex = 0;

  private readonly startOverlay = $("start");
  private readonly startButton = $<HTMLButtonElement>("startBtn");
  private readonly capabilitiesLine = $("capabilities");
  private readonly statusLine = $("status");
  private readonly banner = $("banner");
  private readonly bannerText = $("banner-text");
  private readonly canvas = $<HTMLCanvasElement>("canvas");
  private readonly textSce = $("text-sce");
  private readonly textIllegal = $("text-illegal");
  private readonly diagnostics = $("diagnostics");
  private readonly diagnosticsList = $<HTMLDListElement>("diagnostics-list");
  private readonly settingsPanel = $("settings");
  private readonly settingsForm = $<HTMLFormElement>("settings-form");
  private readonly settingsStatus = $("settings-status");
  private readonly menu = $("menu");
  private readonly menuItems = $<HTMLUListElement>("menu-items");

  async boot(): Promise<void> {
    // Machine-readable state for the E2E specs (and for anyone debugging in
    // devtools): body[data-boot] / [data-phase] / [data-renderer] / [data-audio].
    document.body.dataset.boot = "loading";
    document.body.dataset.phase = "idle";
    // Settings and the wasm module load in parallel with the first paint; the
    // click only has to start audio + GPU.
    const storePromise = openStore();
    const corePromise = loadCore(new URL("osdbits_core.wasm", document.baseURI));

    storePromise
      .then((store) => {
        this.store = store;
        this.repository = new Repository(store);
        this.storageNote = store.durable
          ? `settings persist (${store.kind})`
          : "settings will not persist (no storage available)";
        return this.repository.loadSettings();
      })
      .then(async ({ settings }) => {
        const prefersReduced =
          globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
        this.settings = normalizeSettings({ ...settings, reducedMotion: settings.reducedMotion || prefersReduced });
        this.renderSettingsForm();
        this.applySettingsToUi();
        const session = await this.repository?.loadSession();
        if (session && this.repository) {
          await this.repository.saveSession({
            ...session,
            launches: session.launches + 1,
            lastScene: this.settings.scene,
            lastSeen: Date.now(),
          });
        }
      })
      .catch(() => {
        this.storageNote = "settings will not persist (storage error)";
      });

    try {
      this.core = await corePromise;
      this.setCapabilities();
    } catch (error) {
      this.showBanner(`The simulation core failed to load: ${(error as Error).message}`, true);
      this.capabilitiesLine.textContent = "the simulation core is unavailable";
      this.startButton.disabled = true;
      document.body.dataset.boot = "failed";
      return;
    }

    this.startButton.addEventListener("click", () => {
      void this.onStart();
    });
    this.setCapabilities();
    this.wireControls();
    document.body.dataset.boot = "ready";
  }

  /** The single activation gesture. */
  private async onStart(): Promise<void> {
    if (this.started) return;
    this.started = true;
    this.startButton.disabled = true;
    this.startButton.textContent = "Starting…";
    document.body.dataset.phase = "starting";

    // 1. Audio: created *inside* the gesture, before any await.
    if (this.settings.audio) {
      this.audio.setVolume(this.settings.volume);
      void this.audio.start(this.settings.volume).catch((error: unknown) => {
        this.showBanner(`Audio could not start: ${(error as Error).message}`, false);
      });
    }

    try {
      if (!this.core) throw new Error("the simulation core is not loaded");
      // 2. Renderer (WebGPU, falling back to Canvas 2D), 3. the scene.
      const engine = new Engine(this.canvas, this.core, this.audio, this.settings, {
        onStatus: (status) => this.updateStatus(status),
        onFrame: (frame) => this.updateText(frame),
        onError: (message, fatal) => this.showBanner(message, fatal),
      });
      this.engine = engine;
      await engine.initRenderer();
      engine.applySettings(this.settings, false);
      engine.start();
      this.startOverlay.hidden = true;
      $("toolbar").focus?.();
      this.canvas.focus?.();
      if (this.settings.debugOverlay) this.setPanel(this.diagnostics, true);
      if (this.settings.scene === "menu") this.openMenu();
      document.body.dataset.phase = "running";
      document.body.dataset.renderer = engine.rendererKind;
      document.body.dataset.audio = this.audio.state;
    } catch (error) {
      this.started = false;
      this.startButton.disabled = false;
      this.startButton.textContent = "Start OSDSYS";
      document.body.dataset.phase = "error";
      this.showBanner(`Could not start the scene: ${(error as Error).message}`, true);
    }
  }

  private wireControls(): void {
    $("restartBtn").addEventListener("click", () => {
      this.engine?.restartScene();
    });
    $("settingsBtn").addEventListener("click", () => {
      const open = this.settingsPanel.hidden;
      this.setPanel(this.settingsPanel, open);
      if (open) this.renderSettingsForm();
    });
    $("diagnosticsBtn").addEventListener("click", () => {
      this.setPanel(this.diagnostics, this.diagnostics.hidden);
    });
    $("menuBtn").addEventListener("click", () => {
      if (this.menuOpen) this.closeMenu();
      else this.openMenu();
    });
    $("banner-dismiss").addEventListener("click", () => {
      this.banner.hidden = true;
    });
    $("banner-retry").addEventListener("click", () => {
      void this.retry();
    });
    this.settingsForm.addEventListener("submit", (event) => {
      event.preventDefault();
      void this.saveSettingsFromForm();
    });
    $("settings-reset").addEventListener("click", () => {
      void (async () => {
        this.settings = normalizeSettings(null);
        await this.repository?.saveSettings(this.settings);
        this.renderSettingsForm();
        this.applySettingsToUi();
        this.engine?.applySettings(this.settings, true);
        this.settingsStatus.textContent = "defaults restored";
      })();
    });
    globalThis.addEventListener("resize", () => this.engine?.onResize());
    document.addEventListener("visibilitychange", () => {
      if (document.hidden) void this.audio.suspend();
      else if (this.settings.audio && this.audio.started) void this.audio.resume();
    });
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && this.menuOpen) {
        this.closeMenu();
        return;
      }
      if (this.menuOpen && this.handleMenuKey(event)) return;
      if (event.key === "m" && !isTyping(event.target)) this.openMenu();
    });
    this.buildMenu();
  }

  private async retry(): Promise<void> {
    this.banner.hidden = true;
    try {
      if (!this.engine) {
        this.started = false;
        await this.onStart();
        return;
      }
      await this.engine.recover();
    } catch (error) {
      this.showBanner(`Retry failed: ${(error as Error).message}`, true);
    }
  }

  private setPanel(panel: HTMLElement, open: boolean): void {
    panel.hidden = !open;
    const buttonId =
      panel.id === "settings" ? "settingsBtn" : panel.id === "diagnostics" ? "diagnosticsBtn" : "";
    if (buttonId) {
      $(buttonId).setAttribute("aria-expanded", String(open));
    }
  }

  private setCapabilities(): void {
    const gpu =
      typeof (navigator as Navigator & { gpu?: GPU }).gpu !== "undefined"
        ? "WebGPU available"
        : "WebGPU unavailable — the compatibility renderer will be used";
    const audio = AudioEngine.capabilities();
    this.capabilitiesLine.textContent = [
      gpu,
      audio.available ? "audio ready" : audio.reason,
      this.storageNote || "storage: detecting…",
    ].join(" · ");
  }

  private showBanner(message: string, fatal: boolean): void {
    this.bannerText.textContent = message;
    this.banner.hidden = false;
    this.banner.dataset.fatal = fatal ? "true" : "false";
    this.statusLine.textContent = fatal ? "error" : "recovered";
  }

  private updateStatus(status: EngineStatus): void {
    const renderer = status.renderer === "webgpu" ? "WebGPU" : "Canvas 2D (compat)";
    document.body.dataset.renderer = status.renderer;
    document.body.dataset.audio = this.audio.state;
    document.body.dataset.frame = String(status.coreFrame);
    const frame = this.lastFrame;
    document.body.dataset.scene = frame ? String(frame.scene) : "";
    document.body.dataset.cameraHeight = frame ? frame.cameraPos[2].toFixed(2) : "";
    document.body.dataset.batches = frame ? String(frame.stats.batches) : "";
    this.statusLine.textContent =
      `${renderer} · ${status.fps.toFixed(0)} fps · ${status.frameMs.toFixed(1)} ms/frame · ` +
      `frame ${status.coreFrame}${status.ended ? " · ended" : ""}`;

    if (!this.diagnostics.hidden || this.settings.debugOverlay) {
      this.diagnosticsList.textContent = "";
      const rows: [string, string][] = [
        ["renderer", status.renderer === "webgpu" ? "WebGPU" : "Canvas 2D compatibility"],
        ["renderer error", status.rendererError || "none"],
        ["frames per second", status.fps.toFixed(1)],
        ["frame time", `${status.frameMs.toFixed(2)} ms`],
        ["simulation frame", String(status.coreFrame)],
        ["draw list", frame ? describeFrame(frame) : "–"],
        ["scene", frame ? `${frame.scene} / state ${frame.sceneState}` : "–"],
        ["camera height", frame ? frame.cameraPos[2].toFixed(2) : "–"],
        ["frame hash", this.core ? this.core.hash().toString(16) : "–"],
        ["wasm module", this.core ? `${(this.core.bytes / 1024).toFixed(0)} KiB` : "–"],
        ["storage", this.store ? (this.store.durable ? this.store.kind : "not persistent") : "–"],
        [
          "scene end",
          frame?.ended
            ? `yes (frame ${frame.endFrame}, latch ${frame.endFlag})`
            : "no",
        ],
        ["core error buffer", this.core?.lastError() || "empty"],
      ];
      for (const [k, v] of rows) {
        const dt = document.createElement("dt");
        dt.textContent = k;
        const dd = document.createElement("dd");
        dd.textContent = v;
        this.diagnosticsList.append(dt, dd);
      }
    }
  }

  private updateText(frame: Frame): void {
    this.lastFrame = frame;
    const sce = Math.max(0, Math.min(1, frame.textSceAlpha / 112));
    const illegal = Math.max(0, Math.min(1, frame.textIllegalAlpha / 112));
    this.textSce.style.opacity = sce.toFixed(3);
    this.textIllegal.style.opacity = illegal.toFixed(3);
    this.textSce.hidden = !this.settings.text;
    this.textIllegal.hidden = !this.settings.text;
    if (frame.ended && this.settings.scene === "boot") {
      // The core latches the scene end; the field stays on screen (mode B
      // behaviour) until the user restarts or picks another scene.
      void STATE;
    }
  }

  private buildMenu(): void {
    // The model lives in menu.ts (unit-tested); this only renders it.
    this.menuItems.textContent = "";
    MENU_ITEMS.forEach((item, index) => {
      const li = document.createElement("li");
      li.className = "menu-item";
      li.setAttribute("role", "menuitem");
      li.tabIndex = -1;
      li.textContent = item.hint ? `${item.label} — ${item.hint}` : item.label;
      li.dataset.index = String(index);
      li.addEventListener("click", () => {
        this.menuIndex = index;
        this.runMenuAction(item.id);
      });
      this.menuItems.append(li);
    });
  }

  private runMenuAction(id: string): void {
    switch (id) {
      case "scene:boot":
        this.switchScene("boot");
        break;
      case "scene:idle":
        this.switchScene("idle");
        break;
      case "scene:illegal":
        this.switchScene("illegal");
        break;
      case "settings":
        this.setPanel(this.settingsPanel, true);
        this.renderSettingsForm();
        break;
      case "about":
        this.showAbout();
        break;
      default:
        break;
    }
    this.closeMenu();
  }

  private openMenu(): void {
    if (!this.started) {
      // The menu is a UI overlay; it still needs the engine to exist so that
      // choosing a scene can switch it.
      void this.onStart().then(() => this.openMenu());
      return;
    }
    this.menuOpen = openMenuState().open;
    this.menu.hidden = false;
    $("menuBtn").setAttribute("aria-expanded", "true");
    this.menuIndex = 0;
    this.focusMenuItem();
  }

  private closeMenu(): void {
    this.menuOpen = closeMenuState().open;
    this.menu.hidden = true;
    $("menuBtn").setAttribute("aria-expanded", "false");
    $("menuBtn").focus();
  }

  private focusMenuItem(): void {
    const nodes = [...this.menuItems.children] as HTMLElement[];
    nodes.forEach((node, i) => {
      node.setAttribute("aria-selected", String(i === this.menuIndex));
      node.tabIndex = i === this.menuIndex ? 0 : -1;
    });
    nodes[this.menuIndex]?.focus();
  }

  private handleMenuKey(event: KeyboardEvent): boolean {
    const nodes = [...this.menuItems.children] as HTMLElement[];
    if (nodes.length === 0) return false;
    switch (event.key) {
      case "ArrowDown":
        this.menuIndex = moveMenu({ open: true, index: this.menuIndex }, 1).index;
        break;
      case "ArrowUp":
        this.menuIndex = moveMenu({ open: true, index: this.menuIndex }, -1).index;
        break;
      case "Home":
        this.menuIndex = 0;
        break;
      case "End":
        this.menuIndex = nodes.length - 1;
        break;
      case "Enter":
      case " ":
        event.preventDefault();
        nodes[this.menuIndex].click();
        return true;
      default:
        return false;
    }
    event.preventDefault();
    this.focusMenuItem();
    return true;
  }

  private switchScene(scene: SceneMode): void {
    this.settings = { ...this.settings, scene };
    void this.repository?.saveSettings(this.settings);
    this.engine?.applySettings(this.settings, true);
    void this.engine?.setScene(scene);
    this.audio.silence();
    this.updateSceneLabel();
  }

  private updateSceneLabel(): void {
    this.settingsStatus.textContent = `scene: ${this.settings.scene}`;
  }

  private showAbout(): void {
    this.showBanner(
      "osdbits-wasm reconstructs the PS2 OSDSYS opening and illegal-disc scenes from the " +
        "open-source osdbits analysis. It contains no Sony code, artwork, fonts or audio; " +
        "the simulation is deterministic (same seed, same frames).",
      false,
    );
  }

  private renderSettingsForm(): void {
    this.settingsForm.textContent = "";
    for (const field of FIELDS) {
      const wrapper = document.createElement("div");
      wrapper.className = field.kind === "checkbox" ? "field checkbox" : "field";
      const id = `field-${field.key}`;
      const label = document.createElement("label");
      label.htmlFor = id;
      label.textContent = field.label;
      let input: HTMLInputElement | HTMLSelectElement;
      const value = this.settings[field.key];
      if (field.kind === "select") {
        const select = document.createElement("select");
        select.id = id;
        select.name = field.key;
        for (const opt of field.options) {
          const o = document.createElement("option");
          o.value = opt.value;
          o.textContent = opt.label;
          if (String(value) === opt.value) o.selected = true;
          select.append(o);
        }
        input = select;
      } else if (field.kind === "checkbox") {
        const box = document.createElement("input");
        box.type = "checkbox";
        box.id = id;
        box.name = field.key;
        box.checked = Boolean(value);
        input = box;
        wrapper.append(box, label);
      } else if (field.kind === "range") {
        const range = document.createElement("input");
        range.type = "range";
        range.id = id;
        range.name = field.key;
        range.min = String(field.min);
        range.max = String(field.max);
        range.step = String(field.step);
        range.value = String(value);
        input = range;
        wrapper.append(label, range);
      } else {
        const number = document.createElement("input");
        number.type = "number";
        number.id = id;
        number.name = field.key;
        number.min = String(field.min);
        number.max = String(field.max);
        number.value = String(value);
        input = number;
        wrapper.append(label, number);
      }
      if (field.kind !== "checkbox" && field.kind !== "range") wrapper.append(label, input);
      if (field.kind === "select") wrapper.append(label, input);
      if (field.note) {
        const note = document.createElement("span");
        note.className = "note";
        note.textContent = field.note;
        wrapper.append(note);
      }
      this.settingsForm.append(wrapper);
    }
  }

  private async saveSettingsFromForm(): Promise<void> {
    const data: Record<string, unknown> = { ...this.settings };
    for (const field of FIELDS) {
      const el = this.settingsForm.elements.namedItem(field.key as string);
      if (!el) continue;
      if (el instanceof HTMLInputElement) {
        data[field.key as string] =
          el.type === "checkbox"
            ? el.checked
            : el.type === "number" || el.type === "range"
              ? Number(el.value)
              : el.value;
      } else if (el instanceof HTMLSelectElement) {
        data[field.key as string] = el.value;
      }
    }
    this.settings = normalizeSettings(data);
    await this.repository?.saveSettings(this.settings);
    this.engine?.applySettings(this.settings, true);
    this.applySettingsToUi();
    this.settingsStatus.textContent = this.repository?.durable
      ? "saved"
      : "applied (not persisted: no storage)";
    this.setCapabilities();
  }

  private applySettingsToUi(): void {
    this.audio.setVolume(this.settings.volume);
    this.audio.setMuted(!this.settings.audio);
    this.textSce.hidden = !this.settings.text;
    this.textIllegal.hidden = !this.settings.text;
    if (this.settings.debugOverlay) this.setPanel(this.diagnostics, true);
    const motion = effectiveCameraMotion(this.settings);
    document.documentElement.dataset.cameraMotion = motion ? "on" : "off";
    this.setCapabilities();
  }
}

function isTyping(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement
  );
}
