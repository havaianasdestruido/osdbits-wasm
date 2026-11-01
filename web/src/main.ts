import "./ui/styles.css";
import { AudioEngine } from "./audio/audioEngine";
import { compositeUiRasterOnCanvas2D } from "./renderer/canvas2dRenderer";
import { WebGpuRenderer } from "./renderer/webgpuRenderer";
import {
  DEFAULT_FAITHFUL_SETTINGS,
  RESET_CONFIRMATION_TOKEN,
  StorageManager,
} from "./storage/storageManager";
import { OsdbitsApp } from "./ui/osdbitsApp";
import { logicalSizeForCanvas, PixelRaster } from "./ui/pixelRaster";
import { WasmSimulationBridge } from "./wasm/bridge";

export {
  WasmSimulationBridge,
  WebGpuRenderer,
  StorageManager,
  AudioEngine,
  OsdbitsApp,
  DEFAULT_FAITHFUL_SETTINGS,
  RESET_CONFIRMATION_TOKEN,
};

export interface BootstrapDiagnostics {
  abiMagic: number;
  abiVersion: number;
  hasWebGpu: boolean;
}

interface RawBootstrapExports {
  osdbits_abi_magic: () => number;
  osdbits_abi_version: () => number;
}

export interface OsdbitsHarness {
  app: OsdbitsApp;
  bridge: WasmSimulationBridge;
  storage: StorageManager;
  audio: AudioEngine;
  createRenderer: (resolutionScale?: number) => Promise<WebGpuRenderer>;
  /** Client-space centre of an in-app UI region (see `OsdbitsApp.uiHitCenter`). */
  uiHitCenter: (id: string) => { x: number; y: number; enabled: boolean } | null;
}

declare global {
  interface Window {
    __osdbitsHarness?: OsdbitsHarness;
  }
}

export async function loadBootstrapWasm(wasmUrl = "./wasm/osdbits_wasm.wasm"): Promise<RawBootstrapExports> {
  const response = await fetch(wasmUrl);
  if (!response.ok) {
    throw new Error(`Failed to fetch WASM module (${response.status})`);
  }
  const bytes = await response.arrayBuffer();
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const exportsObj = instance.exports as unknown as Partial<RawBootstrapExports>;
  if (
    typeof exportsObj.osdbits_abi_magic !== "function" ||
    typeof exportsObj.osdbits_abi_version !== "function"
  ) {
    throw new Error("WASM module missing required ABI handshake exports");
  }
  return {
    osdbits_abi_magic: exportsObj.osdbits_abi_magic,
    osdbits_abi_version: exportsObj.osdbits_abi_version,
  };
}

/**
 * Returns a canvas whose 2D context can be requested. A canvas that already claimed a WebGPU
 * context cannot hand out a 2D one, so it is swapped for an identical fresh element.
 */
function canvasForCanvas2D(canvas: HTMLCanvasElement): HTMLCanvasElement {
  if (canvas.getContext("2d") !== null || !canvas.parentNode) return canvas;
  const fresh = canvas.cloneNode(false) as HTMLCanvasElement;
  canvas.replaceWith(fresh);
  return fresh;
}

/** Minimal in-app fatal state: one line on the scene canvas, in the app's own pixel font. */
function drawFatalMessage(canvas: HTMLCanvasElement, message: string): void {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const size = logicalSizeForCanvas(canvas.width || 640, canvas.height || 480);
  const raster = new PixelRaster(size.width, size.height);
  raster.clear();
  const lines = raster.wrap(`OSDBITS ERROR: ${message}`, size.width - 32, 1);
  let y = Math.round(size.height / 2 - (lines.length * 9) / 2);
  for (const line of lines) {
    raster.textCenter(size.width / 2, y, line, [255, 110, 130, 255]);
    y += 9;
  }
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.fillStyle = "#000";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.restore();
  compositeUiRasterOnCanvas2D(ctx, {
    width: raster.width,
    height: raster.height,
    rgba: raster.rgba,
  });
}

async function initBootstrap(): Promise<void> {
  const appEl = document.getElementById("app");
  let canvasEl = document.getElementById("osdbits-canvas");
  if (!appEl || !(canvasEl instanceof HTMLCanvasElement)) return;

  try {
    const bridge = await WasmSimulationBridge.instantiateFromUrl("./wasm/osdbits_wasm.wasm");
    const storage = new StorageManager();
    const audio = new AudioEngine();
    const app = new OsdbitsApp(appEl, canvasEl, bridge, storage, audio);

    await app.initialize();
    canvasEl = document.getElementById("osdbits-canvas") as HTMLCanvasElement;

    window.__osdbitsHarness = {
      app,
      bridge,
      storage,
      audio,
      createRenderer: async (resolutionScale = 1.0) => {
        app.renderer?.dispose();
        app.renderer = null;
        const target = document.getElementById("osdbits-canvas") as HTMLCanvasElement;
        return WebGpuRenderer.create({
          canvas: target,
          resolutionScale,
          aspectMode: "4:3",
        });
      },
      uiHitCenter: (id: string) => app.uiHitCenter(id),
    };

    appEl.dataset.abiMagic = `0x${bridge.abiMagic.toString(16)}`;
    appEl.dataset.abiVersion = String(bridge.abiVersion);
    appEl.dataset.initialMode = app.getLatestSnapshot()?.mode ?? "opening";
    appEl.dataset.appState = "ready";
  } catch (err) {
    // Fatal: the detail goes to the console; the page shows one line in the app's own style.
    const message = err instanceof Error ? err.message : String(err);
    console.error("[osdbits] fatal startup error:", err);
    appEl.dataset.appState = "error";
    appEl.dataset.appError = message;
    const current = document.getElementById("osdbits-canvas");
    if (current instanceof HTMLCanvasElement) {
      drawFatalMessage(canvasForCanvas2D(current), "SCENE FAILED TO START. SEE CONSOLE.");
    }
  }
}

if (typeof document !== "undefined") {
  void initBootstrap();
}
