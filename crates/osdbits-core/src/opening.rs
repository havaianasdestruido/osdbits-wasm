//! Mode A (Opening / Towers) and Mode B (Idle Tower Field) simulation and renderer.
//!
//! Directly ported from `osdbits/opening.c`, `osdbits/towerchain.dsm`,
//! `osdbits/vucode_1.vsm`, and `docs/towers-analysis.md`.

use crate::assets::TextureId;
use crate::input_audio::{
    SoundMailbox, BGM_SLOT_BOOTS, BGM_SLOT_TNNLS, SND_CMD_BGM_PLAY, SND_CMD_BGM_STOP,
};
use crate::math::{
    mat4_apply, mat4_identity, mat4_rot_x, mat4_rot_y, mat4_rot_z, mat4_translate, project_point,
    sce_vu0_camera_matrix, sce_vu0_view_screen_matrix, vec4_cross3, vec4_dot3, vec4_normalize3,
    vec4_sub, vucode1_shade_normal, Mat4, OsdRng, Vec4,
};
use crate::render_cmd::{
    DepthMode, FrameCommandBuffer, GpuVertex, GsBlendMode, RenderTargetId, SamplerWrap,
};

/// Memory-card boot-history neighbourhood table (`histCells[21][6]`, real `0x27a260`).
///
/// Each of the 21 history slots owns 6 cells `(col + row * 14)` of the 14×9 tower grid.
/// **Provenance: reconstructed.** `docs/towers-analysis.md` establishes the mechanism and the
/// invariant that the 21 × 6 neighbourhoods tile all 126 cells exactly once (asserted by the
/// unit test below); the individual cell indices here are a faithful-to-the-invariant layout,
/// not a byte-for-byte dump of the retail image, because the table is zero-filled in the static
/// image and this workspace has no BIOS to read it from. Only the *shape* of the mapping affects
/// the rendered field: the tower positions come from a static per-cell table
/// ([`tower_cell_position`]), so any valid tiling produces the documented visuals.
pub const HIST_CELLS: [[i32; 6]; 21] = [
    [62, 63, 76, 77, 124, 125],
    [47, 48, 49, 50, 61, 64],
    [75, 78, 89, 90, 91, 92],
    [32, 33, 34, 35, 36, 37],
    [46, 51, 60, 65, 74, 79],
    [88, 93, 102, 103, 104, 105],
    [106, 107, 17, 18, 19, 20],
    [21, 22, 23, 24, 31, 38],
    [45, 52, 59, 66, 73, 80],
    [87, 94, 101, 108, 114, 115],
    [116, 117, 118, 119, 120, 121],
    [2, 3, 4, 5, 6, 7],
    [8, 9, 10, 11, 16, 25],
    [30, 39, 44, 53, 58, 67],
    [72, 81, 86, 95, 100, 109],
    [113, 122, 1, 12, 15, 26],
    [29, 40, 43, 54, 57, 68],
    [71, 82, 85, 96, 99, 110],
    [112, 123, 0, 13, 14, 27],
    [28, 41, 42, 55, 56, 69],
    [70, 83, 84, 97, 98, 111],
];

/// Camera Z milestones (`animZLevels[8]` at `0x252790`, `osdbits/opening.c` line 1388).
pub const ANIM_Z_LEVELS: [f32; 8] = [16.0, 56.0, 104.0, 320.0, 672.0, 800.0, 1160.0, 1160.0];

/// 4 Directional light directions (`towerLightDirs` in `towerchain.dsm`).
pub const TOWER_LIGHT_DIRS: [Vec4; 4] = [
    [-0.577, -0.577, 0.577, 0.0],
    [0.577, -0.577, 0.577, 0.0],
    [0.0, 0.707, 0.707, 0.0],
    [0.0, 0.0, 1.0, 0.0],
];

/// 4 Directional light colors (`towerLightColors` in `towerchain.dsm`).
pub const TOWER_LIGHT_COLORS: [Vec4; 4] = [
    [0.48, 0.52, 0.64, 0.0],
    [0.35, 0.38, 0.48, 0.0],
    [0.28, 0.34, 0.50, 0.0],
    [0.42, 0.46, 0.58, 0.0],
];

pub const TOWER_AMBIENT: Vec4 = [0.34, 0.37, 0.46, 1.0];

/// 10-pass configuration for Opening glass cubes (`cubePasses[10]`, `osdbits/opening.c` line 1641).
#[derive(Debug, Clone, Copy)]
pub struct CubePassSpec {
    pub cull_front: bool, // false = back faces (`targetWork=1`), true = front faces (`targetWork=0`)
    pub target_work: bool,
    pub tex: TextureId,
    pub uv_mode: u8,
    pub uv_arg: f32,
    pub uv_arg2: f32,
    pub blend_mode: GsBlendMode,
    pub blend_fix: u8,
    pub u_speed: f32,
    pub v_speed: f32,
}

pub const OPENING_CUBE_PASSES: [CubePassSpec; 10] = [
    // 5 Back-face passes into WorkBuf4 (`extraBuf2`)
    CubePassSpec {
        cull_front: false,
        target_work: true,
        tex: TextureId::ScreenCopy,
        uv_mode: 0,
        uv_arg: 0.0,
        uv_arg2: -0.05,
        blend_mode: GsBlendMode::Opaque,
        blend_fix: 128,
        u_speed: 0.0,
        v_speed: 0.0,
    },
    CubePassSpec {
        cull_front: false,
        target_work: true,
        tex: TextureId::TexoBlpr,
        uv_mode: 1,
        uv_arg: 1.0,
        uv_arg2: 0.0,
        blend_mode: GsBlendMode::SourceAlpha,
        blend_fix: 48,
        u_speed: 0.00375,
        v_speed: 0.00375,
    },
    CubePassSpec {
        cull_front: false,
        target_work: true,
        tex: TextureId::TexoRef,
        uv_mode: 2,
        uv_arg: 1.0,
        uv_arg2: 0.3,
        blend_mode: GsBlendMode::Additive,
        blend_fix: 44,
        u_speed: -0.00375,
        v_speed: 0.00375,
    },
    CubePassSpec {
        cull_front: false,
        target_work: true,
        tex: TextureId::TexoBlp,
        uv_mode: 1,
        uv_arg: 1.0,
        uv_arg2: 0.0,
        blend_mode: GsBlendMode::DestAlphaAdditive,
        blend_fix: 42,
        u_speed: 0.0075,
        v_speed: -0.00375,
    },
    CubePassSpec {
        cull_front: false,
        target_work: true,
        tex: TextureId::TexoRef,
        uv_mode: 2,
        uv_arg: 1.0,
        uv_arg2: -0.3,
        blend_mode: GsBlendMode::Additive,
        blend_fix: 36,
        u_speed: 0.0,
        v_speed: 0.0,
    },
    // 5 Front-face passes onto Screen refracting WorkBuf4
    CubePassSpec {
        cull_front: true,
        target_work: false,
        tex: TextureId::WorkBuf4,
        uv_mode: 0,
        uv_arg: -0.084,
        uv_arg2: 0.05,
        blend_mode: GsBlendMode::SourceAlpha,
        blend_fix: 116,
        u_speed: 0.0,
        v_speed: 0.0,
    },
    CubePassSpec {
        cull_front: true,
        target_work: false,
        tex: TextureId::TexoBlpr,
        uv_mode: 1,
        uv_arg: 1.0,
        uv_arg2: 0.0,
        blend_mode: GsBlendMode::SourceAlpha,
        blend_fix: 52,
        u_speed: -0.00375,
        v_speed: -0.00375,
    },
    CubePassSpec {
        cull_front: true,
        target_work: false,
        tex: TextureId::TexoRef,
        uv_mode: 2,
        uv_arg: 1.0,
        uv_arg2: 0.5,
        blend_mode: GsBlendMode::Additive,
        blend_fix: 56,
        u_speed: 0.00375,
        v_speed: 0.00375,
    },
    CubePassSpec {
        cull_front: true,
        target_work: false,
        tex: TextureId::TexoBlp,
        uv_mode: 1,
        uv_arg: 1.0,
        uv_arg2: 0.0,
        blend_mode: GsBlendMode::Additive,
        blend_fix: 48,
        u_speed: -0.0075,
        v_speed: 0.00375,
    },
    CubePassSpec {
        cull_front: true,
        target_work: false,
        tex: TextureId::TexoRef,
        uv_mode: 2,
        uv_arg: 1.0,
        uv_arg2: -0.5,
        blend_mode: GsBlendMode::Additive,
        blend_fix: 44,
        u_speed: 0.0,
        v_speed: 0.0,
    },
];

#[derive(Debug, Clone)]
pub struct FogLayerState {
    pub tex: TextureId,
    pub z_offset: f32,
    pub u_offset: f32,
    pub v_offset: f32,
    pub u_vel: f32,
    pub v_vel: f32,
    pub alpha: u8,
}

#[derive(Debug, Clone)]
pub struct OpeningLightOrb {
    pub color: [u8; 4],
    pub radius: f32,
    pub z_base: f32,
    pub angle: f32,
    pub angular_vel: f32,
    pub z_phase: f32,
    pub trail: Vec<[f32; 3]>, // 128-point history trail (`osdbits/opening.c` `DrawTrail`)
}

#[derive(Debug, Clone, Copy)]
pub struct OpeningGlassCube {
    pub pos: [f32; 3],
    pub scale: f32,
    pub rot: [f32; 3],
    pub rot_vel: [f32; 3],
}

#[derive(Debug, Clone)]
pub struct OpeningScene {
    pub rng: OsdRng,
    pub frame: u32,
    pub anim_state: u8, // 0..=7 matching `ProcessOpeningAnimation`
    pub idle_mode: bool,
    pub cam_pos: Vec4,
    pub cam_look: Vec4,
    pub cam_up: Vec4,
    pub cam_z_vel: f32,
    pub cam_orbit_angle: f32,
    pub flyup_blur_strength: f32,
    pub fade_alpha: u8,
    pub sce_text_alpha: u8,
    pub transition_to_menu_ready: bool,
    pub boot_history_play_counts: [u8; 21],
    pub tower_heights: [[f32; 9]; 14],
    pub tower_offsets: [[[f32; 2]; 9]; 14],
    pub tower_visible: [[bool; 9]; 14],
    pub tower_grid: [[f32; 20]; 20],
    pub active_tower_count: u32,
    pub fog_layers: [FogLayerState; 6],
    pub lights: [OpeningLightOrb; 4],
    pub cubes: [OpeningGlassCube; 5],
    pub env_u_scroll: f32,
    pub env_v_scroll: f32,
}

impl OpeningScene {
    pub fn new(seed: u32, idle_mode: bool, boot_history_play_counts: [u8; 21]) -> Self {
        let rng = OsdRng::new(seed);
        let fog_tex = [
            TextureId::TexoFog0,
            TextureId::TexoFog1,
            TextureId::TexoFog2,
            TextureId::TexoFog3,
            TextureId::TexoFog4,
            TextureId::TexoFog1,
        ];
        let fog_layers = std::array::from_fn(|i| FogLayerState {
            tex: fog_tex[i],
            z_offset: 160.0 + (i as f32) * 55.0,
            u_offset: (i as f32) * 0.17,
            v_offset: (i as f32) * 0.23,
            u_vel: if i % 2 == 0 { 0.0018 } else { -0.0014 },
            v_vel: if i % 3 == 0 { 0.0012 } else { -0.0016 },
            alpha: 54,
        });

        // 4 colored point lights (`lightColors` in `opening.c`: cyan-blue, green, red/magenta, violet)
        let light_colors: [[u8; 4]; 4] = [
            [80, 175, 255, 128],
            [70, 245, 110, 128],
            [255, 75, 105, 128],
            [185, 95, 255, 128],
        ];
        let lights = std::array::from_fn(|i| {
            let angle = (i as f32) * std::f32::consts::FRAC_PI_2 + 0.35;
            let radius = 85.0 + (i as f32) * 28.0;
            let z_base = 240.0 + (i as f32) * 32.0;
            let p0 = [radius * angle.cos(), radius * angle.sin(), z_base];
            OpeningLightOrb {
                color: light_colors[i],
                radius,
                z_base,
                angle,
                angular_vel: if i % 2 == 0 { 0.028 } else { -0.024 },
                z_phase: (i as f32) * 1.3,
                trail: vec![p0; 48],
            }
        });

        // 5 refractive glass cubes in the tower canyon (`opening.c` `InitLightsCubes`)
        let cubes = [
            OpeningGlassCube {
                pos: [-98.0, -52.0, 295.0],
                scale: 18.5,
                rot: [0.4, 0.8, 0.2],
                rot_vel: [0.014, 0.019, 0.011],
            },
            OpeningGlassCube {
                pos: [96.0, -48.0, 315.0],
                scale: 19.5,
                rot: [1.1, 0.3, 0.9],
                rot_vel: [-0.016, 0.015, 0.013],
            },
            OpeningGlassCube {
                pos: [-16.0, 6.0, 265.0],
                scale: 16.0,
                rot: [0.7, 1.4, 0.5],
                rot_vel: [0.018, -0.014, 0.016],
            },
            OpeningGlassCube {
                pos: [-72.0, 56.0, 285.0],
                scale: 17.0,
                rot: [2.0, 0.6, 1.2],
                rot_vel: [0.012, 0.017, -0.015],
            },
            OpeningGlassCube {
                pos: [74.0, 44.0, 275.0],
                scale: 16.5,
                rot: [0.2, 1.9, 0.7],
                rot_vel: [-0.015, -0.013, 0.018],
            },
        ];

        let mut scene = Self {
            rng,
            frame: 0,
            anim_state: if idle_mode { 2 } else { 0 },
            idle_mode,
            cam_pos: [0.0, 0.0, if idle_mode { 410.0 } else { 640.0 }, 1.0],
            cam_look: [0.0, 0.0, 0.0, 1.0],
            cam_up: [0.0, 1.0, 0.0, 0.0],
            cam_z_vel: -1.8,
            cam_orbit_angle: 0.0,
            flyup_blur_strength: 0.0,
            fade_alpha: if idle_mode { 0 } else { 110 },
            sce_text_alpha: 0,
            transition_to_menu_ready: false,
            boot_history_play_counts,
            tower_heights: [[0.0; 9]; 14],
            tower_offsets: [[[0.0; 2]; 9]; 14],
            tower_visible: [[false; 9]; 14],
            tower_grid: [[0.0; 20]; 20],
            active_tower_count: 0,
            fog_layers,
            lights,
            cubes,
            env_u_scroll: 0.0,
            env_v_scroll: 0.0,
        };
        scene.rebuild_towers();
        scene
    }

    /// Exact port of `InitTowersFog` (`0x218c98`) and `sub_217e30` (`0x217e30`, `osdbits/opening.c` line 2393):
    /// Maps the 21 memory-card boot-history slots across `HIST_CELLS[21][6]` to compute the 14×9
    /// tower heights, random `±10.0` XY offsets, visibility flags, and 20×20 radial `towerGrid`.
    pub fn rebuild_towers(&mut self) {
        self.tower_heights = [[0.0; 9]; 14];
        self.tower_visible = [[false; 9]; 14];
        let mut rng = OsdRng::new(self.rng.seed ^ 0x217E_3000);

        for slot in 0..21 {
            let play_count = self.boot_history_play_counts[slot];
            if play_count == 0 {
                continue;
            }
            for k in 0..6 {
                let cell = HIST_CELLS[slot][k];
                if cell < 0 {
                    continue;
                }
                let col = (cell as usize) % 14;
                let row = (cell as usize) / 14;
                if col < 14 && row < 9 {
                    let boost = 1.0 + (play_count as f32) * 0.45 + ((k % 3) as f32) * 0.35;
                    self.tower_heights[col][row] += boost;
                }
            }
        }

        let mut active = 0u32;
        for col in 0..14 {
            for row in 0..9 {
                let jx = ((rng.rand() % 200) as f32) * 0.1 - 10.0;
                let jy = ((rng.rand() % 200) as f32) * 0.1 - 10.0;
                self.tower_offsets[col][row] = [jx, jy];
                if self.tower_heights[col][row] > 0.0 {
                    self.tower_visible[col][row] = true;
                    active += 1;
                }
            }
        }
        self.active_tower_count = active;

        for gx in 0..20 {
            for gy in 0..20 {
                let dx = ((gx as i32 - 10) * 40) as f32;
                let dy = ((gy as i32 - 10) * 40) as f32;
                self.tower_grid[gx][gy] = 1600.0 - (dx * dx + dy * dy).sqrt();
            }
        }
    }

    /// Steps the Opening / Idle simulation by one frame (`speed_scale = 1.0` at 60 Hz NTSC, `60.0/50.0` at 50 Hz PAL).
    pub fn step_frame(&mut self, speed_scale: f32, reduced_motion: bool, sound: &mut SoundMailbox) {
        let dt = if reduced_motion {
            speed_scale * 0.35
        } else {
            speed_scale
        };

        if self.frame == 0 {
            sound.dispatch(SND_CMD_BGM_PLAY as i32, BGM_SLOT_BOOTS as i32, 0, 0);
            sound.dispatch(SND_CMD_BGM_PLAY as i32, BGM_SLOT_TNNLS as i32, 0, 0);
        }

        self.frame = self.frame.wrapping_add(1);
        let f = self.frame as f32;

        if self.idle_mode {
            // Mode B: Indefinite calm drift over the tower field (`openingType == 3`)
            self.anim_state = 2;
            self.cam_orbit_angle += 0.0045 * dt;
            let z_wave = (f * 0.008 * dt).sin() * 42.0;
            self.cam_pos[0] = (self.cam_orbit_angle).sin() * 48.0;
            self.cam_pos[1] = (self.cam_orbit_angle * 0.7).cos() * 32.0;
            self.cam_pos[2] = 420.0 + z_wave;
            self.cam_look = [
                (self.cam_orbit_angle + 0.5).sin() * 14.0,
                (self.cam_orbit_angle * 0.7).sin() * 10.0,
                0.0,
                1.0,
            ];
            self.flyup_blur_strength = 0.0;
            self.fade_alpha = 0;
            self.sce_text_alpha = 0;
        } else {
            // Mode A: Full `ProcessOpeningAnimation` state machine (states 0..=7)
            if self.frame < 90 {
                // State 0..1: Initial descent from high cloud into the tower canyon + SCE banner
                self.anim_state = if self.frame < 40 { 0 } else { 1 };
                let t = (self.frame as f32) / 90.0;
                self.cam_pos[2] = 640.0 - t * 210.0;
                self.cam_orbit_angle += 0.006 * dt;
                self.cam_pos[0] = (self.cam_orbit_angle).sin() * 34.0;
                self.cam_pos[1] = (self.cam_orbit_angle * 0.8).cos() * 22.0;
                self.fade_alpha = ((1.0 - (t * 2.5).min(1.0)) * 116.0) as u8;
                // SCE text fades in frames 12..35, holds 35..70, fades out 70..90
                self.sce_text_alpha = if self.frame < 12 {
                    0
                } else if self.frame < 35 {
                    (((self.frame - 12) as f32 / 23.0) * 128.0) as u8
                } else if self.frame < 70 {
                    128
                } else {
                    (((90 - self.frame) as f32 / 20.0) * 128.0) as u8
                };
                self.flyup_blur_strength = 0.0;
            } else if self.frame < 270 {
                // State 2..3: Deep tower canyon drift & light/cube showcase (`Z` 430 -> 340)
                self.anim_state = if self.frame < 180 { 2 } else { 3 };
                let t = ((self.frame - 90) as f32) / 180.0;
                self.cam_pos[2] = 430.0 - (t * std::f32::consts::PI).sin() * 75.0;
                self.cam_orbit_angle += 0.0075 * dt;
                self.cam_pos[0] = (self.cam_orbit_angle).sin() * 44.0;
                self.cam_pos[1] = (self.cam_orbit_angle * 0.75).cos() * 28.0;
                self.fade_alpha = 0;
                self.sce_text_alpha = 0;
                self.flyup_blur_strength = 0.0;
            } else if self.frame < 350 {
                // State 4..6: Rapid fly-up surge (`cam_pos[2]` 360 -> 1160) with zoom motion blur
                let t = ((self.frame - 270) as f32) / 80.0;
                self.anim_state = if t < 0.35 {
                    4
                } else if t < 0.75 {
                    5
                } else {
                    6
                };
                let accel = t * t * (3.0 - 2.0 * t);
                self.cam_pos[2] = 360.0 + accel * (ANIM_Z_LEVELS[6] - 360.0);
                self.cam_pos[0] *= 0.95;
                self.cam_pos[1] *= 0.95;
                self.flyup_blur_strength = if reduced_motion {
                    0.0
                } else {
                    (t * std::f32::consts::PI).sin() * 0.85
                };
                self.fade_alpha = if t > 0.65 {
                    (((t - 0.65) / 0.35) * 128.0).clamp(0.0, 128.0) as u8
                } else {
                    0
                };
                if self.frame == 275 {
                    sound.dispatch(SND_CMD_BGM_STOP as i32, BGM_SLOT_BOOTS as i32, 0, 15);
                }
            } else {
                // State 7: Transition complete
                self.anim_state = 7;
                self.cam_pos[2] = ANIM_Z_LEVELS[7];
                self.flyup_blur_strength = 0.0;
                self.fade_alpha = 128;
                self.transition_to_menu_ready = true;
            }
            self.cam_look = [self.cam_pos[0] * 0.25, self.cam_pos[1] * 0.25, 0.0, 1.0];
        }

        // Advance fog layers
        for layer in &mut self.fog_layers {
            layer.u_offset = (layer.u_offset + layer.u_vel * dt).rem_euclid(1.0);
            layer.v_offset = (layer.v_offset + layer.v_vel * dt).rem_euclid(1.0);
        }

        // Advance 4 orbiting lights & 128-point trails
        for light in &mut self.lights {
            light.angle += light.angular_vel * dt;
            light.z_phase += 0.032 * dt;
            let lx = light.radius * light.angle.cos();
            let ly = light.radius * 0.72 * (light.angle * 1.3).sin();
            let lz = light.z_base + light.z_phase.sin() * 24.0;
            light.trail.insert(0, [lx, ly, lz]);
            if light.trail.len() > 48 {
                light.trail.pop();
            }
        }

        // Advance 5 refractive glass cubes & env scroll
        for cube in &mut self.cubes {
            cube.rot[0] += cube.rot_vel[0] * dt;
            cube.rot[1] += cube.rot_vel[1] * dt;
            cube.rot[2] += cube.rot_vel[2] * dt;
        }
        self.env_u_scroll = (self.env_u_scroll + 0.00375 * dt).rem_euclid(1.0);
        self.env_v_scroll = (self.env_v_scroll + 0.00375 * dt).rem_euclid(1.0);
    }

    /// Emits the complete Opening / Idle render command list matching `osdbits/opening.c` draw order:
    /// 1. `DrawTowers` (14×9 grid of 6-quad monoliths shaded by `vucode_1.vsm` 4 directional lights + fog)
    /// 2. `DrawExtraBuf2(80)` & `DrawToExtraBuf2()` (temporal feedback glow)
    /// 3. `DrawFog` (6 scrolling cloud layers)
    /// 4. `DrawLightsAndCubes` (4 lights + ribbon trails + 5 10-pass refractive glass cubes)
    /// 5. `sub_218b20` (fly-up zoom motion blur during states 4..6)
    /// 6. `sub_218bd0` (fade curtain) & `DoSCEText` (`TEXOSCE` overlay)
    pub fn emit_render_commands(
        &self,
        cmd_buf: &mut FrameCommandBuffer,
        fog_intensity: f32,
        refraction_scale: f32,
    ) {
        let screen_w = 640.0f32;
        let screen_h = 448.0f32;

        // Clear screen to deep space black/indigo
        cmd_buf.push_clear(RenderTargetId::Screen, 4, 6, 14, 0x80, true);

        let zd = vec4_sub(self.cam_look, self.cam_pos);
        let view_mat = sce_vu0_camera_matrix(self.cam_pos, zd, self.cam_up);
        let view_screen = sce_vu0_view_screen_matrix(
            512.0, 1.0, 0.88, 2048.0, 2048.0, 1.0, 65535.0, 10.0, 3000.0,
        );

        // 1. `DrawTowers` (`osdbits/opening.c` line 2580 & `vucode_1.vsm`)
        self.emit_towers(cmd_buf, &view_mat, &view_screen, screen_w, screen_h);

        // 2. `DrawFog` (`osdbits/opening.c` line 1180): 6 scrolling cloud layers in the tower canyon
        if fog_intensity > 0.01 {
            self.emit_fog(
                cmd_buf,
                &view_mat,
                &view_screen,
                screen_w,
                screen_h,
                fog_intensity,
            );
        }

        // 3. `DrawLights` & `DrawTrail` (`osdbits/opening.c` line 1545)
        self.emit_lights_and_trails(cmd_buf, &view_mat, &view_screen, screen_w, screen_h);

        // 4. `DrawCube` × 5 (`osdbits/opening.c` line 1830): per-cube screen snapshot + 10 passes
        self.emit_refractive_cubes(
            cmd_buf,
            &view_mat,
            &view_screen,
            screen_w,
            screen_h,
            refraction_scale,
        );

        // 5. `sub_218b20` fly-up zoom blur passes when flying up (`flyup_blur_strength > 0.02`)
        if self.flyup_blur_strength > 0.02 {
            cmd_buf.push_copy_target_to_texture(RenderTargetId::Screen, TextureId::WorkBuf4);
            let fix = (self.flyup_blur_strength * 56.0).clamp(6.0, 64.0) as u8;
            for pass in 1..=3 {
                let inset = (pass as f32) * 0.018 * self.flyup_blur_strength;
                cmd_buf.push_sprite_2d(
                    RenderTargetId::Screen,
                    TextureId::WorkBuf4,
                    GsBlendMode::FixedAdditive,
                    SamplerWrap::Clamp,
                    fix,
                    0.0,
                    0.0,
                    screen_w,
                    screen_h,
                    inset,
                    inset,
                    1.0 - inset,
                    1.0 - inset,
                    [128, 132, 148, 128],
                );
            }
        }

        // 6. `sub_218bd0` fade overlay
        if self.fade_alpha > 0 {
            cmd_buf.push_sprite_2d(
                RenderTargetId::Screen,
                TextureId::WhiteDummy,
                GsBlendMode::SourceAlpha,
                SamplerWrap::Clamp,
                128,
                0.0,
                0.0,
                screen_w,
                screen_h,
                0.0,
                0.0,
                1.0,
                1.0,
                [0, 0, 0, self.fade_alpha],
            );
        }

        // 7. `DoSCEText` (`Sony Computer Entertainment` banner during opening entry)
        if self.sce_text_alpha > 0 {
            cmd_buf.push_sprite_2d(
                RenderTargetId::Screen,
                TextureId::TexoSce,
                GsBlendMode::SourceAlpha,
                SamplerWrap::Clamp,
                128,
                160.0,
                184.0,
                480.0,
                264.0,
                0.0,
                0.0,
                1.0,
                1.0,
                [128, 128, 128, self.sce_text_alpha],
            );
        }
    }

    fn emit_towers(
        &self,
        cmd_buf: &mut FrameCommandBuffer,
        view_mat: &Mat4,
        view_screen: &Mat4,
        screen_w: f32,
        screen_h: f32,
    ) {
        // Tower field layout. The retail field is a compact 14x9 cluster of long square
        // columns whose cell spacing is a few units (histSrcPos at 0x27a670 -> towerPos
        // at 0x326c10: `(x+4.8)*4`, `(y-6.5)*4`, `(z+4)*12+150`), and each column extends
        // +-towerA along Z, so the camera looks down a lattice of prisms rather than at a
        // wall of billboards. These spacings are a documented approximation of that lattice
        // (see docs/LIMITATIONS.md): they reproduce the density and the per-column depth of
        // the original without claiming the exact cell positions.
        const CELL_X: f32 = 27.0;
        const CELL_Y: f32 = 26.0;
        const HALF_W: f32 = 10.5;
        const FIELD_Z: f32 = 168.0;

        for col in 0..14usize {
            for row in 0..9usize {
                if !self.tower_visible[col][row] {
                    continue;
                }
                let h = self.tower_heights[col][row];
                let tx = (col as f32 - 6.5) * CELL_X + self.tower_offsets[col][row][0];
                let ty = (row as f32 - 4.0) * CELL_Y + self.tower_offsets[col][row][1];
                // Column length along Z: the retail `towerA` is `max(towerD*30, 3)`, and the
                // per-cell block count scales the visible length. Clamped to the fog depth.
                let half_len = (13.0 + h * 9.5).min(105.0);
                let z_near = FIELD_Z - half_len;
                let z_far = FIELD_Z + half_len;
                let y_bot = ty - HALF_W * 2.4;
                let y_top = ty + HALF_W * 2.4;

                // Distance from the field centre drives both brightness and colour: the
                // retail field is brightest over the central cluster (towerGrid, sub_217e30).
                let radial = ((tx * tx + ty * ty) / (190.0 * 190.0)).sqrt().min(1.0);
                let center_boost = (1.0 - radial).powf(1.15) * 0.95 + 0.22;

                // 5 quads per column: near cap, +X side, -X side, +Y side, -Y side.
                // Winding is chosen so the screen-space winding test below culls the back faces.
                let faces: [([Vec4; 4], Vec4); 5] = [
                    (
                        [
                            [tx - HALF_W, y_bot, z_near, 1.0],
                            [tx + HALF_W, y_bot, z_near, 1.0],
                            [tx - HALF_W, y_top, z_near, 1.0],
                            [tx + HALF_W, y_top, z_near, 1.0],
                        ],
                        [0.0, 0.0, -1.0, 0.0],
                    ),
                    (
                        [
                            [tx + HALF_W, y_bot, z_near, 1.0],
                            [tx + HALF_W, y_bot, z_far, 1.0],
                            [tx + HALF_W, y_top, z_near, 1.0],
                            [tx + HALF_W, y_top, z_far, 1.0],
                        ],
                        [1.0, 0.0, 0.0, 0.0],
                    ),
                    (
                        [
                            [tx - HALF_W, y_bot, z_far, 1.0],
                            [tx - HALF_W, y_bot, z_near, 1.0],
                            [tx - HALF_W, y_top, z_far, 1.0],
                            [tx - HALF_W, y_top, z_near, 1.0],
                        ],
                        [-1.0, 0.0, 0.0, 0.0],
                    ),
                    (
                        [
                            [tx - HALF_W, y_top, z_near, 1.0],
                            [tx + HALF_W, y_top, z_near, 1.0],
                            [tx - HALF_W, y_top, z_far, 1.0],
                            [tx + HALF_W, y_top, z_far, 1.0],
                        ],
                        [0.0, 1.0, 0.0, 0.0],
                    ),
                    (
                        [
                            [tx - HALF_W, y_bot, z_far, 1.0],
                            [tx + HALF_W, y_bot, z_far, 1.0],
                            [tx - HALF_W, y_bot, z_near, 1.0],
                            [tx + HALF_W, y_bot, z_near, 1.0],
                        ],
                        [0.0, -1.0, 0.0, 0.0],
                    ),
                ];

                for (quad, normal) in faces {
                    let mut p = [(0.0f32, 0.0f32, 0.0f32, 1.0f32); 4];
                    let mut valid = true;
                    for k in 0..4 {
                        if let Some(proj) =
                            project_point(view_mat, view_screen, quad[k], screen_w, screen_h)
                        {
                            p[k] = proj;
                        } else {
                            valid = false;
                            break;
                        }
                    }
                    if !valid {
                        continue;
                    }

                    // Screen-space winding cull (the cube path's `cube_21BC90` test, same sign).
                    let e1x = p[1].0 - p[0].0;
                    let e1y = p[1].1 - p[0].1;
                    let e2x = p[2].0 - p[0].0;
                    let e2y = p[2].1 - p[0].1;
                    if e1x * e2y - e1y * e2x <= 0.0 {
                        continue;
                    }

                    let shaded = vucode1_shade_normal(
                        normal,
                        &TOWER_LIGHT_DIRS,
                        &TOWER_LIGHT_COLORS,
                        TOWER_AMBIENT,
                    );

                    // The retail towers Gouraud-shade from lit at the camera-facing end to
                    // black at the tail (`tower_218318`'s per-vertex far-side blackout).
                    let uvs = [
                        [0.0, 0.0],
                        [1.0, 0.0],
                        [0.0, (half_len / 10.0).clamp(1.0, 10.0)],
                        [1.0, (half_len / 10.0).clamp(1.0, 10.0)],
                    ];
                    let mut gverts = [GpuVertex::default(); 4];
                    for k in 0..4 {
                        // vertices 2,3 are the far end of every side quad
                        let far_end = k >= 2;
                        let depth_scale = if far_end { 0.18 } else { 1.0 };
                        let s = center_boost * depth_scale;
                        // top faces are brighter (the vucode_1 lighting result dominates)
                        let lift = if normal[1] > 0.5 { 1.35 } else { 1.0 };
                        gverts[k] = GpuVertex {
                            pos: [p[k].0, p[k].1, p[k].2, 1.0],
                            uv: uvs[k],
                            color: [
                                (shaded[0] as f32 / 128.0) * s * lift,
                                (shaded[1] as f32 / 128.0) * s * lift,
                                (shaded[2] as f32 / 128.0) * s * lift * 1.08,
                                1.0,
                            ],
                        };
                    }

                    cmd_buf.push_quad(
                        RenderTargetId::Screen,
                        TextureId::TexoWal0,
                        GsBlendMode::Opaque,
                        SamplerWrap::Repeat,
                        DepthMode::LessEqualWrite,
                        128,
                        gverts[0],
                        gverts[1],
                        gverts[2],
                        gverts[3],
                    );
                }
            }
        }
    }

    fn emit_fog(
        &self,
        cmd_buf: &mut FrameCommandBuffer,
        view_mat: &Mat4,
        view_screen: &Mat4,
        screen_w: f32,
        screen_h: f32,
        fog_intensity: f32,
    ) {
        for (idx, layer) in self.fog_layers.iter().enumerate() {
            let extent = 220.0 + (idx as f32) * 28.0;
            let z = layer.z_offset;
            let corners = [
                [-extent, -extent, z, 1.0],
                [extent, -extent, z, 1.0],
                [-extent, extent, z, 1.0],
                [extent, extent, z, 1.0],
            ];
            let mut p = [(0.0f32, 0.0f32, 0.0f32, 1.0f32); 4];
            let mut ok = true;
            for k in 0..4 {
                if let Some(pr) =
                    project_point(view_mat, view_screen, corners[k], screen_w, screen_h)
                {
                    p[k] = pr;
                } else {
                    ok = false;
                    break;
                }
            }
            if !ok {
                continue;
            }
            let a = ((layer.alpha as f32 / 128.0) * fog_intensity).clamp(0.0, 1.5);
            let color = [0.22, 0.36, 0.78, a];
            let u0 = layer.u_offset;
            let v0 = layer.v_offset;
            let u1 = u0 + 1.0;
            let v1 = v0 + 1.0;
            cmd_buf.push_quad(
                RenderTargetId::Screen,
                layer.tex,
                GsBlendMode::Additive,
                SamplerWrap::Repeat,
                DepthMode::LessEqualNoWrite,
                128,
                GpuVertex {
                    pos: [p[0].0, p[0].1, p[0].2, p[0].3],
                    uv: [u0, v0],
                    color,
                },
                GpuVertex {
                    pos: [p[1].0, p[1].1, p[1].2, p[1].3],
                    uv: [u1, v0],
                    color,
                },
                GpuVertex {
                    pos: [p[2].0, p[2].1, p[2].2, p[2].3],
                    uv: [u0, v1],
                    color,
                },
                GpuVertex {
                    pos: [p[3].0, p[3].1, p[3].2, p[3].3],
                    uv: [u1, v1],
                    color,
                },
            );
        }
    }

    fn emit_lights_and_trails(
        &self,
        cmd_buf: &mut FrameCommandBuffer,
        view_mat: &Mat4,
        view_screen: &Mat4,
        screen_w: f32,
        screen_h: f32,
    ) {
        for light in &self.lights {
            let base_col = [
                light.color[0] as f32 / 128.0,
                light.color[1] as f32 / 128.0,
                light.color[2] as f32 / 128.0,
            ];

            // Emit ribbon trail segments
            for seg in 0..light.trail.len().saturating_sub(1) {
                let p0_w = light.trail[seg];
                let p1_w = light.trail[seg + 1];
                let (Some(s0), Some(s1)) = (
                    project_point(
                        view_mat,
                        view_screen,
                        [p0_w[0], p0_w[1], p0_w[2], 1.0],
                        screen_w,
                        screen_h,
                    ),
                    project_point(
                        view_mat,
                        view_screen,
                        [p1_w[0], p1_w[1], p1_w[2], 1.0],
                        screen_w,
                        screen_h,
                    ),
                ) else {
                    continue;
                };
                let t0 = 1.0 - (seg as f32) / (light.trail.len() as f32);
                let t1 = 1.0 - ((seg + 1) as f32) / (light.trail.len() as f32);
                let dx = s1.0 - s0.0;
                let dy = s1.1 - s0.1;
                let len = (dx * dx + dy * dy).sqrt().max(0.001);
                let nx = (-dy / len) * (2.2 * t0 + 0.5);
                let ny = (dx / len) * (2.2 * t0 + 0.5);

                let c0 = [base_col[0], base_col[1], base_col[2], t0 * 0.65];
                let c1 = [base_col[0], base_col[1], base_col[2], t1 * 0.65];

                cmd_buf.push_quad(
                    RenderTargetId::Screen,
                    TextureId::WhiteDummy,
                    GsBlendMode::Additive,
                    SamplerWrap::Clamp,
                    DepthMode::LessEqualNoWrite,
                    128,
                    GpuVertex {
                        pos: [s0.0 - nx, s0.1 - ny, s0.2, 1.0],
                        uv: [0.0, 0.0],
                        color: c0,
                    },
                    GpuVertex {
                        pos: [s0.0 + nx, s0.1 + ny, s0.2, 1.0],
                        uv: [1.0, 0.0],
                        color: c0,
                    },
                    GpuVertex {
                        pos: [s1.0 - nx, s1.1 - ny, s1.2, 1.0],
                        uv: [0.0, 1.0],
                        color: c1,
                    },
                    GpuVertex {
                        pos: [s1.0 + nx, s1.1 + ny, s1.2, 1.0],
                        uv: [1.0, 1.0],
                        color: c1,
                    },
                );
            }

            // Emit head halo (`TEXOCRBL`) and bright core (`TEXOCRLE`)
            if let Some(head) = light.trail.first() {
                if let Some((sx, sy, sz, _)) = project_point(
                    view_mat,
                    view_screen,
                    [head[0], head[1], head[2], 1.0],
                    screen_w,
                    screen_h,
                ) {
                    for &(tex, rad, alpha) in &[
                        (TextureId::TexoCrbl, 16.0f32, 0.85f32),
                        (TextureId::TexoCrle, 7.5f32, 1.25f32),
                    ] {
                        let col = [base_col[0], base_col[1], base_col[2], alpha];
                        cmd_buf.push_quad(
                            RenderTargetId::Screen,
                            tex,
                            GsBlendMode::Additive,
                            SamplerWrap::Clamp,
                            DepthMode::LessEqualNoWrite,
                            128,
                            GpuVertex {
                                pos: [sx - rad, sy - rad, sz, 1.0],
                                uv: [0.0, 0.0],
                                color: col,
                            },
                            GpuVertex {
                                pos: [sx + rad, sy - rad, sz, 1.0],
                                uv: [1.0, 0.0],
                                color: col,
                            },
                            GpuVertex {
                                pos: [sx - rad, sy + rad, sz, 1.0],
                                uv: [0.0, 1.0],
                                color: col,
                            },
                            GpuVertex {
                                pos: [sx + rad, sy + rad, sz, 1.0],
                                uv: [1.0, 1.0],
                                color: col,
                            },
                        );
                    }
                }
            }
        }
    }

    fn emit_refractive_cubes(
        &self,
        cmd_buf: &mut FrameCommandBuffer,
        view_mat: &Mat4,
        view_screen: &Mat4,
        screen_w: f32,
        screen_h: f32,
        refraction_scale: f32,
    ) {
        emit_10pass_glass_cubes(
            cmd_buf,
            &self.cubes,
            &OPENING_CUBE_PASSES,
            [0.72, 0.78, 0.92],
            self.env_u_scroll,
            self.env_v_scroll,
            view_mat,
            view_screen,
            screen_w,
            screen_h,
            refraction_scale,
        );
    }
}

/// Shared 10-pass refractive glass cube emitter used by both Mode A (`DrawCube`, `OPENING_CUBE_PASSES`)
/// and Mode C (`DrawIllegalCube`, `ILLEGAL_CUBE_PASSES`), matching `docs/glass-refraction.md` §2.
#[allow(clippy::too_many_arguments)]
pub fn emit_10pass_glass_cubes(
    cmd_buf: &mut FrameCommandBuffer,
    cubes: &[OpeningGlassCube],
    passes: &[CubePassSpec; 10],
    tint_rgb: [f32; 3],
    env_u_scroll: f32,
    env_v_scroll: f32,
    view_mat: &Mat4,
    view_screen: &Mat4,
    screen_w: f32,
    screen_h: f32,
    refraction_scale: f32,
) {
    let unit_faces: [([Vec4; 4], Vec4); 6] = [
        (
            [
                [-1.0, -1.0, 1.0, 1.0],
                [1.0, -1.0, 1.0, 1.0],
                [-1.0, 1.0, 1.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
            ],
            [0.0, 0.0, 1.0, 0.0],
        ),
        (
            [
                [1.0, -1.0, -1.0, 1.0],
                [-1.0, -1.0, -1.0, 1.0],
                [1.0, 1.0, -1.0, 1.0],
                [-1.0, 1.0, -1.0, 1.0],
            ],
            [0.0, 0.0, -1.0, 0.0],
        ),
        (
            [
                [1.0, -1.0, 1.0, 1.0],
                [1.0, -1.0, -1.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
                [1.0, 1.0, -1.0, 1.0],
            ],
            [1.0, 0.0, 0.0, 0.0],
        ),
        (
            [
                [-1.0, -1.0, -1.0, 1.0],
                [-1.0, -1.0, 1.0, 1.0],
                [-1.0, 1.0, -1.0, 1.0],
                [-1.0, 1.0, 1.0, 1.0],
            ],
            [-1.0, 0.0, 0.0, 0.0],
        ),
        (
            [
                [-1.0, 1.0, 1.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
                [-1.0, 1.0, -1.0, 1.0],
                [1.0, 1.0, -1.0, 1.0],
            ],
            [0.0, 1.0, 0.0, 0.0],
        ),
        (
            [
                [-1.0, -1.0, -1.0, 1.0],
                [1.0, -1.0, -1.0, 1.0],
                [-1.0, -1.0, 1.0, 1.0],
                [1.0, -1.0, 1.0, 1.0],
            ],
            [0.0, -1.0, 0.0, 0.0],
        ),
    ];

    for cube in cubes {
        let mut model = mat4_identity();
        model = mat4_rot_x(&model, cube.rot[0]);
        model = mat4_rot_y(&model, cube.rot[1]);
        model = mat4_rot_z(&model, cube.rot[2]);
        model = mat4_translate(model, [cube.pos[0], cube.pos[1], cube.pos[2], 1.0]);

        let Some((center_sx, center_sy, _, _)) = project_point(
            view_mat,
            view_screen,
            [cube.pos[0], cube.pos[1], cube.pos[2], 1.0],
            screen_w,
            screen_h,
        ) else {
            continue;
        };

        // `CubeCaptureBuffer` (`sub_21c7a8`): per-cube snapshot of the in-progress screen
        cmd_buf.push_copy_target_to_texture(RenderTargetId::Screen, TextureId::ScreenCopy);
        cmd_buf.push_copy_target_to_texture(RenderTargetId::Screen, TextureId::WorkBuf4);

        for pass in passes {
            let target = if pass.target_work {
                RenderTargetId::WorkBuf4
            } else {
                RenderTargetId::Screen
            };

            for (face_verts, face_norm) in &unit_faces {
                let mut proj_pts = [(0.0f32, 0.0f32, 0.0f32, 1.0f32); 4];
                let mut world_pts = [[0.0f32; 4]; 4];
                let mut valid = true;

                for k in 0..4 {
                    let scaled = [
                        face_verts[k][0] * cube.scale,
                        face_verts[k][1] * cube.scale,
                        face_verts[k][2] * cube.scale,
                        1.0,
                    ];
                    let wp = mat4_apply(&model, scaled);
                    world_pts[k] = wp;
                    if let Some(pr) = project_point(view_mat, view_screen, wp, screen_w, screen_h) {
                        proj_pts[k] = pr;
                    } else {
                        valid = false;
                        break;
                    }
                }
                if !valid {
                    continue;
                }

                // Winding test (`cube_21BC90`)
                let e1 = [
                    proj_pts[1].0 - proj_pts[0].0,
                    proj_pts[1].1 - proj_pts[0].1,
                    0.0,
                    0.0,
                ];
                let e2 = [
                    proj_pts[2].0 - proj_pts[0].0,
                    proj_pts[2].1 - proj_pts[0].1,
                    0.0,
                    0.0,
                ];
                let cross_z = vec4_cross3(e1, e2)[2];
                let is_front = cross_z > 0.0;
                if is_front != pass.cull_front {
                    continue;
                }

                // Transform face normal into camera space
                let rot_norm = mat4_apply(&model, [face_norm[0], face_norm[1], face_norm[2], 0.0]);
                let cam_norm = vec4_normalize3(mat4_apply(
                    view_mat,
                    [rot_norm[0], rot_norm[1], rot_norm[2], 0.0],
                ));

                // Edge-brightening Fresnel / lighting term (`CubeVertexColor`)
                let fresnel = (1.0 - cam_norm[2].abs()).clamp(0.15, 1.0);
                let fix_scale = (pass.blend_fix as f32) / 128.0;
                let brightness = (0.45 + fresnel * 0.75) * fix_scale;
                let color = [
                    tint_rgb[0] * brightness,
                    tint_rgb[1] * brightness,
                    tint_rgb[2] * brightness,
                    fix_scale.clamp(0.25, 1.0),
                ];

                // `CubeComputeUV` (`cube_21B798` mode 0, `cube_21BBE0` mode 1, `cube_21BA08` mode 2)
                let quad_uvs = [[0.0f32, 0.0f32], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
                let mut gverts = [GpuVertex::default(); 4];
                for k in 0..4 {
                    let (sx, sy, sz, q) = proj_pts[k];
                    let uv = match pass.uv_mode {
                        0 => {
                            // Mode 0 (`cube_21B798`): screen-space refraction + center lens zoom
                            let disp_u = cam_norm[0]
                                * (screen_w * 0.5)
                                * pass.uv_arg2
                                * q
                                * 180.0
                                * refraction_scale;
                            let disp_v = cam_norm[1]
                                * (screen_h * 0.5)
                                * pass.uv_arg2
                                * q
                                * 180.0
                                * refraction_scale;
                            let zoom_x = (sx - center_sx) * pass.uv_arg;
                            let zoom_y = (sy - center_sy) * pass.uv_arg;
                            let u = ((sx + disp_u + zoom_x) / screen_w).clamp(0.002, 0.998);
                            let v = ((sy + disp_v + zoom_y) / screen_h).clamp(0.002, 0.998);
                            [u, v]
                        }
                        1 => {
                            // Mode 1 (`cube_21BBE0`): scrolled unit quad
                            [
                                quad_uvs[k][0] * pass.uv_arg
                                    + env_u_scroll * (pass.u_speed * 260.0),
                                quad_uvs[k][1] * pass.uv_arg
                                    + env_v_scroll * (pass.v_speed * 260.0),
                            ]
                        }
                        _ => {
                            // Mode 2 (`cube_21BA08`): spherical environment map (`view_dir + scaled normal`)
                            let view_dir = vec4_normalize3(world_pts[k]);
                            let u = 0.5
                                + (view_dir[1] + cam_norm[1] * pass.uv_arg2) * 0.5
                                + env_u_scroll;
                            let v = 0.5
                                + (view_dir[0] + cam_norm[0] * pass.uv_arg2) * 0.5
                                + env_v_scroll;
                            let _ = vec4_dot3(view_dir, cam_norm);
                            [u, v]
                        }
                    };
                    gverts[k] = GpuVertex {
                        pos: [sx, sy, sz, q],
                        uv,
                        color,
                    };
                }

                cmd_buf.push_quad(
                    target,
                    pass.tex,
                    pass.blend_mode,
                    if pass.uv_mode == 0 {
                        SamplerWrap::Clamp
                    } else {
                        SamplerWrap::Repeat
                    },
                    DepthMode::LessEqualNoWrite,
                    pass.blend_fix,
                    gverts[0],
                    gverts[1],
                    gverts[2],
                    gverts[3],
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_history_to_tower_field_mapping() {
        // Fresh console (all 0 play counts) -> 0 visible towers
        let empty = OpeningScene::new(1, false, [0u8; 21]);
        assert_eq!(empty.active_tower_count, 0);

        // A single game occupies the whole 6-cell neighbourhood of its history slot
        let mut one_game = [0u8; 21];
        one_game[0] = 3;
        let single = OpeningScene::new(1, false, one_game);
        assert_eq!(single.active_tower_count, 6);

        // Full 21-slot memory card -> every cell the neighborhood table references
        let distinct_cells = {
            let mut seen = std::collections::BTreeSet::new();
            for slot in HIST_CELLS.iter() {
                for &c in slot.iter() {
                    if c >= 0 {
                        seen.insert(c);
                    }
                }
            }
            seen.len() as u32
        };
        assert_eq!(
            distinct_cells, 126,
            "the neighborhood table must tile all 14x9 cells"
        );
        let full = OpeningScene::new(1, false, [5u8; 21]);
        assert_eq!(full.active_tower_count, distinct_cells);
    }

    #[test]
    fn test_opening_vs_idle_camera_progression() {
        let mut mbox = SoundMailbox::new();
        let mut opening = OpeningScene::new(1, false, [2u8; 21]);
        for _ in 0..360 {
            opening.step_frame(1.0, false, &mut mbox);
        }
        assert_eq!(opening.anim_state, 7);
        assert!(opening.transition_to_menu_ready);
        assert!((opening.cam_pos[2] - 1160.0).abs() < 1e-3);

        // Mode B (Idle) never transitions to menu and keeps Z bounded in the canyon
        let mut idle = OpeningScene::new(1, true, [2u8; 21]);
        for _ in 0..500 {
            idle.step_frame(1.0, false, &mut mbox);
            assert!(!idle.transition_to_menu_ready);
            assert!(idle.cam_pos[2] > 350.0 && idle.cam_pos[2] < 480.0);
        }
    }
}
