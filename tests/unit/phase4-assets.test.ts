import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import {
  PROCEDURAL_SLOT_COUNT,
  TextureSlotId,
  computeFnv1aHex,
  computeSha256Hex,
  synthesizeProceduralTextures,
} from "../../web/src/assets/proceduralTextures";
import { importUserResourceArchive } from "../../web/src/assets/userResourceImporter";
import { WasmSimulationBridge } from "../../web/src/wasm/bridge";

const wasmBytes = readFileSync(
  resolve(process.cwd(), "web/public/wasm/osdbits_wasm.wasm"),
);

/** Encodes raw bytes into a valid OSDSYS LZ stream using shift mode `nMode` (`0..=3`). */
function encodeCleanSyntheticLz(payload: Uint8Array, nMode: 0 | 1 | 2 | 3 = 0): Uint8Array {
  const shift = 14 - nMode;
  const maxBack = (0x3fff >>> nMode) + 1;
  const maxMatchLen = (0xffff >>> shift) + 3;

  const out: number[] = [
    payload.length & 0xff,
    (payload.length >>> 8) & 0xff,
    (payload.length >>> 16) & 0xff,
    (payload.length >>> 24) & 0xff,
  ];

  let pos = 0;
  while (pos < payload.length) {
    const descOffset = out.length;
    out.push(0, 0, 0, 0);
    let desc = nMode & 3;

    for (let step = 0; step < 30 && pos < payload.length; step++) {
      let bestBack = 0;
      let bestLen = 0;
      const searchLimit = Math.min(pos, maxBack, 128);
      for (let back = 1; back <= searchLimit; back++) {
        let mlen = 0;
        while (
          mlen < maxMatchLen &&
          pos + mlen < payload.length &&
          payload[pos + mlen] === payload[pos + mlen - back]
        ) {
          mlen++;
        }
        if (mlen >= 3 && mlen > bestLen) {
          bestLen = mlen;
          bestBack = back;
          if (bestLen >= Math.min(18, maxMatchLen)) break;
        }
      }

      if (bestLen >= 3) {
        desc = (desc | ((1 << (31 - step)) >>> 0)) >>> 0;
        const h = (((bestLen - 3) << shift) | (bestBack - 1)) & 0xffff;
        out.push((h >>> 8) & 0xff, h & 0xff);
        pos += bestLen;
      } else {
        out.push(payload[pos] ?? 0);
        pos++;
      }
    }

    out[descOffset] = (desc >>> 24) & 0xff;
    out[descOffset + 1] = (desc >>> 16) & 0xff;
    out[descOffset + 2] = (desc >>> 8) & 0xff;
    out[descOffset + 3] = desc & 0xff;
  }

  return new Uint8Array(out);
}

function align16(n: number): number {
  return (n + 0x0f) & ~0x0f;
}

/** Builds a clean-room synthetic `ROMDIR` container in memory (`tools/extract-res.py` layout). */
function buildSyntheticRomdirContainer(
  files: ReadonlyArray<{ name: string; data: Uint8Array }>,
): Uint8Array {
  // Entries: RESET (0 bytes), ROMDIR (dirBytes), ...files, terminator (16 zero bytes)
  const entryCount = 2 + files.length + 1;
  const dirBytes = entryCount * 16;
  const dirAligned = align16(dirBytes);

  let totalBytes = dirAligned;
  for (const f of files) {
    totalBytes += align16(f.data.byteLength);
  }

  const buf = new Uint8Array(totalBytes);
  const dv = new DataView(buf.buffer);
  const enc = new TextEncoder();

  const writeEntry = (index: number, name: string, size: number): void => {
    const base = index * 16;
    buf.set(enc.encode(name.slice(0, 10)), base);
    dv.setUint16(base + 10, 0, true);
    dv.setUint32(base + 12, size, true);
  };

  writeEntry(0, "RESET", 0);
  writeEntry(1, "ROMDIR", dirBytes);

  let dataCursor = dirAligned;
  files.forEach((f, idx) => {
    writeEntry(2 + idx, f.name, f.data.byteLength);
    buf.set(f.data, dataCursor);
    dataCursor += align16(f.data.byteLength);
  });

  return buf;
}

const tempDirs: string[] = [];
afterAll(() => {
  for (const d of tempDirs) {
    rmSync(d, { recursive: true, force: true });
  }
});

describe("Phase 4 — Clean-Room Procedural Assets & User Resource Importer", () => {
  it("synthesizes all 23 clean-room slots deterministically with valid mip chains and PS2 alpha semantics", async () => {
    const setA = synthesizeProceduralTextures(0x5053_3230);
    const setB = synthesizeProceduralTextures(0x5053_3230);
    const setDiff = synthesizeProceduralTextures(0x1234_5678);

    expect(setA).toHaveLength(PROCEDURAL_SLOT_COUNT);
    expect(setA.map((s) => s.fnv1aHash)).toEqual(setB.map((s) => s.fnv1aHash));
    expect(setA[TextureSlotId.Wal0]?.fnv1aHash).not.toBe(setDiff[TextureSlotId.Wal0]?.fnv1aHash);

    for (const slot of setA) {
      expect(slot.provenance).toBe("procedural-clean-room");
      const base = slot.mipLevels[0]!;
      expect(base.byteLength).toBe(slot.width * slot.height * 4);

      let nonZeroAlpha = 0;
      for (let i = 3; i < base.length; i += 4) {
        if ((base[i] ?? 0) > 0) nonZeroAlpha++;
      }
      expect(nonZeroAlpha).toBeGreaterThan(0);
    }

    // WAL0 has 6 mip levels: 256 -> 128 -> 64 -> 32 -> 16 -> 8
    const wal0 = setA[TextureSlotId.Wal0]!;
    expect(wal0.mipLevels).toHaveLength(6);
    expect(wal0.mipLevels.map((m) => m.byteLength)).toEqual([
      256 * 256 * 4,
      128 * 128 * 4,
      64 * 64 * 4,
      32 * 32 * 4,
      16 * 16 * 4,
      8 * 8 * 4,
    ]);

    // Format 4 slots (BLP / BLPR) must have RGB == 0 and caustic alpha > 0
    const blp = setA[TextureSlotId.Blp]!.mipLevels[0]!;
    for (let i = 0; i < blp.length; i += 4) {
      expect(blp[i]).toBe(0);
      expect(blp[i + 1]).toBe(0);
      expect(blp[i + 2]).toBe(0);
    }

    // MenuBinv is the exact bitwise complement (~src) of MenuBump RGB with 0x7f alpha
    const bump = setA[TextureSlotId.MenuBump]!.mipLevels[0]!;
    const binv = setA[TextureSlotId.MenuBinv]!.mipLevels[0]!;
    for (let i = 0; i < bump.length; i += 4) {
      expect(binv[i]).toBe((~(bump[i] ?? 0)) & 0xff);
      expect(binv[i + 3]).toBe(0x7f);
    }

    // MenuKabe 128x128 is symmetric across all 4 64x64 quadrants (DecodeKabe)
    const kabe = setA[TextureSlotId.MenuKabe]!.mipLevels[0]!;
    for (let y = 0; y < 64; y += 7) {
      for (let x = 0; x < 64; x += 7) {
        const q00 = (y * 128 + x) * 4;
        const q10 = (y * 128 + (x + 64)) * 4;
        const q01 = ((y + 64) * 128 + x) * 4;
        const q11 = ((y + 64) * 128 + (x + 64)) * 4;
        expect(Array.from(kabe.subarray(q00, q00 + 4))).toEqual(Array.from(kabe.subarray(q10, q10 + 4)));
        expect(Array.from(kabe.subarray(q00, q00 + 4))).toEqual(Array.from(kabe.subarray(q01, q01 + 4)));
        expect(Array.from(kabe.subarray(q00, q00 + 4))).toEqual(Array.from(kabe.subarray(q11, q11 + 4)));
      }
    }

    const sha = await computeSha256Hex(wal0.mipLevels[0]!);
    expect(sha).toMatch(/^[0-9a-f]{64}$/);
    expect(computeFnv1aHex(wal0.mipLevels[0]!)).toBe(wal0.fnv1aHash);
  });

  it("matches host-compiled ps2expand.c byte-for-byte across all 4 shift modes and RLE overlaps", async () => {
    const dir = mkdtempSync(join(tmpdir(), "osdbits-ps2expand-"));
    tempDirs.push(dir);
    const binPath = join(dir, "ps2expand_ref");
    const srcC = resolve(process.cwd(), "ps2expand.c");

    execFileSync("gcc", ["-O2", srcC, "-o", binPath]);

    const bridge = await WasmSimulationBridge.instantiateFromBytes(wasmBytes);

    // Exercise nMode = 0, 1, 2, 3 with RLE runs, periodic texture patterns, and prime-length tails
    for (const nMode of [0, 1, 2, 3] as const) {
      const payload = new Uint8Array(4099);
      for (let i = 0; i < payload.length; i++) {
        if (i >= 200 && i < 520) {
          payload[i] = 0xa5; // Long RLE back=1 overlap run
        } else if (i >= 1000 && i < 1600) {
          payload[i] = payload[i - 16] ?? 0; // Periodic back=16 pattern
        } else {
          payload[i] = (i * 37 + nMode * 19) & 0xff;
        }
      }

      const compressed = encodeCleanSyntheticLz(payload, nMode);
      const inFile = join(dir, `in_${nMode}.lz`);
      const outFile = join(dir, `out_${nMode}.raw`);
      writeFileSync(inFile, compressed);
      execFileSync(binPath, [inFile, outFile]);

      const cOutput = new Uint8Array(readFileSync(outFile));
      const wasmRes = bridge.unpackPs2Lz(compressed);

      expect(wasmRes.ok).toBe(true);
      if (wasmRes.ok) {
        expect(Array.from(wasmRes.data)).toEqual(Array.from(payload));
        expect(Array.from(wasmRes.data)).toEqual(Array.from(cOutput));
      }
    }
  });

  it("imports valid nested BIOS/TEXIMAGE containers and falls back cleanly on corrupted subfiles", async () => {
    const bridge = await WasmSimulationBridge.instantiateFromBytes(wasmBytes);

    // 1. Synthetic TEXOWAL0 (20-byte header + 256x256 PSMCT16)
    const rawWal0 = new Uint8Array(20 + 256 * 256 * 2);
    const walDv = new DataView(rawWal0.buffer);
    for (let i = 0; i < 256 * 256; i++) {
      // Non-zero RGB555 + A=1 => alpha 0x81
      const word = (1 << 15) | (20 << 10) | (18 << 5) | 16;
      walDv.setUint16(20 + i * 2, word, true);
    }

    // 2. Synthetic TEXCKABE (64x64 RGB24)
    const rawKabe = new Uint8Array(64 * 64 * 3);
    for (let i = 0; i < rawKabe.length; i += 3) {
      rawKabe[i] = 30;
      rawKabe[i + 1] = 60;
      rawKabe[i + 2] = 120;
    }

    // 3. Synthetic TEXCBUMP (64x64 gray8) -> should also derive TEXCBINV (~TEXCBUMP)!
    const rawBump = new Uint8Array(64 * 64).fill(0x42);

    // 4. Synthetic TEXOBLP (64x64 format 4)
    const rawBlp = new Uint8Array(64 * 64).fill(0x55);

    // 5. Intentionally corrupted TEXOFOG1 (truncated LZ stream) -> must fall back to procedural!
    const brokenFog1 = new Uint8Array([100, 0, 0, 0, 0xff]);

    const innerTexImage = buildSyntheticRomdirContainer([
      { name: "TEXOWAL0", data: encodeCleanSyntheticLz(rawWal0, 1) },
      { name: "TEXCKABE", data: encodeCleanSyntheticLz(rawKabe, 2) },
      { name: "TEXCBUMP", data: encodeCleanSyntheticLz(rawBump, 0) },
      { name: "TEXOBLP", data: encodeCleanSyntheticLz(rawBlp, 3) },
      { name: "TEXOFOG1", data: brokenFog1 },
    ]);

    const outerBiosRomdir = buildSyntheticRomdirContainer([
      { name: "TEXIMAGE", data: innerTexImage },
    ]);

    const report = await importUserResourceArchive(outerBiosRomdir, bridge);
    expect(report.ok).toBe(true);
    expect(report.archiveSha256).toMatch(/^[0-9a-f]{64}$/);
    expect(report.importedSlots).toEqual([
      "TEXOWAL0",
      "TEXOBLP",
      "TEXCKABE",
      "TEXCBUMP",
      "TEXCBINV",
    ]);
    expect(report.skippedOrInvalidSlots).toHaveLength(1);
    expect(report.skippedOrInvalidSlots[0]?.name).toBe("TEXOFOG1");

    // Imported WAL0 has 6 mips and user-imported provenance
    expect(report.textures[TextureSlotId.Wal0]?.provenance).toBe("user-imported");
    expect(report.textures[TextureSlotId.Wal0]?.mipLevels).toHaveLength(6);
    expect(report.textures[TextureSlotId.Wal0]?.mipLevels[0]?.[3]).toBe(0x81);

    // Derived TEXCBINV has ~0x42 == 0xbd
    expect(report.textures[TextureSlotId.MenuBinv]?.provenance).toBe("user-imported");
    expect(report.textures[TextureSlotId.MenuBinv]?.mipLevels[0]?.[0]).toBe(0xbd);

    // Corrupted TEXOFOG1 fell back cleanly to procedural-clean-room!
    expect(report.textures[TextureSlotId.Fog1]?.provenance).toBe("procedural-clean-room");
  });
});
