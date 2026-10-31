import { describe, expect, it } from "vitest";
import {
  DEFAULT_SETTINGS,
  SETTINGS_VERSION,
  effectiveCameraMotion,
  normalizeSettings,
  rendererOptions,
  stepsForElapsed,
  targetFrameIntervalMs,
} from "../src/settings.js";

describe("normalizeSettings", () => {
  it("returns defaults for junk", () => {
    expect(normalizeSettings(null)).toEqual(DEFAULT_SETTINGS);
    expect(normalizeSettings("nonsense")).toEqual(DEFAULT_SETTINGS);
    expect(normalizeSettings(42)).toEqual(DEFAULT_SETTINGS);
  });

  it("keeps valid values", () => {
    const s = normalizeSettings({
      scene: "illegal",
      pal: true,
      seed: 7,
      games: 3,
      boots: 9,
      quality: "low",
      volume: 0.25,
      audio: false,
    });
    expect(s.scene).toBe("illegal");
    expect(s.pal).toBe(true);
    expect(s.seed).toBe(7);
    expect(s.games).toBe(3);
    expect(s.boots).toBe(9);
    expect(s.quality).toBe("low");
    expect(s.volume).toBe(0.25);
    expect(s.audio).toBe(false);
  });

  it("clamps and coerces out-of-range values instead of throwing", () => {
    const s = normalizeSettings({
      scene: "not-a-scene",
      quality: "ultra",
      seed: -5,
      games: 999,
      boots: -3,
      bootParam: 1000,
      volume: 9,
      animationSpeed: 100,
      renderScale: 0,
      fpsCap: -10,
      hwState: 9,
      pal: "yes",
    });
    expect(s.scene).toBe("boot");
    expect(s.quality).toBe("high");
    expect(s.seed).toBe(0);
    expect(s.games).toBe(64);
    expect(s.boots).toBe(0);
    expect(s.bootParam).toBe(116);
    expect(s.volume).toBe(1);
    expect(s.animationSpeed).toBe(2);
    expect(s.renderScale).toBe(0.5);
    expect(s.fpsCap).toBe(0);
    expect(s.hwState).toBe(2);
    expect(s.pal).toBe(false);
  });

  it("stamps the current schema version", () => {
    expect(normalizeSettings({ version: 0 }).version).toBe(SETTINGS_VERSION);
    expect(normalizeSettings({}).version).toBe(SETTINGS_VERSION);
  });

  it("honours the system reduced-motion preference as a separate flag", () => {
    const s = normalizeSettings({ cameraMotion: true, reducedMotion: true });
    expect(effectiveCameraMotion(s)).toBe(false);
    expect(effectiveCameraMotion(normalizeSettings({}))).toBe(true);
  });
});

describe("timing", () => {
  it("derives the frame interval from the cap or the scene rate", () => {
    expect(targetFrameIntervalMs(normalizeSettings({ fpsCap: 0 }), 60)).toBeCloseTo(16.666, 2);
    expect(targetFrameIntervalMs(normalizeSettings({ fpsCap: 0 }), 50)).toBeCloseTo(20, 5);
    expect(targetFrameIntervalMs(normalizeSettings({ fpsCap: 30 }), 60)).toBeCloseTo(33.333, 2);
  });

  it("clamps the number of steps per frame", () => {
    const s = normalizeSettings({ fpsCap: 0, animationSpeed: 1 });
    expect(stepsForElapsed(s, 60, 16.6)).toBe(1);
    expect(stepsForElapsed(s, 60, 1000)).toBe(4);
    expect(stepsForElapsed(s, 60, 0)).toBe(0);
    expect(stepsForElapsed(normalizeSettings({ reducedMotion: true }), 60, 16)).toBe(0);
  });

  it("scales with the animation speed setting", () => {
    const slow = normalizeSettings({ fpsCap: 60, animationSpeed: 0.5 });
    expect(stepsForElapsed(slow, 60, 33)).toBe(1);
  });
});

describe("rendererOptions", () => {
  it("maps quality presets onto scale and passes", () => {
    expect(rendererOptions(normalizeSettings({ quality: "low" }))).toMatchObject({
      scale: 0.5,
      antialias: false,
      cubePasses: false,
    });
    expect(rendererOptions(normalizeSettings({ quality: "medium" })).cubePasses).toBe(true);
    const high = rendererOptions(normalizeSettings({ quality: "high" }));
    expect(high.scale).toBe(1);
    expect(high.antialias).toBe(true);
  });

  it("never exceeds the safe scale range", () => {
    const r = rendererOptions(normalizeSettings({ quality: "high", renderScale: 2 }));
    expect(r.scale).toBeLessThanOrEqual(2);
    expect(r.scale).toBeGreaterThan(0);
  });

  it("drops the cube passes when cubes are disabled", () => {
    expect(rendererOptions(normalizeSettings({ cubes: false })).cubePasses).toBe(false);
  });
});
