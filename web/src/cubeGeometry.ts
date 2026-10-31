/**
 * Cube face topology, mirrored from `crates/osdbits-core/src/cubes.rs`
 * (`CUBE_FACE_VERTS`, `CUBE_FACE_NORMAL`, qwords at 0x27b090/0x27b0?0).
 *
 * The core emits world-space corners and a per-face winding sign, but not the
 * face table, so the renderer needs its own copy. `tests/abi.parity.test.ts`
 * checks the ordering against the core's exported pass table and a recorded
 * cube record; a change here without a matching core change fails that test.
 */

/** Corner indices of the eight cube vertices, per face, as a quad. */
export const CUBE_FACE_VERTS: readonly (readonly [number, number, number, number])[] = [
  [0, 1, 2, 3],
  [5, 4, 7, 6],
  [4, 0, 6, 2],
  [2, 3, 6, 7],
  [1, 5, 3, 7],
  [4, 5, 0, 1],
];

/** Axis-aligned face normals (xyz; the w component is unused). */
export const CUBE_FACE_NORMAL: readonly (readonly [number, number, number, number])[] = [
  [0, 0, -1, 1],
  [0, 0, 1, 1],
  [-1, 0, 0, 1],
  [0, 1, 0, 1],
  [1, 0, 0, 1],
  [0, -1, 0, 1],
];
