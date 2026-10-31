/**
 * Decoder for the core's version-1 draw list.
 *
 * The core writes one flat `Float32Array` per frame: a 64-word header, then
 * batch records, then vertices, then cube records (see `docs/WASM-ABI.md`).
 * This module turns that into typed views without copying the vertex data, and
 * throws a descriptive `FrameError` on anything malformed — a corrupt frame
 * must never be rendered as garbage.
 */

import {
  BATCH_WORDS,
  CUBE_WORDS,
  FLAG,
  H,
  HEADER_WORDS,
  PRIM,
  VERTEX_WORDS,
  type Blend,
  type Prim,
} from "./abi.js";

export const MAGIC = 0x4f534442; // "OSDB"
export const VERSION = 1;

export class FrameError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "FrameError";
  }
}

export interface Batch {
  prim: Prim;
  tex: number;
  blend: Blend;
  flags: number;
  vertexOffset: number;
  vertexCount: number;
}

export interface FrameStats {
  batches: number;
  vertices: number;
  cubes: number;
  words: number;
}

export interface Frame {
  readonly words: Float32Array;
  readonly frame: number;
  readonly mode: number;
  readonly scene: number;
  readonly flags: number;
  readonly openingState: number;
  readonly openingType: number;
  readonly width: number;
  readonly height: number;
  readonly scrz: number;
  readonly ax: number;
  readonly ay: number;
  readonly rotation: number;
  readonly cameraPos: readonly [number, number, number];
  readonly cameraUp: readonly [number, number, number];
  readonly cameraFwd: readonly [number, number, number];
  /** 16 floats, row-major world->view matrix. */
  readonly view: Float32Array;
  readonly textSceAlpha: number;
  readonly textIllegalAlpha: number;
  readonly lightSeed: number;
  readonly fadeAlpha: number;
  readonly endFrame: number;
  readonly endFlag: number;
  readonly flareIntensity: number;
  readonly sceneState: number;
  readonly cubeAfterBatch: number;
  readonly batches: readonly Batch[];
  readonly vertices: Float32Array;
  readonly cubes: Float32Array;
  readonly stats: FrameStats;
  readonly ended: boolean;
  readonly pal: boolean;
}

function u32(words: Float32Array, index: number): number {
  // Reinterpret the f32 word as u32 (the core stores flags as f32::from_bits).
  return new Uint32Array(words.buffer, words.byteOffset + index * 4, 1)[0];
}

function assertFinite(words: Float32Array): void {
  for (let i = 0; i < words.length; i++) {
    if (!Number.isFinite(words[i])) {
      throw new FrameError(`frame word ${i} is not finite (${words[i]})`);
    }
  }
}

/**
 * Parse one frame.
 *
 * @param words the frame words, exactly `header.total` long (or longer; the
 *   length is taken from the header and validated).
 */
export function parseFrame(words: Float32Array): Frame {
  if (words.length < HEADER_WORDS) {
    throw new FrameError(`frame too short for a header: ${words.length} words`);
  }
  if (u32(words, H.MAGIC) !== MAGIC) {
    throw new FrameError(
      `bad frame magic 0x${u32(words, H.MAGIC).toString(16)} (expected 0x${MAGIC.toString(16)})`,
    );
  }
  if (u32(words, H.VERSION) !== VERSION) {
    throw new FrameError(`unsupported frame version ${u32(words, H.VERSION)}`);
  }

  const total = words[H.TOTAL_WORDS];
  if (!Number.isInteger(total) || total < HEADER_WORDS) {
    throw new FrameError(`bad total word count ${total}`);
  }
  if (total > words.length) {
    throw new FrameError(`frame claims ${total} words but only ${words.length} are available`);
  }
  assertFinite(words.subarray(0, total));

  const batchCount = words[H.BATCH_COUNT];
  const vertexCount = words[H.VERTEX_COUNT];
  const cubeCount = words[H.CUBE_COUNT];
  const offBatches = words[H.OFF_BATCHES];
  const offVerts = words[H.OFF_VERTS];
  const offCubes = words[H.OFF_CUBES];

  if (
    !Number.isInteger(batchCount) ||
    !Number.isInteger(vertexCount) ||
    !Number.isInteger(cubeCount)
  ) {
    throw new FrameError("non-integer section counts");
  }
  if (batchCount < 0 || vertexCount < 0 || cubeCount < 0) {
    throw new FrameError("negative section counts");
  }
  if (offBatches !== HEADER_WORDS) {
    throw new FrameError(`batch section starts at ${offBatches}, expected ${HEADER_WORDS}`);
  }
  const expectVerts = offBatches + batchCount * BATCH_WORDS;
  if (offVerts !== expectVerts) {
    throw new FrameError(
      `vertex section starts at ${offVerts}, expected ${expectVerts} for ${batchCount} batches`,
    );
  }
  const expectCubes = offVerts + vertexCount * VERTEX_WORDS;
  if (offCubes !== expectCubes) {
    throw new FrameError(
      `cube section starts at ${offCubes}, expected ${expectCubes} for ${vertexCount} vertices`,
    );
  }
  if (total !== offCubes + cubeCount * CUBE_WORDS) {
    throw new FrameError(`total ${total} disagrees with sections (cubes end at +${cubeCount * CUBE_WORDS})`);
  }

  const cubes = words.subarray(offCubes, total);
  const batches: Batch[] = [];
  for (let i = 0; i < batchCount; i++) {
    const o = offBatches + i * BATCH_WORDS;
    const prim = words[o];
    const blend = words[o + 2];
    const vertexOffset = words[o + 4];
    const count = words[o + 5];
    if (!Number.isInteger(vertexOffset) || !Number.isInteger(count)) {
      throw new FrameError(`batch ${i} has a non-integer vertex range`);
    }
    if (vertexOffset < 0 || count < 0 || vertexOffset + count > vertexCount) {
      throw new FrameError(
        `batch ${i} vertex range ${vertexOffset}..${vertexOffset + count} exceeds ${vertexCount}`,
      );
    }
    const prims: readonly number[] = Object.values(PRIM);
    if (!prims.includes(prim)) {
      throw new FrameError(`batch ${i} has unknown primitive ${prim}`);
    }
    batches.push({
      prim: prim as Prim,
      tex: words[o + 1],
      blend: blend as Blend,
      flags: words[o + 3],
      vertexOffset,
      vertexCount: count,
    });
  }

  // Stored as a u32 bit pattern (f32::from_bits), like the flags word.
  const cubeAfterBatch = u32(words, H.CUBE_AFTER_BATCH);
  if (!Number.isInteger(cubeAfterBatch) || cubeAfterBatch < 0 || cubeAfterBatch > batchCount) {
    throw new FrameError(`cube insertion index ${cubeAfterBatch} is outside 0..${batchCount}`);
  }

  const view = words.subarray(H.VIEW, H.VIEW + 16);
  for (let i = 0; i < 16; i++) {
    if (!Number.isFinite(view[i])) {
      throw new FrameError("view matrix is not finite");
    }
  }

  const width = words[H.WIDTH];
  const height = words[H.HEIGHT];
  if (!(width > 0) || !(height > 0)) {
    throw new FrameError(`bad virtual screen ${width}x${height}`);
  }

  const flags = u32(words, H.FLAGS);
  return {
    words: words.subarray(0, total),
    frame: words[H.FRAME],
    mode: u32(words, H.MODE),
    scene: words[H.SCENE],
    flags,
    openingState: words[H.OPENING_STATE],
    openingType: words[H.OPENING_TYPE],
    width,
    height,
    scrz: words[H.SCRZ],
    ax: words[H.AX],
    ay: words[H.AY],
    rotation: words[H.ROTATION],
    cameraPos: [words[H.CAM_POS], words[H.CAM_POS + 1], words[H.CAM_POS + 2]],
    cameraUp: [words[H.CAM_UP], words[H.CAM_UP + 1], words[H.CAM_UP + 2]],
    cameraFwd: [words[H.CAM_FWD], words[H.CAM_FWD + 1], words[H.CAM_FWD + 2]],
    view,
    textSceAlpha: words[H.TEXT_SCE_ALPHA],
    textIllegalAlpha: words[H.TEXT_ILLEGAL_ALPHA],
    lightSeed: words[H.LIGHT_SEED],
    fadeAlpha: words[H.FADE_ALPHA],
    endFrame: words[H.END_FRAME],
    endFlag: words[H.END_FLAG],
    flareIntensity: words[H.ILLEGAL_INTENSITY],
    sceneState: words[H.SCENE_STATE],
    cubeAfterBatch,
    batches,
    vertices: words.subarray(offVerts, offCubes),
    cubes,
    stats: { batches: batchCount, vertices: vertexCount, cubes: cubeCount, words: total },
    ended: (flags & FLAG.ENDED) !== 0,
    pal: (flags & FLAG.PAL) !== 0,
  };
}

export interface VertexView {
  x: number;
  y: number;
  z: number;
  u: number;
  v: number;
  r: number;
  g: number;
  b: number;
  a: number;
  aux: number;
}

/** Read vertex `i` (global index into `frame.vertices`). */
export function vertexAt(frame: Frame, i: number): VertexView {
  const o = i * VERTEX_WORDS;
  const w = frame.vertices;
  return {
    x: w[o],
    y: w[o + 1],
    z: w[o + 2],
    u: w[o + 3],
    v: w[o + 4],
    r: w[o + 5],
    g: w[o + 6],
    b: w[o + 7],
    a: w[o + 8],
    aux: w[o + 9],
  };
}

export interface CubeRecord {
  index: number;
  faceSign: Float32Array;
  baseColor: readonly [number, number, number];
  screenCenter: readonly [number, number];
  cornerQ: Float32Array;
  corners: Float32Array;
  viewDir: Float32Array;
  screenNormal: Float32Array;
  vertexLight: Float32Array;
}

/** Decode cube `i` of `frame.cubes`. */
export function cubeAt(frame: Frame, i: number): CubeRecord {
  const o = i * CUBE_WORDS;
  const c = frame.cubes;
  return {
    index: c[o],
    faceSign: c.subarray(o + 2, o + 8),
    baseColor: [c[o + 8], c[o + 9], c[o + 10]],
    screenCenter: [c[o + 11], c[o + 12]],
    cornerQ: c.subarray(o + 13, o + 21),
    corners: c.subarray(o + 21, o + 53),
    viewDir: c.subarray(o + 53, o + 77),
    screenNormal: c.subarray(o + 77, o + 101),
    vertexLight: c.subarray(o + 101, o + 125),
  };
}

/** Human-readable summary for the diagnostics panel. */
export function describeFrame(frame: Frame): string {
  return (
    `frame ${frame.frame} · scene ${frame.scene} · ` +
    `${frame.stats.batches} batches · ${frame.stats.vertices} verts · ` +
    `${frame.stats.cubes} cubes · ${(frame.stats.words * 4) / 1024 | 0} KiB`
  );
}
