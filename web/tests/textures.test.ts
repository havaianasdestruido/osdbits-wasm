import { describe, expect, it } from "vitest";
import { TEX } from "../src/abi.js";
import { createTextureSet, mulberry32, textureDigest } from "../src/textures.js";
import { CUBE_FACE_NORMAL, CUBE_FACE_VERTS } from "../src/cubeGeometry.js";

describe("procedural textures", () => {
  const set = createTextureSet();

  it("covers every texture id the core can emit", () => {
    const ids = set.map((t) => t.id);
    for (const id of [TEX.WALL, TEX.FOG0, TEX.FOG1, TEX.FOG2, TEX.FOG3, TEX.FOG4, TEX.FLAR, TEX.CRBL, TEX.REF]) {
      expect(ids).toContain(id);
    }
  });

  it("has well-formed RGBA buffers", () => {
    for (const t of set) {
      expect(t.data.length).toBe(t.width * t.height * 4);
      expect(t.width).toBeGreaterThan(0);
      expect(t.height).toBeGreaterThan(0);
    }
  });

  it("is deterministic across builds", () => {
    const again = createTextureSet();
    for (let i = 0; i < set.length; i++) {
      expect(textureDigest(again[i])).toBe(textureDigest(set[i]));
    }
  });

  it("is no longer completely black (a regression guard for the generators)", () => {
    const wall = set.find((t) => t.id === TEX.WALL);
    expect(wall).toBeDefined();
    let sum = 0;
    for (let i = 0; i < wall!.data.length; i += 4) sum += wall!.data[i];
    expect(sum / (wall!.data.length / 4)).toBeGreaterThan(8);
  });

  it("mulberry32 is reproducible", () => {
    const a = mulberry32(7);
    const b = mulberry32(7);
    for (let i = 0; i < 10; i++) expect(a()).toBe(b());
  });
});

describe("cube geometry tables", () => {
  it("indexes eight distinct corners", () => {
    const used = new Set<number>();
    for (const face of CUBE_FACE_VERTS) for (const v of face) used.add(v);
    expect(used.size).toBe(8);
  });

  it("has six axis-aligned normals", () => {
    expect(CUBE_FACE_NORMAL).toHaveLength(6);
    for (const n of CUBE_FACE_NORMAL) {
      expect(Math.abs(n[0]) + Math.abs(n[1]) + Math.abs(n[2])).toBe(1);
    }
  });
});
