import "fake-indexeddb/auto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { AudioEngine } from "../../web/src/audio/audioEngine";
import { StorageManager } from "../../web/src/storage/storageManager";
import { OsdbitsApp } from "../../web/src/ui/osdbitsApp";
import { WasmSimulationBridge } from "../../web/src/wasm/bridge";

const WASM_PATH = resolve(process.cwd(), "web/public/wasm/osdbits_wasm.wasm");

describe("Phase 9 — UILess shell: at-rest surface, in-app accessibility, prefers-reduced-motion & asset validation", () => {
  it("draws every control in-app, keeps only the application in the DOM, honors prefers-reduced-motion, and rejects invalid local asset files cleanly", async () => {
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

      // At rest the DOM holds only the canvas and a visually hidden status line: no controls.
      expect(document.body.querySelectorAll("button, input, select, label, nav, header, footer, aside").length).toBe(0);
      // A failed WebGPU attempt may swap in a fresh canvas element so 2D can be claimed; look it up by id.
      const liveCanvas = document.getElementById("osdbits-canvas") as HTMLCanvasElement;
      expect(liveCanvas.getAttribute("aria-label")).toBeTruthy();
      expect(liveCanvas.getAttribute("role")).toBe("application");
      const visibleChildren = Array.from(appEl.children).filter(
        (el) => el.tagName !== "CANVAS" && !el.classList.contains("sr-only"),
      );
      expect(visibleChildren).toEqual([]);

      // Every control is drawn in-app. The title is drawn on the first composed frame.
      await waitFor(() => app.getDrawnUiText().length > 0);
      const drawn = app.getDrawnUiText().join(" | ").toLowerCase();
      expect(drawn).toContain("webgpu unavailable");
      expect(drawn).toContain("proceed with canvas 2d");
      expect(app.uiHitCenter("title-canvas2d")?.enabled).toBe(true);
      expect(app.uiHitCenter("title-start")).toBeNull();

      // Simulate selecting an invalid/corrupt local resource file -> non-crashing rejection message
      const bogusBytes = new Uint8Array([0xde, 0xad, 0xbe, 0xef, 0x00, 0x01]);
      const ok = await app.importResourceBytes(bogusBytes);
      expect(ok).toBe(false);
      expect(app.getPanelStatus().resource).toContain("Import rejected:");

      app.dispose();
    } finally {
      window.matchMedia = originalMatchMedia;
    }
  });
});

async function waitFor(predicate: () => boolean, timeoutMs = 2_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!predicate() && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  expect(predicate()).toBe(true);
}
