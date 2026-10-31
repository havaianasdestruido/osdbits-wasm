/**
 * Numeric constants of the WASM ABI and the frame format.
 *
 * These mirror `crates/osdbits-core/src/{abi,frame}.rs`. A parity test
 * (`tests/abi.parity.test.ts`) loads the real module and fails if they drift;
 * the constants here exist so the rest of the app can be unit-tested without a
 * WASM instance.
 */

export const ABI_VERSION = 1;

export const MODE_BOOT = 0;
export const MODE_IDLE = 1;
export const MODE_ILLEGAL = 2;
export const MODE_MENU = 3;

export const SCENE_KIND_NAMES = ["opening", "illegal", "none"] as const;

export const OPT = {
  PAL: 0,
  GAMES: 1,
  BOOTS: 2,
  FULL_FIELD: 3,
  HW_STATE: 4,
  BOOT_PARAM: 5,
  FOG: 6,
  LIGHTS: 7,
  CUBES: 8,
  CAMERA_MOTION: 9,
  TEXT: 10,
  BLACK_BARS: 11,
} as const;

export const STATE = {
  FRAME: 0,
  Z: 1,
  OPENING_STATE: 2,
  OPENING_TYPE: 3,
  SCENE_KIND: 4,
  ENDED: 5,
  SCE_ALPHA: 6,
  ILLEGAL_ALPHA: 7,
  FLARE_INTENSITY: 8,
  LIT_TOWERS: 9,
  FADE_ALPHA: 10,
  SCENE_STATE: 11,
  LIGHTS_SEED: 12,
  FRAME_WORDS: 13,
  BATCHES: 14,
  VERTICES: 15,
  CUBES: 16,
  WIDTH: 17,
  HEIGHT: 18,
} as const;

export const HW_STATE = {
  NO_DISC: 0,
  DISC_PS2: 1,
  DISC_ILLEGAL: 2,
} as const;

export const HEADER_WORDS = 64;
export const BATCH_WORDS = 8;
export const VERTEX_WORDS = 10;
export const CUBE_WORDS = 128;

/** Header word indices (`frame.rs`). */
export const H = {
  MAGIC: 0,
  VERSION: 1,
  FRAME: 2,
  MODE: 3,
  OPENING_STATE: 4,
  OPENING_TYPE: 5,
  SCENE: 6,
  FLAGS: 7,
  CAM_POS: 8,
  CAM_UP: 12,
  CAM_FWD: 16,
  VIEW: 20,
  SCRZ: 36,
  AX: 37,
  AY: 38,
  ROTATION: 39,
  WIDTH: 40,
  HEIGHT: 41,
  BATCH_COUNT: 42,
  VERTEX_COUNT: 43,
  CUBE_COUNT: 44,
  OFF_BATCHES: 45,
  OFF_VERTS: 46,
  OFF_CUBES: 47,
  TOTAL_WORDS: 48,
  TEXT_SCE_ALPHA: 49,
  TEXT_ILLEGAL_ALPHA: 50,
  HW_STATE: 51,
  LIGHT_SEED: 52,
  FADE_ALPHA: 53,
  END_FRAME: 54,
  END_FLAG: 55,
  ILLEGAL_INTENSITY: 56,
  RED_FLARE: 57,
  SCENE_STATE: 58,
  CUBE_AFTER_BATCH: 59,
} as const;

export const FLAG = {
  END_LATCH: 1 << 0,
  BLACK_BARS: 1 << 1,
  ILLEGAL_WARM: 1 << 2,
  ENDED: 1 << 3,
  PAL: 1 << 4,
} as const;

export const PRIM = {
  SPRITES: 0,
  TRIANGLES: 1,
  LINES: 2,
  SCREEN_SPRITES: 3,
} as const;

export const BLEND = {
  ALPHA: 0,
  SUB: 1,
  ADD: 2,
  MUL: 3,
  FIXALPHA: 4,
  ADD_DEST_ALPHA: 5,
  NONE: 100,
} as const;

export const TEX = {
  NONE: 0,
  WALL: 1,
  FOG0: 2,
  FOG1: 3,
  FOG2: 4,
  FOG3: 5,
  FOG4: 6,
  REF: 7,
  BLPR: 8,
  BLP: 9,
  FLAR: 10,
  CRBL: 11,
  CRLE: 12,
  SCREEN: 13,
  WORK: 14,
} as const;

export type Prim = (typeof PRIM)[keyof typeof PRIM];
export type Blend = (typeof BLEND)[keyof typeof BLEND];
