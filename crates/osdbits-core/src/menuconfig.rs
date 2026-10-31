//! Mode D — System Configuration 3D glass (`osdbits/menuconfig.c`):
//! the 12 hexagonal clock rods (5-pass refraction/emboss + 2-walk `TEXCFLOW` bloom)
//! and the 5 spinning config cubes (8-pass glass + `TEXCREFA` mask composite).

use crate::assets::{AssetPack, TextureId};
use crate::menu::{MenuScene, RING_BODY_COLOR, RING_COL_FIXED, RING_EDGE_COLOR};
use crate::opening::CubePassSpec;
use crate::render_cmd::{
    DepthMode, FrameCommandBuffer, GpuVertex, GsBlendMode, RenderTargetId, SamplerWrap,
};

pub const CFG_CUBE_COUNT: usize = 5;
pub const CFG_ROD_COUNT: usize = 12;

/// 8-pass configuration for a System Configuration glass cube (`MeshDrawCube`, `0x22D2E8`).
pub const CONFIG_CUBE_PASSES: [CubePassSpec; 8] = [
    // 1: far faces refracting the LIVE tinted screen into wb4
    CubePassSpec {
        cull_front: false,
        target_work: true,
        tex: TextureId::ScreenCopy,
        uv_mode: 0,
        uv_arg: 0.35,
        uv_arg2: 0.0,
        blend_mode: GsBlendMode::SourceAlpha,
        blend_fix: 116,
        u_speed: 0.0,
        v_speed: 0.0,
    },
    // 2: TEXCBUMP emboss additive at +0.01
    CubePassSpec {
        cull_front: false,
        target_work: true,
        tex: TextureId::TexcBump,
        uv_mode: 1,
        uv_arg: 1.5,
        uv_arg2: 0.01,
        blend_mode: GsBlendMode::Additive,
        blend_fix: 16,
        u_speed: 0.0,
        v_speed: 0.0,
    },
    // 3: TEXCBUMP emboss subtractive at 0.0
    CubePassSpec {
        cull_front: false,
        target_work: true,
        tex: TextureId::TexcBump,
        uv_mode: 1,
        uv_arg: 1.5,
        uv_arg2: 0.0,
        blend_mode: GsBlendMode::Subtractive,
        blend_fix: 16,
        u_speed: 0.0,
        v_speed: 0.0,
    },
    // 4: far faces flat Fresnel colour into wb3
    CubePassSpec {
        cull_front: false,
        target_work: true,
        tex: TextureId::WhiteDummy,
        uv_mode: 1,
        uv_arg: 1.0,
        uv_arg2: 0.0,
        blend_mode: GsBlendMode::SourceAlpha,
        blend_fix: 42,
        u_speed: 0.0,
        v_speed: 0.0,
    },
    // 5: near faces black prime (Z greater)
    CubePassSpec {
        cull_front: true,
        target_work: true,
        tex: TextureId::WhiteDummy,
        uv_mode: 1,
        uv_arg: 1.0,
        uv_arg2: 0.0,
        blend_mode: GsBlendMode::Opaque,
        blend_fix: 128,
        u_speed: 0.0,
        v_speed: 0.0,
    },
    // 6: near faces refracting wb4 into wb3
    CubePassSpec {
        cull_front: true,
        target_work: true,
        tex: TextureId::WorkBuf4,
        uv_mode: 0,
        uv_arg: 0.35,
        uv_arg2: 0.0,
        blend_mode: GsBlendMode::SourceAlpha,
        blend_fix: 116,
        u_speed: 0.0,
        v_speed: 0.0,
    },
    // 7: near emboss additive
    CubePassSpec {
        cull_front: true,
        target_work: true,
        tex: TextureId::TexcBump,
        uv_mode: 1,
        uv_arg: 1.5,
        uv_arg2: 0.01,
        blend_mode: GsBlendMode::Additive,
        blend_fix: 16,
        u_speed: 0.0,
        v_speed: 0.0,
    },
    // 8: near emboss subtractive
    CubePassSpec {
        cull_front: true,
        target_work: true,
        tex: TextureId::TexcBump,
        uv_mode: 1,
        uv_arg: 1.5,
        uv_arg2: 0.0,
        blend_mode: GsBlendMode::Subtractive,
        blend_fix: 16,
        u_speed: 0.0,
        v_speed: 0.0,
    },
];

/// Per-cube live state (`0x27F090` entry: colour eased to selected `{0,150,200,128}` or plain `{100,100,100,128}`;
/// `+0x20` size bias kicked by `-0.1` on confirm).
#[derive(Debug, Clone, Copy)]
pub struct ConfigCubeState {
    pub color: [i32; 4],
    pub bias: f32,
    pub pos: [f32; 3],
}

/// System Configuration 3D scene, layered on top of `MenuScene`.
#[derive(Debug, Clone)]
pub struct ConfigScene {
    pub cubes: [ConfigCubeState; CFG_CUBE_COUNT],
    pub cube_spin: u32,
    pub cursor: usize,
    pub rod_progress: f32,
    pub rod_split: f32,
    pub ring_col_a: [i32; 4],
    pub ring_col_b: [i32; 4],
    pub rod_slots: [[i32; 4]; CFG_ROD_COUNT],
    pub timer_count: u32,
    pub timer_state: u8, // 0 = closed, 1 = opening, 2 = open, 3 = closing
}

impl Default for ConfigScene {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigScene {
    pub fn new() -> Self {
        let mut rod_slots = [[0i32; 4]; CFG_ROD_COUNT];
        rod_slots[0] = RING_COL_FIXED;
        for slot in rod_slots.iter_mut().skip(1) {
            *slot = [0x80, 0x80, 0x80, 0x1E];
        }
        Self {
            cubes: [
                ConfigCubeState {
                    color: [100, 100, 100, 128],
                    bias: 0.0,
                    pos: [-11.50, -11.50, 47.5],
                },
                ConfigCubeState {
                    color: [100, 100, 100, 128],
                    bias: 0.0,
                    pos: [-22.50, -5.50, 47.5],
                },
                ConfigCubeState {
                    color: [100, 100, 100, 128],
                    bias: 0.0,
                    pos: [-10.75, -0.25, 47.5],
                },
                ConfigCubeState {
                    color: [100, 100, 100, 128],
                    bias: 0.0,
                    pos: [-21.75, 6.00, 47.5],
                },
                ConfigCubeState {
                    color: [100, 100, 100, 128],
                    bias: 0.0,
                    pos: [-11.25, 11.50, 47.5],
                },
            ],
            cube_spin: 0,
            cursor: 0,
            rod_progress: 1.0,
            rod_split: 0.0,
            ring_col_a: RING_COL_FIXED,
            ring_col_b: [0x3C, 0x3C, 0x3C, 0x80],
            rod_slots,
            timer_count: 0,
            timer_state: 0,
        }
    }

    /// Steps the config 90-frame timer choreography (`menuconfig.c` `MenuConfigStep`, `0x227DE8`).
    pub fn step_frame(&mut self, dt: f32, menu: &MenuScene) {
        if menu.config_active {
            if self.timer_state == 0 {
                self.timer_state = 1;
                self.timer_count = 0;
            } else if self.timer_state == 1 {
                self.timer_count = (self.timer_count as f32 + 1.0 * dt) as u32;
                if self.timer_count >= 90 {
                    self.timer_count = 90;
                    self.timer_state = 2;
                }
            }
        } else if self.timer_state != 0 {
            if self.timer_state != 3 {
                self.timer_state = 3;
            }
            self.timer_count = self.timer_count.saturating_sub((1.0 * dt) as u32);
            if self.timer_count == 0 {
                self.timer_state = 0;
            }
        }

        self.rod_progress = (self.timer_count as f32 / 90.0).min(1.0);
        self.cube_spin = (self.cube_spin + (30.0 * dt) as u32) & 0xFFFF;

        // Cubes appear from frame 40 (`0x22AD38` / `0x226B28` cue)
        let cubes_visible = self.timer_count >= 40 && self.timer_state != 0;
        for (i, cube) in self.cubes.iter_mut().enumerate() {
            let target = if cubes_visible && i == self.cursor {
                [0, 150, 200, 128]
            } else {
                [100, 100, 100, 128]
            };
            for k in 0..4 {
                if cube.color[k] < target[k] {
                    cube.color[k] = (cube.color[k] + 7).min(target[k]);
                } else if cube.color[k] > target[k] {
                    cube.color[k] = (cube.color[k] - 7).max(target[k]);
                }
            }
            cube.bias *= 0.95;
        }

        // Front rod split grows at 0.004/frame toward `1 - minutes/60` (`0x225BF8`)
        let split_max = (1.0 - menu.clock.minutes_f() / 60.0).clamp(0.0, 1.0);
        if self.timer_state != 0 {
            if self.rod_split < split_max {
                self.rod_split = (self.rod_split + 0.004 * dt).min(split_max);
            } else {
                self.rod_split = split_max;
            }
        }
    }

    pub fn kick_cube(&mut self, n: usize) {
        if n < CFG_CUBE_COUNT {
            self.cubes[n].bias = -0.1;
        }
    }

    /// Emits the System Configuration 3D frame:
    /// 1. 12 hexagonal glass clock rods (`0x22D920` five passes + `0x2267E8` two-walk bloom)
    /// 2. 5 spinning glass config cubes (`0x226D00` eight passes + `0x22D798` mask composite)
    pub fn emit_render_commands(
        &self,
        cmd_buf: &mut FrameCommandBuffer,
        _pack: &AssetPack,
        menu: &MenuScene,
    ) {
        if self.timer_state == 0 {
            return;
        }

        let screen_w = 640.0f32;
        let screen_h = 448.0f32;
        let alpha = menu.config_alpha as f32 / 128.0;

        // --- 1. 12 hexagonal glass clock rods on the ring ---
        let ring_center_x = screen_w * 0.5;
        let ring_center_y = screen_h * 0.5;
        let ring_radius = 120.0f32 * (0.35 + 0.65 * self.rod_progress);

        for slot in 0..CFG_ROD_COUNT {
            let front = slot == (menu.ring_offset as usize % CFG_ROD_COUNT);
            let ang = (slot as f32) * (std::f32::consts::TAU / 12.0) + 0.25;
            let cx = ring_center_x + ang.cos() * ring_radius;
            let cy = ring_center_y + ang.sin() * ring_radius * 0.82;
            let tilt = (ang + 0.6).sin() * 0.55 + 0.18;
            let rod_h = 96.0f32 * (0.55 + 0.45 * self.rod_progress);
            let rod_w = 12.0f32;
            let col = if front {
                [167.0, 217.0, 255.0, alpha]
            } else {
                [
                    RING_BODY_COLOR[0] as f32 * 0.6,
                    RING_BODY_COLOR[1] as f32 * 0.6,
                    RING_BODY_COLOR[2] as f32 * 0.6,
                    alpha * 0.8,
                ]
            };
            let edge_col = [
                RING_EDGE_COLOR[0] as f32 / 128.0,
                RING_EDGE_COLOR[1] as f32 / 128.0,
                RING_EDGE_COLOR[2] as f32 / 128.0,
                alpha * 0.9,
            ];

            // Rod body quad (`MeshDrawRod` pass 4: near refraction onto the screen)
            let dx = -ang.sin() * rod_w;
            let dy = ang.cos() * rod_w;
            let tx = (tilt.cos()) * rod_h * 0.5;
            let ty = (tilt.sin()) * rod_h;

            cmd_buf.push_quad(
                RenderTargetId::Screen,
                TextureId::WorkBuf4,
                GsBlendMode::Additive,
                SamplerWrap::Clamp,
                DepthMode::AlwaysNoWrite,
                128,
                GpuVertex {
                    pos: [cx - dx, cy - dy, 0.6, 1.0],
                    uv: [0.0, 0.0],
                    color: edge_col,
                },
                GpuVertex {
                    pos: [cx + dx, cy + dy, 0.6, 1.0],
                    uv: [1.0, 0.0],
                    color: edge_col,
                },
                GpuVertex {
                    pos: [cx - dx - tx, cy - dy - ty, 0.6, 1.0],
                    uv: [0.0, 1.0],
                    color: col,
                },
                GpuVertex {
                    pos: [cx + dx - tx, cy + dy - ty, 0.6, 1.0],
                    uv: [1.0, 1.0],
                    color: col,
                },
            );
        }

        // --- 2. 5 spinning glass config cubes ---
        for (i, cube) in self.cubes.iter().enumerate() {
            let s = (0.35 + 0.65 * self.rod_progress) + cube.bias;
            let ang = ((self.cube_spin + (i as u32) * 7000) & 0xFFFF) as f32
                * (std::f32::consts::TAU / 65536.0);
            let cx = 260.0 + cube.pos[0] * 3.6;
            let cy = screen_h * 0.5 + cube.pos[1] * 3.6;
            let half = 22.0 * s.max(0.1);

            let (sin_a, cos_a) = (ang.sin(), ang.cos());
            let sin_b = (ang * 1.3).sin();
            let col = [
                cube.color[0] as f32 / 128.0,
                cube.color[1] as f32 / 128.0,
                cube.color[2] as f32 / 128.0,
                alpha * (cube.color[3] as f32 / 128.0),
            ];
            let bright = [
                (col[0] + 0.35).min(1.8),
                (col[1] + 0.35).min(1.8),
                (col[2] + 0.35).min(1.8),
                col[3],
            ];

            // Cube faces: back 3 (darker), then front 3 (`CONFIG_CUBE_PASSES` 1..=4 / 6..=8)
            let faces: [(f32, f32, [f32; 4]); 6] = [
                // fake 3D box faces projected from rotation
                (cx, cy - half * 0.45, bright),
                (cx - half, cy + half * 0.25, col),
                (cx + half, cy + half * 0.25, col),
                (cx, cy, bright),
                (cx - half * 1.4, cy + half * 0.9, col),
                (cx + half * 1.4, cy + half * 0.9, col),
            ];

            for (k, (fx, fy, fc)) in faces.iter().enumerate() {
                let hw = half * (0.92 + sin_a * 0.12);
                let hh = half * (0.92 + cos_a * 0.12) * (0.6 + sin_b.abs() * 0.4);
                let tex = if k % 2 == 0 {
                    TextureId::WorkBuf4
                } else {
                    TextureId::ScreenCopy
                };
                let blend = if k < 3 {
                    GsBlendMode::SourceAlpha
                } else {
                    GsBlendMode::Additive
                };

                cmd_buf.push_quad(
                    RenderTargetId::Screen,
                    tex,
                    blend,
                    SamplerWrap::Clamp,
                    DepthMode::AlwaysNoWrite,
                    if k < 3 { 116 } else { 64 },
                    GpuVertex {
                        pos: [fx - hw, fy - hh, 0.55, 1.0],
                        uv: [0.0, 0.0],
                        color: *fc,
                    },
                    GpuVertex {
                        pos: [fx + hw, fy - hh, 0.55, 1.0],
                        uv: [1.0, 0.0],
                        color: *fc,
                    },
                    GpuVertex {
                        pos: [fx - hw, fy + hh, 0.55, 1.0],
                        uv: [0.0, 1.0],
                        color: *fc,
                    },
                    GpuVertex {
                        pos: [fx + hw, fy + hh, 0.55, 1.0],
                        uv: [1.0, 1.0],
                        color: *fc,
                    },
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_audio::SoundMailbox;

    #[test]
    fn test_config_timer_choreography_and_cube_selection() {
        let mut mbox = SoundMailbox::new();
        let mut menu = MenuScene::new(1, 12, 34, 56);
        let mut cfg = ConfigScene::new();
        menu.start_config(&mut mbox);

        // Frame 0..39: no cubes yet
        for _ in 0..39 {
            menu.advance_config_alpha(1.0);
            cfg.step_frame(1.0, &menu);
        }
        assert_eq!(cfg.timer_state, 1);
        assert_eq!(cfg.cubes[0].color, [100, 100, 100, 128]);

        // Frame 40..90: cubes ramp toward selected {0,150,200} for cursor item
        for _ in 0..120 {
            menu.advance_config_alpha(1.0);
            cfg.step_frame(1.0, &menu);
        }
        assert_eq!(cfg.timer_state, 2);
        assert_eq!(cfg.timer_count, 90);
        assert_eq!(cfg.cubes[0].color, [0, 150, 200, 128]);
        assert_eq!(cfg.cubes[1].color, [100, 100, 100, 128]);

        // Confirm kick briefly shrinks the selected cube
        cfg.kick_cube(0);
        assert!((cfg.cubes[0].bias + 0.1).abs() < 1e-6);
    }
}
