//! Mode D — Module U Main Menu 3D scene (`osdbits/menu.c` + `osdbits/menuback.c`):
//! software RTC, 7 orbiting orbs with 50-sample trails, `TEXCKABE` 16×33 breathing tunnel,
//! deep-blue screen-tint composite, 5-pass bilinear `ZoomBlur`, and 16-segment ellipse vignette.

use crate::assets::TextureId;
use crate::input_audio::{
    SoundMailbox, BGM_SLOT_CLOKS, BGM_SLOT_RCLKS, SND_CMD_BGM_PLAY, SND_CMD_BGM_STOP,
};
use crate::math::{md_cos, md_sin, vec4_dot3, vec4_normalize3, OsdRng, Vec4};
use crate::render_cmd::{
    DepthMode, FrameCommandBuffer, GpuVertex, GsBlendMode, RenderTargetId, SamplerWrap,
};

pub const NUM_ORBS: usize = 7;
pub const TRAILLEN: usize = 50;
pub const TRAILDRAW: usize = 49;
pub const NRING: usize = 12;

pub const ORB_BASE_COLOR: [u8; 4] = [0x30, 0x62, 0x80, 0x3C];
pub const ORB_CORE_COLOR: [u8; 4] = [0x80, 0x80, 0x80, 0x80];
pub const ORB_CORE_COLOR_WORK: [u8; 4] = [0xFF, 0xFF, 0xFF, 0x80];

pub const RING_BODY_COLOR: [i32; 4] = [0x2D, 0x55, 0x66, 0x80];
pub const RING_EDGE_COLOR: [i32; 4] = [0x3C, 0x3C, 0x3C, 0x80];
pub const RING_COL_A_INIT: [i32; 4] = [0x3C, 0x3C, 0x3C, 0x80];
pub const RING_COL_B_INIT: [i32; 4] = [0x80, 0x80, 0x80, 0x1E];
pub const RING_COL_FIXED: [i32; 4] = [167, 217, 255, 0];

/// Composite tint `{0x37, 0x28, 0x3C, 0x80}` (`menuback.c` line 866): x0.43 red, x0.31 green, x0.47 blue.
pub const COMPOSITE_TINT: [u8; 4] = [0x37, 0x28, 0x3C, 0x80];

/// Software clock (`menu.c` `ClockSeconds` / `ClockMinutes` / `ClockHours`, `0x352980` block).
#[derive(Debug, Clone, Copy)]
pub struct SoftClock {
    pub hours: u32,
    pub minutes: u32,
    pub seconds: u32,
    pub ms: f32,
    pub hold: bool,
}

impl Default for SoftClock {
    fn default() -> Self {
        Self {
            hours: 12,
            minutes: 34,
            seconds: 56,
            ms: 0.0,
            hold: false,
        }
    }
}

impl SoftClock {
    pub fn seconds_f(&self) -> f32 {
        self.seconds as f32 + self.ms * 0.001
    }

    pub fn minutes_f(&self) -> f32 {
        self.minutes as f32 + self.seconds_f() / 60.0
    }

    pub fn hours_f(&self) -> f32 {
        self.hours as f32 + self.minutes_f() / 60.0
    }

    pub fn tick(&mut self, dt: f32) {
        if self.hold {
            return;
        }
        self.ms += (1000.0 / 60.0) * dt;
        while self.ms >= 1000.0 {
            self.ms -= 1000.0;
            self.seconds += 1;
            if self.seconds >= 60 {
                self.seconds = 0;
                self.minutes += 1;
                if self.minutes >= 60 {
                    self.minutes = 0;
                    self.hours = (self.hours + 1) % 24;
                }
            }
        }
    }

    pub fn set(&mut self, h: u32, m: u32, s: u32) {
        self.hours = h % 24;
        self.minutes = m % 60;
        self.seconds = s % 60;
        self.ms = 0.0;
    }
}

/// One of the 7 orbiting menu orbs (`menu.c` `Orb`, 50-sample trail ring).
#[derive(Debug, Clone)]
pub struct MenuOrb {
    pub trail: Vec<[f32; 3]>, // screen-space (x, y, z)
    pub colors: Vec<[u8; 4]>, // per-sample trail colour
    pub phase: f32,
    pub radius: f32,
    pub angle: f32,
    pub speed: f32,
    pub hue: [u8; 3],
}

/// A single 16×33 ribbon of the `TEXCKABE` backdrop tunnel (`menuback.c` `DrawKabeRibbon`).
#[derive(Debug, Clone, Copy)]
pub struct KabeRingVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub u: f32,
    pub v: f32,
    pub shade: f32,
}

/// Complete Module U main menu scene (`menu.c` `MenuScene`, `menuback.c` `MenuBackdrop`).
#[derive(Debug, Clone)]
pub struct MenuScene {
    pub frame: u32,
    pub clock: SoftClock,
    pub orbs: [MenuOrb; NUM_ORBS],
    pub fade_mode: u8,  // 0 = settled, 2 = fade up from black, 3 = fade down
    pub fade_alpha: u8, // 0..128
    pub cam_z: f32,
    pub cam_x: f32,
    pub zoom_blur_passes: u8,
    pub vignette_alpha: u8,
    pub kabe_phase: f32,
    pub rng: OsdRng,
    pub orb_ease: f32,
    pub orb_scale_target: f32,
    pub ring_offset: i32,
    pub ring_split: f32,
    pub ring_col_a: [i32; 4],
    pub ring_col_b: [i32; 4],
    pub config_active: bool,
    pub config_alpha: u8,
}

impl MenuScene {
    pub fn new(seed: u32, clock_hour: u32, clock_minute: u32, clock_second: u32) -> Self {
        let mut rng = OsdRng::new(seed ^ 0x2261_B800);
        let hue_table: [[u8; 3]; 7] = [
            [0x30, 0x62, 0x80],
            [0x46, 0x8C, 0xB4],
            [0x28, 0x4E, 0x80],
            [0x60, 0x90, 0xC8],
            [0x3C, 0x78, 0x92],
            [0x54, 0x9E, 0xB8],
            [0x38, 0x6E, 0x8C],
        ];

        let orbs = std::array::from_fn(|i| {
            let p0 = [
                320.0 + (i as f32) * 18.0 - 64.0,
                210.0 + (i as f32) * 6.0 - 18.0,
                0.35,
            ];
            MenuOrb {
                trail: vec![p0; TRAILLEN],
                colors: vec![ORB_BASE_COLOR; TRAILLEN],
                phase: rng.rand_f32_01() * std::f32::consts::TAU,
                radius: 64.0 + (i as f32) * 8.5,
                angle: (i as f32) * (std::f32::consts::TAU / 7.0),
                speed: 0.006 + (i as f32) * 0.0009,
                hue: hue_table[i],
            }
        });

        let mut clock = SoftClock::default();
        clock.set(clock_hour, clock_minute, clock_second);

        Self {
            frame: 0,
            clock,
            orbs,
            fade_mode: 2,
            fade_alpha: 0,
            cam_z: -100.0,
            cam_x: 10.436,
            zoom_blur_passes: 5,
            vignette_alpha: 0,
            kabe_phase: 0.0,
            rng,
            orb_ease: 0.0,
            orb_scale_target: 1.0,
            ring_offset: 0,
            ring_split: 0.0,
            ring_col_a: RING_COL_A_INIT,
            ring_col_b: RING_COL_B_INIT,
            config_active: false,
            config_alpha: 0,
        }
    }

    /// Steps the Module U main menu by one frame (`menu.c` `MenuFrame`, stages 0..=10).
    pub fn step_frame(&mut self, speed_scale: f32, reduced_motion: bool, sound: &mut SoundMailbox) {
        let dt = if reduced_motion {
            speed_scale * 0.35
        } else {
            speed_scale
        };

        self.frame = self.frame.wrapping_add(1);

        // Global entry fade-up from black (`0x22ADD8(2)`, 128 frames)
        if self.fade_mode == 2 {
            self.fade_alpha = self.fade_alpha.saturating_add(1);
            if self.fade_alpha >= 128 {
                self.fade_alpha = 128;
                self.fade_mode = 0;
            }
        } else if self.fade_mode == 3 {
            self.fade_alpha = self.fade_alpha.saturating_sub(1);
        }

        self.cam_z *= 0.97;
        self.clock.tick(dt);

        // Orb orbit radius eases toward the minute-hand target (`0x2261B8`, `0x225628`)
        let orbit_radius = 46.0 + (self.clock.minutes_f() / 60.0) * 22.0;
        self.orb_ease += (self.orb_scale_target - self.orb_ease) * 0.01 * dt;
        self.ring_split = (1.0 - self.clock.minutes_f() / 60.0).clamp(0.0, 1.0);
        self.ring_offset = (self.clock.hours_f() as i32) % (NRING as i32);

        let screen_w = 640.0f32;
        let screen_h = 448.0f32;

        for (i, orb) in self.orbs.iter_mut().enumerate() {
            orb.angle += orb.speed * dt;
            let wobble = if reduced_motion {
                0.0
            } else {
                (self.frame as f32 * 0.021 + orb.phase).sin() * 6.5
            };
            let scale = orbit_radius * self.orb_ease.max(0.05);
            let sx = screen_w * 0.5 + (orb.angle * 0.85 + (i as f32) * 0.7).cos() * scale * 0.82;
            let sy = screen_h * 0.5 + orb.angle.sin() * scale * 0.72 + wobble;
            let sz = 0.25 + ((self.frame as f32 * 0.015 + orb.phase).sin() * 0.12 + 0.12);

            // Push new head onto 50-sample trail ring
            orb.trail.pop();
            orb.trail.insert(0, [sx, sy, sz]);
            orb.colors.pop();
            orb.colors.insert(
                0,
                [orb.hue[0], orb.hue[1], orb.hue[2], 0x3C + (i as u8) * 2],
            );

            // Orb entry scatter during fade-up (all 7 orbs fly in from random screen positions)
            if self.fade_mode == 2 {
                let t = (self.fade_alpha as f32) / 128.0;
                let scatter_x = (orb.phase).cos() * 180.0 * (1.0 - t);
                let scatter_y = (orb.phase).sin() * 110.0 * (1.0 - t);
                for p in &mut orb.trail {
                    p[0] += scatter_x;
                    p[1] += scatter_y;
                }
            }
        }

        self.kabe_phase = (self.kabe_phase + 0.005 * dt).rem_euclid(1.0);

        if self.config_active {
            sound.dispatch(SND_CMD_BGM_PLAY as i32, BGM_SLOT_CLOKS as i32, 0, 0);
            sound.dispatch(SND_CMD_BGM_PLAY as i32, BGM_SLOT_RCLKS as i32, 0, 0);
        }
    }

    pub fn start_config(&mut self, sound: &mut SoundMailbox) {
        if self.config_active {
            return;
        }
        self.config_active = true;
        self.config_alpha = 0;
        sound.dispatch(SND_CMD_BGM_STOP as i32, BGM_SLOT_RCLKS as i32, 0, 15);
        sound.dispatch(SND_CMD_BGM_PLAY as i32, BGM_SLOT_CLOKS as i32, 0, 0);
    }

    pub fn leave_config(&mut self, sound: &mut SoundMailbox) {
        if !self.config_active {
            return;
        }
        self.config_active = false;
        self.config_alpha = 0;
        sound.dispatch(SND_CMD_BGM_STOP as i32, BGM_SLOT_CLOKS as i32, 0, 15);
        sound.dispatch(SND_CMD_BGM_PLAY as i32, BGM_SLOT_RCLKS as i32, 0, 0);
    }

    /// Config screen entry/exit alpha ramp (`menuconfig.c` `MenuConfigAlpha`, `0x226A60`).
    pub fn advance_config_alpha(&mut self, dt: f32) {
        if self.config_active {
            self.config_alpha = (self.config_alpha as f32 + 8.0 * dt).min(128.0) as u8;
        } else {
            self.config_alpha = self.config_alpha.saturating_sub((8.0 * dt) as u8);
        }
    }

    /// Emits the Module U main menu frame (`menu.c` `MenuFrame`):
    /// 1. `MenuBackdrop` (`0x21D0A0`): `TEXCKABE` tunnel → 5-pass `ZoomBlur` → wb3/wb4 copies → tinted composite
    /// 2. `UpdateOrbs` / `SceneWalk` / `DrawOrb`: 7 orbs with 50-sample trails + `TEXCBLUR` halos + `TEXCNAVI` cores
    /// 3. `MenuVignette` (`0x22AF10`): 16-segment ellipse vignette around the orb cluster
    /// 4. `MenuZoomBlur` (`0x2283D0`): 5 bilinear shrink/stretch round trips
    /// 5. `DrawFadeCurtain` (`0x22AFB8`): global entry fade
    pub fn emit_render_commands(&self, cmd_buf: &mut FrameCommandBuffer, reduced_motion: bool) {
        let screen_w = 640.0f32;
        let screen_h = 448.0f32;

        cmd_buf.push_clear(RenderTargetId::Screen, 0, 0, 0, 0x80, true);

        // 1a. `DrawKabe` (`0x229358`) : 16 ribbons × 33 rings (`menuback.c`)
        if self.config_active {
            self.emit_kabe_tunnel(cmd_buf, self.config_alpha as f32 / 128.0);
        }

        // 1b. `ZoomBlur` (`0x22C3C0`) 5 passes between Screen ↔ WorkBuf4
        if !reduced_motion && self.zoom_blur_passes > 0 {
            self.emit_zoom_blur(cmd_buf, self.zoom_blur_passes);
        }

        // 1c. Pre-tint copies to wb3 (extraBuf1) and wb4 (extraBuf2)
        cmd_buf.push_copy_target_to_texture(RenderTargetId::Screen, TextureId::WorkBuf3);
        cmd_buf.push_copy_target_to_texture(RenderTargetId::Screen, TextureId::WorkBuf4);

        // 2. Orbs + trails
        self.emit_orbs(cmd_buf, screen_w, screen_h);

        // 3. `MenuVignette` (`0x22AF10`): 16-segment dark ellipse vignette
        if self.vignette_alpha > 0 {
            self.emit_vignette(cmd_buf, screen_w, screen_h);
        }

        // 4. `MenuZoomBlur` (`0x2283D0`, 5 passes): the signature soft PS2 radial blur
        if !reduced_motion {
            self.emit_zoom_blur(cmd_buf, 5);
        }

        // 5. Tinted deep-blue composite (`menuback.c` `compositeColor`) if tunnel active
        if self.config_active {
            let t = self.config_alpha as f32 / 128.0;
            cmd_buf.push_sprite_2d(
                RenderTargetId::Screen,
                TextureId::WorkBuf3,
                GsBlendMode::FixedBlend,
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
                [
                    (COMPOSITE_TINT[0] as f32 * t) as u8,
                    (COMPOSITE_TINT[1] as f32 * t) as u8,
                    (COMPOSITE_TINT[2] as f32 * t) as u8,
                    0x80,
                ],
            );
        }

        // 6. `DrawFadeCurtain` (`0x22AFB8`)
        if self.fade_alpha < 128 {
            let a = 128 - self.fade_alpha;
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
                [0, 0, 0, a],
            );
        }
    }

    fn emit_orbs(&self, cmd_buf: &mut FrameCommandBuffer, screen_w: f32, screen_h: f32) {
        let _ = (screen_w, screen_h);
        for orb in &self.orbs {
            // A. 50-sample ribbon trail (`DrawOrbTrail`, `0x22EFF0`)
            // Each segment is a thin camera-facing quad with quadratic-red, linear-blue colour decay.
            for k in 0..TRAILDRAW.min(orb.trail.len().saturating_sub(1)) {
                let p0 = orb.trail[k];
                let p1 = orb.trail[k + 1];
                let age0 = (k as f32) / (TRAILDRAW as f32);
                let age1 = ((k + 1) as f32) / (TRAILDRAW as f32);
                let a0 = ((1.0 - age0).powi(2) * 0.52 * 128.0) as u8;
                let a1 = ((1.0 - age1).powi(2) * 0.52 * 128.0) as u8;
                if a0 == 0 && a1 == 0 {
                    continue;
                }
                let dx = p1[0] - p0[0];
                let dy = p1[1] - p0[1];
                let len = (dx * dx + dy * dy).sqrt().max(0.001);
                let width = 1.6 + (1.0 - age0) * 2.6;
                let nx = (-dy / len) * width;
                let ny = (dx / len) * width;

                // Red channel decays as age^4, green as age^2, blue linearly (`TrailVertex`)
                let red0 = ((1.0 - age0).powi(4) * 255.0) as u8;
                let red1 = ((1.0 - age1).powi(4) * 255.0) as u8;
                let grn0 = ((1.0 - age0).powi(2) * 255.0) as u8;
                let grn1 = ((1.0 - age1).powi(2) * 255.0) as u8;
                let blu0 = ((1.0 - age0) * 255.0) as u8;
                let blu1 = ((1.0 - age1) * 255.0) as u8;

                cmd_buf.push_quad(
                    RenderTargetId::Screen,
                    TextureId::WhiteDummy,
                    GsBlendMode::Additive,
                    SamplerWrap::Clamp,
                    DepthMode::AlwaysNoWrite,
                    128,
                    GpuVertex {
                        pos: [p0[0] - nx, p0[1] - ny, p0[2], 1.0],
                        uv: [0.0, 0.0],
                        color: [
                            red0 as f32 / 128.0,
                            grn0 as f32 / 128.0,
                            blu0 as f32 / 128.0,
                            a0 as f32 / 128.0,
                        ],
                    },
                    GpuVertex {
                        pos: [p0[0] + nx, p0[1] + ny, p0[2], 1.0],
                        uv: [1.0, 0.0],
                        color: [
                            red0 as f32 / 128.0,
                            grn0 as f32 / 128.0,
                            blu0 as f32 / 128.0,
                            a0 as f32 / 128.0,
                        ],
                    },
                    GpuVertex {
                        pos: [p1[0] - nx, p1[1] - ny, p1[2], 1.0],
                        uv: [0.0, 1.0],
                        color: [
                            red1 as f32 / 128.0,
                            grn1 as f32 / 128.0,
                            blu1 as f32 / 128.0,
                            a1 as f32 / 128.0,
                        ],
                    },
                    GpuVertex {
                        pos: [p1[0] + nx, p1[1] + ny, p1[2], 1.0],
                        uv: [1.0, 1.0],
                        color: [
                            red1 as f32 / 128.0,
                            grn1 as f32 / 128.0,
                            blu1 as f32 / 128.0,
                            a1 as f32 / 128.0,
                        ],
                    },
                );
            }

            // B. `TEXCBLUR` halo (`DrawOrbSprite`, half = zscale * 30.0)
            if let Some(head) = orb.trail.first() {
                let half = (head[2] * 30.0 * 0.24).clamp(8.0, 30.0);
                cmd_buf.push_sprite_2d(
                    RenderTargetId::Screen,
                    TextureId::TexcBlur,
                    GsBlendMode::Additive,
                    SamplerWrap::Clamp,
                    128,
                    head[0] - half,
                    head[1] - half,
                    head[0] + half,
                    head[1] + half,
                    0.0,
                    0.0,
                    1.0,
                    1.0,
                    [orb.hue[0], orb.hue[1], orb.hue[2], 0x70],
                );

                // C. `TEXCNAVI` core (`half = zscale * 4.5`)
                let core = (half * 0.18).clamp(2.5, 6.5);
                cmd_buf.push_sprite_2d(
                    RenderTargetId::Screen,
                    TextureId::TexcNavi,
                    GsBlendMode::Additive,
                    SamplerWrap::Clamp,
                    128,
                    head[0] - core,
                    head[1] - core,
                    head[0] + core,
                    head[1] + core,
                    0.0,
                    0.0,
                    1.0,
                    1.0,
                    ORB_CORE_COLOR,
                );
            }
        }
    }

    fn emit_kabe_tunnel(&self, cmd_buf: &mut FrameCommandBuffer, intensity: f32) {
        let screen_w = 640.0f32;
        let screen_h = 448.0f32;
        let cx = screen_w * 0.5;
        let cy = screen_h * 0.5;
        let r_base = 260.0f32;

        for i in 0..16usize {
            let a0 = (i as f32) * (std::f32::consts::TAU / 16.0);
            let a1 = ((i + 1) as f32) * (std::f32::consts::TAU / 16.0);
            let mut quad_pts = Vec::with_capacity(33 * 2);

            for ring in 0..33usize {
                let depth = (ring as f32) / 32.0;
                let wave = 1.0 + ((self.frame as f32) * 0.11 + (ring as f32) * 5.12).sin() * 0.05;
                let radius = r_base * (1.0 + depth * 2.8) * wave;
                let alpha = (1.0 - depth).powi(2) * intensity;

                let x0 = cx + radius * a0.cos();
                let y0 = cy + radius * a0.sin();
                let x1 = cx + radius * a1.cos();
                let y1 = cy + radius * a1.sin();

                // `KabeVertex` colours: near end bright, far end black
                let p = ((32 - ring) as f32 / 32.0).powi(3);
                let cr = (p * 230.0 / 32.0).clamp(0.0, 2.0);
                let cg = (p * 260.0 / 32.0).clamp(0.0, 2.0);
                let cb = (p * 260.0 / 32.0).clamp(0.0, 2.0);
                let v = ring as f32 / 32.0 + self.kabe_phase;
                let u0 = a0 / std::f32::consts::TAU * 3.0;

                quad_pts.push(GpuVertex {
                    pos: [x0, y0, 0.9, 1.0],
                    uv: [u0 + self.kabe_phase, v],
                    color: [cr, cg, cb, alpha],
                });
                quad_pts.push(GpuVertex {
                    pos: [x1, y1, 0.9, 1.0],
                    uv: [u0 + 0.1875 + self.kabe_phase, v],
                    color: [cr, cg, cb, alpha],
                });
            }

            // Build triangle strip triangles between consecutive rings
            let mut tris = Vec::with_capacity(32 * 6);
            for ring in 0..32usize {
                let l0 = quad_pts[ring * 2];
                let r0 = quad_pts[ring * 2 + 1];
                let l1 = quad_pts[(ring + 1) * 2];
                let r1 = quad_pts[(ring + 1) * 2 + 1];
                tris.extend_from_slice(&[l0, r0, l1, r0, r1, l1]);
            }

            cmd_buf.push_triangles(
                RenderTargetId::Screen,
                TextureId::TexcKabe,
                GsBlendMode::Opaque,
                SamplerWrap::Repeat,
                DepthMode::AlwaysWrite,
                128,
                &tris,
            );
        }
    }

    fn emit_zoom_blur(&self, cmd_buf: &mut FrameCommandBuffer, passes: u8) {
        let screen_w = 640.0f32;
        let screen_h = 448.0f32;
        let mut w = 512.0f32;
        let mut h = 288.0f32;

        // Copy current screen into wb4 so we can ping-pong between screen and work buffers
        cmd_buf.push_copy_target_to_texture(RenderTargetId::Screen, TextureId::WorkBuf4);

        for pass in 0..passes {
            let src_tex = if pass % 2 == 0 {
                TextureId::WorkBuf4
            } else {
                TextureId::WorkBuf3
            };
            let dst_buf = if pass % 2 == 0 {
                RenderTargetId::WorkBuf3
            } else {
                RenderTargetId::WorkBuf4
            };
            let dst_tex = if pass % 2 == 0 {
                TextureId::WorkBuf3
            } else {
                TextureId::WorkBuf4
            };

            // Shrink screen → dst buffer
            cmd_buf.push_sprite_2d(
                dst_buf,
                TextureId::WorkBuf4,
                GsBlendMode::Opaque,
                SamplerWrap::Clamp,
                128,
                0.0,
                0.0,
                w,
                h,
                0.0,
                0.0,
                1.0,
                1.0,
                [0x80, 0x80, 0x80, 0x80],
            );

            // Stretch back dst → Screen
            cmd_buf.push_sprite_2d(
                RenderTargetId::Screen,
                src_tex,
                GsBlendMode::FixedBlend,
                SamplerWrap::Clamp,
                96,
                0.0,
                0.0,
                screen_w,
                screen_h,
                0.0,
                0.0,
                1.0,
                1.0,
                [0x80, 0x80, 0x80, 0x80],
            );
            let _ = dst_tex;

            w += 8.0;
            h += 4.5;
        }
    }

    fn emit_vignette(&self, cmd_buf: &mut FrameCommandBuffer, screen_w: f32, screen_h: f32) {
        let cx = 266.0f32;
        let cy = screen_h * 0.5;
        let rx = 74.0f32;
        let ry = 37.0f32;
        let a = self.vignette_alpha;

        let mut tris = Vec::with_capacity(16 * 6);
        for seg in 0..16usize {
            let ang0 = (seg as f32) * (std::f32::consts::TAU / 16.0);
            let ang1 = ((seg + 1) as f32) * (std::f32::consts::TAU / 16.0);

            let inner0 = [
                cx + rx * md_cos((ang0 * 10430.0) as i32),
                cy + ry * md_sin((ang0 * 10430.0) as i32),
            ];
            let inner1 = [
                cx + rx * md_cos((ang1 * 10430.0) as i32),
                cy + ry * md_sin((ang1 * 10430.0) as i32),
            ];
            let mid0 = [
                cx + rx * 1.5 * md_cos((ang0 * 10430.0) as i32),
                cy + ry * 1.5 * md_sin((ang0 * 10430.0) as i32),
            ];
            let mid1 = [
                cx + rx * 1.5 * md_cos((ang1 * 10430.0) as i32),
                cy + ry * 1.5 * md_sin((ang1 * 10430.0) as i32),
            ];
            let outer0 = [
                cx + rx * 10.0 * md_cos((ang0 * 10430.0) as i32),
                cy + ry * 10.0 * md_sin((ang0 * 10430.0) as i32),
            ];
            let outer1 = [
                cx + rx * 10.0 * md_cos((ang1 * 10430.0) as i32),
                cy + ry * 10.0 * md_sin((ang1 * 10430.0) as i32),
            ];

            let clear = [0.0f32, 0.0, 0.0, 0.0];
            let opaque = [0.0f32, 0.0, 0.0, a as f32];
            let _ = (screen_w, screen_h);

            tris.push(GpuVertex {
                pos: [inner0[0], inner0[1], 0.9, 1.0],
                uv: [0.0, 0.0],
                color: clear,
            });
            tris.push(GpuVertex {
                pos: [inner1[0], inner1[1], 0.9, 1.0],
                uv: [0.0, 0.0],
                color: clear,
            });
            tris.push(GpuVertex {
                pos: [mid0[0], mid0[1], 0.9, 1.0],
                uv: [0.0, 0.0],
                color: opaque,
            });
            tris.push(GpuVertex {
                pos: [mid1[0], mid1[1], 0.9, 1.0],
                uv: [0.0, 0.0],
                color: opaque,
            });
            tris.push(GpuVertex {
                pos: [outer0[0], outer0[1], 0.9, 1.0],
                uv: [0.0, 0.0],
                color: opaque,
            });
            tris.push(GpuVertex {
                pos: [outer1[0], outer1[1], 0.9, 1.0],
                uv: [0.0, 0.0],
                color: opaque,
            });
        }

        cmd_buf.push_triangles(
            RenderTargetId::Screen,
            TextureId::WhiteDummy,
            GsBlendMode::SourceAlpha,
            SamplerWrap::Clamp,
            DepthMode::AlwaysWrite,
            128,
            &tris,
        );
    }
}

/// A System Configuration glass rod placed on the 12-slot ring (`menuconfig.c` `CarouselMatrix`, `0x225F80`).
pub fn carousel_rod_world(
    slot: usize,
    ring_offset: i32,
    ring_spin: i32,
    ring_tilt: i32,
) -> (Vec4, f32) {
    let i = (slot as i32 - ring_offset).rem_euclid(NRING as i32);
    let ang_z = (i * 65536 / NRING as i32 - 32768) as f32;
    let _ = (ring_spin, ring_tilt);
    let a = ang_z * (std::f32::consts::TAU / 65536.0);
    let radius = 20.0f32;
    // `mdRotZ(ringTiltZ); mdRotY(ringSpinY); mdRotZ(i*65536/12 - 32768); mdTranslatef(0, 20, 0)`
    let x = radius * a.cos();
    let y = radius * a.sin();
    ([x, 10.0, y, 1.0], a)
}

/// 16-bit angle rotation helper used by Module U.
pub fn md_rotate_angle(angle: i32, radius: f32) -> (f32, f32) {
    (radius * md_cos(angle), radius * md_sin(angle))
}

/// Normalises a `-1..1` unit vector for orb reflection (`vec4_normalize3` re-export for Module U code paths).
pub fn normalize_dir(v: Vec4) -> Vec4 {
    vec4_normalize3(v)
}

/// Dot product helper used by `MeshFresnel`.
pub fn mesh_fresnel_dot(cam: Vec4, normal: Vec4) -> f32 {
    let len = vec4_dot3(cam, cam).sqrt().max(1.0e-6);
    1.0 - (vec4_dot3(cam, normal) / len).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_soft_clock_rollover_and_hold() {
        let mut c = SoftClock::default();
        c.set(23, 59, 59);
        for _ in 0..120 {
            c.tick(1.0);
        }
        assert_eq!(c.hours, 0);
        assert_eq!(c.minutes, 0);
        assert_eq!(c.seconds, 1);

        c.hold = true;
        let s_before = c.seconds;
        c.tick(1.0);
        assert_eq!(c.seconds, s_before);
    }

    #[test]
    fn test_menu_orb_trail_and_entry_fade() {
        let mut mbox = SoundMailbox::new();
        let mut scene = MenuScene::new(1, 12, 34, 56);
        assert_eq!(scene.orbs.len(), 7);
        assert_eq!(scene.orbs[0].trail.len(), TRAILLEN);

        for _ in 0..130 {
            scene.step_frame(1.0, false, &mut mbox);
        }
        assert_eq!(scene.fade_mode, 0);
        assert_eq!(scene.fade_alpha, 128);
        assert_eq!(scene.vignette_alpha, 0);

        let head0 = scene.orbs[0].trail[0];
        let old = scene.orbs[0].trail[TRAILLEN - 1];
        assert!(head0[0] >= 0.0 && head0[0] <= 640.0);
        assert!(head0[1] >= 0.0 && head0[1] <= 448.0);
        assert!((head0[0] - old[0]).abs() > 0.0001);
    }
}
