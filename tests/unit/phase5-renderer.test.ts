import { describe, expect, it } from "vitest";
import { SHADER_SOURCES, type ShaderModuleName } from "../../web/src/renderer/shaders";
import { validateWgslSource } from "../../web/src/renderer/validateWgsl";
import {
  RENDERING_OWNERSHIP_CONTRACT,
  WebGpuRenderer,
  computeAspectViewport,
  ps2BlendStateForMode,
} from "../../web/src/renderer/webgpuRenderer";

describe("Phase 5 — WebGPU Renderer Core & WGSL Shader Validation", () => {
  it("validates all 7 PS2-GS WGSL shader modules via offline naga-wasm", async () => {
    const names = Object.keys(SHADER_SOURCES) as ShaderModuleName[];
    expect(names).toEqual([
      "tower",
      "fogSprite",
      "lineTrail",
      "glassCube",
      "menuRodCube",
      "blitPost",
      "uiOverlay",
    ]);

    for (const name of names) {
      const res = await validateWgslSource(SHADER_SOURCES[name]);
      expect(res.valid, `WGSL module ${name} failed naga validation: ${res.error ?? ""}`).toBe(
        true,
      );
    }
  });

  it("maps all 11 PS2 GS BlendModes[0..10] + opaque (255) accurately while writing As -> Ad", () => {
    expect(RENDERING_OWNERSHIP_CONTRACT.simulationOwner).toBe("rust-wasm-osdbits-core");
    expect(RENDERING_OWNERSHIP_CONTRACT.rasterizationOwner).toBe("typescript-webgpu-renderer");

    expect(ps2BlendStateForMode(false, 4)).toBeUndefined();
    expect(ps2BlendStateForMode(true, 255)).toBeUndefined();

    for (let mode = 0; mode <= 10; mode++) {
      const bs = ps2BlendStateForMode(true, mode);
      expect(bs).toBeDefined();
      // Every PS2 ALPHA_1 mode writes fragment As into destination alpha Ad
      expect(bs?.alpha).toEqual({
        srcFactor: "one",
        dstFactor: "zero",
        operation: "add",
      });
    }

    // Spot-check critical glass refraction & specular modes:
    // Mode 4: Cs*As + Cd*(1-As)
    expect(ps2BlendStateForMode(true, 4)?.color).toEqual({
      srcFactor: "src-alpha",
      dstFactor: "one-minus-src-alpha",
      operation: "add",
    });
    // Mode 5: Cs*As + Cd (additive)
    expect(ps2BlendStateForMode(true, 5)?.color).toEqual({
      srcFactor: "src-alpha",
      dstFactor: "one",
      operation: "add",
    });
    // Mode 8: Cs*Ad + Cd (TEXOREF * TEXOBLP destination alpha!)
    expect(ps2BlendStateForMode(true, 8)?.color).toEqual({
      srcFactor: "dst-alpha",
      dstFactor: "one",
      operation: "add",
    });
    // Mode 9: Cd - Cs*Ad (reverse-subtract destination alpha)
    expect(ps2BlendStateForMode(true, 9)?.color).toEqual({
      srcFactor: "dst-alpha",
      dstFactor: "one",
      operation: "reverse-subtract",
    });
  });

  it("computes aspect-preserving 4:3, 16:9, and fit presentation viewports accurately", () => {
    // 1920x1080 displaying 4:3 => pillarboxed 1440x1080 at x=240
    expect(computeAspectViewport(1920, 1080, "4:3")).toEqual({
      x: 240,
      y: 0,
      width: 1440,
      height: 1080,
    });

    // 1280x1024 (5:4) displaying 16:9 => letterboxed 1280x720 at y=152
    expect(computeAspectViewport(1280, 1024, "16:9")).toEqual({
      x: 0,
      y: 152,
      width: 1280,
      height: 720,
    });

    // fit stretches across full canvas
    expect(computeAspectViewport(1920, 1080, "fit")).toEqual({
      x: 0,
      y: 0,
      width: 1920,
      height: 1080,
    });
  });

  it("detects missing navigator.gpu gracefully without unhandled crashes", async () => {
    expect(WebGpuRenderer.isWebGpuAvailable()).toBe(false);
    const dummyCanvas = document.createElement("canvas");
    await expect(WebGpuRenderer.create({ canvas: dummyCanvas })).rejects.toThrow(
      /WebGPU is not available/,
    );
  });
});
