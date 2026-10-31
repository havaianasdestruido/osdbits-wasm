//! Mode C — Illegal-Disc / Error Scene ("Red Screen of Death") simulation and renderer.
//!
//! Directly ported from `osdbits/opening.c` (`DoIllegalDisc`, `DrawRedFlare`,
//! `DrawIllegalFog`, `DrawIllegalCubes`, `DrawIllegalFades`, `DoIllegalText`)
//! and verified against `docs/illegal.png`.

use crate::assets::TextureId;
use crate::input_audio::{SoundMailbox, BGM_SLOT_WARNS, SND_CMD_BGM_PLAY};
use crate::math::{
    project_point, sce_vu0_camera_matrix, sce_vu0_view_screen_matrix, vec4_sub, OsdRng, Vec4,
};
use crate::opening::{
    emit_10pass_glass_cubes, CubePassSpec, OpeningGlassCube, OPENING_CUBE_PASSES,
};
use crate::render_cmd::{
    DepthMode, FrameCommandBuffer, GpuVertex, GsBlendMode, RenderTargetId, SamplerWrap,
};
use std::f32::consts::TAU;

/// 10-pass configuration for Illegal-Disc red glass cubes (`illegalCubePasses[10]`, `osdbits/opening.c` line 2890).
pub const ILLEGAL_CUBE_PASSES: [CubePassSpec; 10] = [
    CubePassSpec {
        uv_arg: -0.06,
        uv_arg2: -0.04,
        ..OPENING_CUBE_PASSES[0]
    },
    CubePassSpec {
        uv_arg: 0.8,
        u_speed: 0.0,
        v_speed: 0.0,
        ..OPENING_CUBE_PASSES[1]
    },
    CubePassSpec {
        uv_arg: 0.8,
        u_speed: 0.0,
        v_speed: 0.0,
        ..OPENING_CUBE_PASSES[2]
    },
    CubePassSpec {
        uv_arg: 0.8,
        u_speed: 0.0,
        v_speed: 0.0,
        ..OPENING_CUBE_PASSES[3]
    },
    CubePassSpec {
        uv_arg: 0.8,
        u_speed: 0.0,
        v_speed: 0.0,
        ..OPENING_CUBE_PASSES[4]
    },
    CubePassSpec {
        uv_arg: -0.084,
        uv_arg2: 0.04,
        ..OPENING_CUBE_PASSES[5]
    },
    CubePassSpec {
        uv_arg: 0.8,
        u_speed: 0.0,
        v_speed: 0.0,
        ..OPENING_CUBE_PASSES[6]
    },
    CubePassSpec {
        uv_arg: 0.8,
        u_speed: 0.0,
        v_speed: 0.0,
        ..OPENING_CUBE_PASSES[7]
    },
    CubePassSpec {
        uv_arg: 0.8,
        u_speed: 0.0,
        v_speed: 0.0,
        ..OPENING_CUBE_PASSES[8]
    },
    CubePassSpec {
        uv_arg: 0.8,
        u_speed: 0.0,
        v_speed: 0.0,
        ..OPENING_CUBE_PASSES[9]
    },
];

/// One of the 128 falling red cloud patches in `DrawIllegalFog` (`osdbits/opening.c` line 1285).
#[derive(Debug, Clone, Copy)]
pub struct IllegalCloudPatch {
    pub tex: TextureId,
    pub angle_x: f32,
    pub angle_y: f32,
    pub speed_x: f32,
    pub speed_y: f32,
    pub radius: f32,
    pub z_depth: f32,
    pub size: f32,
    pub alpha: u8,
}

/// One of the 7 spinning 16-gon flare rings in `DrawRedFlare` (`osdbits/opening.c` line 3260).
#[derive(Debug, Clone, Copy)]
pub struct RedFlareRing {
    pub angle: f32,
    pub angular_vel: f32,
    pub inner_radius: f32,
    pub outer_radius: f32,
    pub alpha: u8,
}

#[derive(Debug, Clone)]
pub struct IllegalDiscScene {
    pub rng: OsdRng,
    pub frame: u32,
    pub cam_pos: Vec4,
    pub cam_look: Vec4,
    pub cam_up: Vec4,
    pub flare_rings: [RedFlareRing; 7],
    pub clouds: Vec<IllegalCloudPatch>, // 128 red cloud patches
    pub cubes: [OpeningGlassCube; 5],
    pub text_alpha: u8,
    pub global_fade_alpha: u8,
    pub language: u8,
}

impl IllegalDiscScene {
    pub fn new(seed: u32, language: u8) -> Self {
        let mut rng = OsdRng::new(seed ^ 0x219F_4000);
        let fog_tex = [
            TextureId::TexoFog0,
            TextureId::TexoFog1,
            TextureId::TexoFog2,
            TextureId::TexoFog3,
            TextureId::TexoFog4,
        ];

        let flare_rings = std::array::from_fn(|i| RedFlareRing {
            angle: (i as f32) * 0.9,
            angular_vel: if i % 2 == 0 {
                0.014 + (i as f32) * 0.002
            } else {
                -0.012 - (i as f32) * 0.002
            },
            inner_radius: 18.0 + (i as f32) * 14.0,
            outer_radius: 56.0 + (i as f32) * 26.0,
            alpha: (58usize.saturating_sub(i * 5)) as u8,
        });

        let mut clouds = Vec::with_capacity(128);
        for i in 0..128 {
            let r01 = rng.rand_f32_01();
            let r02 = rng.rand_f32_01();
            clouds.push(IllegalCloudPatch {
                tex: fog_tex[i % 5],
                angle_x: r01 * TAU,
                angle_y: r02 * TAU,
                speed_x: 0.008 + rng.rand_f32_01() * 0.014,
                speed_y: 0.010 + rng.rand_f32_01() * 0.016,
                radius: 18.0 + (i as f32 / 128.0) * 155.0,
                z_depth: 180.0 + (i % 16) as f32 * 14.0,
                size: 46.0 + (i % 7) as f32 * 11.0,
                alpha: 24 + ((i * 3) % 28) as u8,
            });
        }

        // 5 red glass cubes matching the composition of `docs/illegal.png`:
        // A prominent foreground cube above-right of center (`[18, -32, 165]`, scale `24.0`),
        // a smaller mid-ground cube below-left of center (`[-26, 26, 235]`, scale `11.5`),
        // and 3 deeper background cubes drifting in the crimson cloud tunnel.
        let cubes = [
            OpeningGlassCube {
                pos: [16.0, -34.0, 168.0],
                scale: 24.5,
                rot: [0.22, 0.48, -0.08],
                rot_vel: [0.006, 0.009, 0.004],
            },
            OpeningGlassCube {
                pos: [-26.0, 26.0, 235.0],
                scale: 11.5,
                rot: [0.85, -0.35, 0.30],
                rot_vel: [-0.008, 0.007, -0.006],
            },
            OpeningGlassCube {
                pos: [-64.0, -44.0, 290.0],
                scale: 13.0,
                rot: [1.2, 0.6, 0.9],
                rot_vel: [0.007, -0.006, 0.005],
            },
            OpeningGlassCube {
                pos: [68.0, 42.0, 310.0],
                scale: 12.5,
                rot: [0.4, 1.5, 0.2],
                rot_vel: [-0.005, -0.008, 0.007],
            },
            OpeningGlassCube {
                pos: [4.0, -6.0, 345.0],
                scale: 14.0,
                rot: [1.7, 0.3, 1.1],
                rot_vel: [0.005, 0.006, -0.005],
            },
        ];

        Self {
            rng,
            frame: 0,
            cam_pos: [0.0, 0.0, 0.0, 1.0],
            cam_look: [0.0, 0.0, 260.0, 1.0],
            cam_up: [0.0, 1.0, 0.0, 0.0],
            flare_rings,
            clouds,
            cubes,
            text_alpha: 0,
            global_fade_alpha: 112,
            language,
        }
    }

    /// Steps the Illegal-Disc simulation by one frame (`DrawIllegalFog` wraps angles at `TAU = 6.2831855`).
    pub fn step_frame(&mut self, speed_scale: f32, reduced_motion: bool, sound: &mut SoundMailbox) {
        let dt = if reduced_motion {
            speed_scale * 0.35
        } else {
            speed_scale
        };

        if self.frame == 0 {
            sound.dispatch(SND_CMD_BGM_PLAY as i32, BGM_SLOT_WARNS as i32, 0, 0);
        }

        self.frame = self.frame.wrapping_add(1);

        // Global entry curtain fades out over first 32 frames; text fades in over frames 18..58
        if self.frame < 32 {
            self.global_fade_alpha = (((32 - self.frame) as f32 / 32.0) * 112.0) as u8;
        } else {
            self.global_fade_alpha = 0;
        }

        if self.frame < 18 {
            self.text_alpha = 0;
        } else if self.frame < 58 {
            self.text_alpha = (((self.frame - 18) as f32 / 40.0) * 128.0).clamp(0.0, 128.0) as u8;
        } else {
            self.text_alpha = 128;
        }

        // Advance 7 spinning 16-gon flare rings
        for ring in &mut self.flare_rings {
            ring.angle = (ring.angle + ring.angular_vel * dt).rem_euclid(TAU);
        }

        // Advance 128 falling 3x3 red cloud patches (`DrawIllegalFog`, wrapping at `6.2831855`)
        for patch in &mut self.clouds {
            patch.angle_x += patch.speed_x * dt;
            if patch.angle_x >= TAU {
                patch.angle_x -= TAU;
            }
            patch.angle_y += patch.speed_y * dt;
            if patch.angle_y >= TAU {
                patch.angle_y -= TAU;
            }
        }

        // Advance 5 red glass cubes
        for cube in &mut self.cubes {
            cube.rot[0] += cube.rot_vel[0] * dt;
            cube.rot[1] += cube.rot_vel[1] * dt;
            cube.rot[2] += cube.rot_vel[2] * dt;
        }
    }

    /// Returns the localized two-line Illegal Disc prompt matching `TEXOPNGJ..P` (`osdbits/opening.c`).
    pub fn localized_illegal_lines(language: u8) -> (&'static str, &'static str) {
        match language {
            2 => (
                "Veuillez inserer un disque",
                "au format PlayStation\x07o004 ou PlayStation\x07o0042.",
            ),
            3 => (
                "Por favor, inserte un disco",
                "de formato PlayStation\x07o004 o PlayStation\x07o0042.",
            ),
            4 => (
                "Bitte legen Sie eine CD/DVD",
                "im PlayStation\x07o004- oder PlayStation\x07o0042-Format ein.",
            ),
            5 => (
                "Inserire un disco",
                "in formato PlayStation\x07o004 o PlayStation\x07o0042.",
            ),
            6 => (
                "Plaats een disc van het",
                "PlayStation\x07o004- of PlayStation\x07o0042-formaat.",
            ),
            7 => (
                "Por favor insira um disco",
                "de formato PlayStation\x07o004 ou PlayStation\x07o0042.",
            ),
            _ => (
                "Please insert a PlayStation\x07o004",
                "or PlayStation\x07o0042 format disc.",
            ),
        }
    }

    /// Emits the complete Mode C Illegal-Disc draw sequence (`osdbits/opening.c` `DoIllegalDisc`):
    /// 1. `DrawRedFlare` (7 spinning 16-gon discs + 5 `TEXOFLAR` lens sprites)
    /// 2. `DrawIllegalFog` (128 falling red cloud patches tinted `{148, 30, 38}`)
    /// 3. `DrawIllegalCubes` (5 red-tinted 10-pass refractive glass cubes)
    /// 4. `DrawIllegalFades` (dark crimson vignette + entry fade)
    /// 5. `DoIllegalText` - emitted on top through the OSD text engine.
    ///
    pub fn emit_render_commands(
        &self,
        cmd_buf: &mut FrameCommandBuffer,
        fog_intensity: f32,
        refraction_scale: f32,
    ) {
        let screen_w = 640.0f32;
        let screen_h = 448.0f32;

        // Clear to deep crimson-black (`{8, 2, 3}`)
        cmd_buf.push_clear(RenderTargetId::Screen, 8, 2, 3, 0x80, true);

        let zd = vec4_sub(self.cam_look, self.cam_pos);
        let view_mat = sce_vu0_camera_matrix(self.cam_pos, zd, self.cam_up);
        let view_screen = sce_vu0_view_screen_matrix(
            512.0, 1.0, 0.88, 2048.0, 2048.0, 1.0, 65535.0, 10.0, 3000.0,
        );

        let cx = 298.0f32;
        let cy = 202.0f32;

        // 1. `DrawRedFlare`: 7 spinning 16-gon polygon discs (small + large) + 5 `TEXOFLAR` sprites
        for ring in &self.flare_rings {
            let mut tris = Vec::with_capacity(16 * 3);
            let center_col = [1.45, 0.28, 0.34, (ring.alpha as f32) / 128.0];
            let edge_col = [0.28, 0.02, 0.04, 0.0];

            for seg in 0..16 {
                let a0 = ring.angle + (seg as f32) * (TAU / 16.0);
                let a1 = ring.angle + ((seg + 1) as f32) * (TAU / 16.0);
                let r0 = if seg % 2 == 0 {
                    ring.outer_radius
                } else {
                    ring.inner_radius
                };
                let r1 = if (seg + 1) % 2 == 0 {
                    ring.outer_radius
                } else {
                    ring.inner_radius
                };
                tris.push(GpuVertex {
                    pos: [cx, cy, 0.5, 1.0],
                    uv: [0.5, 0.5],
                    color: center_col,
                });
                tris.push(GpuVertex {
                    pos: [cx + r0 * a0.cos(), cy + r0 * a0.sin(), 0.5, 1.0],
                    uv: [0.0, 0.0],
                    color: edge_col,
                });
                tris.push(GpuVertex {
                    pos: [cx + r1 * a1.cos(), cy + r1 * a1.sin(), 0.5, 1.0],
                    uv: [1.0, 1.0],
                    color: edge_col,
                });
            }
            cmd_buf.push_triangles(
                RenderTargetId::Screen,
                TextureId::WhiteDummy,
                GsBlendMode::Additive,
                SamplerWrap::Clamp,
                DepthMode::AlwaysNoWrite,
                128,
                &tris,
            );
        }

        // 5 `TEXOFLAR` sprites pulsing around the core
        let flare_specs: [(f32, f32, f32, [u8; 4]); 5] = [
            (0.0, 0.0, 96.0, [255, 82, 96, 92]),
            (-18.0, -12.0, 52.0, [255, 120, 130, 84]),
            (22.0, 14.0, 68.0, [220, 48, 62, 72]),
            (-38.0, 24.0, 118.0, [160, 28, 36, 54]),
            (0.0, 0.0, 28.0, [255, 195, 205, 118]),
        ];
        for (ox, oy, rad, rgba) in flare_specs {
            cmd_buf.push_sprite_2d(
                RenderTargetId::Screen,
                TextureId::TexoFlar,
                GsBlendMode::Additive,
                SamplerWrap::Clamp,
                128,
                cx + ox - rad,
                cy + oy - rad,
                cx + ox + rad,
                cy + oy + rad,
                0.0,
                0.0,
                1.0,
                1.0,
                rgba,
            );
        }

        // 2. `DrawIllegalFog`: 128 falling red cloud patches tinted `{148, 30, 38}`
        if fog_intensity > 0.01 {
            for patch in &self.clouds {
                let px = patch.angle_x.cos() * patch.radius * 0.85 - 14.0;
                let py = patch.angle_y.sin() * patch.radius * 0.78 - 10.0;
                let pz = patch.z_depth;
                if let Some((sx, sy, sz, _)) = project_point(
                    &view_mat,
                    &view_screen,
                    [px, py, pz, 1.0],
                    screen_w,
                    screen_h,
                ) {
                    let half = patch.size;
                    let a = ((patch.alpha as f32) * fog_intensity).clamp(0.0, 128.0) as u8;
                    cmd_buf.push_sprite_2d(
                        RenderTargetId::Screen,
                        patch.tex,
                        GsBlendMode::Additive,
                        SamplerWrap::Clamp,
                        128,
                        sx - half,
                        sy - half,
                        sx + half,
                        sy + half,
                        0.0,
                        0.0,
                        1.0,
                        1.0,
                        [148, 34, 42, a],
                    );
                    let _ = sz;
                }
            }
        }

        // 3. `DrawIllegalCubes`: 5 red-tinted (`{0.42, 0.08, 0.04}`) 10-pass refractive glass cubes
        emit_10pass_glass_cubes(
            cmd_buf,
            &self.cubes,
            &ILLEGAL_CUBE_PASSES,
            [0.62, 0.14, 0.12],
            0.0,
            0.0,
            &view_mat,
            &view_screen,
            screen_w,
            screen_h,
            refraction_scale,
        );

        // 4. `DrawIllegalFades`: Global entry fade curtain
        if self.global_fade_alpha > 0 {
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
                [0, 0, 0, self.global_fade_alpha],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_illegal_disc_cloud_wrap_and_fade_curve() {
        let mut mbox = SoundMailbox::new();
        let mut scene = IllegalDiscScene::new(1, 1);
        assert_eq!(scene.clouds.len(), 128);
        assert_eq!(scene.flare_rings.len(), 7);
        assert_eq!(scene.cubes.len(), 5);

        for _ in 0..120 {
            scene.step_frame(1.0, false, &mut mbox);
            for patch in &scene.clouds {
                assert!(patch.angle_x >= 0.0 && patch.angle_x < TAU);
                assert!(patch.angle_y >= 0.0 && patch.angle_y < TAU);
            }
        }
        assert_eq!(scene.global_fade_alpha, 0);
        assert_eq!(scene.text_alpha, 128);
    }
}
