import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  DrawCommandKindId,
  EXPECTED_ABI_MAGIC,
  EXPECTED_ABI_VERSION,
  EXPECTED_CMD_WORDS,
  EXPECTED_VERTEX_FLOATS,
  WasmSimulationBridge,
} from "../../web/src/wasm/bridge";

const wasmBytes = readFileSync(
  resolve(process.cwd(), "web/public/wasm/osdbits_wasm.wasm"),
);

describe("Phase 3 — WASM C-ABI & TypeScript WasmSimulationBridge", () => {
  it("instantiates real wasm32 binary and verifies ABI magic, version, and stream strides", async () => {
    const bridge = await WasmSimulationBridge.instantiateFromBytes(wasmBytes);
    expect(bridge.abiMagic).toBe(EXPECTED_ABI_MAGIC);
    expect(bridge.abiVersion).toBe(EXPECTED_ABI_VERSION);
    expect(bridge.memoryByteLength).toBeGreaterThanOrEqual(65_536);

    const snap = bridge.stepFrame();
    expect(snap.mode).toBe("opening");
    expect(snap.frame).toBe(1);
    expect(snap.width).toBe(640);
    expect(snap.height).toBe(224);
    expect(snap.towers).toBeGreaterThan(0);
    expect(snap.commands).toBeGreaterThan(0);
    expect(snap.vertices).toBeGreaterThan(0);

    const verts = bridge.getVertexView();
    const cmds = bridge.getCommandView();
    expect(verts.length).toBe(snap.vertices * EXPECTED_VERTEX_FLOATS);
    expect(cmds.length).toBe(snap.commands * EXPECTED_CMD_WORDS);
    for (let i = 0; i < Math.min(verts.length, 480); i++) {
      expect(Number.isFinite(verts[i])).toBe(true);
    }
  });

  it("produces bit-exact deterministic vertex and command streams across independent WASM instances", async () => {
    const a = await WasmSimulationBridge.instantiateFromBytes(wasmBytes);
    const b = await WasmSimulationBridge.instantiateFromBytes(wasmBytes);

    const opts = {
      mode: "opening" as const,
      rngSeed: 1337,
      towerPreset: { kind: "simulated" as const, seed: 42, ngames: 45, nboots: 180 },
      autoTransitionToMenu: false,
    };
    a.initScene(opts);
    b.initScene(opts);

    for (let f = 0; f < 40; f++) {
      const sa = a.stepFrame();
      const sb = b.stepFrame();
      expect(sa).toEqual(sb);
    }

    expect(Array.from(a.getCommandView())).toEqual(Array.from(b.getCommandView()));
    expect(Array.from(a.getVertexView())).toEqual(Array.from(b.getVertexView()));
  });

  it("supports Mode B Idle, Mode C IllegalDisc (localized), Mode D Menu + System Configuration, and Mode E live overrides", async () => {
    const bridge = await WasmSimulationBridge.instantiateFromBytes(wasmBytes);

    // Mode C IllegalDisc in Portuguese (language = 7)
    bridge.initScene({ mode: "illegal-disc", language: 7, pal: true });
    let snap = bridge.stepFrame();
    for (let i = 0; i < 120; i++) {
      snap = bridge.stepFrame();
    }
    expect(snap.mode).toBe("illegal-disc");
    expect(snap.height).toBe(256);
    expect(snap.illegalTextAlpha).toBeGreaterThan(0);
    expect(snap.illegalMsg).toContain("disco");

    // Mode D Menu -> System Configuration -> Clock Adjustment
    bridge.initScene({
      mode: "menu",
      clock: { year: 2026, month: 10, day: 31, hour: 21, minute: 45, second: 15 },
    });
    for (let i = 0; i < 130; i++) {
      snap = bridge.stepFrame();
    }
    expect(snap.menuUi).not.toBeNull();
    expect(snap.menuUi?.screen).toBe("main");
    expect(snap.menuUi?.items.map((x) => x.provenance)).toEqual([
      "approximate",
      "sourced",
      "extension",
    ]);
    expect(snap.menuUi?.interactive).toBe(true);

    expect(bridge.dispatchMenuAction("down")).toBe(true);
    expect(bridge.dispatchMenuAction("confirm")).toBe(true);
    // The 90-frame open choreography swallows input (`menu.c:0x227D08`), so the
    // shell must keep its controls disabled until the core accepts input again.
    snap = bridge.stepFrame();
    expect(snap.menuUi?.screen).toBe("config-in");
    expect(snap.menuUi?.interactive).toBe(false);
    for (let i = 0; i < 92; i++) {
      snap = bridge.stepFrame();
    }
    expect(snap.menuUi?.screen).toBe("config-list");
    expect(snap.menuUi?.interactive).toBe(true);
    expect(snap.rods).toBe(12);
    expect(snap.cubes).toBe(5);
    expect(snap.orbs).toBe(7);

    const decoded = bridge.decodeCommands();
    expect(decoded.some((c) => c.kind === DrawCommandKindId.ApplyMenuBackdropTint)).toBe(true);
    expect(decoded.some((c) => c.kind === DrawCommandKindId.CaptureScreenToWork)).toBe(true);

    // Mode E live overrides: valid succeeds, NaN or out-of-range rejected cleanly without crash
    expect(
      bridge.setLiveOverrides({
        speedMultiplier: 1.5,
        camOffsetX: 2.5,
        camOffsetY: -1.5,
        reducedMotion: true,
        letterbox: true,
      }),
    ).toBe(true);
    expect(bridge.setLiveOverrides({ speedMultiplier: Number.NaN })).toBe(false);
    expect(bridge.setLiveOverrides({ speedMultiplier: -2.0 })).toBe(false);
    expect(bridge.setLiveOverrides({ camOffsetX: 999.0 })).toBe(false);
  });

  it("validates clean synthetic LZ and ROMDIR buffers and rejects malformed/bomb inputs safely", async () => {
    const bridge = await WasmSimulationBridge.instantiateFromBytes(wasmBytes);

    // Build a clean literal-only OSDSYS LZ stream ("HELLO" = 5 bytes):
    // 4-byte LE size (5) + 4-byte BE descriptor (0 = all literals, shift=14) + "HELLO"
    const cleanLz = new Uint8Array([
      5, 0, 0, 0,
      0, 0, 0, 0,
      0x48, 0x45, 0x4c, 0x4c, 0x4f,
    ]);
    const unpacked = bridge.unpackPs2Lz(cleanLz);
    expect(unpacked.ok).toBe(true);
    if (unpacked.ok) {
      expect(new TextDecoder().decode(unpacked.data)).toBe("HELLO");
    }

    // Decompression bomb header (declares 256 MiB > 16 MiB cap)
    const bombLz = new Uint8Array([0, 0, 0, 16, 0, 0, 0, 0]);
    const bombRes = bridge.unpackPs2Lz(bombLz);
    expect(bombRes.ok).toBe(false);
    if (!bombRes.ok) {
      expect(bombRes.errorCode).toBe(-2);
    }

    // Truncated stream
    const truncRes = bridge.unpackPs2Lz(new Uint8Array([5, 0, 0, 0, 0, 0]));
    expect(truncRes.ok).toBe(false);

    // Synthetic clean-room ROMDIR container: RESET(0) + ROMDIR(48) + TESTDATA(5) + terminator(16 zeros)
    const rom = new Uint8Array(64);
    const enc = new TextEncoder();
    rom.set(enc.encode("RESET"), 0);
    rom.set(enc.encode("ROMDIR"), 16);
    new DataView(rom.buffer).setUint32(16 + 12, 64, true); // invalid > 64? Let's set 48!
    new DataView(rom.buffer).setUint32(16 + 12, 48, true);
    rom.set(enc.encode("SYNTH"), 32);
    new DataView(rom.buffer).setUint32(32 + 12, 5, true);

    const romRes = bridge.inspectRomdir(rom);
    expect(romRes.ok).toBe(true);
    if (romRes.ok) {
      expect(romRes.entries.map((e) => e.name)).toEqual(["RESET", "ROMDIR", "SYNTH"]);
    }

    const badRom = bridge.inspectRomdir(enc.encode("NOT A ROMDIR IMAGE AT ALL"));
    expect(badRom.ok).toBe(false);
  });

  it("maintains zero WASM linear-memory growth across 3,000 steady-state frames", async () => {
    const bridge = await WasmSimulationBridge.instantiateFromBytes(wasmBytes);
    bridge.initScene({ mode: "idle", rngSeed: 777 });

    // Warm up 60 frames
    for (let i = 0; i < 60; i++) {
      bridge.stepFrame();
    }
    const baselineBytes = bridge.memoryByteLength;

    for (let i = 0; i < 3_000; i++) {
      const snap = bridge.stepFrame();
      if (i % 1000 === 999) {
        expect(snap.towers).toBeGreaterThan(0);
      }
    }

    expect(bridge.memoryByteLength).toBe(baselineBytes);
  });

  it("rejects corrupted or non-OSDBITS WASM binaries cleanly with structured errors", async () => {
    // Corrupted magic bytes
    await expect(
      WasmSimulationBridge.instantiateFromBytes(new Uint8Array([0x00, 0x61, 0x73, 0x00, 0xff])),
    ).rejects.toThrow();

    // Minimal valid empty WASM module (missing required exports)
    const emptyWasm = new Uint8Array([0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);
    await expect(WasmSimulationBridge.instantiateFromBytes(emptyWasm)).rejects.toThrow(
      /WASM module missing/,
    );
  });
});
