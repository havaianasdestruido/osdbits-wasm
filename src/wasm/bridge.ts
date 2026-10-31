/**
 * Typed bridge to the `osdbits-core` WASM module.
 *
 * The module is compiled with `crate-type = ["cdylib"]` and exports a flat C ABI
 * (`crates/osdbits-core/src/ffi.rs`). Everything below mirrors that ABI exactly; the
 * vertex/command/audio buffers are read as typed-array views over WASM linear memory so a
 * frame costs no serialization at all.
 */

export const VERTEX_FLOATS = 10; // pos[4] + uv[2] + color[4]
export const VERTEX_BYTES = VERTEX_FLOATS * 4;
export const COMMAND_U32 = 10;
export const COMMAND_BYTES = COMMAND_U32 * 4;
export const AUDIO_U16 = 4;
export const AUDIO_BYTES = AUDIO_U16 * 2;

export enum SceneMode {
  Opening = 0,
  Idle = 1,
  Illegal = 2,
  Menu = 3,
}

export enum RenderCmdKind {
  ClearTarget = 0,
  DrawTriangles = 1,
  CopyTargetToTexture = 2,
}

export enum RenderTargetId {
  Screen = 0,
  WorkBuf3 = 1,
  WorkBuf4 = 2,
}

export enum GsBlendMode {
  Opaque = 0,
  SourceAlpha = 1,
  Additive = 2,
  Subtractive = 3,
  FixedAdditive = 4,
  FixedBlend = 5,
  DestAlphaAdditive = 6,
}

export enum SamplerWrap {
  Clamp = 0,
  Repeat = 1,
}

export enum DepthMode {
  AlwaysNoWrite = 0,
  AlwaysWrite = 1,
  LessEqualWrite = 2,
  LessEqualNoWrite = 3,
  GreaterWrite = 4,
}

/** Texture slots. Must match `crates/osdbits-core/src/assets.rs::TextureId`. */
export enum TextureId {
  WhiteDummy = 0,
  TexoSce = 1,
  TexoFog0 = 2,
  TexoFog1 = 3,
  TexoFog2 = 4,
  TexoFog3 = 5,
  TexoFog4 = 6,
  TexoWal0 = 7,
  TexoCrle = 8,
  TexoCrbl = 9,
  TexoFlar = 10,
  TexoRef = 11,
  TexoBlp = 12,
  TexoBlpr = 13,
  TexoPngE = 14,
  TexcKabe = 15,
  TexcBlur = 16,
  TexcNavi = 17,
  TexcBump = 18,
  TexcRefa = 19,
  TexcFlow = 20,
  TexcBinv = 21,
  TexcMaru = 22,
  FntAscii = 23,
  FntExOsd = 24,
  ScreenCopy = 25,
  WorkBuf3 = 26,
  WorkBuf4 = 27,
}

export const STATIC_TEXTURE_COUNT = 25;

/** Physical pad bits (`osdbits/inc.h`); the region swap happens inside Rust. */
export const Pad = {
  L2: 0x0001,
  R2: 0x0002,
  L1: 0x0004,
  R1: 0x0008,
  Triangle: 0x0010,
  /** Physical Circle. Canonical "confirm" in Japan, "cancel" elsewhere. */
  Circle: 0x0020,
  /** Physical Cross. Canonical "confirm" outside Japan. */
  Cross: 0x0040,
  Square: 0x0080,
  Select: 0x0100,
  L3: 0x0200,
  R3: 0x0400,
  Start: 0x0800,
  Up: 0x1000,
  Right: 0x2000,
  Down: 0x4000,
  Left: 0x8000,
} as const;

export interface GpuVertex {
  pos: readonly [number, number, number, number];
  uv: readonly [number, number];
  color: readonly [number, number, number, number];
}

export interface RenderCommand {
  kind: RenderCmdKind;
  target: RenderTargetId;
  textureId: number;
  blendMode: GsBlendMode;
  samplerWrap: SamplerWrap;
  depthMode: DepthMode;
  firstVertex: number;
  vertexCount: number;
  blendFix: number;
  clearRgba8: number;
}

export interface AudioEvent {
  msg: number;
  a: number;
  b: number;
  c: number;
}

export interface EngineState {
  mode: number;
  frame: number;
  fps_target: number;
  opening_finished: boolean;
  vertices: number;
  commands: number;
  audio: number;
  format_version: number;
  language: number;
  screen_size: number;
  digital_out: number;
  component_out: number;
  region_japan: boolean;
  screen: string;
  clock: [number, number, number];
  boot_history: number[];
  active_towers: number;
  asset_source: string;
  asset_imported: boolean;
  ui_texture_count: number;
}

interface CoreExports {
  memory: WebAssembly.Memory;
  osd_init(seed: number, regionJapan: number): number;
  osd_set_mode(mode: number): void;
  osd_get_mode(): number;
  osd_step(physicalButtons: number): void;
  osd_set_paused(paused: number): void;
  osd_set_speed(speed: number): void;
  osd_set_reduced_motion(on: number): void;
  osd_set_fx(fogIntensity: number, refractionScale: number): void;
  osd_vertices_ptr(): number;
  osd_vertex_count(): number;
  osd_commands_ptr(): number;
  osd_command_count(): number;
  osd_audio_ptr(): number;
  osd_audio_count(): number;
  osd_drain_audio(): void;
  osd_vertex_stride(): number;
  osd_command_stride(): number;
  osd_audio_stride(): number;
  osd_texture_count(): number;
  osd_texture_ptr(id: number): number;
  osd_texture_width(id: number): number;
  osd_texture_height(id: number): number;
  osd_set_language(lang: number): void;
  osd_set_config(screenSize: number, digitalOut: number, componentOut: number): void;
  osd_set_region_japan(on: number): void;
  osd_set_clock(hours: number, minutes: number, seconds: number): void;
  osd_set_boot_history(ptr: number, len: number): void;
  osd_state_json_ptr(): number;
  osd_state_json_len(): number;
  osd_expand_lzss(ptr: number, len: number): number;
  osd_expand_result_ptr(): number;
  osd_import_romdir(ptr: number, len: number): number;
  osd_reset_assets(): void;
}

export class WasmCore {
  private exports: CoreExports;
  private encoder = new TextEncoder();
  private decoder = new TextDecoder();

  private constructor(exports: CoreExports) {
    this.exports = exports;
  }

  static async load(url: string): Promise<WasmCore> {
    const response = await fetch(url);
    if (!response.ok) {
      throw new Error(`Failed to fetch WASM module (${response.status} ${response.statusText})`);
    }
    // Stream when the browser allows it, otherwise fall back to an ArrayBuffer instantiation.
    let instance: WebAssembly.Instance;
    if (typeof WebAssembly.instantiateStreaming === 'function') {
      const result = await WebAssembly.instantiateStreaming(response.clone(), {});
      instance = result.instance;
    } else {
      const buffer = await response.arrayBuffer();
      const result = await WebAssembly.instantiate(buffer, {});
      instance = result.instance;
    }
    const exports = instance.exports as unknown as CoreExports;
    if (typeof exports.osd_init !== 'function' || !(exports.memory instanceof WebAssembly.Memory)) {
      throw new Error('WASM module is missing the osdbits-core ABI (osd_init / memory)');
    }
    return new WasmCore(exports);
  }

  init(seed: number, regionJapan: boolean): number {
    return this.exports.osd_init(seed >>> 0, regionJapan ? 1 : 0);
  }

  setMode(mode: SceneMode): void {
    this.exports.osd_set_mode(mode);
  }

  getMode(): SceneMode {
    return this.exports.osd_get_mode() as SceneMode;
  }

  step(physicalButtons: number): void {
    this.exports.osd_step(physicalButtons >>> 0);
  }

  setPaused(paused: boolean): void {
    this.exports.osd_set_paused(paused ? 1 : 0);
  }

  setSpeed(speed: number): void {
    this.exports.osd_set_speed(speed);
  }

  setReducedMotion(on: boolean): void {
    this.exports.osd_set_reduced_motion(on ? 1 : 0);
  }

  setFx(fogIntensity: number, refractionScale: number): void {
    this.exports.osd_set_fx(fogIntensity, refractionScale);
  }

  setLanguage(language: number): void {
    this.exports.osd_set_language(language);
  }

  setConfig(screenSize: number, digitalOut: number, componentOut: number): void {
    this.exports.osd_set_config(screenSize, digitalOut, componentOut);
  }

  setRegionJapan(on: boolean): void {
    this.exports.osd_set_region_japan(on ? 1 : 0);
  }

  setClock(hours: number, minutes: number, seconds: number): void {
    this.exports.osd_set_clock(hours >>> 0, minutes >>> 0, seconds >>> 0);
  }

  resetAssets(): void {
    this.exports.osd_reset_assets();
  }

  /** The current frame's interleaved vertices; valid until the next `step()`. */
  vertexView(): Float32Array {
    const count = this.exports.osd_vertex_count();
    if (count === 0) return new Float32Array(0);
    const ptr = this.exports.osd_vertices_ptr();
    const bytes = count * VERTEX_BYTES;
    return new Float32Array(this.exports.memory.buffer, ptr, bytes / 4);
  }

  commandView(): Uint32Array {
    const count = this.exports.osd_command_count();
    if (count === 0) return new Uint32Array(0);
    const ptr = this.exports.osd_commands_ptr();
    const bytes = count * COMMAND_BYTES;
    return new Uint32Array(this.exports.memory.buffer, ptr, bytes / 4);
  }

  /** Clears the core's audio event buffer after they have been turned into sound. */
  drainAudio(): void {
    this.exports.osd_drain_audio();
  }

  audioView(): Uint16Array {
    const count = this.exports.osd_audio_count();
    if (count === 0) return new Uint16Array(0);
    const ptr = this.exports.osd_audio_ptr();
    const bytes = count * AUDIO_BYTES;
    return new Uint16Array(this.exports.memory.buffer, ptr, bytes / 2);
  }

  vertexCount(): number {
    return this.exports.osd_vertex_count();
  }

  commandCount(): number {
    return this.exports.osd_command_count();
  }

  /** Copies one static texture out of WASM memory into a fresh RGBA byte array. */
  texture(id: number): { width: number; height: number; rgba: Uint8Array } {
    const width = this.exports.osd_texture_width(id);
    const height = this.exports.osd_texture_height(id);
    if (width <= 0 || height <= 0) {
      throw new Error(`Unknown texture id ${id}`);
    }
    const ptr = this.exports.osd_texture_ptr(id);
    const bytes = width * height * 4;
    // Copy immediately: the staging buffer is overwritten by the next texture() call.
    const rgba = new Uint8Array(this.exports.memory.buffer, ptr, bytes).slice();
    return { width, height, rgba };
  }

  textureCount(): number {
    return this.exports.osd_texture_count();
  }

  state(): EngineState {
    const len = this.exports.osd_state_json_len();
    const ptr = this.exports.osd_state_json_ptr();
    const bytes = new Uint8Array(this.exports.memory.buffer, ptr, len);
    // JSON buffer is NUL-terminated.
    const nul = bytes.indexOf(0);
    const end = nul === -1 ? bytes.length : nul;
    return JSON.parse(this.decoder.decode(bytes.subarray(0, end))) as EngineState;
  }

  setBootHistory(counts: readonly number[]): void {
    const buf = new Uint8Array(21);
    for (let i = 0; i < 21; i += 1) buf[i] = Math.max(0, Math.min(99, counts[i] ?? 0));
    const ptr = this.alloc(buf.length);
    try {
      new Uint8Array(this.exports.memory.buffer, ptr, buf.length).set(buf);
      this.exports.osd_set_boot_history(ptr, buf.length);
    } finally {
      this.free(ptr, buf.length);
    }
  }

  /** Decompresses an `expand()` LZSS blob. Returns `null` if the module rejected it. */
  expandLzss(data: Uint8Array): Uint8Array | null {
    const ptr = this.alloc(data.length);
    try {
      new Uint8Array(this.exports.memory.buffer, ptr, data.length).set(data);
      const len = this.exports.osd_expand_lzss(ptr, data.length);
      if (len === 0) return null;
      const outPtr = this.exports.osd_expand_result_ptr();
      return new Uint8Array(this.exports.memory.buffer, outPtr, len).slice();
    } finally {
      this.free(ptr, data.length);
    }
  }

  /** Imports a BIOS image or a bare ROMDIR container. Returns the merged resource count. */
  importRomdir(data: Uint8Array): number {
    const ptr = this.alloc(data.length);
    try {
      new Uint8Array(this.exports.memory.buffer, ptr, data.length).set(data);
      return this.exports.osd_import_romdir(ptr, data.length);
    } finally {
      this.free(ptr, data.length);
    }
  }

  // The module has no allocator export (it owns its own buffers and never stores
  // caller pointers), so scratch space is carved from the memory's current end.
  private scratchOffset = 0;

  private alloc(size: number): number {
    const pages = Math.ceil(size / 65536) + 1;
    const before = this.exports.memory.buffer.byteLength;
    this.exports.memory.grow(pages);
    const base = this.exports.memory.buffer.byteLength - pages * 65536;
    if (base < before) {
      throw new Error('WASM memory layout is inconsistent; cannot allocate scratch space');
    }
    this.scratchOffset = base + size;
    return base;
  }

  private free(_ptr: number, _size: number): void {
    // Scratch pages are reclaimed by the next alloc()'s grow; nothing to release explicitly.
    this.scratchOffset = Math.min(this.scratchOffset, this.exports.memory.buffer.byteLength);
  }

  encodeText(text: string): Uint8Array {
    return this.encoder.encode(text);
  }
}

export function decodeCommands(view: Uint32Array): RenderCommand[] {
  const out: RenderCommand[] = [];
  for (let i = 0; i + COMMAND_U32 <= view.length; i += COMMAND_U32) {
    out.push({
      kind: view[i]! as RenderCmdKind,
      target: view[i + 1]! as RenderTargetId,
      textureId: view[i + 2]!,
      blendMode: view[i + 3]! as GsBlendMode,
      samplerWrap: view[i + 4]! as SamplerWrap,
      depthMode: view[i + 5]! as DepthMode,
      firstVertex: view[i + 6]!,
      vertexCount: view[i + 7]!,
      blendFix: view[i + 8]!,
      clearRgba8: view[i + 9]!,
    });
  }
  return out;
}

export function decodeAudio(view: Uint16Array): AudioEvent[] {
  const out: AudioEvent[] = [];
  for (let i = 0; i + AUDIO_U16 <= view.length; i += AUDIO_U16) {
    out.push({ msg: view[i]!, a: view[i + 1]!, b: view[i + 2]!, c: view[i + 3]! });
  }
  return out;
}

export function decodeVertex(view: Float32Array, index: number): GpuVertex {
  const o = index * VERTEX_FLOATS;
  return {
    pos: [view[o]!, view[o + 1]!, view[o + 2]!, view[o + 3]!],
    uv: [view[o + 4]!, view[o + 5]!],
    color: [view[o + 6]!, view[o + 7]!, view[o + 8]!, view[o + 9]!],
  };
}
