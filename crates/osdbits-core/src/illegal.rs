//! The illegal-disc scene: red cloud particles, seven spinning flare discs,
//! the lens-sprite stack and the screen fades.
//!
//! Ported from `sub_21a438`, `sub_215798`, `flare_21A6D8`, `flare_21AA50`,
//! `flare_21AF18`, `FlareThing`, `DrawFlareSprite`, `DrawIllegalFog`,
//! `DrawRedFlare` and `DrawIllegalFades`.

use crate::math::{clamp, PI, TAU};
use crate::rng::OsdRand;

pub const PARTICLES: usize = 128;
pub const FLARE_DISCS: usize = 7;
pub const FAN_VERTS: usize = 17;

/// `flareColorSmall` (`0x328ce0`): centre rgb + pad, rim rgb + 128.
pub const FLARE_COLOR_SMALL: [i32; 8] = [128, 16, 40, 2056, 0, 0, 0, 128];
/// `flareColorLarge` (`0x328d00`).
pub const FLARE_COLOR_LARGE: [i32; 8] = [48, 8, 12, 128, 0, 0, 0, 128];

/// `0x27a050`: the four tri-strips over the 3x3 patch.
pub const FOG_STRIPS: [[usize; 4]; 4] = [[0, 1, 3, 4], [1, 2, 4, 5], [3, 4, 6, 7], [4, 5, 7, 8]];
/// `0x27a090`.
pub const FOG_UV: [[f32; 2]; 9] = [
    [0.0, 0.0],
    [0.5, 0.0],
    [1.0, 0.0],
    [0.0, 0.5],
    [0.5, 0.5],
    [1.0, 0.5],
    [0.0, 1.0],
    [0.5, 1.0],
    [1.0, 1.0],
];

#[derive(Clone, Debug)]
pub struct IllegalScene {
    /// Shared 3x3 cloud patch vertices (`0x3227d0`).
    pub patch_verts: [[f32; 4]; 9],
    /// Its colours (`0x322860`), RGBA 0..255.
    pub patch_colors: [[i32; 4]; 9],
    /// Particle positions (`0x3228f0`).
    pub pos: [[f32; 4]; PARTICLES],
    /// Particle rotations (`0x3230f0`).
    pub rot: [[f32; 4]; PARTICLES],
    /// Particle state flags (`0x3238f0`).
    pub state: [i32; PARTICLES],
    /// Small flare rings, `[disc][vertex]` incl. the centre at index 0.
    pub ring_small: [[[f32; 4]; FAN_VERTS]; FLARE_DISCS],
    pub ring_large: [[[f32; 4]; FAN_VERTS]; FLARE_DISCS],
    pub flare_rot: [[f32; 4]; FLARE_DISCS],
    pub flare_pos: [[f32; 4]; FLARE_DISCS],
    /// `redFlareIntensity` (`0x2a7f98`).
    pub intensity: f32,
    /// `illegalSceneWarm` (`0x2a77e4`).
    pub warm: i32,
    /// `illegalFadeCounter` (`0x2a77e8`).
    pub fade_counter: i32,
    /// Black overlay alpha 0..128 emitted this frame.
    pub fade_alpha: f32,
}

impl Default for IllegalScene {
    fn default() -> Self {
        Self::new()
    }
}

impl IllegalScene {
    pub fn new() -> Self {
        IllegalScene {
            patch_verts: [[0.0; 4]; 9],
            patch_colors: [[0; 4]; 9],
            pos: [[0.0; 4]; PARTICLES],
            rot: [[0.0; 4]; PARTICLES],
            state: [0; PARTICLES],
            ring_small: [[[0.0; 4]; FAN_VERTS]; FLARE_DISCS],
            ring_large: [[[0.0; 4]; FAN_VERTS]; FLARE_DISCS],
            flare_rot: [[0.0; 4]; FLARE_DISCS],
            flare_pos: [[0.0; 4]; FLARE_DISCS],
            intensity: 0.0,
            warm: 0,
            fade_counter: 0,
            fade_alpha: 0.0,
        }
    }

    /// `sub_21a438` — build the seven flare rings (16-gons of radius 8 and
    /// 28+i*8, centres at height 1160, spin phase i*0.925*2pi/7).
    pub fn init_flare_rings(&mut self) {
        for i in 0..FLARE_DISCS {
            self.ring_small[i][0] = [0.0, 0.0, 0.0, 1.0];
            self.ring_large[i][0] = [0.0, 0.0, 0.0, 1.0];
            let r = i as f32 * 8.0 + 28.0;
            for k in 0..16 {
                let a = k as f32 * PI * 2.0 * 0.0625;
                self.ring_small[i][1 + k] = [a.cos() * 8.0, a.sin() * 8.0, 0.0, 1.0];
                self.ring_large[i][1 + k] = [a.cos() * r, a.sin() * r, 0.0, 1.0];
            }
            self.flare_pos[i] = [0.0, 0.0, 1160.0, 0.0];
            self.flare_rot[i] = [0.0, 0.0, i as f32 * 0.925 * PI * 2.0 / 7.0, 0.0];
        }
    }

    /// `sub_215798` — the shared patch and the 128 particles. Consumes
    /// `1 + 3*128` draws of the `osdRand` stream (documented upstream as the
    /// real stream advance, which is what places `lightsSeed` at draw 386).
    pub fn init_fog(&mut self, rng: &mut OsdRand) {
        for j in 0..3usize {
            for i in 0..3usize {
                self.patch_verts[j * 3 + i] =
                    [(i as f32 - 1.0) * 15.0, (j as f32 - 1.0) * 15.0, 0.0, 1.0];
            }
        }
        for j in 0..3 {
            for i in 0..3 {
                let c = if i == 1 && j == 1 {
                    rng.below(64) + 64
                } else {
                    0
                };
                let idx = j * 3 + i;
                self.patch_colors[idx] = [c, c * 96 / 128, c * 96 / 128, 128];
            }
        }
        for i in 0..PARTICLES {
            self.pos[i][0] = (rng.below(4800) - 2400) as f32 * 0.01;
            self.pos[i][1] = (rng.below(4800) - 2400) as f32 * 0.01;
            self.pos[i][3] = 0.0;
            self.rot[i] = [0.0, 0.0, 0.0, 1.0];
            self.state[i] = 0;
            self.pos[i][2] = (rng.below(805) + 477) as f32;
        }
    }

    /// `sub_219f08` tail: reset the warm-up and end latch state owned here.
    pub fn reset_scene_flags(&mut self) {
        self.warm = 0;
        self.fade_counter = 0;
        self.fade_alpha = 0.0;
    }

    /// `flare_21A6D8` — spin and orbit the discs.
    pub fn update_flare(&mut self, frame: u32) {
        let phase = (frame % 201) as f32 * 0.03125 - PI;
        for i in 0..FLARE_DISCS {
            self.flare_rot[i][0] += (i as f32 + 1.0) * 0.2;
            self.flare_rot[i][2] += (i as f32 + 1.0) * 0.35;
            self.flare_rot[i][1] += (i as f32 + 1.0) * 0.27;
            for k in 0..3 {
                while self.flare_rot[i][k] > PI {
                    self.flare_rot[i][k] -= TAU;
                }
                while self.flare_rot[i][k] < -PI {
                    self.flare_rot[i][k] += TAU;
                }
            }
            let r = ((7 - i) * (7 - i)) as f32 * PI * 2.0 / 64.0;
            let mut f = r + phase;
            while f > PI {
                f -= TAU;
            }
            while f < -PI {
                f += TAU;
            }
            self.flare_pos[i][0] = f.cos() * r * 0.5;
            self.flare_pos[i][1] = f.sin() * r * 0.5;
        }
    }

    /// `DrawRedFlare` head: intensity from the camera height.
    pub fn update_intensity(&mut self, position_z: f32) {
        self.intensity = (740.0 - (1160.0 - position_z)) * 128.0 / 740.0 * 0.6;
    }

    /// `DrawIllegalFog` — advance every particle and return its draw alpha.
    ///
    /// `alphas[i]` is `None` when the particle is not drawn this frame (the
    /// original's two `continue`s), otherwise the alpha in 0..128 scaled.
    pub fn update_particles(&mut self, frame: u32, position_z: f32) -> [Option<f32>; PARTICLES] {
        let global_alpha = if position_z < 672.0 {
            let g = (550.0 - (672.0 - position_z)) * 128.0 / 550.0 * 4.0;
            g.min(128.0)
        } else {
            128.0
        };

        let mut out = [None; PARTICLES];
        for i in 0..PARTICLES {
            let mut rj = (i as f32 + 1.0) * 0.0001;
            if frame & 1 == 0 {
                rj = -rj;
            }
            self.rot[i][2] += rj;
            while self.rot[i][2] > PI {
                self.rot[i][2] -= TAU;
            }
            while self.rot[i][2] < -PI {
                self.rot[i][2] += TAU;
            }

            let fall = ((i as f32 + 1.0) * 0.02 + 1.2) as i32 as f32;
            self.pos[i][2] -= fall;
            let mut d = (position_z + 805.0) - self.pos[i][2];
            let af;
            if d < 192.0 {
                if d < 0.0 {
                    continue;
                }
                af = d * 64.0 / 192.0;
            } else {
                d = self.pos[i][2] - position_z - 32.0;
                if d < 192.0 {
                    if d < 0.0 {
                        self.pos[i][2] = position_z + 805.0;
                        continue;
                    }
                    af = d * 64.0 / 192.0;
                } else {
                    af = 64.0;
                }
            }
            let alpha = af * global_alpha * 0.007_812_5;
            out[i] = Some(clamp(alpha, 0.0, 128.0) / 128.0);
        }
        out
    }

    /// `DrawIllegalFades` (`0x21b398`): the black overlay's alpha for this
    /// frame, 0..128 (0 = no overlay).
    pub fn update_fades(&mut self, position_z: f32, opening_end_flag: i32) -> f32 {
        let z = position_z;
        let mut alpha = 0.0f32;
        if z < 800.0 {
            alpha = 128.0 - (z - 672.0);
            self.fade_counter = 0;
        } else if z > 1128.0 {
            let f = (32.0 - (1160.0 - z)) * 4.0;
            alpha = clamp(f, 0.0, 128.0);
            self.fade_counter = 0;
        }
        if opening_end_flag != 0 {
            self.fade_counter += 1;
            if self.fade_counter > 128 {
                self.fade_counter = 128;
            }
            alpha = self.fade_counter as f32;
        }
        self.fade_alpha = clamp(alpha, 0.0, 128.0);
        self.fade_alpha
    }

    /// Fan colours for a disc (centre and rim), scaled by the flare
    /// intensity / 64 and clamped, per `DrawFlareFans`.
    pub fn fan_colors(&self, small: bool) -> ([f32; 3], [f32; 3]) {
        let blk = if small {
            FLARE_COLOR_SMALL
        } else {
            FLARE_COLOR_LARGE
        };
        let mut center = [0.0f32; 3];
        let mut rim = [0.0f32; 3];
        for c in 0..3 {
            center[c] = (blk[c] as f32 * self.intensity * 0.015_625).clamp(0.0, 255.0) / 255.0;
            rim[c] = (blk[4 + c] as f32 * self.intensity * 0.015_625).clamp(0.0, 255.0) / 255.0;
        }
        (center, rim)
    }

    /// `FlareThing`'s lens-sprite stack: `(size, alpha, rgba, scale)`.
    /// Sizes are in virtual-screen pixels, alpha 0..255.
    pub fn lens_sprites(&self, n: i32) -> [Option<(f32, f32, [f32; 3])>; 5] {
        let n = n.min(8);
        let m = n + 2;
        let mut out = [None; 5];
        for (i, size) in [112.0, 170.0, 256.0, 448.0].iter().enumerate() {
            out[i] = Some((
                *size,
                (m as f32).clamp(0.0, 255.0) / 255.0,
                [128.0 / 255.0, 64.0 / 255.0, 64.0 / 255.0],
            ));
        }
        let mut a = n as f32 * 3.0 + self.intensity * 0.5;
        a = a.clamp(0.0, 255.0);
        let size = self.intensity * 0.125 + 420.0;
        out[4] = Some((
            size,
            a / 255.0,
            [128.0 / 255.0, 112.0 / 255.0, 96.0 / 255.0],
        ));
        out
    }

    /// The `FlareThing` n argument (`intensity * 0.0625` as an int).
    pub fn flare_thing_n(&self) -> i32 {
        (self.intensity * 0.0625) as i32
    }
}

/// Alpha of one flare-tree draw: `intensity*0.015625` clamped, mirroring
/// `DrawFlareFans`.
pub fn fan_alpha_colors(intensity: f32, small: bool) -> ([f32; 3], [f32; 3]) {
    let blk = if small {
        FLARE_COLOR_SMALL
    } else {
        FLARE_COLOR_LARGE
    };
    let mut center = [0.0f32; 3];
    let mut rim = [0.0f32; 3];
    for c in 0..3 {
        center[c] = (blk[c] as f32 * intensity * 0.015_625).clamp(0.0, 255.0) / 255.0;
        rim[c] = (blk[4 + c] as f32 * intensity * 0.015_625).clamp(0.0, 255.0) / 255.0;
    }
    (center, rim)
}

/// RNG draw count consumed by `init_fog`, pinned so the `lightsSeed` draw
/// number stays at 386.
pub const FOG_INIT_RNG_DRAWS: usize = 1 + 3 * PARTICLES;

#[cfg(test)]
mod tests {
    use super::*;

    fn scene() -> (IllegalScene, OsdRand) {
        let mut s = IllegalScene::new();
        let mut rng = OsdRand::new();
        s.init_fog(&mut rng);
        s.init_flare_rings();
        (s, rng)
    }

    #[test]
    fn fog_init_consumes_the_documented_stream() {
        let mut rng = OsdRand::new();
        let mut s = IllegalScene::new();
        s.init_fog(&mut rng);
        // 1 + 3*128 draws: the 386th draw is lightsSeed in the original.
        assert_eq!(FOG_INIT_RNG_DRAWS, 385);
        // The centre patch colour is in 64..127 and the others are black.
        let c = s.patch_colors[4];
        assert!(c[0] >= 64 && c[0] < 128, "{c:?}");
        assert_eq!(s.patch_colors[0][0], 0);
        assert_eq!(c[1], c[0] * 96 / 128);
        assert_eq!(c[3], 128);
        // Positions stay in the documented bands.
        for i in 0..PARTICLES {
            assert!(s.pos[i][0].abs() <= 24.0);
            assert!((477.0..=1282.0).contains(&s.pos[i][2]));
        }
    }

    #[test]
    fn patch_geometry_is_a_30_unit_grid() {
        let (s, _) = scene();
        assert_eq!(s.patch_verts[0], [-15.0, -15.0, 0.0, 1.0]);
        assert_eq!(s.patch_verts[4], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(s.patch_verts[8], [15.0, 15.0, 0.0, 1.0]);
        assert_eq!(FOG_UV[8], [1.0, 1.0]);
    }

    #[test]
    fn flare_rings_have_the_documented_radii() {
        let (s, _) = scene();
        for i in 0..FLARE_DISCS {
            let r = i as f32 * 8.0 + 28.0;
            let p = s.ring_large[i][1];
            assert!(((p[0] * p[0] + p[1] * p[1]).sqrt() - r).abs() < 1e-4);
            let p = s.ring_small[i][5];
            assert!(((p[0] * p[0] + p[1] * p[1]).sqrt() - 8.0).abs() < 1e-4);
            assert_eq!(s.flare_pos[i][2], 1160.0);
        }
    }

    #[test]
    fn flare_rotation_wraps_and_orbits() {
        let (mut s, _) = scene();
        for f in 0..2000 {
            s.update_flare(f);
            for i in 0..FLARE_DISCS {
                for k in 0..3 {
                    assert!((-PI..=PI).contains(&s.flare_rot[i][k]));
                }
                let r = ((7 - i) * (7 - i)) as f32 * PI * 2.0 / 64.0;
                let d = (s.flare_pos[i][0].powi(2) + s.flare_pos[i][1].powi(2)).sqrt();
                assert!((d - r * 0.5).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn particles_fall_and_respawn_above_the_camera() {
        let (mut s, _) = scene();
        let z0: Vec<f32> = (0..PARTICLES).map(|i| s.pos[i][2]).collect();
        let alphas = s.update_particles(1, 672.0);
        assert!(
            alphas.iter().any(|a| a.is_some()),
            "no particle drawn at the scene start"
        );
        // Each particle either falls by trunc((i+1)*0.02 + 1.2) units or
        // respawns 805 above the camera.
        for i in 0..PARTICLES {
            let fall = ((i as f32 + 1.0) * 0.02 + 1.2) as i32 as f32;
            let moved = z0[i] - s.pos[i][2];
            let respawned = (s.pos[i][2] - (672.0 + 805.0)).abs() < 1e-3;
            assert!(
                (moved - fall).abs() < 1e-6 || respawned,
                "particle {i}: moved {moved}, fall {fall}"
            );
        }
        // Long run: every particle eventually respawns at camera+805.
        let mut max_z = f32::MIN;
        for f in 0..4000 {
            s.update_particles(f, 900.0);
            for i in 0..PARTICLES {
                assert!(s.pos[i][2].is_finite());
                max_z = max_z.max(s.pos[i][2]);
            }
        }
        assert!(max_z <= 900.0 + 805.0 + 1.0);
        // Particles never end up below the camera minus 32 by much.
        for i in 0..PARTICLES {
            assert!(
                s.pos[i][2] > 900.0 - 40.0,
                "particle {i} at {}",
                s.pos[i][2]
            );
        }
    }

    #[test]
    fn particle_alpha_never_exceeds_the_cap() {
        let (mut s, _) = scene();
        for f in 0..1000 {
            for a in s.update_particles(f, 700.0).iter().flatten() {
                assert!((0.0..=1.0).contains(a), "{a}");
            }
        }
    }

    #[test]
    fn fades_follow_the_height_ramps() {
        let mut s = IllegalScene::new();
        assert!((s.update_fades(672.0, 0) - 128.0).abs() < 1e-3);
        assert!((s.update_fades(736.0, 0) - 64.0).abs() < 1e-3);
        assert_eq!(s.update_fades(800.0, 0), 0.0);
        assert_eq!(s.update_fades(1000.0, 0), 0.0);
        // Fade to black near the top: z = 1160 -> f = 32*4 = 128.
        assert!((s.update_fades(1160.0, 0) - 128.0).abs() < 1e-3);
        assert!((s.update_fades(1152.0, 0) - 96.0).abs() < 1e-3);
        // End latch: +1 per frame up to 128.
        for _ in 0..200 {
            s.update_fades(1000.0, 1);
        }
        assert_eq!(s.fade_alpha, 128.0);
    }

    #[test]
    fn intensity_and_lens_sprites() {
        let mut s = IllegalScene::new();
        s.update_intensity(1160.0);
        // (740 - 0)*128/740*0.6 = 76.8
        assert!((s.intensity - 76.8).abs() < 0.1);
        let sprites = s.lens_sprites(s.flare_thing_n());
        assert_eq!(sprites.len(), 5);
        let (size, alpha, _) = sprites[4].unwrap();
        assert!((size - (76.8 * 0.125 + 420.0)).abs() < 0.5);
        assert!(alpha > 0.0 && alpha <= 1.0);
        let (c, r) = s.fan_colors(true);
        assert!(c[0] > c[1] && c[2] > c[1]);
        assert_eq!(r, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn intensity_is_zero_at_the_bottom() {
        let mut s = IllegalScene::new();
        s.update_intensity(420.0);
        assert!((s.intensity - 0.0).abs() < 1e-3);
        // Negative below the band (the original does not clamp): documented.
        s.update_intensity(300.0);
        assert!(s.intensity < 0.0);
        // Draw-side clamps keep colours valid.
        let (c, r) = fan_alpha_colors(s.intensity, true);
        assert!(c.iter().all(|v| (0.0..=1.0).contains(v)));
        assert!(r.iter().all(|v| (0.0..=1.0).contains(v)));
    }
}
