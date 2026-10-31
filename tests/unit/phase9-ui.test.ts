import "fake-indexeddb/auto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { AudioEngine } from "../../web/src/audio/audioEngine";
import { StorageManager } from "../../web/src/storage/storageManager";
import { OsdbitsApp } from "../../web/src/ui/osdbitsApp";
import { WasmSimulationBridge } from "../../web/src/wasm/bridge";

const WASM_PATH = resolve(process.cwd(), "web/public/wasm/osdbits_wasm.wasm");

describe("Phase 9 — UI Shell Accessibility, prefers-reduced-motion & Asset Validation Error Handling", () => {
  it("mounts accessible ARIA controls, honors prefers-reduced-motion on first boot, and handles invalid local asset files cleanly", async () => {
    document.body.innerHTML = `
      <div id="app">
        <canvas id="osdbits-canvas" width="640" height="480"></canvas>
      </div>
    `;

    // Mock OS prefers-reduced-motion: reduce
    const originalMatchMedia = window.matchMedia;
    window.matchMedia = ((query: string) => ({
      matches: query.includes("prefers-reduced-motion: reduce"),
      media: query,
      onchange: null,
      addListener() {},
      removeListener() {},
      addEventListener() {},
      removeEventListener() {},
      dispatchEvent: () => false,
    })) as typeof window.matchMedia;

    try {
      const wasmBytes = readFileSync(WASM_PATH);
      const bridge = await WasmSimulationBridge.instantiateFromBytes(wasmBytes);
      const storage = new StorageManager({
        dbName: `ui_test_db_${Date.now()}`,
        enableOpfs: false,
      });
      const audio = new AudioEngine({ contextFactory: () => null });
      const appEl = document.getElementById("app")!;
      const canvasEl = document.getElementById("osdbits-canvas") as HTMLCanvasElement;

      const app = new OsdbitsApp(appEl, canvasEl, bridge, storage, audio);
      await app.initialize();

      // Verify prefers-reduced-motion propagated automatically on rev-0 boot
      expect(app.getSettings().reducedMotion).toBe(true);
      expect((document.getElementById("cfg-reduced-motion") as HTMLInputElement).checked).toBe(
        true,
      );

      // Verify ARIA roles & labels
      expect(document.getElementById("launch-overlay")?.getAttribute("role")).toBe("dialog");
      expect(document.getElementById("customize-drawer")?.getAttribute("role")).toBe("dialog");
      expect(document.getElementById("hud-toolbar")?.getAttribute("aria-label")).toBe(
        "Scene Controls",
      );

      // Simulate selecting an invalid/corrupt local resource file -> non-crashing rejection message
      const bogusBytes = new Uint8Array([0xde, 0xad, 0xbe, 0xef, 0x00, 0x01]);
      const ok = await app.importResourceBytes(bogusBytes);
      expect(ok).toBe(false);
      const statusText = document.getElementById("resource-import-status")?.textContent ?? "";
      expect(statusText).toContain("Import rejected:");

      app.dispose();
    } finally {
      window.matchMedia = originalMatchMedia;
    }
  });
});
