//! The WASM -> TypeScript draw-list frame format (version 1).
//!
//! Rust owns all simulation *and* prepares the draw list; the TypeScript
//! renderer only uploads and draws it. That keeps the ownership model single
//! (see `docs/ARCHITECTURE.md`) and makes the whole visual state inspectable
//! and hashable from tests.
//!
//! Layout (little-endian `f32` words):
//!
//! ```text
//! [ header: 64 words ]
//! [ batch records: 8 words each, `batch_count` of them ]
//! [ vertex pool: 10 words each, `vertex_count` of them ]
//! [ cube records: 128 words each, `cube_count` of them ]
//! ```
//!
//! The reader (TS) validates magic/version/length before trusting anything.
//! Normative field tables live in `docs/WASM-ABI.md`; the constants below are
//! the single source of truth for the Rust side.

/// "OSDB" little-endian.
pub const MAGIC: u32 = 0x4F53_4442;
pub const VERSION: u32 = 1;
pub const HEADER_WORDS: usize = 64;
pub const BATCH_WORDS: usize = 8;
pub const VERTEX_WORDS: usize = 10;
pub const CUBE_WORDS: usize = 128;

// ---- header indices -------------------------------------------------------
pub const H_MAGIC: usize = 0;
pub const H_VERSION: usize = 1;
pub const H_FRAME: usize = 2;
pub const H_MODE: usize = 3;
pub const H_OPENING_STATE: usize = 4;
pub const H_OPENING_TYPE: usize = 5;
pub const H_SCENE: usize = 6;
pub const H_FLAGS: usize = 7;
pub const H_CAM_POS: usize = 8;
pub const H_CAM_UP: usize = 12;
pub const H_CAM_FWD: usize = 16;
pub const H_VIEW: usize = 20;
pub const H_SCRZ: usize = 36;
pub const H_AX: usize = 37;
pub const H_AY: usize = 38;
pub const H_ROTATION: usize = 39;
pub const H_WIDTH: usize = 40;
pub const H_HEIGHT: usize = 41;
pub const H_BATCH_COUNT: usize = 42;
pub const H_VERTEX_COUNT: usize = 43;
pub const H_CUBE_COUNT: usize = 44;
pub const H_OFF_BATCHES: usize = 45;
pub const H_OFF_VERTS: usize = 46;
pub const H_OFF_CUBES: usize = 47;
pub const H_TOTAL_WORDS: usize = 48;
pub const H_TEXT_SCE_ALPHA: usize = 49;
pub const H_TEXT_ILLEGAL_ALPHA: usize = 50;
pub const H_HW_STATE: usize = 51;
pub const H_LIGHT_SEED: usize = 52;
pub const H_FADE_ALPHA: usize = 53;
pub const H_END_FRAME: usize = 54;
pub const H_END_FLAG: usize = 55;
pub const H_ILLEGAL_INTENSITY: usize = 56;
pub const H_RED_FLARE: usize = 57;
pub const H_SCENE_STATE: usize = 58;
/// Index into the batch section at which the cube records belong: batches
/// before it are drawn behind the cubes, batches after it (the screen fades)
/// in front of them, matching the GS draw order.
pub const H_CUBE_AFTER_BATCH: usize = 59;

// ---- header flag bits -----------------------------------------------------
pub const FLAG_END_LATCH: u32 = 1 << 0;
pub const FLAG_BLACK_BARS: u32 = 1 << 1;
pub const FLAG_ILLEGAL_WARM: u32 = 1 << 2;
pub const FLAG_ENDED: u32 = 1 << 3;
pub const FLAG_PAL: u32 = 1 << 4;

// ---- primitive types ------------------------------------------------------
/// Camera-facing billboard; one vertex per sprite (x,y,z / half-w,h / rot).
pub const PRIM_SPRITES: u32 = 0;
/// Triangle list; three vertices per triangle.
pub const PRIM_TRIANGLES: u32 = 1;
/// Line strip.
pub const PRIM_LINES: u32 = 2;
/// Screen-space sprite: x,y in virtual-screen pixels, u,v = half size.
pub const PRIM_SCREEN_SPRITES: u32 = 3;

// ---- blend selectors ------------------------------------------------------
pub const BLEND_NONE: u32 = 100;
pub const BLEND_ALPHA: u32 = 0;
pub const BLEND_SUB: u32 = 1;
pub const BLEND_ADD: u32 = 2;
pub const BLEND_MUL: u32 = 3;
pub const BLEND_FIXALPHA: u32 = 4;
/// `Cs*Ad + Cd` — additive gated by the destination alpha (the real cube
/// environment layers).
pub const BLEND_ADD_DEST_ALPHA: u32 = 5;

// ---- texture ids ----------------------------------------------------------
pub const TEX_NONE: u32 = 0;
pub const TEX_WALL: u32 = 1;
pub const TEX_FOG0: u32 = 2;
pub const TEX_FOG1: u32 = 3;
pub const TEX_FOG2: u32 = 4;
pub const TEX_FOG3: u32 = 5;
pub const TEX_FOG4: u32 = 6;
pub const TEX_REF: u32 = 7;
pub const TEX_BLPR: u32 = 8;
pub const TEX_BLP: u32 = 9;
pub const TEX_FLAR: u32 = 10;
pub const TEX_CRBL: u32 = 11;
pub const TEX_CRLE: u32 = 12;
/// The current in-progress frame (cube selector -2).
pub const TEX_SCREEN: u32 = 13;
/// The cube working buffer (selector -3).
pub const TEX_WORK: u32 = 14;

/// Culling helper shared by the scene emitters: is the world point in front
/// of the camera and inside the virtual screen (with margin)?
pub fn in_front_and_inside(view_pos: [f32; 4], width: f32, height: f32, margin: f32) -> bool {
    if view_pos[2] <= 0.5 {
        return false;
    }
    let z = view_pos[2];
    let sx = crate::math::VIEW_SCREEN_AX * crate::math::VIEW_SCREEN_DISTANCE * view_pos[0] / z;
    let sy = crate::math::VIEW_SCREEN_AY_NTSC * crate::math::VIEW_SCREEN_DISTANCE * view_pos[1] / z;
    sx.abs() <= width * 0.5 + margin && sy.abs() <= height * 0.5 + margin
}

/// Builds the frame buffer, reusing its backing storage between frames so the
/// hot path performs no allocation after warm-up.
pub struct FrameWriter {
    pub header: [f32; HEADER_WORDS],
    batches: Vec<f32>,
    verts: Vec<f32>,
    cubes: Vec<f32>,
    out: Vec<f32>,
    batch_count: u32,
    batch_start_vertex: usize,
    batch_open: bool,
    batch_values: [f32; BATCH_WORDS],
}

impl Default for FrameWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameWriter {
    pub fn new() -> Self {
        FrameWriter {
            header: [0.0; HEADER_WORDS],
            batches: Vec::with_capacity(64 * BATCH_WORDS),
            verts: Vec::with_capacity(8 * 1024 * VERTEX_WORDS),
            cubes: Vec::with_capacity(8 * CUBE_WORDS),
            out: Vec::with_capacity(8 * 1024 * VERTEX_WORDS),
            batch_count: 0,
            batch_start_vertex: 0,
            batch_open: false,
            batch_values: [0.0; BATCH_WORDS],
        }
    }

    /// Reset the section buffers and write the header. `frame`, `mode`, etc.
    /// are filled by the caller (see `Scene::write_frame`).
    pub fn begin(&mut self) {
        self.batches.clear();
        self.verts.clear();
        self.cubes.clear();
        self.batch_count = 0;
        self.batch_start_vertex = 0;
        self.batch_open = false;
        self.header = [0.0; HEADER_WORDS];
    }

    fn header_u32(&mut self, idx: usize, v: u32) {
        self.header[idx] = f32::from_bits(v);
    }

    pub fn set_u32(&mut self, idx: usize, v: u32) {
        self.header_u32(idx, v);
    }

    pub fn set(&mut self, idx: usize, v: f32) {
        self.header[idx] = v;
    }

    pub fn set_vec4(&mut self, idx: usize, v: [f32; 4]) {
        self.header[idx..idx + 4].copy_from_slice(&v);
    }

    pub fn set_matrix(&mut self, idx: usize, m: &crate::math::Mat4) {
        for r in 0..4 {
            self.header[idx + r * 4..idx + r * 4 + 4].copy_from_slice(&m[r]);
        }
    }

    /// Close the currently open batch, if any.
    pub fn end_batch(&mut self) {
        if !self.batch_open {
            return;
        }
        let count = (self.verts.len() / VERTEX_WORDS) - self.batch_start_vertex;
        if count > 0 {
            self.batch_values[4] = self.batch_start_vertex as f32;
            self.batch_values[5] = count as f32;
            self.batches.extend_from_slice(&self.batch_values);
            self.batch_count += 1;
        }
        self.batch_open = false;
    }

    /// Start a batch. Consecutive `push_*` calls with the same parameters are
    /// merged by the caller; this API is deliberately explicit.
    pub fn begin_batch(&mut self, prim: u32, tex: u32, blend: u32, flags: u32) {
        self.end_batch();
        self.batch_values = [0.0; BATCH_WORDS];
        self.batch_values[0] = prim as f32;
        self.batch_values[1] = tex as f32;
        self.batch_values[2] = blend as f32;
        self.batch_values[3] = flags as f32;
        self.batch_start_vertex = self.verts.len() / VERTEX_WORDS;
        self.batch_open = true;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_vertex(
        &mut self,
        x: f32,
        y: f32,
        z: f32,
        u: f32,
        v: f32,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
        aux: f32,
    ) {
        debug_assert!(self.batch_open, "vertex pushed outside a batch");
        debug_assert!(
            x.is_finite() && y.is_finite() && z.is_finite() && u.is_finite() && v.is_finite(),
            "non-finite vertex"
        );
        self.verts
            .extend_from_slice(&[x, y, z, u, v, r, g, b, a, aux]);
    }

    /// One camera-facing billboard. `half_w`/`half_h` are world units,
    /// `rot` is a rotation in radians around the view axis.
    #[allow(clippy::too_many_arguments)]
    pub fn push_sprite(
        &mut self,
        pos: [f32; 3],
        half_w: f32,
        half_h: f32,
        rot: f32,
        rgba: [f32; 4],
    ) {
        // u,v carry the half sizes; `aux` the rotation. Sprite textures are
        // used with their full 0..1 UV rect, which the renderer assumes.
        self.push_vertex(
            pos[0], pos[1], pos[2], half_w, half_h, rgba[0], rgba[1], rgba[2], rgba[3], rot,
        );
    }

    /// One screen-space sprite (`pktSetTexRect`): `x`,`y` in virtual screen
    /// pixels, `w`,`h` in pixels.
    pub fn push_screen_sprite(&mut self, x: f32, y: f32, w: f32, h: f32, rgba: [f32; 4]) {
        self.push_vertex(x, y, 0.0, w, h, rgba[0], rgba[1], rgba[2], rgba[3], 0.0);
    }

    pub fn push_triangle(&mut self, p: [[f32; 3]; 3], uv: [[f32; 2]; 3], rgba: [[f32; 4]; 3]) {
        for i in 0..3 {
            self.push_vertex(
                p[i][0], p[i][1], p[i][2], uv[i][0], uv[i][1], rgba[i][0], rgba[i][1], rgba[i][2],
                rgba[i][3], 0.0,
            );
        }
    }

    pub fn push_line(&mut self, p: [f32; 3], rgba: [f32; 4], width: f32) {
        self.push_vertex(
            p[0], p[1], p[2], width, 0.0, rgba[0], rgba[1], rgba[2], rgba[3], 0.0,
        );
    }

    /// Append one cube record; callers fill the 128-word record.
    pub fn push_cube(&mut self, record: &[f32; CUBE_WORDS]) {
        self.cubes.extend_from_slice(record);
    }

    /// Record that the cube records belong here: after the batches emitted so
    /// far. Called by the scene right before it emits cubes.
    pub fn mark_cubes(&mut self) {
        self.end_batch();
        self.header_u32(H_CUBE_AFTER_BATCH, self.batch_count);
    }

    pub fn vertex_count(&self) -> u32 {
        (self.verts.len() / VERTEX_WORDS) as u32
    }

    pub fn batch_count(&self) -> u32 {
        self.batch_count
    }

    pub fn cube_count(&self) -> u32 {
        (self.cubes.len() / CUBE_WORDS) as u32
    }

    /// Assemble the final buffer and return it. The caller's header fields
    /// `H_OFF_*`/counts are overwritten here so they cannot disagree with the
    /// actual sections.
    pub fn finish(&mut self) -> &[f32] {
        self.end_batch();
        self.header_u32(H_MAGIC, MAGIC);
        self.header_u32(H_VERSION, VERSION);
        self.header[H_BATCH_COUNT] = self.batch_count as f32;
        self.header[H_VERTEX_COUNT] = self.vertex_count() as f32;
        self.header[H_CUBE_COUNT] = self.cube_count() as f32;
        let off_batches = HEADER_WORDS;
        let off_verts = off_batches + self.batches.len();
        let off_cubes = off_verts + self.verts.len();
        self.header[H_OFF_BATCHES] = off_batches as f32;
        self.header[H_OFF_VERTS] = off_verts as f32;
        self.header[H_OFF_CUBES] = off_cubes as f32;
        self.header[H_TOTAL_WORDS] = (off_cubes + self.cubes.len()) as f32;

        self.out.clear();
        self.out
            .reserve(self.header.len() + self.batches.len() + self.verts.len() + self.cubes.len());
        self.out.extend_from_slice(&self.header);
        self.out.extend_from_slice(&self.batches);
        self.out.extend_from_slice(&self.verts);
        self.out.extend_from_slice(&self.cubes);
        &self.out
    }

    /// The assembled frame (valid after [`FrameWriter::finish`]).
    pub fn data(&self) -> &[f32] {
        &self.out
    }

    /// Capacity of the internal buffers (f32 words), for endurance tests
    /// proving the frame path stops growing.
    pub fn capacities(&self) -> [usize; 3] {
        [
            self.batches.capacity(),
            self.verts.capacity(),
            self.cubes.capacity(),
        ]
    }
}

/// FNV-1a over the frame words (as raw bits). Used by determinism tests and
/// by the app's debug overlay.
pub fn frame_hash(words: &[f32]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for w in words {
        let bits = if w.is_finite() && *w == 0.0 {
            0u32
        } else {
            w.to_bits()
        };
        for b in bits.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_constants_do_not_overlap() {
        let mut seen = std::collections::HashSet::new();
        for i in [
            H_MAGIC,
            H_VERSION,
            H_FRAME,
            H_MODE,
            H_OPENING_STATE,
            H_OPENING_TYPE,
            H_SCENE,
            H_FLAGS,
            H_CAM_POS,
            H_CAM_UP,
            H_CAM_FWD,
            H_VIEW,
            H_SCRZ,
            H_AX,
            H_AY,
            H_ROTATION,
            H_WIDTH,
            H_HEIGHT,
            H_BATCH_COUNT,
            H_VERTEX_COUNT,
            H_CUBE_COUNT,
            H_OFF_BATCHES,
            H_OFF_VERTS,
            H_OFF_CUBES,
            H_TOTAL_WORDS,
            H_TEXT_SCE_ALPHA,
            H_TEXT_ILLEGAL_ALPHA,
            H_HW_STATE,
            H_LIGHT_SEED,
            H_FADE_ALPHA,
            H_END_FRAME,
            H_END_FLAG,
            H_ILLEGAL_INTENSITY,
            H_RED_FLARE,
            H_SCENE_STATE,
        ] {
            assert!(i < HEADER_WORDS);
            assert!(seen.insert(i), "header index {i} used twice");
        }
        // The view matrix block must be 16 words inside the header.
        // Compile-time check that the header fields fit and do not overlap.
        const _: () = assert!(H_VIEW + 16 <= HEADER_WORDS);
        const _: () = assert!(H_SCRZ >= H_VIEW + 16);
    }

    #[test]
    fn empty_frame_is_well_formed() {
        let mut w = FrameWriter::new();
        w.begin();
        let out = w.finish();
        assert_eq!(out.len(), HEADER_WORDS);
        assert_eq!(out[H_MAGIC].to_bits(), MAGIC);
        assert_eq!(out[H_VERSION].to_bits(), VERSION);
        assert_eq!(out[H_BATCH_COUNT], 0.0);
        assert_eq!(out[H_VERTEX_COUNT], 0.0);
        assert_eq!(out[H_TOTAL_WORDS] as usize, out.len());
    }

    #[test]
    fn batches_reference_the_right_vertex_ranges() {
        let mut w = FrameWriter::new();
        w.begin();
        w.begin_batch(PRIM_TRIANGLES, TEX_WALL, BLEND_NONE, 0);
        for _ in 0..6 {
            w.push_vertex(1.0, 2.0, 3.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0);
        }
        w.begin_batch(PRIM_SPRITES, TEX_FOG0, BLEND_ADD, 0);
        for _ in 0..3 {
            w.push_sprite([0.0, 0.0, 1.0], 1.0, 1.0, 0.0, [1.0; 4]);
        }
        let out = w.finish();
        assert_eq!(out[H_BATCH_COUNT], 2.0);
        assert_eq!(out[H_VERTEX_COUNT], 9.0);
        let off_b = out[H_OFF_BATCHES] as usize;
        let off_v = out[H_OFF_VERTS] as usize;
        assert_eq!(off_b, HEADER_WORDS);
        assert_eq!(off_v, HEADER_WORDS + 2 * BATCH_WORDS);
        // Batch 0: prim/tex/blend/flags/offset/count.
        assert_eq!(out[off_b], PRIM_TRIANGLES as f32);
        assert_eq!(out[off_b + 1], TEX_WALL as f32);
        assert_eq!(out[off_b + 4], 0.0);
        assert_eq!(out[off_b + 5], 6.0);
        // Batch 1 starts at vertex 6.
        assert_eq!(out[off_b + BATCH_WORDS + 4], 6.0);
        assert_eq!(out[off_b + BATCH_WORDS + 5], 3.0);
        // Sprite half-sizes survive in the vertex's u,v.
        assert_eq!(out[off_v + 6 * VERTEX_WORDS + 3], 1.0);
    }

    #[test]
    fn empty_batches_are_dropped() {
        let mut w = FrameWriter::new();
        w.begin();
        w.begin_batch(PRIM_SPRITES, 0, 0, 0);
        w.begin_batch(PRIM_SPRITES, 0, 0, 0);
        let out = w.finish();
        assert_eq!(out[H_BATCH_COUNT], 0.0);
    }

    #[test]
    fn cubes_are_appended_after_vertices() {
        let mut w = FrameWriter::new();
        w.begin();
        w.begin_batch(PRIM_LINES, TEX_NONE, BLEND_ADD, 0);
        w.push_line([0.0, 0.0, 0.0], [1.0; 4], 1.0);
        let mut rec = [0.0f32; CUBE_WORDS];
        rec[0] = 3.0;
        w.push_cube(&rec);
        let out = w.finish();
        assert_eq!(out[H_CUBE_COUNT], 1.0);
        let off_c = out[H_OFF_CUBES] as usize;
        assert_eq!(out[off_c], 3.0);
        assert_eq!(out[H_TOTAL_WORDS] as usize, out.len());
    }

    #[test]
    fn frame_hash_is_stable_and_sensitive() {
        let mut w = FrameWriter::new();
        w.begin();
        w.set(H_FRAME, 7.0);
        w.begin_batch(PRIM_SPRITES, TEX_CRBL, BLEND_ADD, 0);
        w.push_sprite([1.0, 2.0, 3.0], 0.8, 0.8, 0.0, [1.0; 4]);
        let h1 = frame_hash(w.finish());

        let mut w2 = FrameWriter::new();
        w2.begin();
        w2.set(H_FRAME, 7.0);
        w2.begin_batch(PRIM_SPRITES, TEX_CRBL, BLEND_ADD, 0);
        w2.push_sprite([1.0, 2.0, 3.0], 0.8, 0.8, 0.0, [1.0; 4]);
        let h2 = frame_hash(w2.finish());
        assert_eq!(h1, h2);

        w2.begin();
        w2.set(H_FRAME, 8.0);
        assert_ne!(frame_hash(w2.finish()), h1);
        // -0.0 and +0.0 hash the same (normalised).
        let a = frame_hash(&[0.0]);
        let b = frame_hash(&[-0.0]);
        assert_eq!(a, b);
    }

    #[test]
    fn finish_is_repeatable_without_allocation_growth() {
        let mut w = FrameWriter::new();
        let mut caps = None;
        for f in 0..50 {
            w.begin();
            w.set(H_FRAME, f as f32);
            w.begin_batch(PRIM_TRIANGLES, TEX_WALL, BLEND_NONE, 0);
            for i in 0..64 {
                w.push_vertex(i as f32, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0);
            }
            w.finish();
            let c = w.capacities();
            if let Some(prev) = caps {
                assert_eq!(c, prev, "capacities changed on frame {f}");
            } else if f == 1 {
                caps = Some(c);
            }
        }
    }

    #[test]
    fn in_front_and_inside_rejects_behind_camera() {
        assert!(!in_front_and_inside(
            [0.0, 0.0, -5.0, 1.0],
            640.0,
            224.0,
            0.0
        ));
        assert!(in_front_and_inside(
            [0.0, 0.0, 100.0, 1.0],
            640.0,
            224.0,
            0.0
        ));
        assert!(!in_front_and_inside(
            [5000.0, 0.0, 10.0, 1.0],
            640.0,
            224.0,
            0.0
        ));
        // At z = 100 the screen half-width is 31.25 world units; margin 2
        // lets a point at 31 (317 px) through but not one at 33.5 (343 px).
        assert!(in_front_and_inside(
            [31.0, 0.0, 100.0, 1.0],
            640.0,
            224.0,
            2.0
        ));
        assert!(!in_front_and_inside(
            [33.5, 0.0, 100.0, 1.0],
            640.0,
            224.0,
            2.0
        ));
    }
}
