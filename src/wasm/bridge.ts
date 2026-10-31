/**
 * Typed zero-copy TypeScript host bridge for `osdbits_core.wasm`.
 *
 * Ownership model (per `docs/architecture.md`):
 * - Rust/WASM owns deterministic simulation, procedural clean-room textures,
 *   safe `ROMDIR` + LZ import validation, and contiguous render command buffers.
 * - TypeScript reads `RenderVertex[]`, `RenderBatch[]`, `FrameTelemetry`, and
 *   RGBA8 texture slices directly out of `WebAssembly.Memory` without per-frame
 *   copies or JSON serialization.
 */

export const EXPECTED_ABI_VERSION = 1;

export const enum AppMode {
  Opening = 0,
  Idle = 1,
  Illegal = 2,
  Menu = 3,
}

export const enum BootCompletionPolicy {
  TransitionToMenu = 0,
  TransitionToIdle = 1,
  LoopOpening = 2,
}

export const enum MenuPage {
  MainMenu = 0,
  SystemConfig = 1,
  ClockAdjust = 2,
  BrowserShowcase = 3,
  VersionOverlay = 4,
}

export const enum MenuAction {
  Up = 0,
  Down = 1,
  Left = 2,
  Right = 3,
  Confirm = 4,
  Cancel = 5,
  VersionInfo = 6,
}

export const enum AudioCueBit {
  CursorTick = 1,
  Confirm = 2,
  Cancel = 4,
  TransitionIn = 8,
  TransitionOut = 16,
}

export const enum TextureSlot {
  Untextured = 0,
  TowerWall = 1,
  FogCloud = 2,
  LightStreak = 3,
  LensFlare = 4,
  CubeBluePurple = 5,
  CubeBlueDeep = 6,
  CubeReflect = 7,
  SmokeTunnel = 8,
  OrbHalo = 9,
  OrbCore = 10,
  GlassBump = 11,
  GlassBumpInv = 12,
  GlassFlow = 13,
  ConfigCubeRef = 14,
  UiGlyphAtlas = 15,
  BackdropUntinted = 16,
  BackdropTinted = 17,
  ExtraBloom = 18,
}

export const enum BlendMode {
  Opaque = 0,
  SrcAlpha = 1,
  Additive = 2,
  AdditiveSrc = 3,
  Subtractive = 4,
}

export const enum DepthMode {
  Disabled = 0,
  ReadOnly = 1,
  ReadWrite = 2,
}

export const enum PassActionBit {
  ClampUv = 1,
  CaptureUntintedBackdrop = 2,
  CaptureTintedBackdrop = 4,
  CaptureBloomDownsample = 8,
}

export const enum TargetId {
  MainScreen = 0,
  ExtraBloom = 1,
}

export interface WasmRawExports extends WebAssembly.Exports {
  memory: WebAssembly.Memory;
  osdbits_abi_version(): number;
  osdbits_sizeof_render_vertex(): number;
  osdbits_sizeof_render_batch(): number;
  osdbits_sizeof_telemetry(): number;
  osdbits_using_imported_assets(ptr: number): number;
  osdbits_engine_new(seed: number): number;
  osdbits_engine_free(ptr: number): void;
  osdbits_set_mode(ptr: number, mode: number, resetTimeline: number): void;
  osdbits_set_boot_policy(ptr: number, policy: number): void;
  osdbits_set_seed(ptr: number, seed: number): void;
  osdbits_set_tuning(
    ptr: number,
    isPal: number,
    speedScale: number,
    idleDrift: number,
    fog: number,
    bloom: number,
    refraction: number,
    reducedMotion: number,
    screenSize: number,
    showBanner: number,
    language: number,
    confirmCircle: number,
    towerGames: number,
  ): void;
  osdbits_set_clock(
    ptr: number,
    year: number,
    month: number,
    day: number,
    hour: number,
    minute: number,
    second: number,
    lockExternal: number,
  ): void;
  osdbits_menu_action(ptr: number, action: number): number;
  osdbits_pointer_click(ptr: number, nx: number, ny: number): number;
  osdbits_step_and_render(ptr: number, dtScale: number): number;
  osdbits_vertices_ptr(ptr: number): number;
  osdbits_batches_ptr(ptr: number): number;
  osdbits_texture_generation(ptr: number): number;
  osdbits_texture_count(ptr: number): number;
  osdbits_texture_meta(ptr: number, index: number, outMetaPtr: number): number;
  osdbits_alloc_bytes(len: number): number;
  osdbits_free_bytes(ptr: number, len: number): void;
  osdbits_import_container(ptr: number, dataPtr: number, dataLen: number): number;
  osdbits_import_sha256_ptr(ptr: number): number;
  osdbits_reset_procedural_textures(ptr: number): void;
  osdbits_build_synthetic_teximage(seed: number, outLenPtr: number): number;
}

export interface EngineTuningParams {
  isPal: boolean;
  speedScale: number;
  idleDrift: number;
  fogEnabled: boolean;
  bloomEnabled: boolean;
  refractionEnabled: boolean;
  reducedMotion: boolean;
  /** `0 = 4:3`, `1 = Full`, `2 = 16:9` */
  screenSize: number;
  showBanner: boolean;
  /** `0..=6` (`EN, FR, ES, DE, IT, NL, PT`) */
  language: number;
  confirmWithCircle: boolean;
  /** `0..=21` active memory card title slots */
  towerGames: number;
}

export const DEFAULT_ENGINE_TUNING: EngineTuningParams = {
  isPal: false,
  speedScale: 1.0,
  idleDrift: 1.0,
  fogEnabled: true,
  bloomEnabled: true,
  refractionEnabled: true,
  reducedMotion: false,
  screenSize: 0,
  showBanner: true,
  language: 0,
  confirmWithCircle: false,
  towerGames: 21,
};

export interface FrameTelemetrySnapshot {
  mode: AppMode;
  frameCount: number;
  vertexCount: number;
  batchCount: number;
  seed: number;
  openingState: number;
  menuPage: MenuPage;
  audioCues: number;
  clearColor: [number, number, number, number];
  camPos: [number, number, number];
  completed: boolean;
}

export interface DecodedRenderBatch {
  firstVertex: number;
  vertexCount: number;
  textureId: TextureSlot;
  blendMode: BlendMode;
  depthMode: DepthMode;
  passAction: number;
  targetId: TargetId;
}

export interface TextureSliceView {
  slot: TextureSlot;
  width: number;
  height: number;
  pixels: Uint8Array;
}

export type ImportContainerResult =
  | { ok: true; importedTextureCount: number; sha256Hex: string }
  | { ok: false; errorCode: number; message: string };

const IMPORT_ERROR_MESSAGES: Record<number, string> = {
  [-1]: 'Selected file is empty.',
  [-2]: 'Selected file exceeds the 8 MiB safety limit.',
  [-3]: 'No valid PS2 ROMDIR or TEXIMAGE header was found in the selected file.',
  [-4]: 'ROMDIR directory table is truncated or corrupt.',
  [-5]: 'ROMDIR directory exceeds the 256-entry safety bound.',
  [-6]: 'A ROMDIR entry points outside the file bounds.',
  [-7]: 'Compressed texture stream header is truncated.',
  [-8]: 'Decompressed texture exceeds the 1 MiB subfile safety cap.',
  [-9]: 'Compressed LZ stream ended prematurely.',
  [-10]: 'Compressed LZ stream contains an out-of-bounds backreference.',
  [-11]: 'Container parsed cleanly, but contained no recognized OSDSYS textures.',
  [-100]: 'Invalid WASM engine pointer.',
};

function assertExportShape(exports: WebAssembly.Exports): WasmRawExports {
  const candidate = exports as Partial<WasmRawExports>;
  if (
    !(candidate.memory instanceof WebAssembly.Memory) ||
    typeof candidate.osdbits_abi_version !== 'function' ||
    typeof candidate.osdbits_engine_new !== 'function' ||
    typeof candidate.osdbits_step_and_render !== 'function'
  ) {
    throw new Error('WASM module is missing required osdbits_core exports.');
  }
  const raw = candidate as WasmRawExports;
  const abi = raw.osdbits_abi_version();
  if (abi !== EXPECTED_ABI_VERSION) {
    throw new Error(
      `WASM ABI mismatch: expected v${EXPECTED_ABI_VERSION}, received v${abi}.`,
    );
  }
  if (
    raw.osdbits_sizeof_render_vertex() !== 32 ||
    raw.osdbits_sizeof_render_batch() !== 32 ||
    raw.osdbits_sizeof_telemetry() !== 64
  ) {
    throw new Error('WASM C-ABI struct layout invariant check failed.');
  }
  return raw;
}

/**
 * Instantiate `osdbits_core.wasm` from bytes or a fetch URL.
 */
export async function loadOsdbitsWasm(
  source: BufferSource | string = './wasm/osdbits_core.wasm',
): Promise<WasmRawExports> {
  let instance: WebAssembly.Instance;
  if (typeof source === 'string') {
    const resp = await fetch(source);
    if (!resp.ok) {
      throw new Error(`Failed to fetch WASM binary (${resp.status} ${resp.statusText}).`);
    }
    if (typeof WebAssembly.instantiateStreaming === 'function') {
      try {
        const streamed = await WebAssembly.instantiateStreaming(resp.clone(), {});
        instance = streamed.instance;
      } catch {
        const bytes = await resp.arrayBuffer();
        const fallback = await WebAssembly.instantiate(bytes, {});
        instance = fallback.instance;
      }
    } else {
      const bytes = await resp.arrayBuffer();
      const loaded = await WebAssembly.instantiate(bytes, {});
      instance = loaded.instance;
    }
  } else {
    const loaded = await WebAssembly.instantiate(source, {});
    instance = loaded.instance;
  }
  return assertExportShape(instance.exports);
}

/**
 * Stateful wrapper around one WASM `Engine` allocation with zero-copy buffer views.
 */
export class OsdbitsWasmEngine {
  public readonly raw: WasmRawExports;
  private ptr: number;
  private metaScratchPtr: number;
  private lastVertexCount = 0;
  private lastBatchCount = 0;
  private currentTuning: EngineTuningParams = { ...DEFAULT_ENGINE_TUNING };

  public constructor(raw: WasmRawExports, seed = 0x2000_0304) {
    this.raw = raw;
    this.ptr = raw.osdbits_engine_new(seed >>> 0);
    if (this.ptr === 0) {
      throw new Error('Failed to allocate WASM OsdbitsWasmEngine instance.');
    }
    this.metaScratchPtr = raw.osdbits_alloc_bytes(12);
    if (this.metaScratchPtr === 0) {
      raw.osdbits_engine_free(this.ptr);
      this.ptr = 0;
      throw new Error('Failed to allocate WASM metadata scratch buffer.');
    }
  }

  private ensureAlive(): number {
    if (this.ptr === 0) {
      throw new Error('OsdbitsWasmEngine was already disposed.');
    }
    return this.ptr;
  }

  public dispose(): void {
    if (this.metaScratchPtr !== 0) {
      this.raw.osdbits_free_bytes(this.metaScratchPtr, 12);
      this.metaScratchPtr = 0;
    }
    if (this.ptr !== 0) {
      this.raw.osdbits_engine_free(this.ptr);
      this.ptr = 0;
    }
  }

  public get memoryByteLength(): number {
    return this.raw.memory.buffer.byteLength;
  }

  public setMode(mode: AppMode, resetScene = true): void {
    this.raw.osdbits_set_mode(this.ensureAlive(), mode, resetScene ? 1 : 0);
  }

  public setSeed(seed: number): void {
    this.raw.osdbits_set_seed(this.ensureAlive(), seed >>> 0);
  }

  public setBootCompletionPolicy(policy: BootCompletionPolicy): void {
    this.raw.osdbits_set_boot_policy(this.ensureAlive(), policy);
  }

  public setTuning(tuning: EngineTuningParams): void {
    this.currentTuning = { ...tuning };
    this.raw.osdbits_set_tuning(
      this.ensureAlive(),
      tuning.isPal ? 1 : 0,
      tuning.speedScale,
      tuning.idleDrift,
      tuning.fogEnabled ? 1 : 0,
      tuning.bloomEnabled ? 1 : 0,
      tuning.refractionEnabled ? 1 : 0,
      tuning.reducedMotion ? 1 : 0,
      tuning.screenSize >>> 0,
      tuning.showBanner ? 1 : 0,
      tuning.language >>> 0,
      tuning.confirmWithCircle ? 1 : 0,
      tuning.towerGames >>> 0,
    );
  }

  public getTuning(): Readonly<EngineTuningParams> {
    return this.currentTuning;
  }

  public setRtc(date: Date, lockExternal = false): void {
    this.raw.osdbits_set_clock(
      this.ensureAlive(),
      date.getUTCFullYear(),
      date.getUTCMonth() + 1,
      date.getUTCDate(),
      date.getUTCHours(),
      date.getUTCMinutes(),
      date.getUTCSeconds(),
      lockExternal ? 1 : 0,
    );
  }

  public dispatchMenuAction(action: MenuAction): number {
    return this.raw.osdbits_menu_action(this.ensureAlive(), action) >>> 0;
  }

  public dispatchPointerClick(normX: number, normY: number): number {
    return this.raw.osdbits_pointer_click(this.ensureAlive(), normX, normY) >>> 0;
  }

  /**
   * Advance simulation by `dtFrames` (at 60fps nominal, `1.0` = one frame)
   * and prepare zero-copy vertex + batch buffers in WASM linear memory.
   */
  public stepAndRender(dtFrames = 1.0): FrameTelemetrySnapshot {
    const telemetryPtr = this.raw.osdbits_step_and_render(this.ensureAlive(), dtFrames);
    const buf = this.raw.memory.buffer;
    const u32 = new Uint32Array(buf, telemetryPtr, 16);
    const f32 = new Float32Array(buf, telemetryPtr, 16);

    const vertexCount = u32[2] ?? 0;
    const batchCount = u32[3] ?? 0;
    this.lastVertexCount = vertexCount;
    this.lastBatchCount = batchCount;

    return {
      mode: (u32[0] ?? 0) as AppMode,
      frameCount: u32[1] ?? 0,
      vertexCount,
      batchCount,
      seed: u32[4] ?? 0,
      openingState: u32[5] ?? 0,
      menuPage: (u32[6] ?? 0) as MenuPage,
      audioCues: u32[7] ?? 0,
      clearColor: [f32[8] ?? 0, f32[9] ?? 0, f32[10] ?? 0, f32[11] ?? 1],
      camPos: [f32[12] ?? 0, f32[13] ?? 0, f32[14] ?? 0],
      completed: (u32[15] ?? 0) !== 0,
    };
  }

  /**
   * Zero-copy byte view of the prepared `RenderVertex[]` buffer (`vertexCount * 32` bytes).
   * Safe directly as the `data` source for `device.queue.writeBuffer(...)`.
   */
  public getVertexBytes(): Uint8Array {
    const p = this.ensureAlive();
    const vPtr = this.raw.osdbits_vertices_ptr(p);
    return new Uint8Array(this.raw.memory.buffer, vPtr, this.lastVertexCount * 32);
  }

  /**
   * Decode the current frame's `RenderBatch` commands from WASM linear memory.
   */
  public getBatches(): DecodedRenderBatch[] {
    const p = this.ensureAlive();
    const bPtr = this.raw.osdbits_batches_ptr(p);
    const bLen = this.lastBatchCount;
    const words = new Uint32Array(this.raw.memory.buffer, bPtr, bLen * 8);
    const out: DecodedRenderBatch[] = [];
    for (let i = 0; i < bLen; i++) {
      const base = i * 8;
      out.push({
        firstVertex: words[base] ?? 0,
        vertexCount: words[base + 1] ?? 0,
        textureId: (words[base + 2] ?? 0) as TextureSlot,
        blendMode: (words[base + 3] ?? 0) as BlendMode,
        depthMode: (words[base + 4] ?? 0) as DepthMode,
        passAction: words[base + 5] ?? 0,
        targetId: (words[base + 6] ?? 0) as TargetId,
      });
    }
    return out;
  }

  public get textureGeneration(): number {
    return this.raw.osdbits_texture_generation(this.ensureAlive()) >>> 0;
  }

  public get usingImportedAssets(): boolean {
    return this.raw.osdbits_using_imported_assets(this.ensureAlive()) !== 0;
  }

  /**
   * Return all static/procedural/imported RGBA8 texture views (`15` slots).
   */
  public getAllTextures(): TextureSliceView[] {
    const p = this.ensureAlive();
    const count = this.raw.osdbits_texture_count(p);
    const out: TextureSliceView[] = [];
    for (let idx = 0; idx < count; idx++) {
      const pixPtr = this.raw.osdbits_texture_meta(p, idx, this.metaScratchPtr);
      if (pixPtr === 0) continue;
      const meta = new Uint32Array(this.raw.memory.buffer, this.metaScratchPtr, 3);
      const slot = (meta[0] ?? 0) as TextureSlot;
      const width = meta[1] ?? 0;
      const height = meta[2] ?? 0;
      const byteLen = width * height * 4;
      if (byteLen === 0) continue;
      out.push({
        slot,
        width,
        height,
        pixels: new Uint8Array(this.raw.memory.buffer, pixPtr, byteLen),
      });
    }
    return out;
  }

  public getTextureView(slot: TextureSlot): TextureSliceView | null {
    return this.getAllTextures().find((t) => t.slot === slot) ?? null;
  }

  /**
   * Validate and import a user-supplied `TEXIMAGE` or BIOS `ROMDIR` binary in WASM.
   */
  public importContainer(bytes: Uint8Array): ImportContainerResult {
    const p = this.ensureAlive();
    if (bytes.byteLength === 0) {
      return {
        ok: false,
        errorCode: -1,
        message: IMPORT_ERROR_MESSAGES[-1] ?? 'Empty file.',
      };
    }
    if (bytes.byteLength > 8 * 1024 * 1024) {
      return {
        ok: false,
        errorCode: -2,
        message: IMPORT_ERROR_MESSAGES[-2] ?? 'File too large.',
      };
    }

    const bufPtr = this.raw.osdbits_alloc_bytes(bytes.byteLength);
    if (bufPtr === 0) {
      return {
        ok: false,
        errorCode: -100,
        message: 'Failed to allocate temporary WASM import buffer.',
      };
    }

    try {
      new Uint8Array(this.raw.memory.buffer, bufPtr, bytes.byteLength).set(bytes);
      const rc = this.raw.osdbits_import_container(p, bufPtr, bytes.byteLength);
      if (rc < 0) {
        return {
          ok: false,
          errorCode: rc,
          message: IMPORT_ERROR_MESSAGES[rc] ?? `Import failed (code ${rc}).`,
        };
      }
      const shaPtr = this.raw.osdbits_import_sha256_ptr(p);
      const shaAscii = new Uint8Array(this.raw.memory.buffer, shaPtr, 64);
      const sha256Hex = new TextDecoder().decode(shaAscii);
      return {
        ok: true,
        importedTextureCount: rc,
        sha256Hex,
      };
    } finally {
      this.raw.osdbits_free_bytes(bufPtr, bytes.byteLength);
    }
  }

  public resetProceduralTextures(): void {
    this.raw.osdbits_reset_procedural_textures(this.ensureAlive());
  }

  /**
   * Build a clean-room synthetic `TEXIMAGE` archive (zero Sony bytes) for testing.
   */
  public buildSyntheticTeximage(seed: number): Uint8Array {
    const outLenPtr = this.raw.osdbits_alloc_bytes(4);
    if (outLenPtr === 0) {
      throw new Error('Failed to allocate length word.');
    }
    try {
      const dataPtr = this.raw.osdbits_build_synthetic_teximage(seed >>> 0, outLenPtr);
      const len = new DataView(this.raw.memory.buffer).getUint32(outLenPtr, true);
      if (dataPtr === 0 || len === 0) {
        throw new Error('Failed to build synthetic TEXIMAGE.');
      }
      try {
        return new Uint8Array(this.raw.memory.buffer, dataPtr, len).slice();
      } finally {
        this.raw.osdbits_free_bytes(dataPtr, len);
      }
    } finally {
      this.raw.osdbits_free_bytes(outLenPtr, 4);
    }
  }
}
