//! Flat `#[repr(C)]` zero-copy vertex and draw-batch command buffers shared across
//! the Rust/WASM <-> TypeScript boundary, with homogeneous near-plane clipping.

use crate::math::Vec4;

/// Texture slot identifiers bound by the host WebGPU / Canvas2D renderer.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureSlot {
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

/// PS2 GS `ALPHA_1` blend mode equivalents.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode {
    Opaque = 0,
    /// `(Cs - Cd) * As + Cd`
    SrcAlpha = 1,
    /// `(Cs - 0) * As + Cd`
    Additive = 2,
    /// `(0 - Cs) * As + Cd = Cd - Cs * As` (`reverse-subtract` in WebGPU)
    Subtractive = 3,
    /// Destination-alpha / silhouette composite
    DestAlpha = 4,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepthMode {
    None = 0,
    ReadOnly = 1,
    ReadWrite = 2,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetId {
    MainScreen = 0,
    ExtraBloom = 1,
}

/// Bitflags executed by the renderer immediately before drawing a `RenderBatch`.
pub mod pass_action {
    pub const NONE: u32 = 0;
    /// Copy `MainScreen` -> `BackdropUntinted` before drawing this batch.
    pub const CAPTURE_UNTINTED_BACKDROP: u32 = 1 << 0;
    /// Copy `MainScreen` -> `BackdropTinted` before drawing this batch.
    pub const CAPTURE_TINTED_BACKDROP: u32 = 1 << 1;
    /// Downsample `MainScreen` -> `ExtraBloom` before drawing this batch.
    pub const CAPTURE_BLOOM_DOWNSAMPLE: u32 = 1 << 2;
    /// Clamp texture UVs to `[0, 1]` instead of repeating.
    pub const CLAMP_UV: u32 = 1 << 3;
}

/// Per-vertex / shader flags packed in `RenderVertex::flags`.
pub mod vertex_flags {
    pub const NONE: u32 = 0;
    /// Apply PS2 GS `2x` color modulation (`rgb * 255 / 128`, where `0x80 == 1.0`).
    pub const PS2_MODULATE_2X: u32 = 1 << 0;
}

/// 32-byte `#[repr(C)]` vertex read directly from WASM linear memory by WebGPU.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderVertex {
    pub clip_pos: [f32; 4],
    pub uv: [f32; 2],
    pub color: [u8; 4],
    pub flags: u32,
}

const _: () = assert!(core::mem::size_of::<RenderVertex>() == 32);

impl RenderVertex {
    #[inline]
    #[must_use]
    pub const fn new(clip_pos: [f32; 4], uv: [f32; 2], color: [u8; 4], flags: u32) -> Self {
        Self {
            clip_pos,
            uv,
            color,
            flags,
        }
    }

    #[inline]
    fn lerp(self, other: Self, t: f32) -> Self {
        let inv = 1.0 - t;
        let lerp_u8 = |a: u8, b: u8| -> u8 {
            ((a as f32) * inv + (b as f32) * t + 0.5).clamp(0.0, 255.0) as u8
        };
        Self {
            clip_pos: [
                self.clip_pos[0] * inv + other.clip_pos[0] * t,
                self.clip_pos[1] * inv + other.clip_pos[1] * t,
                self.clip_pos[2] * inv + other.clip_pos[2] * t,
                self.clip_pos[3] * inv + other.clip_pos[3] * t,
            ],
            uv: [
                self.uv[0] * inv + other.uv[0] * t,
                self.uv[1] * inv + other.uv[1] * t,
            ],
            color: [
                lerp_u8(self.color[0], other.color[0]),
                lerp_u8(self.color[1], other.color[1]),
                lerp_u8(self.color[2], other.color[2]),
                lerp_u8(self.color[3], other.color[3]),
            ],
            flags: self.flags,
        }
    }
}

/// 32-byte `#[repr(C)]` draw-batch descriptor read directly from WASM linear memory.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderBatch {
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub texture_id: u32,
    pub blend_mode: u32,
    pub depth_mode: u32,
    pub pass_action: u32,
    pub target_id: u32,
    pub reserved: u32,
}

const _: () = assert!(core::mem::size_of::<RenderBatch>() == 32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatchKey {
    pub texture: TextureSlot,
    pub blend: BlendMode,
    pub depth: DepthMode,
    pub action: u32,
    pub target: TargetId,
}

/// Pre-allocated render command builder with zero steady-state heap allocations.
pub struct RenderList {
    pub clear_color: [f32; 4],
    pub vertices: Vec<RenderVertex>,
    pub batches: Vec<RenderBatch>,
    near_w: f32,
}

impl Default for RenderList {
    fn default() -> Self {
        Self::with_capacity(49_152, 256)
    }
}

impl RenderList {
    #[must_use]
    pub fn with_capacity(vertex_cap: usize, batch_cap: usize) -> Self {
        Self {
            clear_color: [0.0, 0.0, 0.0, 1.0],
            vertices: Vec::with_capacity(vertex_cap),
            batches: Vec::with_capacity(batch_cap),
            near_w: 0.65,
        }
    }

    pub fn clear(&mut self, clear_color: [f32; 4]) {
        self.clear_color = clear_color;
        self.vertices.clear();
        self.batches.clear();
    }

    pub fn begin_batch(&mut self, key: BatchKey) {
        if key.action == 0
            && let Some(last) = self.batches.last()
            && last.texture_id == key.texture as u32
            && last.blend_mode == key.blend as u32
            && last.depth_mode == key.depth as u32
            && last.target_id == key.target as u32
        {
            return;
        }
        self.batches.push(RenderBatch {
            first_vertex: self.vertices.len() as u32,
            vertex_count: 0,
            texture_id: key.texture as u32,
            blend_mode: key.blend as u32,
            depth_mode: key.depth as u32,
            pass_action: key.action,
            target_id: key.target as u32,
            reserved: 0,
        });
    }

    /// Emit an action-only marker batch (e.g., capture backdrop before a multi-pass object)
    /// or attach the action onto the next batch cleanly.
    pub fn emit_pass_action(&mut self, action: u32, target: TargetId) {
        self.batches.push(RenderBatch {
            first_vertex: self.vertices.len() as u32,
            vertex_count: 0,
            texture_id: TextureSlot::Untextured as u32,
            blend_mode: BlendMode::Opaque as u32,
            depth_mode: DepthMode::None as u32,
            pass_action: action,
            target_id: target as u32,
            reserved: 0,
        });
    }

    #[inline]
    fn push_raw_tri(&mut self, a: RenderVertex, b: RenderVertex, c: RenderVertex) {
        for v in [a, b, c] {
            if !v.clip_pos[0].is_finite()
                || !v.clip_pos[1].is_finite()
                || !v.clip_pos[2].is_finite()
                || !v.clip_pos[3].is_finite()
                || v.clip_pos[3] <= 1e-4
            {
                return;
            }
        }
        self.vertices.push(a);
        self.vertices.push(b);
        self.vertices.push(c);
        if let Some(batch) = self.batches.last_mut() {
            batch.vertex_count += 3;
        }
    }

    /// Clip a triangle against `clip_pos.w >= self.near_w` and emit 0, 1, or 2 triangles.
    pub fn push_clipped_tri(&mut self, v0: RenderVertex, v1: RenderVertex, v2: RenderVertex) {
        let near = self.near_w;
        let input = [v0, v1, v2];
        let mut out = [RenderVertex::default(); 6];
        let mut out_len = 0usize;

        let mut prev = input[2];
        let mut prev_in = prev.clip_pos[3] >= near;

        for &curr in &input {
            let curr_in = curr.clip_pos[3] >= near;
            if curr_in != prev_in {
                let denom = curr.clip_pos[3] - prev.clip_pos[3];
                if denom.abs() > 1e-6 {
                    let t = ((near - prev.clip_pos[3]) / denom).clamp(0.0, 1.0);
                    let mut hit = prev.lerp(curr, t);
                    hit.clip_pos[3] = near;
                    // Clamp clip_z into [0, near]
                    hit.clip_pos[2] = hit.clip_pos[2].clamp(0.0, near);
                    out[out_len] = hit;
                    out_len += 1;
                }
            }
            if curr_in {
                let mut fixed = curr;
                fixed.clip_pos[2] = fixed.clip_pos[2].clamp(0.0, fixed.clip_pos[3]);
                out[out_len] = fixed;
                out_len += 1;
            }
            prev = curr;
            prev_in = curr_in;
        }

        if out_len >= 3 {
            self.push_raw_tri(out[0], out[1], out[2]);
            if out_len >= 4 {
                self.push_raw_tri(out[0], out[2], out[3]);
            }
        }
    }

    /// Emit a quad `(v0, v1, v2, v3)` in counter-clockwise/perimeter order as two clipped triangles.
    #[inline]
    pub fn push_clipped_quad(&mut self, q: [RenderVertex; 4]) {
        // Guard-band reject if all 4 vertices are far behind near plane or far beyond far plane
        if q[0].clip_pos[3] < self.near_w
            && q[1].clip_pos[3] < self.near_w
            && q[2].clip_pos[3] < self.near_w
            && q[3].clip_pos[3] < self.near_w
        {
            return;
        }
        self.push_clipped_tri(q[0], q[1], q[2]);
        self.push_clipped_tri(q[0], q[2], q[3]);
    }

    /// Emit a 2D screen-space rectangle in virtual PS2 coordinates `(0..screen_w, 0..screen_h)`
    /// (`y` increasing downward, matching PS2 GS 2D sprites).
    #[allow(clippy::too_many_arguments)]
    pub fn push_screen_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        screen_w: f32,
        screen_h: f32,
        uv_rect: [f32; 4], // [u0, v0, u1, v1]
        color: [u8; 4],
        flags: u32,
    ) {
        let x0 = (x / screen_w) * 2.0 - 1.0;
        let x1 = ((x + w) / screen_w) * 2.0 - 1.0;
        let y0 = 1.0 - (y / screen_h) * 2.0;
        let y1 = 1.0 - ((y + h) / screen_h) * 2.0;
        let [u0, v0, u1, v1] = uv_rect;

        let a = RenderVertex::new([x0, y0, 0.5, 1.0], [u0, v0], color, flags);
        let b = RenderVertex::new([x1, y0, 0.5, 1.0], [u1, v0], color, flags);
        let c = RenderVertex::new([x1, y1, 0.5, 1.0], [u1, v1], color, flags);
        let d = RenderVertex::new([x0, y1, 0.5, 1.0], [u0, v1], color, flags);
        self.push_raw_tri(a, b, c);
        self.push_raw_tri(a, c, d);
    }

    /// Helper: convert clip-space `Vec4` into normalized screen UV `[0, 1]^2` (`y` top-down).
    #[inline]
    #[must_use]
    pub fn clip_to_screen_uv(clip: Vec4) -> [f32; 2] {
        let w = clip.w.max(0.5);
        let ndc_x = clip.x / w;
        let ndc_y = clip.y / w;
        [0.5 + 0.5 * ndc_x, 0.5 - 0.5 * ndc_y]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_plane_clipper_splits_straddling_triangle_cleanly() {
        let mut rl = RenderList::with_capacity(64, 8);
        rl.begin_batch(BatchKey {
            texture: TextureSlot::Untextured,
            blend: BlendMode::SrcAlpha,
            depth: DepthMode::None,
            action: 0,
            target: TargetId::MainScreen,
        });
        let behind = RenderVertex::new([0.0, 0.0, -2.0, -2.0], [0.0, 0.0], [255; 4], 0);
        let front_a = RenderVertex::new([-1.0, 0.0, 2.0, 4.0], [0.0, 1.0], [255; 4], 0);
        let front_b = RenderVertex::new([1.0, 0.0, 2.0, 4.0], [1.0, 1.0], [255; 4], 0);
        rl.push_clipped_tri(behind, front_a, front_b);

        // One behind, two in front -> clipped quad -> 2 triangles (6 vertices)
        assert_eq!(rl.vertices.len(), 6);
        for v in &rl.vertices {
            assert!(v.clip_pos[3] >= 0.65 - 1e-5);
            assert!(v.clip_pos[0].is_finite() && v.clip_pos[1].is_finite());
        }
    }
}
