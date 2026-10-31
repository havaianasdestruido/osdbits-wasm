/**
 * Strict, zero-`any` TypeScript bridge to `osdbits-wasm` (`wasm32-unknown-unknown`).
 *
 * Provides deterministic simulation lifecycle, Mode A–E configuration & live overrides,
 * Mode D menu input dispatch, zero-copy `Float32Array` / `Uint32Array` render-stream views
 * safe across linear-memory growth, structured `FrameInspectionSnapshot` decoding, and
 * bounds-checked `ps2expand` LZ / `ROMDIR` validation for optional user-supplied resources.
 */

export const EXPECTED_ABI_MAGIC = 0x4f53_4442;
export const EXPECTED_ABI_VERSION = 1;
export const EXPECTED_VERTEX_FLOATS = 12;
export const EXPECTED_CMD_WORDS = 8;

export type VisualModeName = "opening" | "idle" | "illegal-disc" | "menu";

export type OsdLanguageCode = 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7;

export type MenuActionName =
  | "up"
  | "down"
  | "left"
  | "right"
  | "confirm"
  | "cancel"
  | "options"
  | "display";

export type MenuEntryProvenanceTag = "sourced" | "approximate" | "extension";

export const enum DrawCommandKindId {
  Triangles = 0,
  Lines = 1,
  CaptureScreenToWork = 2,
  ApplyExtraBufFeedback = 3,
  ApplyMotionBlur = 4,
  ApplyMenuBackdropTint = 5,
  ApplyMenuZoomBlur = 6,
  DrawFullScreenFade = 7,
  DrawBannerOverlay = 8,
  DrawIllegalTextOverlay = 9,
  DrawLetterboxBars = 10,
}

export interface DecodedPipelineFlags {
  abe: boolean;
  blendMode: number;
  blendFix: number;
  depthTest: boolean;
  depthWrite: boolean;
  uvClamp: boolean;
}

export interface DecodedDrawCommand {
  kind: DrawCommandKindId;
  target: 0 | 1;
  textureId: number;
  flags: DecodedPipelineFlags;
  firstVertex: number;
  vertexCount: number;
  aux0: number;
  aux1: number;
}

export interface MenuUiItemSnapshot {
  label: string;
  value: string;
  selected: boolean;
  alpha: number;
  provenance: MenuEntryProvenanceTag;
}

export interface MenuUiSnapshot {
  screen:
    | "main"
    | "browser-approx"
    | "config-in"
    | "config-list"
    | "config-edit-val"
    | "config-edit-clock"
    | "config-out"
    | "version-approx"
    | "none";
  /** True when the core accepts menu input; false during the fade-in/90-frame
   * choreography. The shell disables its controls while this is false. */
  interactive: boolean;
  date: string;
  time: string;
  title: string;
  hintConfirm: string;
  hintCancel: string;
  showOptions: boolean;
  markerAlpha: number;
  clockCursor: number;
  items: MenuUiItemSnapshot[];
  clockFields?: string[];
}

export interface FrameInspectionSnapshot {
  mode: VisualModeName;
  modeId: number;
  frame: number;
  seed: number;
  width: number;
  height: number;
  camPos: [number, number, number];
  camRot: number;
  openingState: number;
  finished: boolean;
  towers: number;
  cubes: number;
  orbs: number;
  rods: number;
  commands: number;
  vertices: number;
  bannerAlpha: number;
  illegalTextAlpha: number;
  fadeAlpha: number;
  illegalMsg: string;
  audioCues: number[];
  menuUi: MenuUiSnapshot | null;
}

export type TowerPresetSpec =
  | { kind: "captured" }
  | { kind: "full" }
  | { kind: "simulated"; seed: number; ngames: number; nboots?: number };

export interface ClockInitSpec {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  second: number;
}

export interface SceneEffectFlags {
  bootToGame?: boolean;
  autoTransitionToMenu?: boolean;
  pal?: boolean;
  reducedMotion?: boolean;
  enableFog?: boolean;
  enableLights?: boolean;
  enableCubes?: boolean;
  enableGlow?: boolean;
  enableMotionBlur?: boolean;
  letterbox?: boolean;
  confirmCancelSwap?: boolean;
}

export interface SceneInitOptions extends SceneEffectFlags {
  mode?: VisualModeName;
  language?: OsdLanguageCode;
  rngSeed?: number;
  towerPreset?: TowerPresetSpec;
  clock?: ClockInitSpec;
}

export interface LiveOverrideOptions extends SceneEffectFlags {
  speedMultiplier?: number;
  camOffsetX?: number;
  camOffsetY?: number;
  language?: OsdLanguageCode;
}

export interface RomdirEntrySummary {
  name: string;
  offset: number;
  size: number;
}

export type ResourceUnpackResult =
  | { ok: true; data: Uint8Array }
  | { ok: false; errorCode: number; reason: string };

export type RomdirInspectResult =
  | { ok: true; entries: RomdirEntrySummary[] }
  | { ok: false; errorCode: number; reason: string };

interface RawWasmExports {
  memory: WebAssembly.Memory;
  osdbits_abi_magic: () => number;
  osdbits_abi_version: () => number;
  osdbits_vertex_stride_floats: () => number;
  osdbits_cmd_stride_words: () => number;
  osdbits_init_scene: (
    mode: number,
    optionFlags: number,
    language: number,
    rngSeed: number,
    towerPresetKind: number,
    towerSeed: number,
    towerNgames: number,
    towerNboots: number,
    year: number,
    month: number,
    day: number,
    hour: number,
    minute: number,
    second: number,
  ) => number;
  osdbits_set_live_overrides: (
    speedMultiplier: number,
    camOffsetX: number,
    camOffsetY: number,
    optionFlags: number,
    language: number,
  ) => number;
  osdbits_dispatch_action: (actionCode: number) => number;
  osdbits_enter_system_config: () => void;
  osdbits_step_frame: () => number;
  osdbits_vertex_ptr: () => number;
  osdbits_vertex_float_len: () => number;
  osdbits_cmd_ptr: () => number;
  osdbits_cmd_u32_len: () => number;
  osdbits_inspect_ptr: () => number;
  osdbits_inspect_len: () => number;
  osdbits_prepare_upload: (byteLen: number) => number;
  osdbits_unpack_lz_upload: () => number;
  osdbits_inspect_romdir_upload: () => number;
  osdbits_unpacked_ptr: () => number;
  osdbits_unpacked_len: () => number;
}

const MODE_NAME_TO_ID: Record<VisualModeName, number> = {
  opening: 0,
  idle: 1,
  "illegal-disc": 2,
  menu: 3,
};

const ACTION_NAME_TO_CODE: Record<MenuActionName, number> = {
  up: 0,
  down: 1,
  left: 2,
  right: 3,
  confirm: 4,
  cancel: 5,
  options: 6,
  display: 7,
};

const RESOURCE_ERROR_REASONS: Record<number, string> = {
  [-1]: "Compressed stream shorter than 4-byte size header",
  [-2]: "Declared expanded size exceeds 16 MiB safety cap",
  [-3]: "Unexpected end of compressed LZ stream",
  [-4]: "LZ back-reference offset exceeds produced output",
  [-5]: "No RESET/ROMDIR directory table found in image",
  [-6]: "Malformed or out-of-bounds ROMDIR directory entry",
};

const REQUIRED_FN_EXPORTS: ReadonlyArray<Exclude<keyof RawWasmExports, "memory">> = [
  "osdbits_abi_magic",
  "osdbits_abi_version",
  "osdbits_vertex_stride_floats",
  "osdbits_cmd_stride_words",
  "osdbits_init_scene",
  "osdbits_set_live_overrides",
  "osdbits_dispatch_action",
  "osdbits_enter_system_config",
  "osdbits_step_frame",
  "osdbits_vertex_ptr",
  "osdbits_vertex_float_len",
  "osdbits_cmd_ptr",
  "osdbits_cmd_u32_len",
  "osdbits_inspect_ptr",
  "osdbits_inspect_len",
  "osdbits_prepare_upload",
  "osdbits_unpack_lz_upload",
  "osdbits_inspect_romdir_upload",
  "osdbits_unpacked_ptr",
  "osdbits_unpacked_len",
];

function validateRawExports(exportsObj: WebAssembly.Exports): RawWasmExports {
  if (!(exportsObj.memory instanceof WebAssembly.Memory)) {
    throw new Error("WASM module missing exported WebAssembly.Memory");
  }
  for (const fnName of REQUIRED_FN_EXPORTS) {
    if (typeof exportsObj[fnName] !== "function") {
      throw new Error(`WASM module missing required C-ABI function export: ${fnName}`);
    }
  }
  return exportsObj as unknown as RawWasmExports;
}

export function encodeSceneOptionFlags(flags: SceneEffectFlags): number {
  const bootToGame = flags.bootToGame ?? false;
  const autoTransitionToMenu = flags.autoTransitionToMenu ?? true;
  const pal = flags.pal ?? false;
  const reducedMotion = flags.reducedMotion ?? false;
  const enableFog = flags.enableFog ?? true;
  const enableLights = flags.enableLights ?? true;
  const enableCubes = flags.enableCubes ?? true;
  const enableGlow = flags.enableGlow ?? true;
  const enableMotionBlur = flags.enableMotionBlur ?? true;
  const letterbox = flags.letterbox ?? false;
  const confirmCancelSwap = flags.confirmCancelSwap ?? true;

  return (
    ((bootToGame ? 1 : 0) << 0) |
    ((autoTransitionToMenu ? 1 : 0) << 1) |
    ((pal ? 1 : 0) << 2) |
    ((reducedMotion ? 1 : 0) << 3) |
    ((enableFog ? 1 : 0) << 4) |
    ((enableLights ? 1 : 0) << 5) |
    ((enableCubes ? 1 : 0) << 6) |
    ((enableGlow ? 1 : 0) << 7) |
    ((enableMotionBlur ? 1 : 0) << 8) |
    ((letterbox ? 1 : 0) << 9) |
    ((confirmCancelSwap ? 1 : 0) << 10)
  ) >>> 0;
}

export function decodePipelineFlags(packed: number): DecodedPipelineFlags {
  return {
    abe: (packed & 1) !== 0,
    blendMode: (packed >>> 1) & 0xff,
    blendFix: (packed >>> 9) & 0xff,
    depthTest: ((packed >>> 17) & 1) !== 0,
    depthWrite: ((packed >>> 18) & 1) !== 0,
    uvClamp: ((packed >>> 19) & 1) !== 0,
  };
}

export class WasmSimulationBridge {
  private readonly raw: RawWasmExports;
  private readonly utf8Decoder = new TextDecoder("utf-8", { fatal: true });
  private currentOptionFlags: number;
  private currentLanguage: OsdLanguageCode = 1;

  private constructor(raw: RawWasmExports) {
    this.raw = raw;
    this.currentOptionFlags = encodeSceneOptionFlags({});

    const magic = this.raw.osdbits_abi_magic() >>> 0;
    const version = this.raw.osdbits_abi_version() >>> 0;
    const vStride = this.raw.osdbits_vertex_stride_floats() >>> 0;
    const cStride = this.raw.osdbits_cmd_stride_words() >>> 0;

    if (magic !== EXPECTED_ABI_MAGIC) {
      throw new Error(
        `WASM ABI magic mismatch: expected 0x${EXPECTED_ABI_MAGIC.toString(16)}, got 0x${magic.toString(16)}`,
      );
    }
    if (version !== EXPECTED_ABI_VERSION) {
      throw new Error(`WASM ABI version mismatch: expected ${EXPECTED_ABI_VERSION}, got ${version}`);
    }
    if (vStride !== EXPECTED_VERTEX_FLOATS || cStride !== EXPECTED_CMD_WORDS) {
      throw new Error(
        `WASM stream stride mismatch: vertex=${vStride} (expected ${EXPECTED_VERTEX_FLOATS}), cmd=${cStride} (expected ${EXPECTED_CMD_WORDS})`,
      );
    }
  }

  static async instantiateFromBytes(bytes: BufferSource): Promise<WasmSimulationBridge> {
    const result = await WebAssembly.instantiate(bytes, {});
    const instance = "instance" in result ? result.instance : result;
    const raw = validateRawExports(instance.exports);
    const bridge = new WasmSimulationBridge(raw);
    bridge.initScene();
    return bridge;
  }

  static async instantiateFromUrl(wasmUrl = "./wasm/osdbits_wasm.wasm"): Promise<WasmSimulationBridge> {
    const response = await fetch(wasmUrl);
    if (!response.ok) {
      throw new Error(`Failed to fetch WASM binary at ${wasmUrl} (HTTP ${response.status})`);
    }
    const bytes = await response.arrayBuffer();
    return WasmSimulationBridge.instantiateFromBytes(bytes);
  }

  get abiMagic(): number {
    return this.raw.osdbits_abi_magic() >>> 0;
  }

  get abiVersion(): number {
    return this.raw.osdbits_abi_version() >>> 0;
  }

  get memoryByteLength(): number {
    return this.raw.memory.buffer.byteLength;
  }

  initScene(options: SceneInitOptions = {}): void {
    const modeName = options.mode ?? "opening";
    if (!(modeName in MODE_NAME_TO_ID)) {
      throw new RangeError(`Unsupported VisualModeName: ${String(modeName)}`);
    }
    const modeId = MODE_NAME_TO_ID[modeName];
    const language = options.language ?? 1;
    if (!Number.isInteger(language) || language < 0 || language > 7) {
      throw new RangeError(`Language code out of range [0..7]: ${String(language)}`);
    }

    const optionFlags = encodeSceneOptionFlags(options);
    const rngSeed = (options.rngSeed ?? 1) >>> 0;

    let presetKind = 0;
    let towerSeed = 0;
    let towerNgames = 0;
    let towerNboots = 0;
    if (options.towerPreset) {
      switch (options.towerPreset.kind) {
        case "captured":
          presetKind = 0;
          break;
        case "full":
          presetKind = 1;
          break;
        case "simulated":
          presetKind = 2;
          towerSeed = options.towerPreset.seed >>> 0;
          towerNgames = Math.max(0, Math.min(126, Math.trunc(options.towerPreset.ngames)));
          towerNboots = Math.max(0, Math.min(50_000, Math.trunc(options.towerPreset.nboots ?? 0)));
          break;
      }
    }

    const clk: ClockInitSpec = options.clock ?? {
      year: 2026,
      month: 10,
      day: 31,
      hour: 20,
      minute: 30,
      second: 0,
    };

    const ok = this.raw.osdbits_init_scene(
      modeId,
      optionFlags,
      language,
      rngSeed,
      presetKind,
      towerSeed,
      towerNgames,
      towerNboots,
      clk.year >>> 0,
      clk.month >>> 0,
      clk.day >>> 0,
      clk.hour >>> 0,
      clk.minute >>> 0,
      clk.second >>> 0,
    );
    if (ok !== 1) {
      throw new Error("osdbits_init_scene rejected configuration parameters");
    }

    this.currentOptionFlags = optionFlags;
    this.currentLanguage = language;
  }

  setLiveOverrides(overrides: LiveOverrideOptions): boolean {
    const speed = overrides.speedMultiplier ?? 1.0;
    const camX = overrides.camOffsetX ?? 0.0;
    const camY = overrides.camOffsetY ?? 0.0;
    const lang = overrides.language ?? this.currentLanguage;
    const flags = encodeSceneOptionFlags({
      bootToGame: (this.currentOptionFlags & (1 << 0)) !== 0,
      autoTransitionToMenu:
        overrides.autoTransitionToMenu ?? (this.currentOptionFlags & (1 << 1)) !== 0,
      pal: (this.currentOptionFlags & (1 << 2)) !== 0,
      reducedMotion: overrides.reducedMotion ?? (this.currentOptionFlags & (1 << 3)) !== 0,
      enableFog: overrides.enableFog ?? (this.currentOptionFlags & (1 << 4)) !== 0,
      enableLights: overrides.enableLights ?? (this.currentOptionFlags & (1 << 5)) !== 0,
      enableCubes: overrides.enableCubes ?? (this.currentOptionFlags & (1 << 6)) !== 0,
      enableGlow: overrides.enableGlow ?? (this.currentOptionFlags & (1 << 7)) !== 0,
      enableMotionBlur:
        overrides.enableMotionBlur ?? (this.currentOptionFlags & (1 << 8)) !== 0,
      letterbox: overrides.letterbox ?? (this.currentOptionFlags & (1 << 9)) !== 0,
      confirmCancelSwap:
        overrides.confirmCancelSwap ?? (this.currentOptionFlags & (1 << 10)) !== 0,
    });

    const ok = this.raw.osdbits_set_live_overrides(speed, camX, camY, flags, lang);
    if (ok === 1) {
      this.currentOptionFlags = flags;
      if (Number.isInteger(lang) && lang >= 0 && lang <= 7) {
        this.currentLanguage = lang;
      }
      return true;
    }
    return false;
  }

  dispatchMenuAction(action: MenuActionName): boolean {
    if (!(action in ACTION_NAME_TO_CODE)) {
      return false;
    }
    return this.raw.osdbits_dispatch_action(ACTION_NAME_TO_CODE[action]) === 1;
  }

  enterSystemConfig(): void {
    this.raw.osdbits_enter_system_config();
  }

  stepFrame(): FrameInspectionSnapshot {
    this.raw.osdbits_step_frame();
    return this.readInspection();
  }

  /**
   * Zero-copy `Float32Array` view over the current frame's universal 12-float vertices
   * (`clip_pos[4]`, `uv_q[4]`, `gs_rgba[4]`). Valid until the next WASM call that mutates memory.
   */
  getVertexView(): Float32Array {
    const ptr = this.raw.osdbits_vertex_ptr() >>> 0;
    const len = this.raw.osdbits_vertex_float_len() >>> 0;
    if (len === 0) {
      return new Float32Array(0);
    }
    return new Float32Array(this.raw.memory.buffer, ptr, len);
  }

  /**
   * Zero-copy `Uint32Array` view over the current frame's 8-word draw commands.
   * Valid until the next WASM call that mutates memory.
   */
  getCommandView(): Uint32Array {
    const ptr = this.raw.osdbits_cmd_ptr() >>> 0;
    const len = this.raw.osdbits_cmd_u32_len() >>> 0;
    if (len === 0) {
      return new Uint32Array(0);
    }
    return new Uint32Array(this.raw.memory.buffer, ptr, len);
  }

  decodeCommands(): DecodedDrawCommand[] {
    const words = this.getCommandView();
    const count = Math.floor(words.length / EXPECTED_CMD_WORDS);
    const out: DecodedDrawCommand[] = [];
    for (let i = 0; i < count; i++) {
      const base = i * EXPECTED_CMD_WORDS;
      out.push({
        kind: words[base] as DrawCommandKindId,
        target: (words[base + 1] === 1 ? 1 : 0) as 0 | 1,
        textureId: words[base + 2] ?? 255,
        flags: decodePipelineFlags(words[base + 3] ?? 0),
        firstVertex: words[base + 4] ?? 0,
        vertexCount: words[base + 5] ?? 0,
        aux0: words[base + 6] ?? 0,
        aux1: words[base + 7] ?? 0,
      });
    }
    return out;
  }

  readInspection(): FrameInspectionSnapshot {
    const ptr = this.raw.osdbits_inspect_ptr() >>> 0;
    const len = this.raw.osdbits_inspect_len() >>> 0;
    if (len === 0) {
      throw new Error("No inspection snapshot available — call stepFrame() first");
    }
    const bytes = new Uint8Array(this.raw.memory.buffer, ptr, len);
    const json = this.utf8Decoder.decode(bytes);
    return JSON.parse(json) as FrameInspectionSnapshot;
  }

  unpackPs2Lz(compressed: Uint8Array): ResourceUnpackResult {
    if (compressed.byteLength === 0) {
      return {
        ok: false,
        errorCode: -1,
        reason: "Empty input buffer",
      };
    }
    const ptr = this.raw.osdbits_prepare_upload(compressed.byteLength) >>> 0;
    if (ptr === 0) {
      return {
        ok: false,
        errorCode: -2,
        reason: "Upload buffer exceeds 16 MiB safety cap",
      };
    }
    new Uint8Array(this.raw.memory.buffer, ptr, compressed.byteLength).set(compressed);
    const status = this.raw.osdbits_unpack_lz_upload();
    if (status < 0) {
      return {
        ok: false,
        errorCode: status,
        reason: RESOURCE_ERROR_REASONS[status] ?? `Resource unpack error (${status})`,
      };
    }
    const outPtr = this.raw.osdbits_unpacked_ptr() >>> 0;
    const outLen = this.raw.osdbits_unpacked_len() >>> 0;
    const copy = new Uint8Array(outLen);
    if (outLen > 0) {
      copy.set(new Uint8Array(this.raw.memory.buffer, outPtr, outLen));
    }
    return { ok: true, data: copy };
  }

  inspectRomdir(container: Uint8Array): RomdirInspectResult {
    if (container.byteLength === 0) {
      return {
        ok: false,
        errorCode: -5,
        reason: "Empty container buffer",
      };
    }
    const ptr = this.raw.osdbits_prepare_upload(container.byteLength) >>> 0;
    if (ptr === 0) {
      return {
        ok: false,
        errorCode: -2,
        reason: "Container exceeds 16 MiB upload cap",
      };
    }
    new Uint8Array(this.raw.memory.buffer, ptr, container.byteLength).set(container);
    const status = this.raw.osdbits_inspect_romdir_upload();
    if (status < 0) {
      return {
        ok: false,
        errorCode: status,
        reason: RESOURCE_ERROR_REASONS[status] ?? `ROMDIR parse error (${status})`,
      };
    }
    const outPtr = this.raw.osdbits_unpacked_ptr() >>> 0;
    const outLen = this.raw.osdbits_unpacked_len() >>> 0;
    const json = this.utf8Decoder.decode(new Uint8Array(this.raw.memory.buffer, outPtr, outLen));
    const entries = JSON.parse(json) as RomdirEntrySummary[];
    return { ok: true, entries };
  }
}
