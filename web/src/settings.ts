/**
 * User settings: the single source of truth for persistence.
 *
 * Every field is validated and normalised here, so a corrupt or older record
 * in IndexedDB can never reach the engine. Migrations are explicit: an unknown
 * schema version is migrated forward (or rejected) rather than silently
 * reinterpreted.
 */

import { HW_STATE } from "./abi.js";

export const SETTINGS_VERSION = 1;

export type SceneMode = "boot" | "idle" | "illegal" | "menu";
export type Quality = "low" | "medium" | "high";

export interface Settings {
  version: number;
  scene: SceneMode;
  pal: boolean;
  seed: number;
  games: number;
  boots: number;
  fullField: boolean;
  hwState: number;
  bootParam: number;
  cameraMotion: boolean;
  reducedMotion: boolean;
  animationSpeed: number;
  fpsCap: number;
  quality: Quality;
  renderScale: number;
  fog: boolean;
  lights: boolean;
  cubes: boolean;
  text: boolean;
  blackBars: boolean;
  audio: boolean;
  volume: number;
  debugOverlay: boolean;
  firstRunDone: boolean;
}

export const DEFAULT_SETTINGS: Settings = {
  version: SETTINGS_VERSION,
  scene: "boot",
  pal: false,
  seed: 4242,
  games: 16,
  boots: 120,
  fullField: false,
  hwState: HW_STATE.NO_DISC,
  bootParam: 100,
  cameraMotion: true,
  reducedMotion: false,
  animationSpeed: 1,
  fpsCap: 0,
  quality: "high",
  renderScale: 1,
  fog: true,
  lights: true,
  cubes: true,
  text: true,
  blackBars: false,
  audio: true,
  volume: 0.6,
  debugOverlay: false,
  firstRunDone: false,
};

export const SCENES: readonly SceneMode[] = ["boot", "idle", "illegal", "menu"];
export const QUALITIES: readonly Quality[] = ["low", "medium", "high"];

function clamp(value: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, value));
}

function num(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function int(value: unknown, fallback: number, lo: number, hi: number): number {
  return clamp(Math.round(num(value, fallback)), lo, hi);
}

function bool(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

/**
 * Coerce an arbitrary value (e.g. `JSON.parse(record)`) into valid settings.
 * Unknown fields are dropped; invalid fields fall back to defaults; never
 * throws.
 */
export function normalizeSettings(raw: unknown): Settings {
  if (typeof raw !== "object" || raw === null) {
    return { ...DEFAULT_SETTINGS };
  }
  const r = raw as Record<string, unknown>;
  const scene = SCENES.includes(r.scene as SceneMode) ? (r.scene as SceneMode) : DEFAULT_SETTINGS.scene;
  const quality = QUALITIES.includes(r.quality as Quality)
    ? (r.quality as Quality)
    : DEFAULT_SETTINGS.quality;

  return {
    version: SETTINGS_VERSION,
    scene,
    pal: bool(r.pal, DEFAULT_SETTINGS.pal),
    seed: int(r.seed, DEFAULT_SETTINGS.seed, 0, 0xffffffff),
    games: int(r.games, DEFAULT_SETTINGS.games, 0, 64),
    boots: int(r.boots, DEFAULT_SETTINGS.boots, 0, 1000),
    fullField: bool(r.fullField, DEFAULT_SETTINGS.fullField),
    hwState: int(r.hwState, DEFAULT_SETTINGS.hwState, 0, 2),
    bootParam: int(r.bootParam, DEFAULT_SETTINGS.bootParam, 100, 116),
    cameraMotion: bool(r.cameraMotion, DEFAULT_SETTINGS.cameraMotion),
    reducedMotion: bool(r.reducedMotion, DEFAULT_SETTINGS.reducedMotion),
    animationSpeed: clamp(num(r.animationSpeed, DEFAULT_SETTINGS.animationSpeed), 0.25, 2),
    fpsCap: int(r.fpsCap, DEFAULT_SETTINGS.fpsCap, 0, 240),
    quality,
    renderScale: clamp(num(r.renderScale, DEFAULT_SETTINGS.renderScale), 0.5, 2),
    fog: bool(r.fog, DEFAULT_SETTINGS.fog),
    lights: bool(r.lights, DEFAULT_SETTINGS.lights),
    cubes: bool(r.cubes, DEFAULT_SETTINGS.cubes),
    text: bool(r.text, DEFAULT_SETTINGS.text),
    blackBars: bool(r.blackBars, DEFAULT_SETTINGS.blackBars),
    audio: bool(r.audio, DEFAULT_SETTINGS.audio),
    volume: clamp(num(r.volume, DEFAULT_SETTINGS.volume), 0, 1),
    debugOverlay: bool(r.debugOverlay, DEFAULT_SETTINGS.debugOverlay),
    firstRunDone: bool(r.firstRunDone, DEFAULT_SETTINGS.firstRunDone),
  };
}

/** The scene mode the engine should start in, honouring reduced motion. */
export function effectiveCameraMotion(s: Settings): boolean {
  return s.cameraMotion && !s.reducedMotion;
}

/** `fpsCap` of 0 means "follow the simulation's 50/60 Hz". */
export function targetFrameIntervalMs(s: Settings, simulationFps: number): number {
  const fps = s.fpsCap > 0 ? s.fpsCap : simulationFps;
  return 1000 / clamp(fps, 1, 240);
}

/** How many simulation steps one wall-clock frame should run. */
export function stepsForElapsed(
  settings: Settings,
  simulationFps: number,
  elapsedMs: number,
): number {
  if (settings.reducedMotion) return 0;
  const interval = targetFrameIntervalMs(settings, simulationFps);
  const speed = clamp(settings.animationSpeed, 0.25, 2);
  return clamp(Math.round((elapsedMs / interval) * speed), 0, 4);
}

/** Renderer options derived from the quality preset. */
export function rendererOptions(s: Settings): {
  scale: number;
  antialias: boolean;
  cubePasses: boolean;
  maxBatches: number;
} {
  const preset =
    s.quality === "low"
      ? { scale: 0.5, antialias: false, cubePasses: false, maxBatches: 16 }
      : s.quality === "medium"
        ? { scale: 0.75, antialias: false, cubePasses: true, maxBatches: 64 }
        : { scale: 1, antialias: true, cubePasses: true, maxBatches: 128 };
  return {
    scale: clamp(preset.scale * s.renderScale, 0.25, 2),
    antialias: preset.antialias,
    cubePasses: preset.cubePasses && s.cubes,
    maxBatches: preset.maxBatches,
  };
}

export const SETTINGS_FIELDS: readonly (keyof Settings)[] = [
  "scene",
  "pal",
  "seed",
  "games",
  "boots",
  "fullField",
  "hwState",
  "bootParam",
  "cameraMotion",
  "reducedMotion",
  "animationSpeed",
  "fpsCap",
  "quality",
  "renderScale",
  "fog",
  "lights",
  "cubes",
  "text",
  "blackBars",
  "audio",
  "volume",
  "debugOverlay",
];
