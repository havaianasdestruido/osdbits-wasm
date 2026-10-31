/**
 * Loader for the simulation core (`osdbits_core.wasm`).
 *
 * The module imports nothing, so instantiation is a plain
 * `WebAssembly.instantiate`. Every failure mode (fetch, compile, instantiate,
 * ABI mismatch, missing export) is turned into a `CoreLoadError` with a
 * message the UI can show verbatim; nothing here throws raw WebAssembly
 * errors at the caller.
 */

import { ABI_VERSION } from "./abi.js";
import { FrameError, parseFrame, type Frame } from "./frame.js";

export class CoreLoadError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "CoreLoadError";
  }
}

export interface CoreEvent {
  kind: number;
  arg: number;
}

interface CoreExports {
  memory: WebAssembly.Memory;
  osd_abi_version(): number;
  osd_init(mode: number, seed: number): number;
  osd_reset(mode: number, seed: number): number;
  osd_set_option(id: number, value: number): number;
  osd_get_option(id: number): number;
  osd_step(): number;
  osd_draw(): number;
  osd_frame_ptr(): number;
  osd_hash_lo(): number;
  osd_hash_hi(): number;
  osd_state(id: number): number;
  osd_event_count(): number;
  osd_event_kind(index: number): number;
  osd_event_arg(index: number): number;
  osd_pass_table_ptr(): number;
  osd_last_error_ptr(): number;
  osd_last_error_len(): number;
  osd_header_words(): number;
  osd_batch_words(): number;
  osd_vertex_words(): number;
  osd_cube_words(): number;
  osd_fps(): number;
  osd_install_panic_hook(): void;
}

const REQUIRED: readonly (keyof CoreExports)[] = [
  "memory",
  "osd_init",
  "osd_set_option",
  "osd_step",
  "osd_draw",
  "osd_frame_ptr",
  "osd_state",
  "osd_pass_table_ptr",
];

export interface Core {
  readonly exports: Pick<CoreExports, "memory">;
  init(mode: number, seed: number): void;
  setOption(id: number, value: number): boolean;
  getOption(id: number): number;
  step(): number;
  draw(): Frame;
  state(id: number): number;
  events(): CoreEvent[];
  fps(): number;
  passTable(): Float32Array;
  lastError(): string;
  hash(): bigint;
  readonly bytes: number;
}

/**
 * Instantiate the core from a URL or an existing response.
 *
 * @param source URL (fetched with `fetch`) or a preloaded `ArrayBuffer`.
 */
export async function loadCore(source: string | URL | ArrayBuffer): Promise<Core> {
  let buffer: ArrayBuffer;
  let bytes = 0;
  if (source instanceof ArrayBuffer) {
    buffer = source;
    bytes = source.byteLength;
  } else {
    let response: Response;
    try {
      response = await fetch(source);
    } catch (error) {
      throw new CoreLoadError(
        `could not fetch the simulation core (${source}): ${(error as Error).message}`,
      );
    }
    if (!response.ok) {
      throw new CoreLoadError(
        `could not fetch the simulation core (${source}): HTTP ${response.status}`,
      );
    }
    buffer = await response.arrayBuffer();
    bytes = buffer.byteLength;
  }

  let instance: WebAssembly.Instance;
  try {
    const result = await WebAssembly.instantiate(buffer, {});
    instance = result.instance;
  } catch (error) {
    throw new CoreLoadError(
      `the simulation core failed to instantiate: ${(error as Error).message}`,
    );
  }

  const raw = instance.exports as unknown as Record<string, unknown>;
  for (const name of REQUIRED) {
    if (!(name in raw)) {
      throw new CoreLoadError(`the simulation core is missing the export "${name}"`);
    }
  }
  const e = raw as unknown as CoreExports;

  const version = e.osd_abi_version();
  if (version !== ABI_VERSION) {
    throw new CoreLoadError(
      `ABI version mismatch: the module reports ${version}, this shell expects ${ABI_VERSION}`,
    );
  }
  if (e.osd_header_words() !== 64 || e.osd_vertex_words() !== 10 || e.osd_batch_words() !== 8) {
    throw new CoreLoadError("the simulation core reports an unexpected frame layout");
  }

  e.osd_install_panic_hook();

  const f32 = (): Float32Array => new Float32Array(e.memory.buffer);

  const core: Core = {
    exports: { memory: e.memory },
    bytes,
    init(mode, seed) {
      const rc = e.osd_init(mode, seed);
      if (rc !== 0) {
        throw new CoreLoadError(`osd_init(${mode}, ${seed}) failed with code ${rc}`);
      }
    },
    setOption(id, value) {
      return e.osd_set_option(id, value) === 0;
    },
    getOption(id) {
      return e.osd_get_option(id);
    },
    step() {
      return e.osd_step();
    },
    draw() {
      const words = e.osd_draw();
      if (words === 0) {
        throw new FrameError("osd_draw() produced no words");
      }
      const start = e.osd_frame_ptr() / 4;
      // A view over the wasm memory: re-derived every frame because the memory
      // may grow, which detaches the previous ArrayBuffer.
      const view = f32().subarray(start, start + words);
      return parseFrame(view);
    },
    state(id) {
      return e.osd_state(id);
    },
    events() {
      const out: CoreEvent[] = [];
      const n = e.osd_event_count();
      for (let i = 0; i < n; i++) {
        out.push({ kind: e.osd_event_kind(i), arg: e.osd_event_arg(i) });
      }
      return out;
    },
    fps() {
      return e.osd_fps();
    },
    passTable() {
      const ptr = e.osd_pass_table_ptr() / 4;
      return f32().slice(ptr, ptr + 240);
    },
    lastError() {
      const len = e.osd_last_error_len();
      if (len === 0) return "";
      const ptr = e.osd_last_error_ptr();
      return new TextDecoder().decode(new Uint8Array(e.memory.buffer, ptr, len));
    },
    hash() {
      const hi = BigInt(e.osd_hash_hi() >>> 0);
      const lo = BigInt(e.osd_hash_lo() >>> 0);
      return (hi << 32n) | lo;
    },
  };
  return core;
}
