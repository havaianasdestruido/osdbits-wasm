/**
 * The engine: owns the wasm core, the renderer and the fixed-step loop.
 *
 * Responsibilities
 *  - drive the core at a fixed simulation rate (50/60 Hz from the core) and
 *    scale by the user's animation-speed setting;
 *  - hand each frame's draw list to the renderer;
 *  - survive renderer failure by falling back WebGPU -> Canvas 2D without a
 *    page reload, and surface *why* it happened;
 *  - turn core events into audio.
 *
 * It performs no DOM work beyond the canvas: the app layer owns the UI.
 */

import { OPT, MODE_BOOT, MODE_IDLE, MODE_ILLEGAL, STATE } from "./abi.js";
import { Canvas2dRenderer } from "./canvas2d.js";
import type { Frame } from "./frame.js";
import { WebGpuRenderer, type RendererStats } from "./gpu/renderer.js";
import type { Core } from "./wasm.js";
import { createTextureSet, type ProceduralTexture } from "./textures.js";
import {
  effectiveCameraMotion,
  rendererOptions,
  stepsForElapsed,
  type SceneMode,
  type Settings,
} from "./settings.js";
import type { AudioEngine } from "./audio.js";

export type RendererKind = "webgpu" | "canvas2d";

export interface RendererHandle {
  kind: RendererKind;
  draw(frame: Frame): RendererStats;
  resize(width: number, height: number): void;
  destroy(): void;
}

export interface EngineStatus {
  renderer: RendererKind;
  rendererError: string;
  fps: number;
  frameMs: number;
  stats: RendererStats;
  coreFrame: number;
  ended: boolean;
}

export interface EngineCallbacks {
  onStatus(status: EngineStatus): void;
  onFrame(frame: Frame): void;
  onError(message: string, fatal: boolean): void;
}

const MAX_FRAME_MS = 1000 / 15; // never simulate more than ~4 steps per rAF

function modeFor(scene: SceneMode): number {
  switch (scene) {
    case "idle":
      return MODE_IDLE;
    case "illegal":
      return MODE_ILLEGAL;
    case "menu":
      // The menu is a UI overlay on the idle field: the core has no menu
      // geometry (see docs/LIMITATIONS.md).
      return MODE_IDLE;
    default:
      return MODE_BOOT;
  }
}

export class Engine {
  private renderer: RendererHandle | null = null;
  private rendererError = "";
  private settings: Settings;
  private running = false;
  private raf = 0;
  private lastTime = 0;
  private accumulator = 0;
  private frames = 0;
  private fpsTime = 0;
  private fps = 0;
  private frameMs = 0;
  private lastStats: RendererStats = {
    draws: 0,
    triangles: 0,
    batchesSkipped: 0,
    batchesDrawn: 0,
  };
  private lastFrame: Frame | null = null;
  private readonly textures: ProceduralTexture[];
  private generation = 0;

  constructor(
    private readonly canvas: HTMLCanvasElement,
    private readonly core: Core,
    private readonly audio: AudioEngine,
    settings: Settings,
    private readonly callbacks: EngineCallbacks,
  ) {
    this.settings = settings;
    this.textures = createTextureSet();
  }

  get rendererKind(): RendererKind {
    return this.renderer?.kind ?? "canvas2d";
  }

  get lastRenderedFrame(): Frame | null {
    return this.lastFrame;
  }

  /** Build the best renderer available; falls back with a reason. */
  async initRenderer(): Promise<void> {
    const opts = rendererOptions(this.settings);
    try {
      this.renderer = await WebGpuRenderer.create(this.canvas, this.textures, opts);
      this.rendererError = "";
    } catch (error) {
      this.rendererError = (error as Error).message;
      this.renderer = new Canvas2dRenderer(this.canvas, this.textures, opts.cubePasses);
      this.callbacks.onError(
        `WebGPU is unavailable (${this.rendererError}); using the compatibility renderer.`,
        false,
      );
    }
    this.resize();
  }

  private resize(): void {
    if (!this.renderer) return;
    const dpr = Math.min(2, globalThis.devicePixelRatio || 1);
    const opts = rendererOptions(this.settings);
    const rect = this.canvas.getBoundingClientRect();
    const width = Math.max(1, Math.floor((rect.width || 640) * dpr * opts.scale));
    const height = Math.max(1, Math.floor((rect.height || 224) * dpr * opts.scale));
    if (this.canvas.width !== width || this.canvas.height !== height) {
      this.canvas.width = width;
      this.canvas.height = height;
    }
    this.renderer.resize(width, height);
  }

  applySettings(settings: Settings, restart: boolean): void {
    const wasPal = this.settings.pal;
    this.settings = settings;
    this.core.setOption(OPT.PAL, settings.pal ? 1 : 0);
    this.core.setOption(OPT.GAMES, settings.games);
    this.core.setOption(OPT.BOOTS, settings.boots);
    this.core.setOption(OPT.FULL_FIELD, settings.fullField ? 1 : 0);
    this.core.setOption(OPT.HW_STATE, settings.hwState);
    this.core.setOption(OPT.BOOT_PARAM, settings.bootParam);
    this.core.setOption(OPT.FOG, settings.fog ? 1 : 0);
    this.core.setOption(OPT.LIGHTS, settings.lights ? 1 : 0);
    this.core.setOption(OPT.CUBES, settings.cubes ? 1 : 0);
    this.core.setOption(OPT.CAMERA_MOTION, effectiveCameraMotion(settings) ? 1 : 0);
    this.core.setOption(OPT.TEXT, settings.text ? 1 : 0);
    this.core.setOption(OPT.BLACK_BARS, settings.blackBars ? 1 : 0);
    this.audio.setVolume(settings.volume);
    this.audio.setMuted(!settings.audio);
    this.resize();
    if (restart || wasPal !== settings.pal) {
      this.restartScene();
    }
  }

  restartScene(): void {
    this.audio.silence();
    this.initScene();
  }

  private initScene(): void {
    const mode = modeFor(this.settings.scene);
    this.core.init(mode, this.settings.seed >>> 0);
    this.accumulator = 0;
    this.generation++;
  }

  start(): void {
    if (this.running) return;
    this.initScene();
    this.running = true;
    this.lastTime = performance.now();
    this.fpsTime = this.lastTime;
    this.frames = 0;
    this.raf = requestAnimationFrame(this.tick);
  }

  stop(): void {
    this.running = false;
    if (this.raf) cancelAnimationFrame(this.raf);
    this.raf = 0;
  }

  private tick = (now: number): void => {
    if (!this.running) return;
    this.raf = requestAnimationFrame(this.tick);
    const delta = Math.min(MAX_FRAME_MS, Math.max(0, now - this.lastTime));
    this.lastTime = now;

    const fps = this.core.fps();
    this.accumulator += delta;
    const interval = 1000 / fps;
    let steps = 0;
    while (this.accumulator >= interval && steps < 4) {
      this.core.step();
      this.accumulator -= interval;
      steps++;
      for (const event of this.core.events()) this.audio.handleEvent(event.kind, event.arg);
    }
    // When the tab was hidden or the machine is too slow, drop time instead of
    // spiralling; the scene is an animation, not a clock.
    if (steps === 0 && delta > 0) {
      const speedSteps = stepsForElapsed(this.settings, fps, delta);
      for (let i = 0; i < Math.min(speedSteps, 2); i++) this.core.step();
    }

    try {
      const frame = this.core.draw();
      this.lastFrame = frame;
      const stats = this.renderer?.draw(frame) ?? this.lastStats;
      this.lastStats = stats;
      this.callbacks.onFrame(frame);
      const z = this.core.state(STATE.Z);
      if (this.audio.state === "running") this.audio.updateFlyUp(z);
    } catch (error) {
      this.handleFrameError(error);
      return;
    }

    this.frames++;
    if (now - this.fpsTime >= 500) {
      this.fps = (this.frames * 1000) / (now - this.fpsTime);
      this.frames = 0;
      this.fpsTime = now;
    }
    this.frameMs = this.frameMs * 0.9 + delta * 0.1;
    this.callbacks.onStatus({
      renderer: this.rendererKind,
      rendererError: this.rendererError,
      fps: this.fps,
      frameMs: this.frameMs,
      stats: this.lastStats,
      coreFrame: this.core.state(STATE.FRAME),
      ended: this.core.state(STATE.ENDED) === 1,
    });
  };

  /**
   * A failed frame must not kill the page: fall back to the other renderer
   * once, then keep running or stop with a visible error.
   */
  private handleFrameError(error: unknown): void {
    const message = error instanceof Error ? error.message : String(error);
    if (this.renderer?.kind === "webgpu") {
      try {
        this.renderer.destroy();
      } catch {
        /* the device may already be gone */
      }
      this.rendererError = message;
      this.renderer = new Canvas2dRenderer(this.canvas, this.textures, false);
      this.callbacks.onError(
        `The WebGPU renderer failed (${message}); switched to the compatibility renderer.`,
        false,
      );
      return;
    }
    this.stop();
    this.callbacks.onError(`Rendering failed: ${message}`, true);
  }

  /** Rebuild everything after a failure, without a page reload. */
  async recover(): Promise<void> {
    this.stop();
    this.renderer?.destroy();
    this.renderer = null;
    await this.initRenderer();
    this.start();
  }

  /** Handle a window resize. */
  onResize(): void {
    this.resize();
  }

  async setScene(scene: SceneMode): Promise<void> {
    this.settings = { ...this.settings, scene };
    if (this.running) this.restartScene();
  }

  get currentSettings(): Settings {
    return this.settings;
  }

  get sceneGeneration(): number {
    return this.generation;
  }
}

