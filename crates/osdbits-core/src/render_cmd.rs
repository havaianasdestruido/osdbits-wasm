//! Zero-copy `#[repr(C)]` GPU vertex and render command structures shared between
//! Rust (`wasm32-unknown-unknown`) and the TypeScript WebGPU renderer.

use crate::assets::TextureId;

/// Flat 32-byte (`8 × f32`) vertex consumed directly from WASM linear memory by WebGPU.
/// - `pos`: `[x, y, z, q]` where `x` is in `0..640`, `y` is in `0..448`, `z` is in `0..1` depth, `q` is `1/w`.
/// - `uv`: `[u, v]` normalized texture coordinates (divided by `q` before interpolation when perspective-correct).
/// - `color`: `[r, g, b, a]` in `0.0 .. 2.0` (PS2 `0x80 = 1.0`, `0xFF = 1.992`).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GpuVertex {
    pub pos: [f32; 4],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

/// Render target framebuffer selector (`FRAME_1`).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderTargetId {
    Screen = 0,
    WorkBuf3 = 1, // `extraBuf1`
    WorkBuf4 = 2, // `extraBuf2`
}

/// Command action kind in the frame command stream.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderCmdKind {
    /// Clears the specified `target` to `(r, g, b, a)` and optionally resets the depth buffer.
    ClearTarget = 0,
    /// Draws `vertex_count` triangle vertices (`first_vertex .. first_vertex + vertex_count`) to `target`.
    DrawTriangles = 1,
    /// Copies the current `source_target` texture into `dest_tex` (`ScreenCopy`, `WorkBuf3`, or `WorkBuf4`).
    CopyTargetToTexture = 2,
}

/// PS2 GS `ALPHA_1` blend mode (`opening.c` `BlendModes[0..10]` and `docs/glass-refraction.md` §0).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GsBlendMode {
    /// `abe = 0`: Opaque write (`Cv = Cs`).
    Opaque = 0,
    /// Mode 4 (`0,1,0,1`, `0x44`): `(Cs - Cd) * As + Cd` (standard source-alpha blend).
    SourceAlpha = 1,
    /// Mode 5 (`0,2,0,1`, `0x48`): `Cs * As + Cd` (additive by source alpha).
    Additive = 2,
    /// Mode 6 (`2,0,0,1`, `0x42`): `Cd - Cs * As` (subtractive by source alpha, used in `TEXCBUMP` emboss).
    Subtractive = 3,
    /// Mode 0 (`0,2,2,1`): `Cs * FIX + Cd` (additive by fixed `blend_fix / 128.0`).
    FixedAdditive = 4,
    /// Mode 2 (`0,1,2,1`): `(Cs - Cd) * FIX + Cd` (lerp blend by fixed `blend_fix / 128.0`).
    FixedBlend = 5,
    /// Mode 8 (`0,2,1,1`): `Cs * Ad + Cd` (additive by destination alpha).
    DestAlphaAdditive = 6,
}

/// Per-primitive texture wrap mode (`CLAMP_1`).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplerWrap {
    Clamp = 0,
    Repeat = 1,
}

/// PS2 GS `TEST_1` / `ZBUF_1` depth test and write configuration.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepthMode {
    AlwaysNoWrite = 0,
    AlwaysWrite = 1,
    LessEqualWrite = 2,
    LessEqualNoWrite = 3,
    GreaterWrite = 4,
}

/// Flat 32-byte (`8 × u32`) command record read directly from WASM linear memory by TypeScript.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderPassCmd {
    pub kind: u32,
    pub target: u32,
    pub texture_id: u32,
    pub blend_mode: u32,
    pub sampler_wrap: u32,
    pub depth_mode: u32,
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub blend_fix: u32,
    pub clear_rgba8: u32,
}

/// Frame builder accumulating triangles and GS state transitions during a single simulation tick.
#[derive(Debug, Clone, Default)]
pub struct FrameCommandBuffer {
    pub vertices: Vec<GpuVertex>,
    pub commands: Vec<RenderPassCmd>,
}

impl FrameCommandBuffer {
    pub fn new() -> Self {
        Self {
            vertices: Vec::with_capacity(32768),
            commands: Vec::with_capacity(512),
        }
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.commands.clear();
    }

    pub fn push_clear(
        &mut self,
        target: RenderTargetId,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
        clear_depth: bool,
    ) {
        let rgba = (r as u32) | ((g as u32) << 8) | ((b as u32) << 16) | ((a as u32) << 24);
        self.commands.push(RenderPassCmd {
            kind: RenderCmdKind::ClearTarget as u32,
            target: target as u32,
            texture_id: 0,
            blend_mode: 0,
            sampler_wrap: 0,
            depth_mode: if clear_depth { 1 } else { 0 },
            first_vertex: 0,
            vertex_count: 0,
            blend_fix: 0,
            clear_rgba8: rgba,
        });
    }

    pub fn push_copy_target_to_texture(&mut self, src_target: RenderTargetId, dst_tex: TextureId) {
        self.commands.push(RenderPassCmd {
            kind: RenderCmdKind::CopyTargetToTexture as u32,
            target: src_target as u32,
            texture_id: dst_tex as u32,
            blend_mode: 0,
            sampler_wrap: 0,
            depth_mode: 0,
            first_vertex: 0,
            vertex_count: 0,
            blend_fix: 0,
            clear_rgba8: 0,
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_quad(
        &mut self,
        target: RenderTargetId,
        texture_id: TextureId,
        blend_mode: GsBlendMode,
        sampler_wrap: SamplerWrap,
        depth_mode: DepthMode,
        blend_fix: u8,
        v0: GpuVertex,
        v1: GpuVertex,
        v2: GpuVertex,
        v3: GpuVertex,
    ) {
        self.push_triangles(
            target,
            texture_id,
            blend_mode,
            sampler_wrap,
            depth_mode,
            blend_fix,
            &[v0, v1, v2, v2, v1, v3],
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_sprite_2d(
        &mut self,
        target: RenderTargetId,
        texture_id: TextureId,
        blend_mode: GsBlendMode,
        sampler_wrap: SamplerWrap,
        blend_fix: u8,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        u0: f32,
        v0: f32,
        u1: f32,
        v1: f32,
        rgba: [u8; 4],
    ) {
        let color = [
            (rgba[0] as f32) / 128.0,
            (rgba[1] as f32) / 128.0,
            (rgba[2] as f32) / 128.0,
            (rgba[3] as f32) / 128.0,
        ];
        let z = 0.01f32;
        self.push_quad(
            target,
            texture_id,
            blend_mode,
            sampler_wrap,
            DepthMode::AlwaysNoWrite,
            blend_fix,
            GpuVertex {
                pos: [x0, y0, z, 1.0],
                uv: [u0, v0],
                color,
            },
            GpuVertex {
                pos: [x1, y0, z, 1.0],
                uv: [u1, v0],
                color,
            },
            GpuVertex {
                pos: [x0, y1, z, 1.0],
                uv: [u0, v1],
                color,
            },
            GpuVertex {
                pos: [x1, y1, z, 1.0],
                uv: [u1, v1],
                color,
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_triangles(
        &mut self,
        target: RenderTargetId,
        texture_id: TextureId,
        blend_mode: GsBlendMode,
        sampler_wrap: SamplerWrap,
        depth_mode: DepthMode,
        blend_fix: u8,
        tris: &[GpuVertex],
    ) {
        if tris.is_empty() {
            return;
        }
        let first_vertex = self.vertices.len() as u32;
        self.vertices.extend_from_slice(tris);
        let vertex_count = tris.len() as u32;

        // Coalesce with the previous DrawTriangles command if state matches identically
        if let Some(last) = self.commands.last_mut() {
            if last.kind == RenderCmdKind::DrawTriangles as u32
                && last.target == target as u32
                && last.texture_id == texture_id as u32
                && last.blend_mode == blend_mode as u32
                && last.sampler_wrap == sampler_wrap as u32
                && last.depth_mode == depth_mode as u32
                && last.blend_fix == blend_fix as u32
                && last.first_vertex + last.vertex_count == first_vertex
            {
                last.vertex_count += vertex_count;
                return;
            }
        }

        self.commands.push(RenderPassCmd {
            kind: RenderCmdKind::DrawTriangles as u32,
            target: target as u32,
            texture_id: texture_id as u32,
            blend_mode: blend_mode as u32,
            sampler_wrap: sampler_wrap as u32,
            depth_mode: depth_mode as u32,
            first_vertex,
            vertex_count,
            blend_fix: blend_fix as u32,
            clear_rgba8: 0,
        });
    }
}
