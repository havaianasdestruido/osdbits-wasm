import "./ui/styles.css";
import { AudioEngine } from "./audio/audioEngine";
import { WebGpuRenderer } from "./renderer/webgpuRenderer";
import {
  DEFAULT_FAITHFUL_SETTINGS,
  RESET_CONFIRMATION_TOKEN,
  StorageManager,
} from "./storage/storageManager";
import { OsdbitsApp } from "./ui/osdbitsApp";
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

async function initBootstrap(): Promise<void> {
  const appEl = document.getElementById("app");
  const canvasEl = document.getElementById("osdbits-canvas");
  if (!appEl || !(canvasEl instanceof HTMLCanvasElement)) return;

  try {
    const bridge = await WasmSimulationBridge.instantiateFromUrl("./wasm/osdbits_wasm.wasm");
    const storage = new StorageManager();
    const audio = new AudioEngine();
    const app = new OsdbitsApp(appEl, canvasEl, bridge, storage, audio);

    await app.initialize();

    window.__osdbitsHarness = {
      app,
      bridge,
      storage,
      audio,
      createRenderer: async (resolutionScale = 1.0) => {
        app.renderer?.dispose();
        app.renderer = null;
        return WebGpuRenderer.create({
          canvas: canvasEl,
          resolutionScale,
          aspectMode: "4:3",
        });
      },
    };

    appEl.dataset.abiMagic = `0x${bridge.abiMagic.toString(16)}`;
    appEl.dataset.abiVersion = String(bridge.abiVersion);
    appEl.dataset.initialMode = app.getLatestSnapshot()?.mode ?? "opening";
    appEl.dataset.appState = "ready";
  } catch (err) {
    appEl.dataset.appState = "error";
    appEl.dataset.appError = err instanceof Error ? err.message : String(err);
  }
}

if (typeof document !== "undefined") {
  void initBootstrap();
}
