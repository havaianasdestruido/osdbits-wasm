//! Deterministic reconstruction of Module U (`osdbits/menu.c`, `menuback.c`,
//! `menuconfig.c`, `menutext.c`) for **Mode D (OSDSYS Menu & System Configuration)**:
//! - 7 RTC-driven orbiting light orbs with 50-sample `(age^4, age^2, age)` decaying trails
//! - `16x33` breathing `TEXCKABE` smoke tunnel + pre-tint refraction capture + `{0x37,0x28,0x3C}` tint
//! - 12 hexagonal bevelled glass clock rods (`MENUGEOM`) with split active-hour minute fill bar
//!   and 2-walk `TEXCFLOW` + `TEXCBUMP` / `~TEXCBUMP` sparkle
//! - 5 zig-zag camera-space glass config cubes (`±2.64`, identity camera) with selection cyan glow & kick
//! - Interactive Main Menu, System Configuration, live Clock Adjustment editor, Version Info, and
//!   transparently badged Browser Showcase.

use crate::math::{Mat4, MatrixDrive, TAU, Vec3, Vec4, clampf, cosf, md_angle_to_rad, sinf};
use crate::osd_font::{PadGlyph, emit_osd_text, emit_pad_glyph, measure_osd_text};
use crate::procedural_assets::{MeshQuad, build_cube_mesh, build_hex_rod_mesh};
use crate::render_list::{
    BatchKey, BlendMode, DepthMode, RenderList, RenderVertex, TargetId, TextureSlot, pass_action,
    vertex_flags,
};

/// Audio cue bitflags returned to TypeScript Web Audio engine after input/state transitions.
pub mod audio_cue {
    pub const NONE: u32 = 0;
    pub const CURSOR_TICK: u32 = 1 << 0;
    pub const CONFIRM: u32 = 1 << 1;
    pub const CANCEL: u32 = 1 << 2;
    pub const TRANSITION_IN: u32 = 1 << 3;
    pub const TRANSITION_OUT: u32 = 1 << 4;
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Up = 0,
    Down = 1,
    Left = 2,
    Right = 3,
    Confirm = 4,
    Cancel = 5,
    VersionInfo = 6,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuPage {
    MainMenu = 0,
    SystemConfig = 1,
    ClockAdjust = 2,
    BrowserShowcase = 3,
    VersionOverlay = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RtcClock {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl Default for RtcClock {
    fn default() -> Self {
        Self {
            year: 2026,
            month: 10,
            day: 31,
            hour: 20,
            minute: 24,
            second: 15,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MenuConfigValues {
    /// `0 = 4:3`, `1 = Full`, `2 = 16:9`
    pub screen_size: u8,
    pub digital_out_optical: bool,
    /// `0 = Y Cb/Pb Cr/Pr`, `1 = RGB`
    pub component_video_out: u8,
    /// `0..=6` (`EN, FR, ES, DE, IT, NL, PT`)
    pub language: u8,
    /// Regional O/X swap: `false` = Cross confirms (Western default), `true` = Circle confirms (JP)
    pub confirm_with_circle: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct MenuTuning {
    pub is_pal: bool,
    pub speed_scale: f32,
    pub bloom_enabled: bool,
    pub refraction_enabled: bool,
    pub reduced_motion: bool,
}

impl Default for MenuTuning {
    fn default() -> Self {
        Self {
            is_pal: false,
            speed_scale: 1.0,
            bloom_enabled: true,
            refraction_enabled: true,
            reduced_motion: false,
        }
    }
}

/// Exact zig-zag camera-space placement table for the 5 System Config glass cubes (`0x27F090`).
const CONFIG_CUBE_POS: [Vec4; 5] = [
    Vec4::new(-11.50, -11.50, 47.5, 0.0),
    Vec4::new(-22.50, -5.50, 47.5, 0.0),
    Vec4::new(-10.75, -0.25, 47.5, 0.0),
    Vec4::new(-21.75, 6.00, 47.5, 0.0),
    Vec4::new(-11.25, 11.50, 47.5, 0.0),
];

/// Entry-scatter initial orb tints (`menu.c`) easing into `{0x30, 0x62, 0x80}`.
const ORB_ENTRY_COLORS: [[f32; 3]; 7] = [
    [130.0, 190.0, 255.0],
    [100.0, 220.0, 255.0],
    [150.0, 165.0, 255.0],
    [90.0, 200.0, 255.0],
    [160.0, 215.0, 255.0],
    [115.0, 180.0, 255.0],
    [140.0, 235.0, 255.0],
];

const TRAIL_LEN: usize = 50;

#[derive(Clone, Debug)]
struct OrbState {
    trail: [Vec3; TRAIL_LEN],
    head: usize,
    filled: usize,
    scatter_phase: f32,
    color_rgb: [f32; 3],
}

pub struct MenuScene {
    pub page: MenuPage,
    pub main_cursor: u8,
    pub config_cursor: u8,
    pub clock_field_cursor: u8,
    pub browser_cursor: u8,
    pub values: MenuConfigValues,

    pub clock: RtcClock,
    edit_clock_backup: RtcClock,
    clock_subsec: f32,
    pub use_external_clock: bool,

    pub frame_count: u32,
    pub sim_time: f32,
    cam_z_off: f32,
    orb_ease: f32,
    menu_scale: f32,
    menu_scale_target: f32,
    orbs: [OrbState; 7],

    /// `0.0` on Main Menu, ramps `0.0 -> 130.0` entering System Configuration (`menuconfig.c`)
    pub cfg_anim: f32,
    cfg_open: bool,
    tunnel_phase: f32,
    ring_spin_y: i32,
    ring_tilt_z: i32,
    flow_phase: f32,
    cube_spin: i32,
    cube_tint: [[f32; 3]; 5],
    cube_kick: [f32; 5],

    rod_mesh: [MeshQuad; 16],
    cube_mesh: [MeshQuad; 6],
    pending_audio_cues: u32,
}

impl Default for MenuScene {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuScene {
    #[must_use]
    pub fn new() -> Self {
        let mut s = Self {
            page: MenuPage::MainMenu,
            main_cursor: 0,
            config_cursor: 0,
            clock_field_cursor: 3, // Hour default
            browser_cursor: 0,
            values: MenuConfigValues::default(),
            clock: RtcClock::default(),
            edit_clock_backup: RtcClock::default(),
            clock_subsec: 0.0,
            use_external_clock: false,
            frame_count: 0,
            sim_time: 0.0,
            cam_z_off: -100.0,
            orb_ease: 0.4,
            menu_scale: 1.0,
            menu_scale_target: 1.0,
            orbs: core::array::from_fn(|i| OrbState {
                trail: [Vec3::default(); TRAIL_LEN],
                head: 0,
                filled: 0,
                scatter_phase: 1.0,
                color_rgb: ORB_ENTRY_COLORS[i],
            }),
            cfg_anim: 0.0,
            cfg_open: false,
            tunnel_phase: 0.0,
            ring_spin_y: 0,
            ring_tilt_z: -5400,
            flow_phase: 0.0,
            cube_spin: 0,
            cube_tint: [[100.0, 100.0, 100.0]; 5],
            cube_kick: [0.0; 5],
            rod_mesh: build_hex_rod_mesh(),
            cube_mesh: build_cube_mesh(2.64, 1.5),
            pending_audio_cues: 0,
        };
        s.reset_entry_animation();
        s
    }

    pub fn reset_entry_animation(&mut self) {
        self.frame_count = 0;
        self.sim_time = 0.0;
        self.cam_z_off = -100.0;
        self.menu_scale = 1.0;
        self.menu_scale_target = 1.0;
        self.orb_ease = f32::from(self.clock.minute) / 60.0;
        for (i, orb) in self.orbs.iter_mut().enumerate() {
            orb.head = 0;
            orb.filled = 0;
            orb.scatter_phase = 1.0;
            orb.color_rgb = ORB_ENTRY_COLORS[i];
        }
    }

    pub fn set_clock(&mut self, clock: RtcClock, lock_external: bool) {
        if self.page != MenuPage::ClockAdjust {
            self.clock = RtcClock {
                year: clock.year.clamp(2000, 2099),
                month: clock.month.clamp(1, 12),
                day: clock.day.clamp(1, 31),
                hour: clock.hour % 24,
                minute: clock.minute % 60,
                second: clock.second % 60,
            };
            self.clock_subsec = 0.0;
        }
        self.use_external_clock = lock_external;
    }

    pub fn take_audio_cues(&mut self) -> u32 {
        let c = self.pending_audio_cues;
        self.pending_audio_cues = 0;
        c
    }

    pub fn handle_action(&mut self, action: MenuAction) -> u32 {
        let mut cue = audio_cue::NONE;
        match self.page {
            MenuPage::MainMenu => match action {
                MenuAction::Up => {
                    if self.main_cursor > 0 {
                        self.main_cursor -= 1;
                        cue |= audio_cue::CURSOR_TICK;
                    }
                }
                MenuAction::Down => {
                    if self.main_cursor < 1 {
                        self.main_cursor += 1;
                        cue |= audio_cue::CURSOR_TICK;
                    }
                }
                MenuAction::Confirm => {
                    if self.main_cursor == 0 {
                        self.page = MenuPage::BrowserShowcase;
                        cue |= audio_cue::CONFIRM | audio_cue::TRANSITION_IN;
                    } else {
                        self.page = MenuPage::SystemConfig;
                        self.cfg_open = true;
                        cue |= audio_cue::CONFIRM | audio_cue::TRANSITION_IN;
                    }
                }
                MenuAction::VersionInfo => {
                    self.page = MenuPage::VersionOverlay;
                    cue |= audio_cue::CONFIRM;
                }
                _ => {}
            },
            MenuPage::SystemConfig => match action {
                MenuAction::Up => {
                    self.config_cursor = if self.config_cursor == 0 {
                        4
                    } else {
                        self.config_cursor - 1
                    };
                    cue |= audio_cue::CURSOR_TICK;
                }
                MenuAction::Down => {
                    self.config_cursor = (self.config_cursor + 1) % 5;
                    cue |= audio_cue::CURSOR_TICK;
                }
                MenuAction::Left | MenuAction::Right => {
                    let forward = action == MenuAction::Right;
                    if self.adjust_config_item(forward) {
                        self.cube_kick[self.config_cursor as usize] = -0.10;
                        cue |= audio_cue::CURSOR_TICK;
                    }
                }
                MenuAction::Confirm => {
                    self.cube_kick[self.config_cursor as usize] = -0.10;
                    if self.config_cursor == 0 {
                        self.edit_clock_backup = self.clock;
                        self.page = MenuPage::ClockAdjust;
                        // Sink orbs into center (`MenuOrbScaleTarget(0)`)
                        self.menu_scale_target = 0.08;
                        cue |= audio_cue::CONFIRM;
                    } else {
                        let _ = self.adjust_config_item(true);
                        cue |= audio_cue::CONFIRM;
                    }
                }
                MenuAction::Cancel => {
                    self.page = MenuPage::MainMenu;
                    self.cfg_open = false;
                    cue |= audio_cue::CANCEL | audio_cue::TRANSITION_OUT;
                }
                MenuAction::VersionInfo => {}
            },
            MenuPage::ClockAdjust => match action {
                MenuAction::Left => {
                    self.clock_field_cursor = if self.clock_field_cursor == 0 {
                        5
                    } else {
                        self.clock_field_cursor - 1
                    };
                    cue |= audio_cue::CURSOR_TICK;
                }
                MenuAction::Right => {
                    self.clock_field_cursor = (self.clock_field_cursor + 1) % 6;
                    cue |= audio_cue::CURSOR_TICK;
                }
                MenuAction::Up | MenuAction::Down => {
                    let delta: i32 = if action == MenuAction::Up { 1 } else { -1 };
                    self.adjust_clock_digit(delta);
                    cue |= audio_cue::CURSOR_TICK;
                }
                MenuAction::Confirm => {
                    self.page = MenuPage::SystemConfig;
                    self.menu_scale_target = 1.0;
                    cue |= audio_cue::CONFIRM;
                }
                MenuAction::Cancel => {
                    self.clock = self.edit_clock_backup;
                    self.page = MenuPage::SystemConfig;
                    self.menu_scale_target = 1.0;
                    cue |= audio_cue::CANCEL;
                }
                MenuAction::VersionInfo => {}
            },
            MenuPage::BrowserShowcase => match action {
                MenuAction::Left => {
                    self.browser_cursor = if self.browser_cursor == 0 {
                        20
                    } else {
                        self.browser_cursor - 1
                    };
                    cue |= audio_cue::CURSOR_TICK;
                }
                MenuAction::Right => {
                    self.browser_cursor = (self.browser_cursor + 1) % 21;
                    cue |= audio_cue::CURSOR_TICK;
                }
                MenuAction::Up => {
                    self.browser_cursor = (self.browser_cursor + 21 - 7) % 21;
                    cue |= audio_cue::CURSOR_TICK;
                }
                MenuAction::Down => {
                    self.browser_cursor = (self.browser_cursor + 7) % 21;
                    cue |= audio_cue::CURSOR_TICK;
                }
                MenuAction::Cancel | MenuAction::Confirm => {
                    self.page = MenuPage::MainMenu;
                    cue |= audio_cue::CANCEL | audio_cue::TRANSITION_OUT;
                }
                MenuAction::VersionInfo => {}
            },
            MenuPage::VersionOverlay => {
                if matches!(
                    action,
                    MenuAction::Cancel | MenuAction::Confirm | MenuAction::VersionInfo
                ) {
                    self.page = MenuPage::MainMenu;
                    cue |= audio_cue::CANCEL;
                }
            }
        }
        self.pending_audio_cues |= cue;
        cue
    }

    /// Pointer click/tap hit-test in normalized screen coordinates `(nx, ny) ∈ [0, 1]^2`.
    pub fn handle_pointer_click(&mut self, nx: f32, ny: f32) -> u32 {
        let sx = nx * 640.0;
        let sy = ny * 224.0;

        match self.page {
            MenuPage::MainMenu => {
                // Main menu items at x = 380..620, y = 92..148
                if (340.0..=630.0).contains(&sx) {
                    if (84.0..118.0).contains(&sy) {
                        self.main_cursor = 0;
                        return self.handle_action(MenuAction::Confirm);
                    }
                    if (118.0..156.0).contains(&sy) {
                        self.main_cursor = 1;
                        return self.handle_action(MenuAction::Confirm);
                    }
                }
                if sy > 192.0 && sx > 440.0 {
                    return self.handle_action(MenuAction::VersionInfo);
                }
                0
            }
            MenuPage::SystemConfig => {
                if sy > 194.0 && sx < 220.0 {
                    return self.handle_action(MenuAction::Cancel);
                }
                for row in 0..5u8 {
                    let y0 = 52.0 + f32::from(row) * 26.0;
                    if (y0 - 4.0..y0 + 22.0).contains(&sy) && sx >= 220.0 {
                        if self.config_cursor == row {
                            return self.handle_action(MenuAction::Confirm);
                        }
                        self.config_cursor = row;
                        let cue = audio_cue::CURSOR_TICK;
                        self.pending_audio_cues |= cue;
                        return cue;
                    }
                }
                0
            }
            MenuPage::ClockAdjust => {
                if sy > 190.0 {
                    if sx < 320.0 {
                        return self.handle_action(MenuAction::Cancel);
                    }
                    return self.handle_action(MenuAction::Confirm);
                }
                if (72.0..108.0).contains(&sy) {
                    return self.handle_action(MenuAction::Up);
                }
                if (136.0..172.0).contains(&sy) {
                    return self.handle_action(MenuAction::Down);
                }
                0
            }
            MenuPage::BrowserShowcase | MenuPage::VersionOverlay => {
                if sy > 186.0 {
                    return self.handle_action(MenuAction::Cancel);
                }
                if self.page == MenuPage::BrowserShowcase {
                    let col = (((sx - 64.0) / 74.0).floor() as i32).clamp(0, 6) as u8;
                    let row = (((sy - 56.0) / 42.0).floor() as i32).clamp(0, 2) as u8;
                    let idx = (row * 7 + col).min(20);
                    self.browser_cursor = idx;
                    let cue = audio_cue::CURSOR_TICK;
                    self.pending_audio_cues |= cue;
                    return cue;
                }
                0
            }
        }
    }

    fn adjust_config_item(&mut self, forward: bool) -> bool {
        match self.config_cursor {
            1 => {
                self.values.screen_size = if forward {
                    (self.values.screen_size + 1) % 3
                } else {
                    (self.values.screen_size + 2) % 3
                };
                true
            }
            2 => {
                self.values.digital_out_optical = !self.values.digital_out_optical;
                true
            }
            3 => {
                self.values.component_video_out = 1 - (self.values.component_video_out & 1);
                true
            }
            4 => {
                self.values.language = if forward {
                    (self.values.language + 1) % 7
                } else {
                    (self.values.language + 6) % 7
                };
                true
            }
            _ => false,
        }
    }

    fn adjust_clock_digit(&mut self, delta: i32) {
        match self.clock_field_cursor {
            0 => {
                let y = (i32::from(self.clock.year) - 2000 + delta).rem_euclid(100) + 2000;
                self.clock.year = y as u16;
            }
            1 => {
                let m = (i32::from(self.clock.month) - 1 + delta).rem_euclid(12) + 1;
                self.clock.month = m as u8;
            }
            2 => {
                let d = (i32::from(self.clock.day) - 1 + delta).rem_euclid(31) + 1;
                self.clock.day = d as u8;
            }
            3 => {
                self.clock.hour = (i32::from(self.clock.hour) + delta).rem_euclid(24) as u8;
            }
            4 => {
                self.clock.minute = (i32::from(self.clock.minute) + delta).rem_euclid(60) as u8;
            }
            _ => {
                self.clock.second = (i32::from(self.clock.second) + delta).rem_euclid(60) as u8;
            }
        }
    }

    pub fn step(&mut self, dt_scale: f32, tuning: MenuTuning) {
        let base_step = if tuning.is_pal { 1.2 } else { 1.0 };
        let motion_damp = if tuning.reduced_motion { 0.45 } else { 1.0 };
        let dt = clampf(base_step * dt_scale * tuning.speed_scale, 0.0, 4.0);
        let fps = if tuning.is_pal { 50.0 } else { 60.0 };

        self.frame_count = self.frame_count.saturating_add(1);
        self.sim_time += dt;

        // Advance internal RTC if not frozen in ClockAdjust or externally locked
        if self.page != MenuPage::ClockAdjust && !self.use_external_clock {
            self.clock_subsec += dt / fps;
            while self.clock_subsec >= 1.0 {
                self.clock_subsec -= 1.0;
                self.clock.second += 1;
                if self.clock.second >= 60 {
                    self.clock.second = 0;
                    self.clock.minute = (self.clock.minute + 1) % 60;
                    if self.clock.minute == 0 {
                        self.clock.hour = (self.clock.hour + 1) % 24;
                    }
                }
            }
        }

        // Camera entry ease (`zOff *= 0.97`, `menu.c`)
        self.cam_z_off *= 0.97_f32.powf(dt);
        if self.cam_z_off.abs() < 0.02 {
            self.cam_z_off = 0.0;
        }

        // Orb scale ease (`* 0.96` toward target, `menu.c`)
        self.menu_scale += (self.menu_scale_target - self.menu_scale) * (1.0 - 0.92_f32.powf(dt));
        let min_target = f32::from(self.clock.minute) / 60.0;
        self.orb_ease += (min_target - self.orb_ease) * 0.05 * dt;

        // System Config 130-frame animation ramp (`menuconfig.c`)
        if self.cfg_open {
            self.cfg_anim = (self.cfg_anim + 2.2 * dt).min(130.0);
        } else {
            self.cfg_anim = (self.cfg_anim - 2.6 * dt).max(0.0);
        }

        self.tunnel_phase = (self.tunnel_phase + 0.012 * dt * motion_damp).rem_euclid(TAU * 10.0);
        self.ring_spin_y = self
            .ring_spin_y
            .wrapping_add((42.0 * dt * motion_damp) as i32);
        self.ring_tilt_z = -5400 + (sinf(self.sim_time * 0.014) * 1400.0 * motion_damp) as i32;
        self.flow_phase = (self.flow_phase + 0.008 * dt * motion_damp).rem_euclid(1.0);
        self.cube_spin = self
            .cube_spin
            .wrapping_add((30.0 * dt * motion_damp) as i32);

        for i in 0..5 {
            let selected = (self.page == MenuPage::SystemConfig
                || self.page == MenuPage::ClockAdjust)
                && self.config_cursor as usize == i;
            let target_rgb = if selected {
                [0.0, 150.0, 200.0]
            } else {
                [100.0, 100.0, 100.0]
            };
            for (tint_ch, &target_ch) in self.cube_tint[i].iter_mut().zip(&target_rgb) {
                *tint_ch += (target_ch - *tint_ch) * 0.14 * dt;
            }
            self.cube_kick[i] *= 0.86_f32.powf(dt);
        }

        // Compute 7 RTC-driven orb world positions (`menu.c` MatrixDrive equations)
        let sec_f =
            f32::from(self.clock.second) + self.clock_subsec + self.sim_time * 0.016 * motion_damp;
        let min_f = f32::from(self.clock.minute) + sec_f / 60.0;
        let rate_x = ((sec_f * 65_536.0) / 60.0) as i32;
        let rate_y = ((min_f * 65_536.0) / 60.0) as i32;
        let base_radius = (self.orb_ease * 7.25 + 10.0) * self.menu_scale;

        let mut md = MatrixDrive::new();
        for (i, orb) in self.orbs.iter_mut().enumerate() {
            orb.scatter_phase = (orb.scatter_phase - 0.011 * dt).max(0.0);
            // Ease orb tint toward `orbBaseColor {0x30, 0x62, 0x80}` (scaled to 0..255)
            let target = [96.0, 196.0, 255.0];
            for (ch, &target_ch) in orb.color_rgb.iter_mut().zip(&target) {
                *ch += (target_ch - *ch) * 0.04 * dt;
            }

            md.unit();
            md.rot_y(rate_y.wrapping_mul(12).wrapping_add((i as i32) * 9362));
            md.rot_x(((i as i32) + 21).wrapping_mul(rate_x / 18));
            let scatter = orb.scatter_phase * orb.scatter_phase * 28.0;
            let pos = md
                .top
                .apply(Vec4::new(base_radius + scatter, 0.0, 0.0, 1.0))
                .xyz();

            orb.trail[orb.head] = pos;
            orb.head = (orb.head + 1) % TRAIL_LEN;
            if orb.filled < TRAIL_LEN {
                orb.filled += 1;
            }
        }
    }

    pub fn emit_frame(&self, rl: &mut RenderList, tuning: MenuTuning) {
        let screen_w = 640.0_f32;
        let screen_h = if tuning.is_pal { 256.0_f32 } else { 224.0_f32 };
        let screen_ay = if tuning.is_pal { 0.5405_f32 } else { 0.47_f32 };

        // Module U camera (`pos = (10.436, 0, -103 + zOff)`, `rot = (0.031, 0.145, 0)`, `scrz = 512`)
        let eye = Vec3::new(10.436, 0.0, -103.0 + self.cam_z_off);
        let fwd = Vec3::new(-sinf(0.145), sinf(0.031), cosf(0.145)).normalized();
        let up = Vec3::new(0.0, 1.0, 0.0);
        let menu_cam = Mat4::ps2_camera(eye, fwd, up);
        let menu_proj =
            Mat4::ps2_webgpu_projection(512.0, 1.0, screen_ay, screen_w, screen_h, 1.0, 45_000.0);
        let vp = menu_proj * menu_cam;

        rl.clear([0.008, 0.012, 0.028, 1.0]);

        // 1. System Configuration Smoke Tunnel (`menuback.c`) when `cfg_anim > 0`
        let cfg_t = clampf(self.cfg_anim / 80.0, 0.0, 1.0);
        if cfg_t > 0.01 {
            self.emit_smoke_tunnel(rl, vp, cfg_t, tuning.reduced_motion);
            // Capture UNTINTED backdrop into `BackdropUntinted` BEFORE applying `{0x37,0x28,0x3C}` tint!
            if tuning.refraction_enabled {
                rl.emit_pass_action(pass_action::CAPTURE_UNTINTED_BACKDROP, TargetId::MainScreen);
            }
            // Apply `menuback.c` `{0x37, 0x28, 0x3C}` dark violet-charcoal screen tint
            rl.begin_batch(BatchKey {
                texture: TextureSlot::Untextured,
                blend: BlendMode::SrcAlpha,
                depth: DepthMode::None,
                action: 0,
                target: TargetId::MainScreen,
            });
            rl.push_screen_rect(
                0.0,
                0.0,
                screen_w,
                screen_h,
                screen_w,
                screen_h,
                [0.0; 4],
                [24, 16, 34, (cfg_t * 172.0) as u8],
                0,
            );
        } else {
            // Subtle deep cobalt radial glow behind the 7 orbs on Main Menu
            rl.begin_batch(BatchKey {
                texture: TextureSlot::OrbHalo,
                blend: BlendMode::Additive,
                depth: DepthMode::None,
                action: pass_action::CLAMP_UV,
                target: TargetId::MainScreen,
            });
            rl.push_screen_rect(
                72.0,
                18.0,
                340.0,
                screen_h - 36.0,
                screen_w,
                screen_h,
                [0.0, 0.0, 1.0, 1.0],
                [20, 48, 118, 92],
                0,
            );
        }

        // 2. 7 RTC-driven Orbiting Light Orbs + 50-sample `(age^4, age^2, age)` trails (`menu.c`)
        self.emit_orbs_and_trails(rl, vp);

        // 5-pass `MenuZoomBlur` radial bloom around the orb constellation (`menu.c`)
        if tuning.bloom_enabled && cfg_t < 0.95 {
            rl.emit_pass_action(pass_action::CAPTURE_BLOOM_DOWNSAMPLE, TargetId::MainScreen);
            rl.begin_batch(BatchKey {
                texture: TextureSlot::ExtraBloom,
                blend: BlendMode::Additive,
                depth: DepthMode::None,
                action: pass_action::CLAMP_UV,
                target: TargetId::MainScreen,
            });
            let passes = if tuning.reduced_motion { 2 } else { 4 };
            for p in 0..passes {
                let grow = (p + 1) as f32 * 4.0;
                rl.push_screen_rect(
                    -grow,
                    -grow * 0.5,
                    screen_w + grow * 2.0,
                    screen_h + grow,
                    screen_w,
                    screen_h,
                    [0.0, 0.0, 1.0, 1.0],
                    [180, 220, 255, ((1.0 - cfg_t) * 34.0) as u8],
                    0,
                );
            }
        }

        // 3. System Configuration 12 Glass Clock Rods & 5 Zig-Zag Glass Cubes (`menuconfig.c`)
        if cfg_t > 0.02 {
            self.emit_clock_rods(rl, menu_cam, menu_proj, cfg_t, tuning.refraction_enabled);
        }
        if self.cfg_anim > 55.0 {
            let cube_t = clampf((self.cfg_anim - 55.0) / 55.0, 0.0, 1.0);
            self.emit_config_cubes(rl, menu_proj, cube_t, tuning.refraction_enabled);
        }

        // 4. 2D OSD Navigation Typography, Clock Editor, Browser Showcase, and Button Hints (`menutext.c`)
        self.emit_osd_overlay(rl, screen_w, screen_h);

        // 5. Optional 16:9 letterbox (`values.screen_size == 2`)
        if self.values.screen_size == 2 {
            let bar = 18.0_f32;
            rl.begin_batch(BatchKey {
                texture: TextureSlot::Untextured,
                blend: BlendMode::Subtractive,
                depth: DepthMode::None,
                action: 0,
                target: TargetId::MainScreen,
            });
            rl.push_screen_rect(
                0.0, 0.0, screen_w, bar, screen_w, screen_h, [0.0; 4], [255; 4], 0,
            );
            rl.push_screen_rect(
                0.0,
                screen_h - bar,
                screen_w,
                bar,
                screen_w,
                screen_h,
                [0.0; 4],
                [255; 4],
                0,
            );
        }
    }

    fn emit_smoke_tunnel(&self, rl: &mut RenderList, vp: Mat4, alpha_scale: f32, reduced: bool) {
        rl.begin_batch(BatchKey {
            texture: TextureSlot::SmokeTunnel,
            blend: BlendMode::SrcAlpha,
            depth: DepthMode::None,
            action: 0,
            target: TargetId::MainScreen,
        });

        let segs = 16usize;
        let rings = 14usize;
        let breath_amp = if reduced { 0.015 } else { 0.05 };
        let u_scroll = (self.tunnel_phase * 0.08).rem_euclid(1.0);
        let v_scroll = (self.tunnel_phase * 0.14).rem_euclid(1.0);

        for rz in (0..rings).rev() {
            let z0 = -80.0 + (rz as f32) * 185.0;
            let z1 = -80.0 + ((rz + 1) as f32) * 185.0;
            let r0 = 145.0 * (1.0 + breath_amp * sinf(self.tunnel_phase + (rz as f32) * 0.45));
            let r1 =
                145.0 * (1.0 + breath_amp * sinf(self.tunnel_phase + ((rz + 1) as f32) * 0.45));
            let fade0 = clampf(1.0 - (rz as f32) / (rings as f32), 0.12, 1.0);
            let fade1 = clampf(1.0 - ((rz + 1) as f32) / (rings as f32), 0.08, 1.0);
            let c0 = [
                (145.0 * fade0) as u8,
                (155.0 * fade0) as u8,
                (195.0 * fade0) as u8,
                (alpha_scale * 225.0) as u8,
            ];
            let c1 = [
                (145.0 * fade1) as u8,
                (155.0 * fade1) as u8,
                (195.0 * fade1) as u8,
                (alpha_scale * 210.0) as u8,
            ];

            for s in 0..segs {
                let a0 = (s as f32) * (TAU / (segs as f32));
                let a1 = ((s + 1) as f32) * (TAU / (segs as f32));
                let p00 = vp.apply(Vec4::new(cosf(a0) * r0, sinf(a0) * r0, z0, 1.0));
                let p10 = vp.apply(Vec4::new(cosf(a1) * r0, sinf(a1) * r0, z0, 1.0));
                let p11 = vp.apply(Vec4::new(cosf(a1) * r1, sinf(a1) * r1, z1, 1.0));
                let p01 = vp.apply(Vec4::new(cosf(a0) * r1, sinf(a0) * r1, z1, 1.0));

                let u0 = (s as f32) / (segs as f32) * 2.0 + u_scroll;
                let u1 = ((s + 1) as f32) / (segs as f32) * 2.0 + u_scroll;
                let v0 = (rz as f32) * 0.35 - v_scroll;
                let v1 = ((rz + 1) as f32) * 0.35 - v_scroll;

                rl.push_clipped_quad([
                    RenderVertex::new([p00.x, p00.y, p00.z, p00.w], [u0, v0], c0, 0),
                    RenderVertex::new([p10.x, p10.y, p10.z, p10.w], [u1, v0], c0, 0),
                    RenderVertex::new([p11.x, p11.y, p11.z, p11.w], [u1, v1], c1, 0),
                    RenderVertex::new([p01.x, p01.y, p01.z, p01.w], [u0, v1], c1, 0),
                ]);
            }
        }
    }

    fn emit_orbs_and_trails(&self, rl: &mut RenderList, vp: Mat4) {
        // 1. Decaying 50-sample trails with exact `(age^4, age^2, age)` RGB falloff (`menu.c`)
        rl.begin_batch(BatchKey {
            texture: TextureSlot::LightStreak,
            blend: BlendMode::Additive,
            depth: DepthMode::None,
            action: pass_action::CLAMP_UV,
            target: TargetId::MainScreen,
        });

        for orb in &self.orbs {
            if orb.filled >= 2 {
                for step in 0..(orb.filled - 1) {
                    let ia = (orb.head + TRAIL_LEN - 1 - step) % TRAIL_LEN;
                    let ib = (orb.head + TRAIL_LEN - 2 - step) % TRAIL_LEN;
                    let pa = orb.trail[ia];
                    let pb = orb.trail[ib];

                    // `f` runs 1.0 (newest) -> 0.0 (oldest): `(r * f^4, g * f^2, b * f)`!
                    let f = 1.0 - (step as f32) / (orb.filled as f32);
                    let f2 = f * f;
                    let f4 = f2 * f2;
                    let r = clampf(orb.color_rgb[0] * 1.6 * f4, 0.0, 255.0) as u8;
                    let g = clampf(orb.color_rgb[1] * 1.2 * f2, 0.0, 255.0) as u8;
                    let b = clampf(orb.color_rgb[2] * f, 0.0, 255.0) as u8;
                    let a = (f * 175.0) as u8;
                    let w = 0.25 + 0.55 * f;

                    let c0 = vp.apply(Vec4::new(pa.x, pa.y - w, pa.z, 1.0));
                    let c1 = vp.apply(Vec4::new(pa.x, pa.y + w, pa.z, 1.0));
                    let c2 = vp.apply(Vec4::new(pb.x, pb.y + w, pb.z, 1.0));
                    let c3 = vp.apply(Vec4::new(pb.x, pb.y - w, pb.z, 1.0));
                    rl.push_clipped_quad([
                        RenderVertex::new([c0.x, c0.y, c0.z, c0.w], [0.0, 0.0], [r, g, b, a], 0),
                        RenderVertex::new([c1.x, c1.y, c1.z, c1.w], [0.0, 1.0], [r, g, b, a], 0),
                        RenderVertex::new([c2.x, c2.y, c2.z, c2.w], [1.0, 1.0], [r, g, b, a], 0),
                        RenderVertex::new([c3.x, c3.y, c3.z, c3.w], [1.0, 0.0], [r, g, b, a], 0),
                    ]);
                }
            }
        }

        // 2. `TEXCBLUR` outer halo
        rl.begin_batch(BatchKey {
            texture: TextureSlot::OrbHalo,
            blend: BlendMode::Additive,
            depth: DepthMode::None,
            action: pass_action::CLAMP_UV,
            target: TargetId::MainScreen,
        });
        for orb in &self.orbs {
            if orb.filled == 0 {
                continue;
            }
            let p = orb.trail[(orb.head + TRAIL_LEN - 1) % TRAIL_LEN];
            let s = 5.4_f32;
            let col = [
                orb.color_rgb[0] as u8,
                orb.color_rgb[1] as u8,
                orb.color_rgb[2] as u8,
                165,
            ];
            let q = [
                vp.apply(Vec4::new(p.x - s, p.y - s, p.z, 1.0)),
                vp.apply(Vec4::new(p.x + s, p.y - s, p.z, 1.0)),
                vp.apply(Vec4::new(p.x + s, p.y + s, p.z, 1.0)),
                vp.apply(Vec4::new(p.x - s, p.y + s, p.z, 1.0)),
            ];
            rl.push_clipped_quad([
                RenderVertex::new([q[0].x, q[0].y, q[0].z, q[0].w], [0.0, 0.0], col, 0),
                RenderVertex::new([q[1].x, q[1].y, q[1].z, q[1].w], [1.0, 0.0], col, 0),
                RenderVertex::new([q[2].x, q[2].y, q[2].z, q[2].w], [1.0, 1.0], col, 0),
                RenderVertex::new([q[3].x, q[3].y, q[3].z, q[3].w], [0.0, 1.0], col, 0),
            ]);
        }

        // 3. `TEXCNAVI` bright star core
        rl.begin_batch(BatchKey {
            texture: TextureSlot::OrbCore,
            blend: BlendMode::Additive,
            depth: DepthMode::None,
            action: pass_action::CLAMP_UV,
            target: TargetId::MainScreen,
        });
        for orb in &self.orbs {
            if orb.filled == 0 {
                continue;
            }
            let p = orb.trail[(orb.head + TRAIL_LEN - 1) % TRAIL_LEN];
            let s = 1.95_f32;
            let col = [235, 248, 255, 250];
            let q = [
                vp.apply(Vec4::new(p.x - s, p.y - s, p.z, 1.0)),
                vp.apply(Vec4::new(p.x + s, p.y - s, p.z, 1.0)),
                vp.apply(Vec4::new(p.x + s, p.y + s, p.z, 1.0)),
                vp.apply(Vec4::new(p.x - s, p.y + s, p.z, 1.0)),
            ];
            rl.push_clipped_quad([
                RenderVertex::new([q[0].x, q[0].y, q[0].z, q[0].w], [0.0, 0.0], col, 0),
                RenderVertex::new([q[1].x, q[1].y, q[1].z, q[1].w], [1.0, 0.0], col, 0),
                RenderVertex::new([q[2].x, q[2].y, q[2].z, q[2].w], [1.0, 1.0], col, 0),
                RenderVertex::new([q[3].x, q[3].y, q[3].z, q[3].w], [0.0, 1.0], col, 0),
            ]);
        }
    }

    fn emit_clock_rods(
        &self,
        rl: &mut RenderList,
        menu_cam: Mat4,
        menu_proj: Mat4,
        cfg_t: f32,
        refraction_enabled: bool,
    ) {
        let active_hour_slot = (self.clock.hour % 12) as usize;
        // Exact `menuconfig.c` active-hour rod minute split: `1.0 - min / 60.0`!
        let min_fill_frac = clampf(f32::from(self.clock.minute) / 60.0, 0.04, 1.0);
        let fly_scale = 0.25 + 0.75 * cfg_t;
        let alpha_base = (cfg_t * 196.0) as u8;

        let mut md = MatrixDrive::new();

        for i in 0..12usize {
            // Exact `CarouselMatrix(i, ringTiltZ, ringSpinY)` (`menuconfig.c:1631`)
            md.unit();
            md.rot_z(self.ring_tilt_z);
            md.rot_y(self.ring_spin_y);
            md.rot_z(((i as i32) * 65_536) / 12 - 32_768);
            md.translatef(0.0, 20.0 * fly_scale, 0.0);
            md.rot_y(self.ring_spin_y.wrapping_mul(4));

            let mv = menu_cam * md.top;
            let mvp = menu_proj * mv;
            let is_active_hour = i == active_hour_slot;

            // Pass 1: Untinted Smoke-Tunnel Refraction (`BackdropUntinted`)
            rl.begin_batch(BatchKey {
                texture: if refraction_enabled {
                    TextureSlot::BackdropUntinted
                } else {
                    TextureSlot::GlassFlow
                },
                blend: BlendMode::SrcAlpha,
                depth: DepthMode::ReadWrite,
                action: pass_action::CLAMP_UV,
                target: TargetId::MainScreen,
            });

            for (qi, quad) in self.rod_mesh.iter().enumerate() {
                let vn = mv
                    .apply(Vec4::new(quad.normal.x, quad.normal.y, quad.normal.z, 0.0))
                    .xyz()
                    .normalized();

                // Active-hour prism side faces (`qi >= 10`) split along Y into lower luminous minute-fill bar + upper clear glass!
                if is_active_hour && qi >= 10 {
                    let y_split = 26.0 * min_fill_frac;
                    let bot_a = quad.verts[0];
                    let bot_b = quad.verts[1];
                    let mid_b = Vec3::new(quad.verts[1].x, y_split, quad.verts[1].z);
                    let mid_a = Vec3::new(quad.verts[0].x, y_split, quad.verts[0].z);
                    let top_b = quad.verts[2];
                    let top_a = quad.verts[3];

                    let emit_part = |rl: &mut RenderList, pts: [Vec3; 4], col: [u8; 4]| {
                        let mut out_q = [RenderVertex::default(); 4];
                        for (vi, &v) in pts.iter().enumerate() {
                            let clip = mvp.apply(Vec4::new(v.x, v.y * fly_scale, v.z, 1.0));
                            let mut suv = RenderList::clip_to_screen_uv(clip);
                            suv[0] = clampf(suv[0] - 0.032 * vn.x, 0.01, 0.99);
                            suv[1] = clampf(suv[1] + 0.032 * vn.y, 0.01, 0.99);
                            out_q[vi] = RenderVertex::new(
                                [clip.x, clip.y, clip.z, clip.w],
                                suv,
                                col,
                                vertex_flags::PS2_MODULATE_2X,
                            );
                        }
                        rl.push_clipped_quad(out_q);
                    };

                    // Lower `{167, 217, 255}` ice-blue minute-fill bar (`menuconfig.c`)
                    emit_part(
                        rl,
                        [bot_a, bot_b, mid_b, mid_a],
                        [167, 217, 255, alpha_base.saturating_add(45)],
                    );
                    // Upper clear crystal portion
                    emit_part(
                        rl,
                        [mid_a, mid_b, top_b, top_a],
                        [140, 162, 204, alpha_base],
                    );
                } else {
                    let col = if is_active_hour {
                        [165, 210, 255, alpha_base.saturating_add(30)]
                    } else {
                        [134, 152, 192, alpha_base]
                    };
                    let mut out_q = [RenderVertex::default(); 4];
                    for (vi, &v) in quad.verts.iter().enumerate() {
                        let clip = mvp.apply(Vec4::new(v.x, v.y * fly_scale, v.z, 1.0));
                        let mut suv = RenderList::clip_to_screen_uv(clip);
                        suv[0] = clampf(suv[0] - 0.028 * vn.x, 0.01, 0.99);
                        suv[1] = clampf(suv[1] + 0.028 * vn.y, 0.01, 0.99);
                        out_q[vi] = RenderVertex::new(
                            [clip.x, clip.y, clip.z, clip.w],
                            suv,
                            col,
                            vertex_flags::PS2_MODULATE_2X,
                        );
                    }
                    rl.push_clipped_quad(out_q);
                }
            }

            // Pass 2: 2-walk `TEXCFLOW` + `TEXCBUMP` / `TEXCBINV` (`~TEXCBUMP`) bevel sparkle (`menuconfig.c:1619`)
            for (tex, ofs_sign) in [
                (TextureSlot::GlassFlow, 1.0_f32),
                (TextureSlot::GlassBump, -0.008_f32),
                (TextureSlot::GlassBumpInv, 0.008_f32),
            ] {
                rl.begin_batch(BatchKey {
                    texture: tex,
                    blend: BlendMode::Additive,
                    depth: DepthMode::ReadOnly,
                    action: 0,
                    target: TargetId::MainScreen,
                });
                let sparkle_a = if tex == TextureSlot::GlassFlow {
                    (cfg_t * 62.0) as u8
                } else {
                    (cfg_t * 28.0) as u8
                };
                for quad in &self.rod_mesh {
                    let vn = mv
                        .apply(Vec4::new(quad.normal.x, quad.normal.y, quad.normal.z, 0.0))
                        .xyz()
                        .normalized();
                    let fresnel = (1.0 - vn.z.abs()).powi(2);
                    let col = [
                        190,
                        225,
                        255,
                        ((f32::from(sparkle_a)) * (0.35 + 0.65 * fresnel)) as u8,
                    ];
                    let mut out_q = [RenderVertex::default(); 4];
                    for (vi, &v) in quad.verts.iter().enumerate() {
                        let clip = mvp.apply(Vec4::new(v.x, v.y * fly_scale, v.z, 1.0));
                        let uv = [
                            quad.uvs[vi][0] + self.flow_phase * ofs_sign,
                            quad.uvs[vi][1] + self.flow_phase,
                        ];
                        out_q[vi] = RenderVertex::new([clip.x, clip.y, clip.z, clip.w], uv, col, 0);
                    }
                    rl.push_clipped_quad(out_q);
                }
            }
        }
    }

    fn emit_config_cubes(
        &self,
        rl: &mut RenderList,
        menu_proj: Mat4,
        cube_t: f32,
        refraction_enabled: bool,
    ) {
        let mut md = MatrixDrive::new();
        for (i, &cube_pos) in CONFIG_CUBE_POS.iter().enumerate() {
            if refraction_enabled {
                rl.emit_pass_action(pass_action::CAPTURE_TINTED_BACKDROP, TargetId::MainScreen);
            }

            // Exact `menuconfig.c:1999`: identity camera (`cubeCamera`), zig-zag `CONFIG_CUBE_POS[i]` (`z = 47.5`), `ang = cubeSpin + i*7000`
            let ang = self.cube_spin.wrapping_add((i as i32) * 7000);
            let scale = (0.35 + 0.65 * cube_t) * (1.0 + self.cube_kick[i]);
            md.unit();
            md.translate_vec4(cube_pos);
            md.rot_x(ang);
            md.rot_y(ang);
            md.rot_z(ang);

            // Identity camera: `mv == md.top`!
            let mv = md.top;
            let mvp = menu_proj * mv;
            let tint_rgb = self.cube_tint[i];

            rl.begin_batch(BatchKey {
                texture: if refraction_enabled {
                    TextureSlot::BackdropTinted
                } else {
                    TextureSlot::CubeBlueDeep
                },
                blend: BlendMode::SrcAlpha,
                depth: DepthMode::None,
                action: pass_action::CLAMP_UV,
                target: TargetId::MainScreen,
            });

            for front_pass in [false, true] {
                for quad in &self.cube_mesh {
                    let vn = mv
                        .apply(Vec4::new(quad.normal.x, quad.normal.y, quad.normal.z, 0.0))
                        .xyz()
                        .normalized();
                    let center = mv
                        .apply(Vec4::new(
                            quad.verts[0].x * scale,
                            quad.verts[0].y * scale,
                            quad.verts[0].z * scale,
                            1.0,
                        ))
                        .xyz();
                    let is_front = vn.dot(center) < 0.0;
                    if is_front != front_pass {
                        continue;
                    }
                    let col = [
                        clampf(tint_rgb[0] + 95.0, 0.0, 255.0) as u8,
                        clampf(tint_rgb[1] + 95.0, 0.0, 255.0) as u8,
                        clampf(tint_rgb[2] + 105.0, 0.0, 255.0) as u8,
                        (cube_t * if is_front { 205.0 } else { 145.0 }) as u8,
                    ];
                    let mut out_q = [RenderVertex::default(); 4];
                    for (vi, &v) in quad.verts.iter().enumerate() {
                        let clip = mvp.apply(Vec4::new(v.x * scale, v.y * scale, v.z * scale, 1.0));
                        let mut suv = RenderList::clip_to_screen_uv(clip);
                        suv[0] = clampf(suv[0] - 0.055 * vn.x, 0.01, 0.99);
                        suv[1] = clampf(suv[1] + 0.055 * vn.y, 0.01, 0.99);
                        out_q[vi] = RenderVertex::new(
                            [clip.x, clip.y, clip.z, clip.w],
                            suv,
                            col,
                            vertex_flags::PS2_MODULATE_2X,
                        );
                    }
                    rl.push_clipped_quad(out_q);
                }
            }

            // `TEXCREFA` spherical specular reflection + `+0.01` bump highlight
            rl.begin_batch(BatchKey {
                texture: TextureSlot::ConfigCubeRef,
                blend: BlendMode::Additive,
                depth: DepthMode::None,
                action: pass_action::CLAMP_UV,
                target: TargetId::MainScreen,
            });
            for quad in &self.cube_mesh {
                let vn = mv
                    .apply(Vec4::new(quad.normal.x, quad.normal.y, quad.normal.z, 0.0))
                    .xyz()
                    .normalized();
                let center = mv
                    .apply(Vec4::new(
                        quad.verts[0].x * scale,
                        quad.verts[0].y * scale,
                        quad.verts[0].z * scale,
                        1.0,
                    ))
                    .xyz();
                if vn.dot(center) >= 0.0 {
                    continue;
                }
                let fresnel = (1.0 - vn.z.abs()).powi(2);
                let col = [
                    clampf(120.0 + tint_rgb[0] * 0.6, 0.0, 255.0) as u8,
                    clampf(160.0 + tint_rgb[1] * 0.6, 0.0, 255.0) as u8,
                    255,
                    (cube_t * (58.0 + 135.0 * fresnel)) as u8,
                ];
                let mut out_q = [RenderVertex::default(); 4];
                for (vi, &v) in quad.verts.iter().enumerate() {
                    let clip = mvp.apply(Vec4::new(v.x * scale, v.y * scale, v.z * scale, 1.0));
                    let env_uv = [
                        clampf(
                            0.5 + 0.45 * vn.x + 0.01 * md_angle_to_rad(ang).sin(),
                            0.02,
                            0.98,
                        ),
                        clampf(0.5 - 0.45 * vn.y, 0.02, 0.98),
                    ];
                    out_q[vi] = RenderVertex::new([clip.x, clip.y, clip.z, clip.w], env_uv, col, 0);
                }
                rl.push_clipped_quad(out_q);
            }
        }
    }

    fn emit_osd_overlay(&self, rl: &mut RenderList, screen_w: f32, screen_h: f32) {
        let confirm_glyph = if self.values.confirm_with_circle {
            PadGlyph::Circle
        } else {
            PadGlyph::Cross
        };
        let cancel_glyph = if self.values.confirm_with_circle {
            PadGlyph::Cross
        } else {
            PadGlyph::Circle
        };

        // Top subtle RTC clock header
        let clock_str = format!(
            "{:04}/{:02}/{:02}  {:02}:{:02}:{:02}",
            self.clock.year,
            self.clock.month,
            self.clock.day,
            self.clock.hour,
            self.clock.minute,
            self.clock.second
        );
        emit_osd_text(
            rl,
            &clock_str,
            screen_w - measure_osd_text(&clock_str, 0.85) - 24.0,
            14.0,
            0.85,
            [168, 198, 240, 205],
            screen_w,
            screen_h,
        );

        match self.page {
            MenuPage::MainMenu => {
                let items = match self.values.language {
                    1 => ["Navigateur", "Configuration du systeme"],
                    2 => ["Navegador", "Configuracion del sistema"],
                    3 => ["Browser", "Systemkonfiguration"],
                    4 => ["Browser", "Configurazione del sistema"],
                    5 => ["Browser", "Systeemconfiguratie"],
                    6 => ["Navegador", "Configuracao do sistema"],
                    _ => ["Browser", "System Configuration"],
                };
                for (idx, label) in items.iter().enumerate() {
                    let selected = self.main_cursor as usize == idx;
                    let y = 94.0 + (idx as f32) * 28.0;
                    let col = if selected {
                        [140, 224, 255, 255]
                    } else {
                        [182, 192, 212, 185]
                    };
                    if selected {
                        emit_osd_text(
                            rl,
                            ">",
                            368.0,
                            y,
                            1.05,
                            [95, 205, 255, 255],
                            screen_w,
                            screen_h,
                        );
                    }
                    emit_osd_text(rl, label, 388.0, y, 1.05, col, screen_w, screen_h);
                }

                // Bottom hint bar (`Enter` + `Version`)
                let by = screen_h - 24.0;
                emit_pad_glyph(
                    rl,
                    confirm_glyph,
                    388.0,
                    by,
                    13.0,
                    [130, 198, 255, 230],
                    screen_w,
                    screen_h,
                );
                emit_osd_text(
                    rl,
                    "Enter",
                    406.0,
                    by + 1.0,
                    0.85,
                    [205, 218, 238, 225],
                    screen_w,
                    screen_h,
                );
                emit_pad_glyph(
                    rl,
                    PadGlyph::Triangle,
                    476.0,
                    by,
                    13.0,
                    [120, 232, 188, 230],
                    screen_w,
                    screen_h,
                );
                emit_osd_text(
                    rl,
                    "Version",
                    494.0,
                    by + 1.0,
                    0.85,
                    [205, 218, 238, 225],
                    screen_w,
                    screen_h,
                );
            }
            MenuPage::SystemConfig => {
                emit_osd_text(
                    rl,
                    "System Configuration",
                    236.0,
                    24.0,
                    1.05,
                    [218, 232, 255, 240],
                    screen_w,
                    screen_h,
                );

                let screen_val = match self.values.screen_size {
                    1 => "Full",
                    2 => "16:9",
                    _ => "4:3",
                };
                let optical_val = if self.values.digital_out_optical {
                    "On"
                } else {
                    "Off"
                };
                let comp_val = if self.values.component_video_out == 0 {
                    "Y Cb/Pb Cr/Pr"
                } else {
                    "RGB"
                };
                let lang_val = match self.values.language {
                    1 => "Francais",
                    2 => "Espanol",
                    3 => "Deutsch",
                    4 => "Italiano",
                    5 => "Nederlands",
                    6 => "Portugues",
                    _ => "English",
                };

                let rows: [(&str, &str); 5] = [
                    ("Clock Adjustment", &clock_str[11..19]),
                    ("Screen Size", screen_val),
                    ("DIGITAL OUT (OPTICAL)", optical_val),
                    ("Component Video Out", comp_val),
                    ("Language", lang_val),
                ];

                for (idx, (title, val)) in rows.iter().enumerate() {
                    let selected = self.config_cursor as usize == idx;
                    let y = 56.0 + (idx as f32) * 26.0;
                    let title_col = if selected {
                        [135, 225, 255, 255]
                    } else {
                        [185, 195, 216, 190]
                    };
                    let val_col = if selected {
                        [245, 252, 255, 255]
                    } else {
                        [150, 168, 196, 175]
                    };
                    if selected {
                        emit_osd_text(
                            rl,
                            ">",
                            226.0,
                            y,
                            0.95,
                            [85, 205, 255, 255],
                            screen_w,
                            screen_h,
                        );
                    }
                    emit_osd_text(rl, title, 244.0, y, 0.92, title_col, screen_w, screen_h);
                    emit_osd_text(rl, val, 468.0, y, 0.92, val_col, screen_w, screen_h);
                }

                let by = screen_h - 24.0;
                emit_pad_glyph(
                    rl,
                    cancel_glyph,
                    44.0,
                    by,
                    13.0,
                    [240, 145, 155, 230],
                    screen_w,
                    screen_h,
                );
                emit_osd_text(
                    rl,
                    "Back",
                    62.0,
                    by + 1.0,
                    0.85,
                    [205, 218, 238, 225],
                    screen_w,
                    screen_h,
                );
                emit_pad_glyph(
                    rl,
                    confirm_glyph,
                    480.0,
                    by,
                    13.0,
                    [130, 198, 255, 230],
                    screen_w,
                    screen_h,
                );
                emit_osd_text(
                    rl,
                    "Select",
                    498.0,
                    by + 1.0,
                    0.85,
                    [205, 218, 238, 225],
                    screen_w,
                    screen_h,
                );
            }
            MenuPage::ClockAdjust => {
                rl.begin_batch(BatchKey {
                    texture: TextureSlot::Untextured,
                    blend: BlendMode::SrcAlpha,
                    depth: DepthMode::None,
                    action: 0,
                    target: TargetId::MainScreen,
                });
                rl.push_screen_rect(
                    168.0,
                    66.0,
                    388.0,
                    92.0,
                    screen_w,
                    screen_h,
                    [0.0; 4],
                    [10, 18, 38, 205],
                    0,
                );

                emit_osd_text(
                    rl,
                    "Clock Adjustment",
                    186.0,
                    74.0,
                    1.0,
                    [210, 232, 255, 245],
                    screen_w,
                    screen_h,
                );

                let fields = [
                    format!("{:04}", self.clock.year),
                    format!("{:02}", self.clock.month),
                    format!("{:02}", self.clock.day),
                    format!("{:02}", self.clock.hour),
                    format!("{:02}", self.clock.minute),
                    format!("{:02}", self.clock.second),
                ];
                let seps = ["/", "/", "   ", ":", ":", ""];
                let mut fx = 192.0_f32;
                let fy = 114.0_f32;
                for (i, part) in fields.iter().enumerate() {
                    let active = self.clock_field_cursor as usize == i;
                    let col = if active {
                        [115, 232, 255, 255]
                    } else {
                        [205, 214, 232, 205]
                    };
                    if active {
                        emit_osd_text(rl, "^", fx + 4.0, fy - 14.0, 0.8, col, screen_w, screen_h);
                        emit_osd_text(rl, "v", fx + 4.0, fy + 15.0, 0.8, col, screen_w, screen_h);
                    }
                    emit_osd_text(rl, part, fx, fy, 1.1, col, screen_w, screen_h);
                    fx += measure_osd_text(part, 1.1) + 4.0;
                    if !seps[i].is_empty() {
                        emit_osd_text(
                            rl,
                            seps[i],
                            fx,
                            fy,
                            1.05,
                            [155, 174, 204, 195],
                            screen_w,
                            screen_h,
                        );
                        fx += measure_osd_text(seps[i], 1.05) + 4.0;
                    }
                }

                let by = screen_h - 24.0;
                emit_pad_glyph(
                    rl,
                    cancel_glyph,
                    44.0,
                    by,
                    13.0,
                    [240, 145, 155, 230],
                    screen_w,
                    screen_h,
                );
                emit_osd_text(
                    rl,
                    "Cancel",
                    62.0,
                    by + 1.0,
                    0.85,
                    [205, 218, 238, 225],
                    screen_w,
                    screen_h,
                );
                emit_pad_glyph(
                    rl,
                    confirm_glyph,
                    492.0,
                    by,
                    13.0,
                    [130, 198, 255, 230],
                    screen_w,
                    screen_h,
                );
                emit_osd_text(
                    rl,
                    "OK",
                    510.0,
                    by + 1.0,
                    0.85,
                    [205, 218, 238, 225],
                    screen_w,
                    screen_h,
                );
            }
            MenuPage::BrowserShowcase => {
                rl.begin_batch(BatchKey {
                    texture: TextureSlot::Untextured,
                    blend: BlendMode::SrcAlpha,
                    depth: DepthMode::None,
                    action: 0,
                    target: TargetId::MainScreen,
                });
                rl.push_screen_rect(
                    44.0,
                    38.0,
                    552.0,
                    148.0,
                    screen_w,
                    screen_h,
                    [0.0; 4],
                    [8, 16, 36, 198],
                    0,
                );

                emit_osd_text(
                    rl,
                    "Browser // Tower History Slots [Approximate]",
                    58.0,
                    44.0,
                    0.88,
                    [168, 218, 255, 240],
                    screen_w,
                    screen_h,
                );

                for idx in 0..21u8 {
                    let col_i = idx % 7;
                    let row_i = idx / 7;
                    let gx = 64.0 + f32::from(col_i) * 74.0;
                    let gy = 68.0 + f32::from(row_i) * 34.0;
                    let active = self.browser_cursor == idx;
                    rl.begin_batch(BatchKey {
                        texture: TextureSlot::TowerWall,
                        blend: BlendMode::SrcAlpha,
                        depth: DepthMode::None,
                        action: 0,
                        target: TargetId::MainScreen,
                    });
                    let box_col = if active {
                        [110, 210, 255, 235]
                    } else {
                        [58, 78, 118, 175]
                    };
                    rl.push_screen_rect(
                        gx,
                        gy,
                        62.0,
                        26.0,
                        screen_w,
                        screen_h,
                        [0.0, 0.0, 1.0, 1.0],
                        box_col,
                        0,
                    );

                    let slot_lbl = format!("S{:02}", idx + 1);
                    emit_osd_text(
                        rl,
                        &slot_lbl,
                        gx + 14.0,
                        gy + 6.0,
                        0.82,
                        [240, 248, 255, 245],
                        screen_w,
                        screen_h,
                    );
                }

                let by = screen_h - 24.0;
                emit_pad_glyph(
                    rl,
                    cancel_glyph,
                    44.0,
                    by,
                    13.0,
                    [240, 145, 155, 230],
                    screen_w,
                    screen_h,
                );
                emit_osd_text(
                    rl,
                    "Back",
                    62.0,
                    by + 1.0,
                    0.85,
                    [205, 218, 238, 225],
                    screen_w,
                    screen_h,
                );
            }
            MenuPage::VersionOverlay => {
                rl.begin_batch(BatchKey {
                    texture: TextureSlot::Untextured,
                    blend: BlendMode::SrcAlpha,
                    depth: DepthMode::None,
                    action: 0,
                    target: TargetId::MainScreen,
                });
                rl.push_screen_rect(
                    96.0,
                    48.0,
                    448.0,
                    124.0,
                    screen_w,
                    screen_h,
                    [0.0; 4],
                    [10, 18, 38, 218],
                    0,
                );
                emit_osd_text(
                    rl,
                    "Version Information",
                    118.0,
                    58.0,
                    1.0,
                    [155, 224, 255, 248],
                    screen_w,
                    screen_h,
                );
                emit_osd_text(
                    rl,
                    "Engine:   osdbits-wasm 0.1.0 (Rust + WebGPU)",
                    118.0,
                    86.0,
                    0.85,
                    [215, 226, 245, 235],
                    screen_w,
                    screen_h,
                );
                emit_osd_text(
                    rl,
                    "Assets:   Clean-Room Procedural (Zero BIOS)",
                    118.0,
                    108.0,
                    0.85,
                    [215, 226, 245, 235],
                    screen_w,
                    screen_h,
                );
                emit_osd_text(
                    rl,
                    "Study:    aap/osdbits OSDSYS Reconstruction",
                    118.0,
                    130.0,
                    0.85,
                    [215, 226, 245, 235],
                    screen_w,
                    screen_h,
                );

                let by = screen_h - 24.0;
                emit_pad_glyph(
                    rl,
                    cancel_glyph,
                    44.0,
                    by,
                    13.0,
                    [240, 145, 155, 230],
                    screen_w,
                    screen_h,
                );
                emit_osd_text(
                    rl,
                    "Back",
                    62.0,
                    by + 1.0,
                    0.85,
                    [205, 218, 238, 225],
                    screen_w,
                    screen_h,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_navigation_and_clock_editor_transitions_work_cleanly() {
        let mut menu = MenuScene::new();
        let tuning = MenuTuning::default();

        assert_eq!(menu.page, MenuPage::MainMenu);
        assert_eq!(menu.main_cursor, 0);

        // Move down to System Configuration and enter
        assert_ne!(
            menu.handle_action(MenuAction::Down) & audio_cue::CURSOR_TICK,
            0
        );
        assert_eq!(menu.main_cursor, 1);
        assert_ne!(
            menu.handle_action(MenuAction::Confirm) & audio_cue::CONFIRM,
            0
        );
        assert_eq!(menu.page, MenuPage::SystemConfig);

        // Step animation so rods and cubes open
        for _ in 0..80 {
            menu.step(1.0, tuning);
        }
        assert!(menu.cfg_anim > 100.0);

        // Enter Clock Adjustment (cursor 0) -> orbs sink toward center (`menu_scale_target < 0.2`)
        menu.handle_action(MenuAction::Confirm);
        assert_eq!(menu.page, MenuPage::ClockAdjust);
        assert!(menu.menu_scale_target < 0.2);

        let orig_hour = menu.clock.hour;
        menu.handle_action(MenuAction::Up);
        assert_eq!(menu.clock.hour, (orig_hour + 1) % 24);

        // Cancel restores backed-up clock
        menu.handle_action(MenuAction::Cancel);
        assert_eq!(menu.page, MenuPage::SystemConfig);
        assert_eq!(menu.clock.hour, orig_hour);

        let mut rl = RenderList::default();
        menu.emit_frame(&mut rl, tuning);
        assert!(rl.vertices.len() > 500);
        for v in &rl.vertices {
            assert!(v.clip_pos.iter().all(|c| c.is_finite()));
        }
    }

    #[test]
    fn pointer_hit_testing_activates_system_config_and_back() {
        let mut menu = MenuScene::new();
        // Click System Configuration on Main Menu (`x ~ 450/640`, `y ~ 130/224`)
        let cue = menu.handle_pointer_click(450.0 / 640.0, 130.0 / 224.0);
        assert_ne!(cue & audio_cue::CONFIRM, 0);
        assert_eq!(menu.page, MenuPage::SystemConfig);

        // Click Back in bottom-left (`x ~ 70/640`, `y ~ 208/224`)
        menu.handle_pointer_click(70.0 / 640.0, 208.0 / 224.0);
        assert_eq!(menu.page, MenuPage::MainMenu);
    }
}
