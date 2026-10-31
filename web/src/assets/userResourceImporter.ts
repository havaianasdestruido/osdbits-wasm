/**
 * Local-only, validated user-supplied `ROMDIR` / `TEXIMAGE` resource importer (`P04-002`).
 *
 * Uses `WasmSimulationBridge` (`expand_ps2_lz` + `find_romdir`/`parse_romdir`) to unpack optional
 * user-provided PS2 BIOS or `TEXIMAGE` containers in memory without network upload, decodes each
 * PS2 GS pixel format (`PSMCT32`, `PSMCT16` + `TEXA`, `format 3/4/5`, `DecodeKabe`, `DecodeBump`),
 * computes SHA-256 & FNV-1a integrity hashes, and falls back cleanly to procedural clean-room
 * textures for any missing or invalid slot.
 */

import type { WasmSimulationBridge } from "../wasm/bridge";
import {
  buildRgbaMipChain,
  computeFnv1aHex,
  computeSha256Hex,
  invertBumpTexture,
  synthesizeProceduralTextures,
  TextureSlotId,
  type ProceduralTextureAsset,
} from "./proceduralTextures";

type RawSlotEncoding =
  | "psmct32"
  | "psmct16_off20"
  | "format3_white_alpha"
  | "format4_black_alpha"
  | "format5_lum_alpha"
  | "kabe_rgb24_quad"
  | "bump_gray8";

interface KnownSubfileSpec {
  subfileName: string;
  slotId: TextureSlotId;
  width: number;
  height: number;
  mipCount: number;
  encoding: RawSlotEncoding;
  expectedExpandedBytes: number;
}

const KNOWN_SUBFILES: ReadonlyArray<KnownSubfileSpec> = [
  { subfileName: "TEXOSCE", slotId: TextureSlotId.Banner, width: 256, height: 64, mipCount: 1, encoding: "format5_lum_alpha", expectedExpandedBytes: 256 * 64 * 2 },
  { subfileName: "TEXOFOG0", slotId: TextureSlotId.Fog0, width: 128, height: 128, mipCount: 1, encoding: "psmct16_off20", expectedExpandedBytes: 20 + 128 * 128 * 2 },
  { subfileName: "TEXOFOG1", slotId: TextureSlotId.Fog1, width: 64, height: 64, mipCount: 1, encoding: "psmct16_off20", expectedExpandedBytes: 20 + 64 * 64 * 2 },
  { subfileName: "TEXOFOG2", slotId: TextureSlotId.Fog2, width: 64, height: 64, mipCount: 1, encoding: "psmct16_off20", expectedExpandedBytes: 20 + 64 * 64 * 2 },
  { subfileName: "TEXOFOG3", slotId: TextureSlotId.Fog3, width: 64, height: 64, mipCount: 1, encoding: "psmct16_off20", expectedExpandedBytes: 20 + 64 * 64 * 2 },
  { subfileName: "TEXOFOG4", slotId: TextureSlotId.Fog4, width: 64, height: 64, mipCount: 1, encoding: "psmct16_off20", expectedExpandedBytes: 20 + 64 * 64 * 2 },
  { subfileName: "TEXOWAL0", slotId: TextureSlotId.Wal0, width: 256, height: 256, mipCount: 6, encoding: "psmct16_off20", expectedExpandedBytes: 20 + 256 * 256 * 2 },
  { subfileName: "TEXOCRLE", slotId: TextureSlotId.Crle, width: 64, height: 64, mipCount: 1, encoding: "psmct32", expectedExpandedBytes: 64 * 64 * 4 },
  { subfileName: "TEXOCRBL", slotId: TextureSlotId.Crbl, width: 64, height: 64, mipCount: 1, encoding: "psmct32", expectedExpandedBytes: 64 * 64 * 4 },
  { subfileName: "TEXOFLAR", slotId: TextureSlotId.Flar, width: 128, height: 128, mipCount: 1, encoding: "psmct16_off20", expectedExpandedBytes: 20 + 128 * 128 * 2 },
  { subfileName: "TEXOREF", slotId: TextureSlotId.Ref, width: 128, height: 128, mipCount: 1, encoding: "psmct16_off20", expectedExpandedBytes: 20 + 128 * 128 * 2 },
  { subfileName: "TEXOBLP", slotId: TextureSlotId.Blp, width: 64, height: 64, mipCount: 1, encoding: "format4_black_alpha", expectedExpandedBytes: 64 * 64 },
  { subfileName: "TEXOBLPR", slotId: TextureSlotId.Blpr, width: 64, height: 64, mipCount: 1, encoding: "format4_black_alpha", expectedExpandedBytes: 64 * 64 },
  { subfileName: "TEXCKABE", slotId: TextureSlotId.MenuKabe, width: 128, height: 128, mipCount: 1, encoding: "kabe_rgb24_quad", expectedExpandedBytes: 64 * 64 * 3 },
  { subfileName: "TEXCBLUR", slotId: TextureSlotId.MenuBlur, width: 64, height: 64, mipCount: 1, encoding: "format3_white_alpha", expectedExpandedBytes: 64 * 64 },
  { subfileName: "TEXCNAVI", slotId: TextureSlotId.MenuNavi, width: 64, height: 64, mipCount: 1, encoding: "format3_white_alpha", expectedExpandedBytes: 64 * 64 },
  { subfileName: "TEXCBUMP", slotId: TextureSlotId.MenuBump, width: 64, height: 64, mipCount: 1, encoding: "bump_gray8", expectedExpandedBytes: 64 * 64 },
  { subfileName: "TEXCFLOW", slotId: TextureSlotId.MenuFlow, width: 64, height: 64, mipCount: 1, encoding: "bump_gray8", expectedExpandedBytes: 64 * 64 },
  { subfileName: "TEXCREFA", slotId: TextureSlotId.MenuRefa, width: 64, height: 64, mipCount: 1, encoding: "bump_gray8", expectedExpandedBytes: 64 * 64 },
  { subfileName: "TEXCMARU", slotId: TextureSlotId.MenuMaru, width: 64, height: 64, mipCount: 1, encoding: "psmct32", expectedExpandedBytes: 64 * 64 * 4 },
];

export interface UserResourceImportReport {
  ok: boolean;
  archiveSha256: string;
  archiveFnv1a: string;
  archiveByteLength: number;
  importedSlots: string[];
  skippedOrInvalidSlots: Array<{ name: string; reason: string }>;
  textures: ProceduralTextureAsset[];
  errorMessage?: string;
}

function decodePsmct16WithTexa(src: Uint8Array, byteOffset: number, pixelCount: number): Uint8Array {
  const out = new Uint8Array(pixelCount * 4);
  for (let i = 0; i < pixelCount; i++) {
    const lo = src[byteOffset + i * 2] ?? 0;
    const hi = src[byteOffset + i * 2 + 1] ?? 0;
    const word = lo | (hi << 8);
    const r5 = word & 0x1f;
    const g5 = (word >>> 5) & 0x1f;
    const b5 = (word >>> 10) & 0x1f;
    const a1 = (word >>> 15) & 1;

    const r8 = (r5 << 3) | (r5 >>> 2);
    const g8 = (g5 << 3) | (g5 >>> 2);
    const b8 = (b5 << 3) | (b5 >>> 2);
    // GS TEXA { TA0 = 0x7F, AEM = 1, TA1 = 0x81 }
    const ps2Alpha = a1 === 1 ? 0x81 : r5 === 0 && g5 === 0 && b5 === 0 ? 0x00 : 0x7f;

    const o = i * 4;
    out[o] = r8;
    out[o + 1] = g8;
    out[o + 2] = b8;
    out[o + 3] = ps2Alpha;
  }
  return out;
}

function decodeExpandedSlot(raw: Uint8Array, spec: KnownSubfileSpec): Uint8Array | null {
  if (raw.byteLength < spec.expectedExpandedBytes) {
    return null;
  }
  const pixelCount = spec.width * spec.height;
  switch (spec.encoding) {
    case "psmct32": {
      const copy = new Uint8Array(pixelCount * 4);
      copy.set(raw.subarray(0, pixelCount * 4));
      return copy;
    }
    case "psmct16_off20":
      return decodePsmct16WithTexa(raw, 20, pixelCount);
    case "format3_white_alpha": {
      const out = new Uint8Array(pixelCount * 4);
      for (let i = 0; i < pixelCount; i++) {
        const o = i * 4;
        out[o] = 255;
        out[o + 1] = 255;
        out[o + 2] = 255;
        out[o + 3] = raw[i] ?? 0;
      }
      return out;
    }
    case "format4_black_alpha": {
      const out = new Uint8Array(pixelCount * 4);
      for (let i = 0; i < pixelCount; i++) {
        out[i * 4 + 3] = raw[i] ?? 0;
      }
      return out;
    }
    case "format5_lum_alpha": {
      const out = new Uint8Array(pixelCount * 4);
      for (let i = 0; i < pixelCount; i++) {
        const lum = raw[i * 2] ?? 0;
        const alpha = raw[i * 2 + 1] ?? 0;
        const o = i * 4;
        out[o] = lum;
        out[o + 1] = lum;
        out[o + 2] = lum;
        out[o + 3] = alpha;
      }
      return out;
    }
    case "kabe_rgb24_quad": {
      // Matches `DecodeKabe` (`menuback.c:61`): `64x64` RGB24 mirrored into `128x128` RGBA8
      const halfW = 64;
      const halfH = 64;
      const fullW = 128;
      const out = new Uint8Array(128 * 128 * 4);
      let sp = 0;
      for (let y = 0; y < halfH; y++) {
        for (let x = 0; x < halfW; x++) {
          const r = raw[sp] ?? 0;
          const g = raw[sp + 1] ?? 0;
          const b = raw[sp + 2] ?? 0;
          sp += 3;
          const coords: ReadonlyArray<[number, number]> = [
            [x, y],
            [x + halfW, y],
            [x, y + halfH],
            [x + halfW, y + halfH],
          ];
          for (const [qx, qy] of coords) {
            const o = (qy * fullW + qx) * 4;
            out[o] = r;
            out[o + 1] = g;
            out[o + 2] = b;
            out[o + 3] = 0x7f;
          }
        }
      }
      return out;
    }
    case "bump_gray8": {
      const out = new Uint8Array(pixelCount * 4);
      for (let i = 0; i < pixelCount; i++) {
        const lum = raw[i] ?? 0;
        const o = i * 4;
        out[o] = lum;
        out[o + 1] = lum;
        out[o + 2] = lum;
        out[o + 3] = 0x7f;
      }
      return out;
    }
  }
}

export async function importUserResourceArchive(
  archiveBytes: Uint8Array,
  bridge: WasmSimulationBridge,
  proceduralSeed = 0x5053_3230,
): Promise<UserResourceImportReport> {
  const baseline = synthesizeProceduralTextures(proceduralSeed);
  const archiveFnv1a = computeFnv1aHex(archiveBytes);
  const archiveSha256 = await computeSha256Hex(archiveBytes);

  const outer = bridge.inspectRomdir(archiveBytes);
  if (!outer.ok) {
    return {
      ok: false,
      archiveSha256,
      archiveFnv1a,
      archiveByteLength: archiveBytes.byteLength,
      importedSlots: [],
      skippedOrInvalidSlots: [],
      textures: baseline,
      errorMessage: outer.reason,
    };
  }

  // Descend into nested `TEXIMAGE` container if input is a full BIOS ROMDIR image (`tools/extract-res.py:114`)
  let texContainer = archiveBytes;
  let texEntries = outer.entries;
  const texImageEntry = outer.entries.find((e) => e.name === "TEXIMAGE");
  if (texImageEntry) {
    const sliceEnd = texImageEntry.offset + texImageEntry.size;
    if (sliceEnd > archiveBytes.byteLength) {
      return {
        ok: false,
        archiveSha256,
        archiveFnv1a,
        archiveByteLength: archiveBytes.byteLength,
        importedSlots: [],
        skippedOrInvalidSlots: [],
        textures: baseline,
        errorMessage: "TEXIMAGE entry exceeds outer container bounds",
      };
    }
    texContainer = archiveBytes.subarray(texImageEntry.offset, sliceEnd);
    const inner = bridge.inspectRomdir(texContainer);
    if (!inner.ok) {
      return {
        ok: false,
        archiveSha256,
        archiveFnv1a,
        archiveByteLength: archiveBytes.byteLength,
        importedSlots: [],
        skippedOrInvalidSlots: [],
        textures: baseline,
        errorMessage: `Nested TEXIMAGE error: ${inner.reason}`,
      };
    }
    texEntries = inner.entries;
  }

  const importedSlots: string[] = [];
  const skippedOrInvalidSlots: Array<{ name: string; reason: string }> = [];

  for (const spec of KNOWN_SUBFILES) {
    const entry = texEntries.find((e) => e.name === spec.subfileName);
    if (!entry) {
      continue;
    }
    const end = entry.offset + entry.size;
    if (entry.size === 0 || end > texContainer.byteLength) {
      skippedOrInvalidSlots.push({
        name: spec.subfileName,
        reason: "Subfile entry empty or out of container bounds",
      });
      continue;
    }

    const compressed = texContainer.subarray(entry.offset, end);
    const unpacked = bridge.unpackPs2Lz(compressed);
    if (!unpacked.ok) {
      skippedOrInvalidSlots.push({
        name: spec.subfileName,
        reason: unpacked.reason,
      });
      continue;
    }

    const rgba = decodeExpandedSlot(unpacked.data, spec);
    if (!rgba) {
      skippedOrInvalidSlots.push({
        name: spec.subfileName,
        reason: `Expanded size ${unpacked.data.byteLength} < expected ${spec.expectedExpandedBytes}`,
      });
      continue;
    }

    const mipLevels = buildRgbaMipChain(rgba, spec.width, spec.height, spec.mipCount);
    baseline[spec.slotId] = {
      id: spec.slotId,
      name: spec.subfileName,
      width: spec.width,
      height: spec.height,
      mipLevels,
      provenance: "user-imported",
      fnv1aHash: computeFnv1aHex(rgba),
    };
    importedSlots.push(spec.subfileName);

    // Automatically derive `TEXCBINV` (`~TEXCBUMP`) when `TEXCBUMP` is imported (`menuconfig.c:245`)
    if (spec.slotId === TextureSlotId.MenuBump) {
      const binv = invertBumpTexture(rgba);
      baseline[TextureSlotId.MenuBinv] = {
        id: TextureSlotId.MenuBinv,
        name: "TEXCBINV",
        width: 64,
        height: 64,
        mipLevels: [binv],
        provenance: "user-imported",
        fnv1aHash: computeFnv1aHex(binv),
      };
      importedSlots.push("TEXCBINV");
    }
  }

  return {
    ok: importedSlots.length > 0,
    archiveSha256,
    archiveFnv1a,
    archiveByteLength: archiveBytes.byteLength,
    importedSlots,
    skippedOrInvalidSlots,
    textures: baseline,
    errorMessage:
      importedSlots.length === 0 ? "No valid OSDSYS texture subfiles found in archive" : undefined,
  };
}
