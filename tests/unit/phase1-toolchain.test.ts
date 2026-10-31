import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  scanDirectoryForProprietaryArtifacts,
  verifyHistoricalProvenance,
} from "../../scripts/check-legal-artifacts.mjs";
import { validateWgslSource } from "../../web/src/renderer/validateWgsl";

const repoRoot = resolve(__dirname, "../..");

describe("Phase 1 Toolchain, WASM, WGSL & Legal Guardrails", () => {
  it("loads compiled osdbits_wasm.wasm and verifies ABI magic & version", async () => {
    const wasmPath = join(repoRoot, "web", "public", "wasm", "osdbits_wasm.wasm");
    const wasmBytes = readFileSync(wasmPath);
    const { instance } = await WebAssembly.instantiate(wasmBytes, {});
    const exportsObj = instance.exports as {
      osdbits_abi_magic: () => number;
      osdbits_abi_version: () => number;
    };
    expect(exportsObj.osdbits_abi_magic() >>> 0).toBe(0x4f534442);
    expect(exportsObj.osdbits_abi_version() >>> 0).toBe(1);
  });

  it("validates valid WGSL shaders and rejects invalid WGSL or missing entry points", async () => {
    const validShader = `
      struct VertexOut {
        @builtin(position) position: vec4<f32>,
        @location(0) color: vec4<f32>,
      };

      @vertex
      fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOut {
        var out: VertexOut;
        let x = f32(i32(idx) - 1);
        out.position = vec4<f32>(x, 0.0, 0.5, 1.0);
        out.color = vec4<f32>(0.5, 0.7, 1.0, 1.0);
        return out;
      }

      @fragment
      fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
        return clamp(in.color * (255.0 / 128.0), vec4<f32>(0.0), vec4<f32>(1.0));
      }
    `;
    const okResult = await validateWgslSource(validShader, ["vs_main", "fs_main"]);
    expect(okResult.valid).toBe(true);
    expect(okResult.entryPoints).toContain("vs_main");
    expect(okResult.entryPoints).toContain("fs_main");

    const missingEntry = await validateWgslSource(validShader, ["vs_main", "non_existent_entry"]);
    expect(missingEntry.valid).toBe(false);
    expect(missingEntry.error).toContain("non_existent_entry");

    const brokenShader = `
      @fragment
      fn fs_broken() -> @location(0) vec4<f32> {
        let bad: f32 = vec3<f32>(1.0, 2.0, 3.0);
        return vec4<f32>(bad);
      }
    `;
    const brokenResult = await validateWgslSource(brokenShader);
    expect(brokenResult.valid).toBe(false);
    expect(brokenResult.error).toBeTruthy();
  });

  it("passes legal & historical provenance checks on repo and catches synthetic BIOS contraband", () => {
    const repoScan = scanDirectoryForProprietaryArtifacts(repoRoot);
    expect(repoScan.violations).toEqual([]);
    const provenance = verifyHistoricalProvenance(repoRoot);
    expect(provenance.ok).toBe(true);

    const tempRoot = mkdtempSync(join(tmpdir(), "osdbits-legal-test-"));
    try {
      mkdirSync(join(tempRoot, "osdbits", "res"), { recursive: true });
      writeFileSync(join(tempRoot, "osdbits", "res", "TEXOSCE_EXP.inc"), "0x00,\n");
      writeFileSync(join(tempRoot, "scph10000.bin"), Buffer.alloc(32, 0xaa));
      const romdirHeader = Buffer.alloc(32, 0);
      Buffer.from("TEXIMAGE\0\0", "ascii").copy(romdirHeader, 0);
      writeFileSync(join(tempRoot, "mystery.dat"), romdirHeader);

      const badScan = scanDirectoryForProprietaryArtifacts(tempRoot);
      expect(badScan.violations.length).toBeGreaterThanOrEqual(3);
      const paths = badScan.violations.map((v) => v.path);
      expect(paths).toContain("osdbits/res");
      expect(paths).toContain("scph10000.bin");
      expect(paths).toContain("mystery.dat");
    } finally {
      rmSync(tempRoot, { recursive: true, force: true });
    }
  });
});
