/**
 * Integration test against the real wasm module.
 *
 * This is the only test in this folder that executes the compiled core, so it
 * is also the one that catches ABI drift. It needs
 * `web/public/osdbits_core.wasm`, which `scripts/build-wasm.sh` produces; the
 * file is a build artefact and is not committed. If it is missing the tests are
 * *skipped and reported*, never silently green.
 */

import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";
import { BLEND, HEADER_WORDS, PRIM, STATE, TEX } from "../src/abi.js";
import { parseFrame } from "../src/frame.js";
import { loadCore, type Core } from "../src/wasm.js";

const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = resolve(here, "../public/osdbits_core.wasm");
const haveWasm = existsSync(wasmPath);

if (!haveWasm) {
  // Visible in the test log: a skipped suite is not a passing suite.
  console.warn(
    `[wasm.parity] skipping: ${wasmPath} not found — run "bash scripts/build-wasm.sh" first`,
  );
}

describe.skipIf(!haveWasm)("wasm ABI parity", () => {
  let core: Core;

  beforeAll(async () => {
    core = await loadCore(readFileSync(wasmPath).buffer as ArrayBuffer);
  });

  it("reports a compatible ABI version", () => {
    expect(core.exports.memory).toBeInstanceOf(WebAssembly.Memory);
    expect(core.fps()).toBe(60);
  });

  it("produces frames the TypeScript parser accepts", () => {
    core.init(0, 4242);
    for (let i = 0; i < 30; i++) core.step();
    const frame = core.draw();
    expect(frame.width).toBe(640);
    expect(frame.height).toBe(224);
    expect(frame.batches.length).toBeGreaterThan(0);
    expect(frame.vertices.length).toBe(frame.stats.vertices * 10);
    // Towers are the first batch: triangles, wall texture, opaque.
    expect(frame.batches[0]).toMatchObject({ prim: PRIM.TRIANGLES, tex: TEX.WALL, blend: BLEND.NONE });
    // Then the fog layers.
    expect(frame.batches[1].blend).toBe(BLEND.ADD);
    expect(frame.cubeAfterBatch).toBeGreaterThan(0);
  });

  it("replays deterministically", () => {
    const hashOf = (): bigint => {
      core.init(0, 4242);
      for (let i = 0; i < 120; i++) {
        core.step();
        core.draw();
      }
      return core.hash();
    };
    const a = hashOf();
    const b = hashOf();
    expect(a).toBe(b);
    expect(a).not.toBe(0n);
  });

  it("sticks to a bounded draw list", () => {
    core.init(0, 7);
    let maxVerts = 0;
    for (let i = 0; i < 400; i++) {
      core.step();
      const frame = core.draw();
      maxVerts = Math.max(maxVerts, frame.stats.vertices);
      expect(frame.stats.batches).toBeLessThan(64);
      expect(frame.stats.vertices).toBeLessThan(60000);
    }
    expect(maxVerts).toBeGreaterThan(0);
  });

  it("ends the boot scene and keeps idle running", () => {
    core.init(0, 1);
    let frames = 0;
    while (core.state(STATE.ENDED) !== 1 && frames < 4000) {
      core.step();
      frames++;
    }
    expect(core.state(STATE.ENDED)).toBe(1);
    expect(frames).toBeLessThan(4000);

    core.init(1, 1);
    for (let i = 0; i < 600; i++) core.step();
    expect(core.state(STATE.ENDED)).toBe(0);
    expect(core.state(STATE.OPENING_STATE)).toBe(0);
  });

  it("accepts options and reports them back", () => {
    expect(core.setOption(0, 1)).toBe(true); // PAL
    core.init(0, 1);
    expect(core.state(STATE.HEIGHT)).toBe(256);
    expect(core.fps()).toBe(50);
    expect(core.setOption(0, 0)).toBe(true);
    core.init(0, 1);
    expect(core.state(STATE.HEIGHT)).toBe(224);
    expect(core.setOption(1, 1000)).toBe(false); // games out of range
  });

  it("hands back the cube pass table the renderer mirrors", () => {
    const table = core.passTable();
    expect(table).toHaveLength(240);
    expect(table[4]).toBe(122); // opening pass 0 blend fix
    expect(table[120 + 10]).toBeCloseTo(0.8, 6); // illegal pass 0 uv zoom
    expect(table[120 + 24 + 7]).toBe(4); // illegal pass 2 colour mode
  });

  it("reports core errors instead of trapping silently", () => {
    // osd_init with an out-of-range mode returns a code, exposed as a throw.
    expect(() => core.init(99, 1)).toThrow(/failed with code/);
    expect(core.lastError()).toContain("unknown mode");
    core.init(0, 1);
  });

  it("round-trips events", () => {
    core.init(0, 3);
    let events = 0;
    for (let i = 0; i < 400; i++) {
      core.step();
      events += core.events().length;
    }
    expect(events).toBeGreaterThan(0);
  });

  it("emits a header-only frame for the menu mode", () => {
    core.init(3, 1);
    core.step();
    const raw = core.draw();
    expect(raw.stats.batches).toBe(0);
    expect(raw.words.length).toBe(HEADER_WORDS);
    expect(() => parseFrame(raw.words)).not.toThrow();
  });
});
