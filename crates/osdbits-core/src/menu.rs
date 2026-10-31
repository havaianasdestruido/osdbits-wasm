//! Module U (`0x21C910..0x230440`) Main Menu & System Configuration deterministic simulation (`Mode D`).
//!
//! Faithfully ports:
//! - `osdbits/menu.c`: Software RTC clock (`ClockTick` + calendar rollover), camera fly-in,
//!   7 RTC-driven orbs (`UpdateOrbs` `0x2261B8`), 150-subframe `TrailPoint` rings with `(f^4, f^2, f)`
//!   asymmetric color decay, scatter fly-in, Clock-Adjustment orb sink (`menuScaleTarget = 0.0`),
//!   16-segment elliptical `DrawVignette` (`0x22AF10`), and fade curtain (`0x22AFB8`).
//! - `osdbits/menuback.c`: 16-ribbon x 33-ring breathing `TEXCKABE` cylinder tunnel (`0x21D0A0`),
//!   deep-blue composite tint `{0x37, 0x28, 0x3C}`, and 5-pass screen zoom blur (`0x22C3C0`).
//! - `osdbits/menuconfig.c` & `osdbits/menutext.c`: 90-frame `cfgAnim` choreography (`0x227390`),
//!   clean-room procedural 16-face bevelled hexagonal prism `rodModel` + 6-face `cubeModel`
//!   (with optional user-imported `MENUGEOM` override), 12 clock crystal rods (`MeshDrawRod` &
//!   `MeshDrawRodSplit` minute progress bar), 5 glass item cubes (`w = 0` crack-free flat Z,
//!   identity `cubeCamera`, cyan cursor ease `{0, 150, 200}`, `-0.1` confirm scale kick),
//!   and keyboard/pointer/gamepad navigation + Clock Adjustment & setting editors.
//!
//! Provenance tags (`MenuEntryProvenance`) explicitly distinguish reconstructed Module U behavior
//! from clean-room browser approximations/extensions (`Browser`, `Version`, `Scene Customization`).

use std::f32::consts::TAU;

use crate::math::{
    clamp_f32, gs_projected_to_webgpu_clip, mat_apply, mat_camera, mat_mul, mat_rot_x, mat_rot_y,
    mat_rot_z, mat_unit, mat_view_screen, pixel_to_ndc, GsProjectedVertex, Mat4, OsdRng, Vec4,
};
use crate::opening::{
    AudioCue, OsdLanguage, PackedColorVertex, PackedTexturedVertex, TextureId, TexturedBatch,
    VideoStandard,
};

pub const NUM_ORBS: usize = 7;
pub const TRAIL_LEN: usize = 50;
pub const TRAIL_DRAW: usize = 49;
pub const NUM_RODS: usize = 12;
pub const NUM_CFG_CUBES: usize = 5;

const KABE_RADIUS: f32 = 6000.0;
const KABE_ANGDIV: f32 = 65535.0;
const KABE_SCROLL: f32 = 0.0002;
const KABE_WOBBLE: f32 = 0.05;
const KABE_NEARZ: f32 = 2050.0;
const KABE_CAPZ: f32 = 38750.0;
const KABE_SEGMENTS: usize = 16;
const KABE_RINGS: usize = 33;
const KABE_RING_STEP: f32 = 1250.0;
const KABE_RING_Z0: f32 = -2500.0;

const MENU_CAM_POS: Vec4 = [10.436, 0.0, -103.0, 0.0];
const MENU_CAM_FWD: Vec4 = [0.0, 0.0, 1.0, 1.0];
const MENU_CAM_UP: Vec4 = [0.0, 1.0, 0.0, 1.0];
const MENU_CAM_ROT: Vec4 = [0.031, 0.145, 0.0, 0.0];

const ORB_BASE_COLOR: [i32; 4] = [0x30, 0x62, 0x80, 0x3C];
const ORB_COLORS: [[i32; 4]; NUM_ORBS] = [
    [0x00, 0x00, 0x80, 0x3C],
    [0x00, 0x80, 0x00, 0x3C],
    [0x00, 0x80, 0x80, 0x3C],
    [0x80, 0x00, 0x00, 0x3C],
    [0x80, 0x00, 0x44, 0x3C],
    [0x80, 0x44, 0x00, 0x3C],
    [0x80, 0x80, 0x80, 0x3C],
];
const ORB_CORE_COLOR: [i32; 4] = [0x80, 0x80, 0x80, 0x80];

const CFG_COL_BODY: [i32; 4] = [0x2D, 0x55, 0x66, 0x80];
const CFG_COL_EDGE: [i32; 4] = [0x3C, 0x3C, 0x3C, 0x80];
const CFG_COL_RING_A: [i32; 4] = [0x3C, 0x3C, 0x3C, 0x80];
const CFG_COL_RING_B: [i32; 4] = [0x80, 0x80, 0x80, 0x1E];
const CFG_COL_FIXED: [i32; 4] = [167, 217, 255, 0];
const CFG_COL_FRONT: [i32; 4] = [(0x2D + 167) / 2, (0x55 + 217) / 2, (0x66 + 255) / 2, 0x80];

const CUBE_COL_SEL: [i32; 4] = [0, 150, 200, 128];
const CUBE_COL_PLAIN: [i32; 4] = [100, 100, 100, 128];

/// Exact retail camera-space positions at `0x27F090` (`w = 0.0`, see `menu.c:363` & `docs/menu-config.md:3204`).
const MENU_CUBE_POS: [Vec4; NUM_CFG_CUBES] = [
    [-11.50, -11.50, 47.5, 0.0],
    [-22.50, -5.50, 47.5, 0.0],
    [-10.75, -0.25, 47.5, 0.0],
    [-21.75, 6.00, 47.5, 0.0],
    [-11.25, 11.50, 47.5, 0.0],
];

const MESH_LOWER_MASK: u32 = 0xFF00; // faces 8..=15
const MESH_UPPER_MASK: u32 = 0xFFFF & !0x0300; // faces 0..=7, 10..=15 (skip bottom cap 8..=9)

#[inline]
fn md_angle(a: i32) -> f32 {
    (a as i16 as f32) * (TAU / 65536.0)
}

#[inline]
fn md_sin(a: i32) -> f32 {
    md_angle(a).sin()
}

#[inline]
fn md_cos(a: i32) -> f32 {
    md_angle(a).cos()
}

/// Module U stack-top rotation/translation helpers (`menu.c:330..395`).
#[inline]
fn md_rot_x(top: &mut Mat4, a: i32) {
    let r = mat_rot_x(md_angle(a));
    *top = mat_mul(top, &r);
}

#[inline]
fn md_rot_y(top: &mut Mat4, a: i32) {
    let r = mat_rot_y(md_angle(a));
    *top = mat_mul(top, &r);
}

#[inline]
fn md_rot_z(top: &mut Mat4, a: i32) {
    let r = mat_rot_z(md_angle(a));
    *top = mat_mul(top, &r);
}

#[inline]
fn md_translate_v4(top: &mut Mat4, v4: Vec4) {
    let o = mat_apply(top, v4);
    top[3] = o;
}

#[inline]
fn md_translatef(top: &mut Mat4, x: f32, y: f32, z: f32) {
    md_translate_v4(top, [x, y, z, 1.0]);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnimTimer {
    pub duration: i32,
    pub count: i32,
    pub edge: bool,
    pub state: u8,
}

impl AnimTimer {
    pub const fn new(duration: i32) -> Self {
        Self {
            duration,
            count: 0,
            edge: false,
            state: 0,
        }
    }

    pub fn open(&mut self) {
        if self.state == 0 {
            self.count = 0;
            self.edge = true;
            self.state = 1;
        }
    }

    pub fn close(&mut self) {
        if self.state == 2 {
            self.edge = true;
            self.count = self.duration;
            self.state = 3;
        }
    }

    pub fn step(&mut self) {
        self.edge = false;
        if self.state == 1 {
            self.count += 1;
            if self.count >= self.duration {
                self.count = self.duration;
                self.edge = true;
                self.state = 2;
            }
        } else if self.state == 3 {
            self.count -= 1;
            if self.count <= 0 {
                self.count = 0;
                self.edge = true;
                self.state = 0;
            }
        }
    }

    pub fn interp(&self, max_val: i32) -> i32 {
        if self.duration > 0 {
            (self.count * max_val) / self.duration
        } else {
            0
        }
    }
}

/// Deterministic software RTC clock (`menu.c:108` + `menutext.c:1082` calendar rules).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuClock {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub ms: f32,
    pub hold: bool,
}

impl Default for MenuClock {
    fn default() -> Self {
        Self::new(2026, 10, 31, 12, 34, 56)
    }
}

impl MenuClock {
    pub fn new(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Self {
        let y = year.clamp(2000, 2099);
        let m = month.clamp(1, 12);
        let max_d = Self::days_in_month(y, m);
        Self {
            year: y,
            month: m,
            day: day.clamp(1, max_d),
            hour: hour % 24,
            minute: minute % 60,
            second: second % 60,
            ms: 0.0,
            hold: false,
        }
    }

    pub const fn is_leap_year(year: u16) -> bool {
        (year % 4 == 0) && (year % 100 != 0 || year % 400 == 0)
    }

    pub const fn days_in_month(year: u16, month: u8) -> u8 {
        const MDAYS: [u8; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let idx = ((month.saturating_sub(1)) % 12) as usize;
        if idx == 1 && Self::is_leap_year(year) {
            29
        } else {
            MDAYS[idx]
        }
    }

    pub fn tick(&mut self, standard: VideoStandard) {
        if self.hold {
            return;
        }
        let step_ms = match standard {
            VideoStandard::Ntsc => 1000.0 / 59.94,
            VideoStandard::Pal => 1000.0 / 50.0,
        };
        self.advance_ms(step_ms);
    }

    pub fn advance_ms(&mut self, delta_ms: f32) {
        if !delta_ms.is_finite() || delta_ms <= 0.0 {
            return;
        }
        self.ms += delta_ms;
        while self.ms >= 1000.0 {
            self.ms -= 1000.0;
            self.second += 1;
            if self.second >= 60 {
                self.second = 0;
                self.minute += 1;
                if self.minute >= 60 {
                    self.minute = 0;
                    self.hour += 1;
                    if self.hour >= 24 {
                        self.hour = 0;
                        self.day += 1;
                        if self.day > Self::days_in_month(self.year, self.month) {
                            self.day = 1;
                            self.month += 1;
                            if self.month > 12 {
                                self.month = 1;
                                self.year = if self.year >= 2099 {
                                    2000
                                } else {
                                    self.year + 1
                                };
                            }
                        }
                    }
                }
            }
        }
    }

    #[inline]
    pub fn seconds_float(&self) -> f32 {
        f32::from(self.second) + self.ms * 0.001
    }

    #[inline]
    pub fn minutes_float(&self) -> f32 {
        f32::from(self.minute) + self.seconds_float() / 60.0
    }

    #[inline]
    pub fn hours_float(&self) -> f32 {
        f32::from(self.hour % 12) + self.minutes_float() / 60.0
    }

    pub fn format_date(&self) -> String {
        format!("{:04}/{:02}/{:02}", self.year, self.month, self.day)
    }

    pub fn format_time(&self) -> String {
        format!(" {:2}:{:02}:{:02}", self.hour, self.minute, self.second)
    }

    pub fn format_full(&self) -> String {
        format!(
            "{:04}/{:02}/{:02} {:2}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Cancel,
    Options,
    Display,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEntryProvenance {
    /// Directly reconstructed from retail Module U (`0x21C910..0x230440`) code & savestates.
    SourcedReconstruction,
    /// Present in retail OSDSYS (`Browser`, `Version`) where the target sub-module (`mcman` / version ROM page)
    /// is approximated by a clean-room browser panel rather than claiming unverified ROM behavior.
    ApproximatedSubscreen,
    /// Explicit `osdbits-wasm` Mode E user customization / scene switcher entry.
    CleanRoomExtension,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuScreenState {
    MainMenu,
    BrowserApproxPanel,
    SystemConfigTransitionIn,
    SystemConfigList,
    SystemConfigEditingValue,
    SystemConfigEditingClock,
    SystemConfigTransitionOut,
    VersionApproxPanel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenAspectSetting {
    FourByThree = 0,
    Full = 1,
    SixteenByNine = 2,
}

impl ScreenAspectSetting {
    pub const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Full,
            2 => Self::SixteenByNine,
            _ => Self::FourByThree,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::FourByThree => "4:3",
            Self::Full => "Full",
            Self::SixteenByNine => "16:9",
        }
    }
}

#[derive(Clone, Debug)]
pub struct MeshGeometry {
    pub nfaces: usize,
    /// `nfaces * 4` vertices `(x, y, z, 1)`.
    pub verts: Vec<Vec4>,
    /// `nfaces` outward normals `(nx, ny, nz, 0)`.
    pub norms: Vec<Vec4>,
    /// `nfaces * 4` UVs `(u, v, 0, 0)`.
    pub uvs: Vec<[f32; 4]>,
    pub is_user_supplied: bool,
}

impl MeshGeometry {
    /// Clean-room procedural 16-face bevelled hexagonal prism matching `osdbits/menuconfig.c:25,392`:
    /// - Faces `0..=1`: flat top cap at `y = 26.39`, radius `2.34`
    /// - Faces `2..=7`: bevel ring `y = 26.0..26.39`, radius `2.6 -> 2.34`
    /// - Faces `8..=9`: flat bottom cap at `y = 0.0`, radius `2.6`
    /// - Faces `10..=15`: 6 shaft sides `y = 0.0..26.0`, radius `2.6`, `V` spanning `0.0..2.0`.
    pub fn clean_room_rod() -> Self {
        let nfaces = 16;
        let mut verts = Vec::with_capacity(nfaces * 4);
        let mut norms = Vec::with_capacity(nfaces);
        let mut uvs = Vec::with_capacity(nfaces * 4);

        let hex_pt = |idx: usize, r: f32, y: f32| -> Vec4 {
            let a = (idx % 6) as f32 * (TAU / 6.0);
            [r * a.sin(), y, r * a.cos(), 1.0]
        };

        // 0..=1: Top cap at y = 26.39 (normal +Y), split hexagon (0,1,2,3,4,5) into two tristrip quads:
        // TRISTRIP (v0, v1, v5, v2) and (v5, v2, v4, v3) with outward +Y winding.
        for &(i0, i1, i2, i3) in &[(0_usize, 1, 5, 2), (5, 2, 4, 3)] {
            verts.push(hex_pt(i0, 2.34, 26.39));
            verts.push(hex_pt(i1, 2.34, 26.39));
            verts.push(hex_pt(i2, 2.34, 26.39));
            verts.push(hex_pt(i3, 2.34, 26.39));
            norms.push([0.0, 1.0, 0.0, 0.0]);
            uvs.extend_from_slice(&[
                [0.0, 0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0, 0.0],
            ]);
        }

        // 2..=7: 6 bevel faces (y = 26.0, r = 2.6 -> y = 26.39, r = 2.34)
        let bevel_ny = 0.26_f32 / (0.26_f32 * 0.26 + 0.39 * 0.39).sqrt();
        let bevel_nr = 0.39_f32 / (0.26_f32 * 0.26 + 0.39 * 0.39).sqrt();
        for side in 0..6 {
            let next = (side + 1) % 6;
            let mid_a = (side as f32 + 0.5) * (TAU / 6.0);
            verts.push(hex_pt(side, 2.6, 26.0));
            verts.push(hex_pt(next, 2.6, 26.0));
            verts.push(hex_pt(side, 2.34, 26.39));
            verts.push(hex_pt(next, 2.34, 26.39));
            norms.push([
                bevel_nr * mid_a.sin(),
                bevel_ny,
                bevel_nr * mid_a.cos(),
                0.0,
            ]);
            uvs.extend_from_slice(&[
                [0.0, 1.85, 0.0, 0.0],
                [1.0, 1.85, 0.0, 0.0],
                [0.0, 2.0, 0.0, 0.0],
                [1.0, 2.0, 0.0, 0.0],
            ]);
        }

        // 8..=9: Bottom cap at y = 0.0 (normal -Y)
        for &(i0, i1, i2, i3) in &[(0_usize, 5, 1, 4), (1, 4, 2, 3)] {
            verts.push(hex_pt(i0, 2.6, 0.0));
            verts.push(hex_pt(i1, 2.6, 0.0));
            verts.push(hex_pt(i2, 2.6, 0.0));
            verts.push(hex_pt(i3, 2.6, 0.0));
            norms.push([0.0, -1.0, 0.0, 0.0]);
            uvs.extend_from_slice(&[
                [0.0, 0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0, 0.0],
            ]);
        }

        // 10..=15: 6 shaft faces (y = 0.0..26.0, r = 2.6, UV V = 0.0..2.0)
        for side in 0..6 {
            let next = (side + 1) % 6;
            let mid_a = (side as f32 + 0.5) * (TAU / 6.0);
            verts.push(hex_pt(side, 2.6, 0.0));
            verts.push(hex_pt(next, 2.6, 0.0));
            verts.push(hex_pt(side, 2.6, 26.0));
            verts.push(hex_pt(next, 2.6, 26.0));
            norms.push([mid_a.sin(), 0.0, mid_a.cos(), 0.0]);
            uvs.extend_from_slice(&[
                [0.0, 0.0, 0.0, 0.0],
                [1.5, 0.0, 0.0, 0.0],
                [0.0, 2.0, 0.0, 0.0],
                [1.5, 2.0, 0.0, 0.0],
            ]);
        }

        Self {
            nfaces,
            verts,
            norms,
            uvs,
            is_user_supplied: false,
        }
    }

    /// Clean-room procedural 6-face cube matching `osdbits/menuconfig.c:27` (`half = 2.64`, `UV = 0..1.5`).
    pub fn clean_room_cube() -> Self {
        let h = 2.64_f32;
        let faces: [([Vec4; 4], Vec4); 6] = [
            // +Y
            (
                [
                    [-h, h, -h, 1.0],
                    [h, h, -h, 1.0],
                    [-h, h, h, 1.0],
                    [h, h, h, 1.0],
                ],
                [0.0, 1.0, 0.0, 0.0],
            ),
            // -Y
            (
                [
                    [-h, -h, h, 1.0],
                    [h, -h, h, 1.0],
                    [-h, -h, -h, 1.0],
                    [h, -h, -h, 1.0],
                ],
                [0.0, -1.0, 0.0, 0.0],
            ),
            // +Z
            (
                [
                    [-h, -h, h, 1.0],
                    [-h, h, h, 1.0],
                    [h, -h, h, 1.0],
                    [h, h, h, 1.0],
                ],
                [0.0, 0.0, 1.0, 0.0],
            ),
            // -X
            (
                [
                    [-h, -h, -h, 1.0],
                    [-h, h, -h, 1.0],
                    [-h, -h, h, 1.0],
                    [-h, h, h, 1.0],
                ],
                [-1.0, 0.0, 0.0, 0.0],
            ),
            // -Z
            (
                [
                    [h, -h, -h, 1.0],
                    [h, h, -h, 1.0],
                    [-h, -h, -h, 1.0],
                    [-h, h, -h, 1.0],
                ],
                [0.0, 0.0, -1.0, 0.0],
            ),
            // +X
            (
                [
                    [h, -h, h, 1.0],
                    [h, h, h, 1.0],
                    [h, -h, -h, 1.0],
                    [h, h, -h, 1.0],
                ],
                [1.0, 0.0, 0.0, 0.0],
            ),
        ];

        let mut verts = Vec::with_capacity(24);
        let mut norms = Vec::with_capacity(6);
        let mut uvs = Vec::with_capacity(24);
        for (fv, fnrm) in faces {
            verts.extend_from_slice(&fv);
            norms.push(fnrm);
            uvs.extend_from_slice(&[
                [0.0, 0.0, 0.0, 0.0],
                [1.5, 0.0, 0.0, 0.0],
                [0.0, 1.5, 0.0, 0.0],
                [1.5, 1.5, 0.0, 0.0],
            ]);
        }
        Self {
            nfaces: 6,
            verts,
            norms,
            uvs,
            is_user_supplied: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TrailPoint {
    pub pos: [f32; 4],
    pub col: [i32; 4],
}

#[derive(Clone, Debug)]
pub struct OrbState {
    pub head: usize,
    pub sub: u8,
    pub wrapped: bool,
    pub trail: [TrailPoint; TRAIL_LEN],
}

impl Default for OrbState {
    fn default() -> Self {
        Self {
            head: 0,
            sub: 0,
            wrapped: false,
            trail: [TrailPoint::default(); TRAIL_LEN],
        }
    }
}

impl OrbState {
    pub fn push_trail(&mut self, pos: [f32; 4], col: [i32; 4]) {
        self.trail[self.head] = TrailPoint { pos, col };
        self.sub += 1;
        if self.sub != 3 {
            return;
        }
        self.sub = 0;
        self.head += 1;
        if self.head == TRAIL_LEN {
            self.head = 0;
            self.wrapped = true;
        }
        self.trail[self.head] = TrailPoint { pos, col };
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RingSlot {
    pub progress: f32,
    pub split: f32,
    pub col0: [i32; 4],
    pub col1: [i32; 4],
}

impl Default for RingSlot {
    fn default() -> Self {
        Self {
            progress: 0.0,
            split: 0.0,
            col0: CFG_COL_BODY,
            col1: CFG_COL_EDGE,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MenuUiItem {
    pub label: String,
    pub value_text: String,
    pub selected: bool,
    pub alpha: u8,
    pub provenance: MenuEntryProvenance,
}

#[derive(Clone, Debug, Default)]
pub struct MenuUiSnapshot {
    pub screen: Option<MenuScreenState>,
    /// True when [`MenuSim::input_ready`] accepts input right now, i.e. when
    /// `dispatch_action` would not swallow the event. The browser shell uses this
    /// to keep menu controls disabled instead of silently dropping clicks during
    /// the 90-frame System Configuration choreography (`menu.c:0x227D08`).
    pub input_ready: bool,
    pub date_text: String,
    pub time_text: String,
    pub title: String,
    pub items: Vec<MenuUiItem>,
    pub clock_edit_fields: Option<[String; 6]>,
    pub clock_edit_cursor: u8,
    pub hint_confirm_label: &'static str,
    pub hint_cancel_label: &'static str,
    pub show_options_hint: bool,
    pub page_marker_alpha: u8,
}

#[derive(Clone, Debug, Default)]
pub struct MenuRenderList {
    pub kabe_tunnel_batch: Option<TexturedBatch>,
    pub backdrop_tint_rgb: [u8; 3],
    pub orb_trail_vertices: Vec<PackedColorVertex>,
    pub orb_halo_batch: Option<TexturedBatch>,
    pub orb_core_batch: Option<TexturedBatch>,
    pub rod_refract_batches: Vec<TexturedBatch>,
    pub rod_bump_batches: Vec<TexturedBatch>,
    pub cube_refract_batches: Vec<TexturedBatch>,
    pub cube_bump_batches: Vec<TexturedBatch>,
    pub cube_refl_batches: Vec<TexturedBatch>,
    pub vignette_vertices: Vec<PackedColorVertex>,
    pub zoom_blur_passes: u8,
    pub fade_curtain_rgba: [u8; 4],
    pub letterbox_bar_ratio: f32,
    pub ui: MenuUiSnapshot,
}

impl MenuRenderList {
    pub fn clear(&mut self) {
        self.kabe_tunnel_batch = None;
        self.backdrop_tint_rgb = [0x37, 0x28, 0x3C];
        self.orb_trail_vertices.clear();
        self.orb_halo_batch = None;
        self.orb_core_batch = None;
        self.rod_refract_batches.clear();
        self.rod_bump_batches.clear();
        self.cube_refract_batches.clear();
        self.cube_bump_batches.clear();
        self.cube_refl_batches.clear();
        self.vignette_vertices.clear();
        self.zoom_blur_passes = 0;
        self.fade_curtain_rgba = [0, 0, 0, 0];
        self.letterbox_bar_ratio = 0.0;
        self.ui.items.clear();
        self.ui.clock_edit_fields = None;
        self.ui.title.clear();
        self.ui.page_marker_alpha = 0;
    }
}

#[derive(Clone, Debug)]
struct TransformedMeshFace {
    normal: Vec4,
    cam: [Vec4; 4],
    proj: [GsProjectedVertex; 4],
    uv: [[f32; 2]; 4],
    cull: bool,
}

#[derive(Clone, Debug)]
pub struct MenuSim {
    pub standard: VideoStandard,
    pub clock: MenuClock,
    pub frame_count: u32,
    pub back_frame: u32,
    pub from_opening: bool,

    pub cam_z_offset: f32,
    pub camera_matrix: Mat4,
    pub view_screen_matrix: Mat4,

    pub menu_scale: f32,
    pub menu_scale_target: f32,
    pub orb_ease: f32,
    pub orb_ease_out: f32,
    pub orb_tilt_z: i16,
    pub orb_spin_y: i16,
    pub orb_phase: [i32; NUM_ORBS],
    pub orbs: [OrbState; NUM_ORBS],
    pub orb_trail_timer: AnimTimer,

    pub fade_mode: u8,
    pub fade_alpha: i32,
    pub fade_color: [u8; 3],
    pub vig_timer: AnimTimer,
    pub bg_timer: AnimTimer,

    pub main_menu_anim: AnimTimer,
    pub main_menu_cursor: usize,
    pub custom_main_entries: Vec<String>,
    pub requested_custom_entry: Option<usize>,

    pub cfg_dur40: i32,
    pub cfg_dur40b: i32,
    pub cfg_dur10: i32,
    pub cfg_anim: AnimTimer,
    pub cfg_cube_timer: AnimTimer,
    pub carousel_timer: AnimTimer,

    pub ring: [RingSlot; NUM_RODS],
    pub ring_offset: usize,
    pub ring_spin_y: i16,
    pub ring_tilt_z: i16,
    pub ring_split_max: f32,
    pub ring_col_a: [i32; 4],
    pub ring_col_b: [i32; 4],

    pub cube_spin: u16,
    pub cube_color: [[i32; 4]; NUM_CFG_CUBES],
    pub cube_bias: [f32; NUM_CFG_CUBES],
    pub cube_tracker: [i32; 4],
    pub config_item_alpha: [i32; NUM_CFG_CUBES],
    pub config_cursor: usize,
    pub config_mode: u8, // 0 = item list, 1 = editing item
    pub config_phase: i32,

    // Settings values
    pub screen_aspect: ScreenAspectSetting,
    pub digital_out_on: bool,
    pub component_rgb: bool,
    pub language: OsdLanguage,
    pub confirm_cancel_swap: bool, // true = Cross Enter / Circle Back; false = Circle Enter / Cross Back

    // Clock adjustment scratch
    pub clock_edit_cursor: usize,
    pub clock_edit_vals: [i32; 6],
    clock_saved_hms: [u8; 3],

    pub screen_override: Option<MenuScreenState>,
    pub rod_model: MeshGeometry,
    pub cube_model: MeshGeometry,
    pub reduced_motion: bool,
    pub enable_zoom_blur: bool,
}

impl MenuSim {
    pub fn new(
        standard: VideoStandard,
        clock: MenuClock,
        language: OsdLanguage,
        rng_seed: u32,
        from_opening: bool,
    ) -> Self {
        let mut rng = OsdRng::new(rng_seed);
        let mut orb_phase = [0_i32; NUM_ORBS];
        for p in &mut orb_phase {
            *p = rng.next_i31() % 65536;
        }

        let fps = standard.fps() as i32;
        let cfg_dur40 = (fps * 40) / 60;
        let cfg_dur10 = fps / 6;
        let cfg_dur40b = (fps * 40) / 60;

        let mut orb_trail_timer = AnimTimer::new((fps << 8) / 60);
        orb_trail_timer.open();

        let vig_timer = AnimTimer::new((fps * 80) / 60);
        let bg_timer = AnimTimer::new(cfg_dur40);
        let main_menu_anim = AnimTimer::new(cfg_dur10);

        let orb_tilt_z = (clock.hours_float() * (65536.0 / 12.0)) as i32 as i16;
        let orb_spin_y = (clock.seconds_float() * (65536.0 / 60.0)) as i32 as i16;

        let ring_offset = usize::from(clock.hour % 12);
        let ring_tilt_z = ((ring_offset << 16) / NUM_RODS) as i16;
        let ring_spin_y = orb_spin_y;
        let ring_split_max = 1.0 - clock.minutes_float() / 60.0;

        let mut ring = [RingSlot::default(); NUM_RODS];
        ring[ring_offset].col0 = CFG_COL_FRONT;
        ring[ring_offset].col1 = CFG_COL_RING_B;

        let mut sim = Self {
            standard,
            clock,
            frame_count: 0,
            back_frame: 0,
            from_opening,
            cam_z_offset: -100.0,
            camera_matrix: mat_unit(),
            view_screen_matrix: mat_unit(),
            menu_scale: 1.0,
            menu_scale_target: 1.0,
            orb_ease: 0.0,
            orb_ease_out: 0.0,
            orb_tilt_z,
            orb_spin_y,
            orb_phase,
            orbs: std::array::from_fn(|_| OrbState::default()),
            orb_trail_timer,
            fade_mode: 2,
            fade_alpha: 0,
            fade_color: [0, 0, 0],
            vig_timer,
            bg_timer,
            main_menu_anim,
            main_menu_cursor: 0,
            custom_main_entries: vec!["Scene & Visual Options".to_owned()],
            requested_custom_entry: None,
            cfg_dur40,
            cfg_dur40b,
            cfg_dur10,
            cfg_anim: AnimTimer::new(cfg_dur40b + cfg_dur40 + cfg_dur10),
            cfg_cube_timer: AnimTimer::new(cfg_dur40),
            carousel_timer: AnimTimer::new(1),
            ring,
            ring_offset,
            ring_spin_y,
            ring_tilt_z,
            ring_split_max,
            ring_col_a: CFG_COL_FIXED,
            ring_col_b: CFG_COL_RING_A,
            cube_spin: 0,
            cube_color: [[128, 128, 128, 128]; NUM_CFG_CUBES],
            cube_bias: [0.0; NUM_CFG_CUBES],
            cube_tracker: [128, 128, 128, 128],
            config_item_alpha: [128, 0, 0, 0, 0],
            config_cursor: 0,
            config_mode: 0,
            config_phase: 0,
            screen_aspect: ScreenAspectSetting::FourByThree,
            digital_out_on: false,
            component_rgb: false,
            language,
            confirm_cancel_swap: true,
            clock_edit_cursor: 0,
            clock_edit_vals: [
                i32::from(clock.year),
                i32::from(clock.month),
                i32::from(clock.day),
                i32::from(clock.hour),
                i32::from(clock.minute),
                0,
            ],
            clock_saved_hms: [clock.hour, clock.minute, clock.second],
            screen_override: None,
            rod_model: MeshGeometry::clean_room_rod(),
            cube_model: MeshGeometry::clean_room_cube(),
            reduced_motion: false,
            enable_zoom_blur: true,
        };
        sim.update_camera();
        sim
    }

    pub fn main_menu_entry_count(&self) -> usize {
        2 + self.custom_main_entries.len()
    }

    pub fn current_screen(&self) -> MenuScreenState {
        if let Some(ov) = self.screen_override {
            return ov;
        }
        match self.cfg_anim.state {
            0 => MenuScreenState::MainMenu,
            1 => MenuScreenState::SystemConfigTransitionIn,
            3 => MenuScreenState::SystemConfigTransitionOut,
            _ => {
                if self.config_mode == 1 {
                    if self.config_cursor == 0 {
                        MenuScreenState::SystemConfigEditingClock
                    } else {
                        MenuScreenState::SystemConfigEditingValue
                    }
                } else {
                    MenuScreenState::SystemConfigList
                }
            }
        }
    }

    pub fn enter_system_config(&mut self, audio: &mut Vec<AudioCue>) {
        if self.cfg_anim.state != 0 {
            return;
        }
        self.screen_override = None;
        self.cfg_anim.duration = self.cfg_dur40b + self.cfg_dur40 + self.cfg_dur10;
        self.cfg_anim.open();
        self.vig_timer.close();
        self.bg_timer.open();
        if self.carousel_timer.state == 0 {
            self.carousel_timer.open();
            for s in &mut self.ring {
                s.progress = 0.0;
            }
        }
        audio.push(AudioCue::MenuConfirm);
    }

    pub fn leave_system_config(&mut self, audio: &mut Vec<AudioCue>) {
        if self.cfg_anim.state != 2 {
            return;
        }
        self.cfg_anim.close();
        audio.push(AudioCue::MenuCancel);
    }

    /// True when [`Self::dispatch_action`] would accept input in the current state.
    ///
    /// Both the main-menu fade-in and the System Configuration choreography
    /// (`cfgAnim` states 1/3, `menu.c:0x227D08`) ignore input while they play.
    /// The UI shell mirrors this so that controls are only shown as clickable
    /// once the core actually accepts the event.
    pub fn input_ready(&self) -> bool {
        // Sub-panels accept Confirm/Cancel to close at any time.
        if self.screen_override.is_some() {
            return true;
        }
        if self.cfg_anim.state == 0 {
            return self.main_menu_anim.state == 2 || self.fade_mode == 0;
        }
        self.cfg_anim.state == 2
    }

    pub fn dispatch_action(&mut self, action: MenuAction, audio: &mut Vec<AudioCue>) {
        // Sub-panels (`Browser`, `Version`) close on Cancel or Confirm
        if let Some(_ov) = self.screen_override {
            if matches!(action, MenuAction::Cancel | MenuAction::Confirm) {
                self.screen_override = None;
                audio.push(AudioCue::MenuCancel);
            }
            return;
        }

        // Main-menu fade-in and System Configuration transitions swallow input.
        if !self.input_ready() {
            return;
        }

        if self.cfg_anim.state == 0 {
            let total = self.main_menu_entry_count();
            match action {
                MenuAction::Up => {
                    self.main_menu_cursor = (self.main_menu_cursor + total - 1) % total;
                    audio.push(AudioCue::MenuCursorMove);
                }
                MenuAction::Down => {
                    self.main_menu_cursor = (self.main_menu_cursor + 1) % total;
                    audio.push(AudioCue::MenuCursorMove);
                }
                MenuAction::Confirm => match self.main_menu_cursor {
                    0 => {
                        self.screen_override = Some(MenuScreenState::BrowserApproxPanel);
                        audio.push(AudioCue::MenuConfirm);
                    }
                    1 => self.enter_system_config(audio),
                    idx => {
                        self.requested_custom_entry = Some(idx - 2);
                        audio.push(AudioCue::MenuConfirm);
                    }
                },
                MenuAction::Options => {
                    self.screen_override = Some(MenuScreenState::VersionApproxPanel);
                    audio.push(AudioCue::MenuConfirm);
                }
                _ => {}
            }
            return;
        }

        if self.config_mode == 1 {
            self.dispatch_config_edit_input(action, audio);
            return;
        }

        match action {
            MenuAction::Up => {
                self.config_cursor = (self.config_cursor + NUM_CFG_CUBES - 1) % NUM_CFG_CUBES;
                audio.push(AudioCue::MenuCursorMove);
            }
            MenuAction::Down => {
                self.config_cursor = (self.config_cursor + 1) % NUM_CFG_CUBES;
                audio.push(AudioCue::MenuCursorMove);
            }
            MenuAction::Confirm => {
                if self.config_cursor == 0 {
                    self.open_clock_editor();
                }
                self.config_phase = 0;
                self.config_mode = 1;
                audio.push(AudioCue::MenuConfirm);
            }
            MenuAction::Cancel => {
                self.leave_system_config(audio);
            }
            MenuAction::Options => {
                if self.config_cursor == 0 {
                    self.screen_override = Some(MenuScreenState::VersionApproxPanel);
                    audio.push(AudioCue::MenuConfirm);
                }
            }
            _ => {}
        }
    }

    fn open_clock_editor(&mut self) {
        self.menu_scale_target = 0.0;
        self.orb_trail_timer.close();
        self.clock.hold = true;
        self.clock_saved_hms = [self.clock.hour, self.clock.minute, self.clock.second];
        self.clock_edit_cursor = 0;
        self.clock_edit_vals = [
            i32::from(self.clock.year),
            i32::from(self.clock.month),
            i32::from(self.clock.day),
            i32::from(self.clock.hour),
            i32::from(self.clock.minute),
            0, // Real 0x21DF68 zeroes seconds to park carousel at spin 0!
        ];
        self.sync_clock_from_edit_vals();
    }

    fn sync_clock_from_edit_vals(&mut self) {
        let y = self.clock_edit_vals[0].clamp(2000, 2099) as u16;
        let m = self.clock_edit_vals[1].clamp(1, 12) as u8;
        let max_d = i32::from(MenuClock::days_in_month(y, m));
        if self.clock_edit_vals[2] > max_d {
            self.clock_edit_vals[2] = max_d;
        }
        let d = self.clock_edit_vals[2].clamp(1, max_d) as u8;
        let hh = (self.clock_edit_vals[3].rem_euclid(24)) as u8;
        let mm = (self.clock_edit_vals[4].rem_euclid(60)) as u8;
        let ss = (self.clock_edit_vals[5].rem_euclid(60)) as u8;

        self.clock.year = y;
        self.clock.month = m;
        self.clock.day = d;
        self.clock.hour = hh;
        self.clock.minute = mm;
        self.clock.second = ss;
        self.clock.ms = 0.0;
    }

    fn reopen_orb_trail_after_clock(&mut self) {
        self.menu_scale_target = 1.0;
        self.clock.hold = false;
        if self.orb_trail_timer.state == 0 {
            self.orb_trail_timer.open();
        } else if self.orb_trail_timer.state == 3 {
            self.orb_trail_timer.state = 1;
        }
    }

    fn dispatch_config_edit_input(&mut self, action: MenuAction, audio: &mut Vec<AudioCue>) {
        if self.config_cursor == 0 {
            match action {
                MenuAction::Left => {
                    self.clock_edit_cursor = (self.clock_edit_cursor + 5) % 6;
                    audio.push(AudioCue::MenuValueStep);
                }
                MenuAction::Right => {
                    self.clock_edit_cursor = (self.clock_edit_cursor + 1) % 6;
                    audio.push(AudioCue::MenuValueStep);
                }
                MenuAction::Up | MenuAction::Down => {
                    let dir = if action == MenuAction::Up { 1 } else { -1 };
                    let f = self.clock_edit_cursor;
                    let (min_v, max_v) = match f {
                        0 => (2000, 2099),
                        1 => (1, 12),
                        2 => (
                            1,
                            i32::from(MenuClock::days_in_month(
                                self.clock_edit_vals[0] as u16,
                                self.clock_edit_vals[1] as u8,
                            )),
                        ),
                        3 => (0, 23),
                        _ => (0, 59),
                    };
                    let mut v = self.clock_edit_vals[f] + dir;
                    if v > max_v {
                        v = min_v;
                    } else if v < min_v {
                        v = max_v;
                    }
                    self.clock_edit_vals[f] = v;
                    self.sync_clock_from_edit_vals();
                    audio.push(AudioCue::MenuValueStep);
                }
                MenuAction::Confirm => {
                    self.cube_bias[0] = -0.1; // real 0x227BE8 kick!
                    self.sync_clock_from_edit_vals();
                    self.reopen_orb_trail_after_clock();
                    self.config_mode = 0;
                    self.config_phase = 0;
                    audio.push(AudioCue::MenuConfirm);
                }
                MenuAction::Cancel => {
                    self.clock.hour = self.clock_saved_hms[0];
                    self.clock.minute = self.clock_saved_hms[1];
                    self.clock.second = self.clock_saved_hms[2];
                    self.clock.ms = 0.0;
                    self.reopen_orb_trail_after_clock();
                    self.config_mode = 0;
                    self.config_phase = 0;
                    audio.push(AudioCue::MenuCancel);
                }
                _ => {}
            }
            return;
        }

        match action {
            MenuAction::Left | MenuAction::Right | MenuAction::Up | MenuAction::Down => {
                let forward = matches!(action, MenuAction::Right | MenuAction::Down);
                match self.config_cursor {
                    1 => {
                        let cur = self.screen_aspect as u8;
                        let next = if forward {
                            (cur + 1) % 3
                        } else {
                            (cur + 2) % 3
                        };
                        self.screen_aspect = ScreenAspectSetting::from_u8(next);
                    }
                    2 => self.digital_out_on = !self.digital_out_on,
                    3 => self.component_rgb = !self.component_rgb,
                    4 => {
                        let cur = self.language as u8;
                        let next = if forward {
                            (cur + 1) % 8
                        } else {
                            (cur + 7) % 8
                        };
                        self.language = OsdLanguage::from_u8(next);
                    }
                    _ => {}
                }
                audio.push(AudioCue::MenuValueStep);
            }
            MenuAction::Confirm => {
                self.cube_bias[self.config_cursor] = -0.1;
                self.config_mode = 0;
                self.config_phase = 0;
                audio.push(AudioCue::MenuConfirm);
            }
            MenuAction::Cancel => {
                self.config_mode = 0;
                self.config_phase = 0;
                audio.push(AudioCue::MenuCancel);
            }
            _ => {}
        }
    }

    pub fn step(&mut self, render_out: &mut MenuRenderList, audio_out: &mut Vec<AudioCue>) {
        render_out.clear();
        if self.frame_count == 0 {
            audio_out.push(AudioCue::StartMenuAmbience);
        }

        let screen_w = self.standard.width() as f32;
        let screen_h = self.standard.height() as f32;

        // Stage 1: MenuCamera (0x21CFD8)
        self.update_camera();
        self.cam_z_offset *= if self.reduced_motion { 0.85 } else { 0.97 };

        // Stage 2: MenuBackdrop (0x21D0A0)
        self.bg_timer.step();
        if self.bg_timer.state != 0 && self.fade_mode == 0 {
            self.emit_kabe_tunnel(screen_w, screen_h, render_out);
        }

        // Stage 3: SceneReset / CarouselEmit / UpdateOrbs / SceneWalk (0x2268F0)
        self.emit_orbs_and_rods(screen_w, screen_h, render_out);

        // Stage 4: MenuVignette (0x22AF10) & Zoom blur (0x22C3C0) & 5 Config Cubes (0x226FA8)
        self.vig_timer.step();
        if self.fade_mode == 0 && self.vig_timer.state != 0 {
            self.emit_vignette(screen_w, screen_h, render_out);
        }
        if self.enable_zoom_blur && !self.reduced_motion && self.cfg_anim.state != 0 {
            render_out.zoom_blur_passes = 5;
        }
        self.step_and_emit_config_cubes(screen_w, screen_h, render_out);

        // Stage 5: Config & MainMenu state machines + 2D UI snapshot + Letterbox + Fade Curtain
        self.step_config_state_machine();
        self.step_main_menu_anim();
        self.build_ui_snapshot(render_out);

        if matches!(
            self.screen_aspect,
            ScreenAspectSetting::FourByThree | ScreenAspectSetting::SixteenByNine
        ) {
            let content = screen_w * 0.0625 * 9.0 * self.standard.menu_screen_ay();
            let bar = ((screen_h - content) * 0.5).max(0.0);
            render_out.letterbox_bar_ratio = clamp_f32(bar / screen_h, 0.0, 0.35);
        }

        let curtain_a = (128 - self.fade_alpha).clamp(0, 128) as u8;
        if curtain_a > 0 {
            render_out.fade_curtain_rgba = [
                self.fade_color[0],
                self.fade_color[1],
                self.fade_color[2],
                curtain_a,
            ];
        }

        // Stage 10: MenuConfigCarousel (0x225BF8), menuScale ease, FadeStep, ClockTick
        self.step_carousel();
        self.menu_scale += (self.menu_scale_target - self.menu_scale) * 0.1;
        self.step_fade();
        if self.config_mode == 1 && self.config_cursor == 0 {
            self.sync_clock_from_edit_vals();
        } else {
            self.clock.tick(self.standard);
        }

        self.back_frame = self.back_frame.wrapping_add(1);
        self.frame_count = self.frame_count.saturating_add(1);
    }

    fn update_camera(&mut self) {
        self.view_screen_matrix = mat_view_screen(
            512.0,
            1.0,
            self.standard.menu_screen_ay(),
            2048.0,
            2048.0,
            1.0,
            16_777_215.0,
            1.0,
            65536.0,
        );
        let pos = [
            MENU_CAM_POS[0],
            MENU_CAM_POS[1],
            MENU_CAM_POS[2] + self.cam_z_offset,
            0.0,
        ];
        let mut m = mat_unit();
        m = mat_mul(&mat_rot_x(MENU_CAM_ROT[0]), &m);
        m = mat_mul(&mat_rot_y(MENU_CAM_ROT[1]), &m);
        m = mat_mul(&mat_rot_z(MENU_CAM_ROT[2]), &m);
        let zd = mat_apply(&m, MENU_CAM_FWD);
        let yd = mat_apply(&m, MENU_CAM_UP);
        let p = mat_apply(&m, pos);
        self.camera_matrix = mat_camera(p, zd, yd);
    }

    fn emit_kabe_tunnel(&self, screen_w: f32, screen_h: f32, out: &mut MenuRenderList) {
        let fade_add = self.bg_timer.interp(40) as f32;
        let scroll_scale = if self.reduced_motion { 0.25 } else { 1.0 };
        let scroll = ((self.back_frame % 5000) as f32) * KABE_SCROLL * scroll_scale;
        let wobble_amp = if self.reduced_motion {
            0.01
        } else {
            KABE_WOBBLE
        };

        let mut verts = Vec::with_capacity(KABE_SEGMENTS * KABE_RINGS * 6);
        for seg in 0..KABE_SEGMENTS {
            let ang0 = (seg * 65536 / KABE_SEGMENTS) as i32;
            let ang1 = ((seg + 1) * 65536 / KABE_SEGMENTS) as i32;
            let e0 = [KABE_RADIUS * md_sin(ang0), KABE_RADIUS * md_cos(ang0)];
            let e1 = [KABE_RADIUS * md_sin(ang1), KABE_RADIUS * md_cos(ang1)];
            let s0 = (ang0 as f32) / KABE_ANGDIV;
            let s1 = (ang1 as f32) / KABE_ANGDIV;
            let tint0 = ((md_cos(ang0) + 1.0) * 10.0) as i32 as f32;
            let tint1 = ((md_cos(ang1) + 1.0) * 10.0) as i32 as f32;

            let mut strip_pairs: Vec<[PackedTexturedVertex; 2]> =
                Vec::with_capacity(KABE_RINGS + 1);

            for i in 0..KABE_RINGS {
                let scale =
                    1.0 + md_sin((self.back_frame as i32) * 100 + (i as i32) * 5120) * wobble_amp;
                let z = (i as f32) * KABE_RING_STEP + KABE_RING_Z0;
                let w0 = [e0[0] * scale, e0[1] * scale, z, 1.0];
                let w1 = [e1[0] * scale, e1[1] * scale, z, 1.0];

                let c0 = mat_apply(&self.camera_matrix, w0);
                let c1 = mat_apply(&self.camera_matrix, w1);
                if c0[2] < KABE_NEARZ || c1[2] < KABE_NEARZ {
                    continue;
                }
                let p0 = self.project_cam_vertex(c0);
                let p1 = self.project_cam_vertex(c1);
                if (p0.gs_xyz[0] - 2000.0).abs() > 1000.0
                    || (p1.gs_xyz[0] - 2000.0).abs() > 1000.0
                    || (p0.gs_xyz[1] - 2000.0).abs() > 1000.0
                    || (p1.gs_xyz[1] - 2000.0).abs() > 1000.0
                {
                    continue;
                }

                let tc = (i as f32) * (1.0 / 32.0) + scroll;
                let k = (KABE_RINGS - 1 - i) as i32;
                let pp = (k * k * k) >> 10;
                let rr = ((pp * 230) >> 5) as f32;
                let gg = ((pp * 260) >> 5) as f32;

                let v0 = PackedTexturedVertex {
                    clip_pos: gs_projected_to_webgpu_clip(&p0, screen_w, screen_h, 1.0),
                    uv_q: [s0 * 3.0, tc * 3.0, p0.q, 0.0],
                    gs_rgba: [
                        (rr + fade_add + tint0).min(255.0),
                        (gg + fade_add + tint0).min(255.0),
                        (gg + fade_add + tint0).min(255.0),
                        64.0,
                    ],
                };
                let v1 = PackedTexturedVertex {
                    clip_pos: gs_projected_to_webgpu_clip(&p1, screen_w, screen_h, 1.0),
                    uv_q: [s1 * 3.0, tc * 3.0, p1.q, 0.0],
                    gs_rgba: [
                        (rr + fade_add + tint1).min(255.0),
                        (gg + fade_add + tint1).min(255.0),
                        (gg + fade_add + tint1).min(255.0),
                        64.0,
                    ],
                };
                strip_pairs.push([v0, v1]);
            }

            // Cap ring at z = 38750
            let cap_cam = mat_apply(&self.camera_matrix, [0.0, 0.0, KABE_CAPZ, 1.0]);
            if cap_cam[2] > 1.0 {
                let pcap = self.project_cam_vertex(cap_cam);
                let tc = 1.0 + scroll;
                let clip = gs_projected_to_webgpu_clip(&pcap, screen_w, screen_h, 1.0);
                strip_pairs.push([
                    PackedTexturedVertex {
                        clip_pos: clip,
                        uv_q: [s0 * 3.0, tc * 3.0, pcap.q, 0.0],
                        gs_rgba: [fade_add, fade_add, fade_add, 64.0],
                    },
                    PackedTexturedVertex {
                        clip_pos: clip,
                        uv_q: [s1 * 3.0, tc * 3.0, pcap.q, 0.0],
                        gs_rgba: [fade_add, fade_add, fade_add, 64.0],
                    },
                ]);
            }

            for pair_win in strip_pairs.windows(2) {
                let [a0, a1] = pair_win[0];
                let [b0, b1] = pair_win[1];
                verts.push(a0);
                verts.push(a1);
                verts.push(b0);
                verts.push(b0);
                verts.push(a1);
                verts.push(b1);
            }
        }

        if !verts.is_empty() {
            out.kabe_tunnel_batch = Some(TexturedBatch {
                texture: TextureId::MenuKabe,
                blend_mode: 255, // opaque
                blend_fix: 128,
                depth_test: true,
                depth_write: true,
                uv_clamp: false,
                vertices: verts,
            });
        }
    }

    fn emit_orbs_and_rods(&mut self, screen_w: f32, screen_h: f32, out: &mut MenuRenderList) {
        #[derive(Clone)]
        enum DeferredItem {
            Orb {
                depth: f32,
                idx: usize,
                world: Mat4,
            },
            Rod {
                depth: f32,
                slot: usize,
                world: Mat4,
                progress: f32,
                size: f32,
                split: f32,
                col0: [i32; 4],
                aux: f32,
            },
        }

        let mut list: Vec<DeferredItem> = Vec::with_capacity(NUM_ORBS + NUM_RODS);

        // 1. Carousel rods if active (`ring[0].progress > 0.05`)
        if self.ring[0].progress > 0.05 {
            for i in 0..NUM_RODS {
                let slot = (i + self.ring_offset) % NUM_RODS;
                let mut md_top = mat_unit();
                md_rot_z(&mut md_top, i32::from(self.ring_tilt_z));
                md_rot_y(&mut md_top, i32::from(self.ring_spin_y));
                md_rot_z(&mut md_top, ((i as i32) * 65536) / 12 - 32768);
                md_translatef(&mut md_top, 0.0, 20.0, 0.0);
                md_rot_y(&mut md_top, i32::from(self.ring_spin_y) * 4);

                let cam_m = mat_mul(&self.camera_matrix, &md_top);
                list.push(DeferredItem::Rod {
                    depth: cam_m[3][2],
                    slot,
                    world: md_top,
                    progress: self.ring[slot].progress,
                    size: if i == 0 { 200.0 } else { 160.0 },
                    split: if i == 0 { self.ring[slot].split } else { -1.0 },
                    col0: self.ring[slot].col0,
                    aux: if i == 0 { 100.0 } else { 0.0 },
                });
            }
        }

        // 2. UpdateOrbs (0x2261B8)
        let rate_x = self.clock.seconds_float() * (65536.0 / 60.0);
        let rate_y = self.clock.minutes_float() * (65536.0 / 60.0);
        self.orb_ease_out = 1.0 - self.clock.minutes_float() * (1.0 / 60.0);
        let t = self.orb_ease;
        let radius = (t * 7.25 + 10.0) * self.menu_scale;
        self.orb_ease = t + ((1.0 - self.orb_ease_out) - t) * 0.005;

        for i in 0..NUM_ORBS {
            let mut md_top = mat_unit();
            md_rot_z(&mut md_top, i32::from(self.orb_tilt_z));
            md_rot_y(&mut md_top, i32::from(self.orb_spin_y));
            md_rot_z(&mut md_top, -32768);
            md_rot_y(&mut md_top, (rate_y * 1100.0) as i32);
            md_rot_x(&mut md_top, (((i + 21) as f32) * rate_x) as i32);
            md_translatef(&mut md_top, 0.0, radius, 0.0);
            md_rot_y(&mut md_top, 8192);

            let cam_m = mat_mul(&self.camera_matrix, &md_top);
            list.push(DeferredItem::Orb {
                depth: cam_m[3][2],
                idx: i,
                world: md_top,
            });
        }

        // Insertion-sort descending by camera-space Z (farthest first, `0x225D40`)
        list.sort_by(|a, b| {
            let da = match a {
                DeferredItem::Orb { depth, .. } | DeferredItem::Rod { depth, .. } => *depth,
            };
            let db = match b {
                DeferredItem::Orb { depth, .. } | DeferredItem::Rod { depth, .. } => *depth,
            };
            db.total_cmp(&da)
        });

        let mut halo_verts = Vec::with_capacity(NUM_ORBS * 6);
        let mut core_verts = Vec::with_capacity(NUM_ORBS * 6);

        for item in list {
            match item {
                DeferredItem::Orb { idx, world, .. } => {
                    self.emit_single_orb(
                        idx,
                        &world,
                        screen_w,
                        screen_h,
                        &mut halo_verts,
                        &mut core_verts,
                        &mut out.orb_trail_vertices,
                    );
                }
                DeferredItem::Rod {
                    slot,
                    world,
                    progress,
                    size,
                    split,
                    col0,
                    aux,
                    ..
                } => {
                    self.emit_rod_record(
                        slot, &world, progress, size, split, col0, aux, screen_w, screen_h, out,
                    );
                }
            }
        }

        if !halo_verts.is_empty() {
            out.orb_halo_batch = Some(TexturedBatch {
                texture: TextureId::MenuBlur,
                blend_mode: 5, // additive Cs*As + Cd
                blend_fix: 128,
                depth_test: false,
                depth_write: false,
                uv_clamp: true,
                vertices: halo_verts,
            });
        }
        if !core_verts.is_empty() {
            out.orb_core_batch = Some(TexturedBatch {
                texture: TextureId::MenuNavi,
                blend_mode: 5,
                blend_fix: 128,
                depth_test: false,
                depth_write: false,
                uv_clamp: true,
                vertices: core_verts,
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_single_orb(
        &mut self,
        idx: usize,
        world: &Mat4,
        screen_w: f32,
        screen_h: f32,
        halo_verts: &mut Vec<PackedTexturedVertex>,
        core_verts: &mut Vec<PackedTexturedVertex>,
        trail_verts: &mut Vec<PackedColorVertex>,
    ) {
        let cam_m = mat_mul(&self.camera_matrix, world);
        let a = mat_apply(&cam_m, [0.0, 0.0, 0.0, 1.0]);
        let p = self.project_cam_vertex(a);
        let mut sp = [
            p.gs_xyz[0] - 2048.0,
            p.gs_xyz[1] - 2048.0,
            p.gs_xyz[2],
            p.w.max(1.0),
        ];

        // Entry scatter fly-in (0x226360)
        if !self.reduced_motion
            && ((self.fade_mode == 2 && (self.from_opening || idx == 0))
                || (self.fade_mode == 3 && idx == 0))
        {
            let amp = 1.0 - md_sin(self.fade_alpha * 128);
            let dx = (screen_w * 0.5) * md_cos(self.orb_phase[idx]) * amp;
            let dy = (screen_h * 0.5) * md_sin(self.orb_phase[idx]) * amp;
            if self.fade_mode == 2 {
                sp[0] += dx;
                sp[1] += dy;
            } else {
                let t = (self.fade_alpha as f32) / 128.0;
                sp[0] = sp[0] * t + dx * 1.5;
                sp[1] = sp[1] * t + dy * 1.5;
            }
        }

        let col = if self.fade_mode == 2 || self.fade_mode == 3 {
            let c = self.fade_alpha.clamp(0, 128);
            [
                (ORB_COLORS[idx][0] * (128 - c) + ORB_BASE_COLOR[0] * c) >> 7,
                (ORB_COLORS[idx][1] * (128 - c) + ORB_BASE_COLOR[1] * c) >> 7,
                (ORB_COLORS[idx][2] * (128 - c) + ORB_BASE_COLOR[2] * c) >> 7,
                (ORB_COLORS[idx][3] * (128 - c) + ORB_BASE_COLOR[3] * c) >> 7,
            ]
        } else {
            ORB_BASE_COLOR
        };

        self.orbs[idx].push_trail(sp, col);
        self.orb_trail_timer.step();

        let timer_scale_num = self.orb_trail_timer.interp(128);
        if timer_scale_num <= 0 {
            return;
        }
        let timer_frac = (timer_scale_num as f32) / 128.0;

        // 1. Trail line strip segments with (f^4, f^2, f) RGB decay
        let orb = &self.orbs[idx];
        let mut prev_v: Option<PackedColorVertex> = None;
        if orb.wrapped {
            for k in 0..TRAIL_DRAW {
                let f = ((128 - (k as i32) * 3).max(0) as f32 * timer_frac) as i32;
                let r_idx = (orb.head + 100 - k) % TRAIL_LEN;
                let tp = &orb.trail[r_idx];
                let cv = Self::build_trail_vertex(tp, f, screen_w, screen_h);
                if let Some(pv) = prev_v {
                    trail_verts.push(pv);
                    trail_verts.push(cv);
                }
                prev_v = Some(cv);
            }
        } else if orb.head > 1 {
            let n = orb.head - 1;
            for k in 0..n {
                let f =
                    ((128 - ((k * TRAIL_LEN / n) as i32) * 3).max(0) as f32 * timer_frac) as i32;
                let tp = &orb.trail[orb.head - k];
                let cv = Self::build_trail_vertex(tp, f, screen_w, screen_h);
                if let Some(pv) = prev_v {
                    trail_verts.push(pv);
                    trail_verts.push(cv);
                }
                prev_v = Some(cv);
            }
        }

        // 2. Halo (half = zscale * 30.0) & Core (half = zscale * 4.5)
        let head_pt = &orb.trail[orb.head];
        let zscale = head_pt.pos[2].max(0.0) * 6.5e-6;
        let halo_a = self.orb_trail_timer.interp(head_pt.col[3]) as f32;
        let core_a = self.orb_trail_timer.interp(ORB_CORE_COLOR[3]) as f32;

        Self::push_orb_sprite(
            head_pt.pos,
            zscale * 30.0,
            [
                head_pt.col[0] as f32,
                head_pt.col[1] as f32,
                head_pt.col[2] as f32,
                halo_a,
            ],
            screen_w,
            screen_h,
            halo_verts,
        );
        Self::push_orb_sprite(
            head_pt.pos,
            zscale * 4.5,
            [
                ORB_CORE_COLOR[0] as f32,
                ORB_CORE_COLOR[1] as f32,
                ORB_CORE_COLOR[2] as f32,
                core_a,
            ],
            screen_w,
            screen_h,
            core_verts,
        );
    }

    fn build_trail_vertex(
        tp: &TrailPoint,
        f: i32,
        screen_w: f32,
        screen_h: f32,
    ) -> PackedColorVertex {
        let b = (tp.col[2] * f) >> 7;
        let g = (tp.col[1] * f * f) >> 14;
        let r = (((((tp.col[0] * f * f) >> 7) * f) >> 7) * f) >> 14;
        let a = f >> 1;
        let px = tp.pos[0] + screen_w * 0.5;
        let py = tp.pos[1] + screen_h * 0.5;
        let ndc = pixel_to_ndc(px, py, screen_w, screen_h);
        let z_ndc = clamp_f32(1.0 / tp.pos[3].max(1.0), 0.0, 1.0);
        PackedColorVertex {
            clip_pos: [ndc[0], ndc[1], z_ndc, 1.0],
            gs_rgba: [r as f32, g as f32, b as f32, (a * 2).min(128) as f32],
        }
    }

    fn push_orb_sprite(
        pos: [f32; 4],
        half: f32,
        rgba: [f32; 4],
        screen_w: f32,
        screen_h: f32,
        out: &mut Vec<PackedTexturedVertex>,
    ) {
        if half <= 0.01 || rgba[3] <= 0.0 {
            return;
        }
        let cx = pos[0] + screen_w * 0.5;
        let cy = pos[1] + screen_h * 0.5;
        let x0 = cx - half;
        let y0 = cy - half * 0.5;
        let x1 = cx + half;
        let y1 = cy + half * 0.5;
        let c00 = pixel_to_ndc(x0, y0, screen_w, screen_h);
        let c10 = pixel_to_ndc(x1, y0, screen_w, screen_h);
        let c01 = pixel_to_ndc(x0, y1, screen_w, screen_h);
        let c11 = pixel_to_ndc(x1, y1, screen_w, screen_h);
        let qv = [
            PackedTexturedVertex {
                clip_pos: [c00[0], c00[1], 0.0, 1.0],
                uv_q: [0.0, 0.0, 1.0, 0.0],
                gs_rgba: rgba,
            },
            PackedTexturedVertex {
                clip_pos: [c10[0], c10[1], 0.0, 1.0],
                uv_q: [1.0, 0.0, 1.0, 0.0],
                gs_rgba: rgba,
            },
            PackedTexturedVertex {
                clip_pos: [c01[0], c01[1], 0.0, 1.0],
                uv_q: [0.0, 1.0, 1.0, 0.0],
                gs_rgba: rgba,
            },
            PackedTexturedVertex {
                clip_pos: [c11[0], c11[1], 0.0, 1.0],
                uv_q: [1.0, 1.0, 1.0, 0.0],
                gs_rgba: rgba,
            },
        ];
        out.push(qv[0]);
        out.push(qv[1]);
        out.push(qv[2]);
        out.push(qv[2]);
        out.push(qv[1]);
        out.push(qv[3]);
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_rod_record(
        &self,
        slot: usize,
        world: &Mat4,
        progress: f32,
        size: f32,
        split: f32,
        col0: [i32; 4],
        aux: f32,
        screen_w: f32,
        screen_h: f32,
        out: &mut MenuRenderList,
    ) {
        if progress <= 0.0 {
            return;
        }
        let phase = (slot as f32) * 0.1;
        let rod_bump_col = [8, 8, 8, 128];

        if split > 0.0 {
            // MeshDrawRodSplit (`0x22D9D4`): lower (`MESH_LOWER_MASK`, `ringColA`, `size = aux = 100`)
            // + upper (`MESH_UPPER_MASK`, translated `(0, 26*progress*split, 0)`, `col0`, `size = 200`)
            let (low_faces, obj_x, obj_y) = self.transform_mesh(
                &self.rod_model,
                &self.camera_matrix,
                world,
                [1.0, progress * split, 1.0],
                false,
            );
            let ref_x = obj_x * 0.9;
            let ref_y = obj_y * 0.9;

            let mut world_up = *world;
            md_translatef(&mut world_up, 0.0, 26.0 * progress * split, 0.0);
            let (up_faces, _, _) = self.transform_mesh(
                &self.rod_model,
                &self.camera_matrix,
                &world_up,
                [1.0, progress * (1.0 - split), 1.0],
                false,
            );

            let mut refract_verts = Vec::new();
            for back in [false, true] {
                Self::emit_mesh_refract_faces(
                    &low_faces,
                    MESH_LOWER_MASK,
                    back,
                    self.ring_col_a,
                    aux,
                    ref_x,
                    ref_y,
                    screen_w,
                    screen_h,
                    &mut refract_verts,
                );
                Self::emit_mesh_refract_faces(
                    &up_faces,
                    MESH_UPPER_MASK,
                    back,
                    col0,
                    size,
                    ref_x,
                    ref_y,
                    screen_w,
                    screen_h,
                    &mut refract_verts,
                );
            }
            if !refract_verts.is_empty() {
                out.rod_refract_batches.push(TexturedBatch {
                    texture: TextureId::CapturedScreen,
                    blend_mode: 4,
                    blend_fix: 128,
                    depth_test: true,
                    depth_write: true,
                    uv_clamp: false,
                    vertices: refract_verts,
                });
            }

            let tofs = 2.0 * progress * split;
            let mut bump_add = Vec::new();
            Self::emit_mesh_bump_faces(
                &low_faces,
                MESH_LOWER_MASK,
                false,
                rod_bump_col,
                phase - 0.008,
                phase - 0.008,
                0.1,
                screen_w,
                screen_h,
                &mut bump_add,
            );
            Self::emit_mesh_bump_faces(
                &up_faces,
                MESH_UPPER_MASK,
                false,
                rod_bump_col,
                phase - 0.008,
                phase - 0.008 + tofs,
                0.1,
                screen_w,
                screen_h,
                &mut bump_add,
            );
            if !bump_add.is_empty() {
                out.rod_bump_batches.push(TexturedBatch {
                    texture: TextureId::MenuBump,
                    blend_mode: 5,
                    blend_fix: 128,
                    depth_test: true,
                    depth_write: false,
                    uv_clamp: false,
                    vertices: bump_add,
                });
            }
            return;
        }

        let (faces, obj_x, obj_y) = self.transform_mesh(
            &self.rod_model,
            &self.camera_matrix,
            world,
            [1.0, progress, 1.0],
            false,
        );
        let ref_x = obj_x * 0.9;
        let ref_y = obj_y * 0.9;

        let mut refract_verts = Vec::new();
        for back in [false, true] {
            Self::emit_mesh_refract_faces(
                &faces,
                0xFFFF,
                back,
                col0,
                size,
                ref_x,
                ref_y,
                screen_w,
                screen_h,
                &mut refract_verts,
            );
        }
        if !refract_verts.is_empty() {
            out.rod_refract_batches.push(TexturedBatch {
                texture: TextureId::CapturedScreen,
                blend_mode: 4,
                blend_fix: 128,
                depth_test: true,
                depth_write: true,
                uv_clamp: false,
                vertices: refract_verts,
            });
        }

        let mut bump_verts = Vec::new();
        Self::emit_mesh_bump_faces(
            &faces,
            0xFFFF,
            false,
            rod_bump_col,
            phase - 0.008,
            phase - 0.008,
            0.1,
            screen_w,
            screen_h,
            &mut bump_verts,
        );
        if !bump_verts.is_empty() {
            out.rod_bump_batches.push(TexturedBatch {
                texture: TextureId::MenuBump,
                blend_mode: 5,
                blend_fix: 128,
                depth_test: true,
                depth_write: false,
                uv_clamp: false,
                vertices: bump_verts,
            });
        }
    }

    fn step_and_emit_config_cubes(
        &mut self,
        screen_w: f32,
        screen_h: f32,
        out: &mut MenuRenderList,
    ) {
        self.cfg_cube_timer.step();
        if !self.reduced_motion {
            self.cube_spin = self.cube_spin.wrapping_add(30);
        }

        // MenuConfigCubeState (`0x226BB8`)
        for i in 0..NUM_CFG_CUBES {
            let target = if i == self.config_cursor {
                Self::color_ease(&mut self.cube_tracker, CUBE_COL_SEL, 1);
                self.config_item_alpha[i] = (self.config_item_alpha[i] + 8).min(128);
                self.cube_tracker
            } else {
                self.config_item_alpha[i] = (self.config_item_alpha[i] - 8).max(0);
                CUBE_COL_PLAIN
            };
            Self::color_ease(&mut self.cube_color[i], target, 7);
            self.cube_bias[i] *= 0.95;
        }

        if self.cfg_cube_timer.state == 0 {
            return;
        }

        let s = if self.cfg_cube_timer.duration > 0 {
            (self.cfg_cube_timer.count as f32) / (self.cfg_cube_timer.duration as f32)
        } else {
            0.0
        };
        let cube_cam = mat_unit(); // real 0x352840 identity camera!

        for (i, &cube_pos) in MENU_CUBE_POS.iter().enumerate() {
            let scale = (s + self.cube_bias[i]).max(0.01);
            let ang = i32::from(self.cube_spin) + (i as i32) * 7000;
            let mut md_top = mat_unit();
            md_translate_v4(&mut md_top, cube_pos); // w = 0.0 preserved!
            md_rot_x(&mut md_top, ang);
            md_rot_y(&mut md_top, ang);
            md_rot_z(&mut md_top, ang);

            let (faces, obj_x, obj_y) = self.transform_mesh(
                &self.cube_model,
                &cube_cam,
                &md_top,
                [scale, scale, scale],
                true,
            );
            let ref_x = obj_x * 0.35;
            let ref_y = obj_y * 0.35;

            let mut refract_verts = Vec::new();
            for back in [false, true] {
                Self::emit_mesh_refract_faces(
                    &faces,
                    0x3F,
                    back,
                    self.cube_color[i],
                    200.0,
                    ref_x,
                    ref_y,
                    screen_w,
                    screen_h,
                    &mut refract_verts,
                );
            }
            if !refract_verts.is_empty() {
                out.cube_refract_batches.push(TexturedBatch {
                    texture: TextureId::CapturedScreen,
                    blend_mode: 4,
                    blend_fix: 128,
                    depth_test: false,
                    depth_write: false,
                    uv_clamp: false,
                    vertices: refract_verts,
                });
            }

            let mut bump_verts = Vec::new();
            Self::emit_mesh_bump_faces(
                &faces,
                0x3F,
                false,
                [16, 16, 16, 128],
                0.01,
                0.01,
                0.0,
                screen_w,
                screen_h,
                &mut bump_verts,
            );
            if !bump_verts.is_empty() {
                out.cube_bump_batches.push(TexturedBatch {
                    texture: TextureId::MenuBump,
                    blend_mode: 5,
                    blend_fix: 128,
                    depth_test: false,
                    depth_write: false,
                    uv_clamp: false,
                    vertices: bump_verts,
                });
            }

            let mut refl_verts = Vec::new();
            Self::emit_mesh_refl_faces(
                &faces,
                0x3F,
                true,
                [120, 120, 120, 96],
                screen_w,
                screen_h,
                &mut refl_verts,
            );
            if !refl_verts.is_empty() {
                out.cube_refl_batches.push(TexturedBatch {
                    texture: TextureId::MenuRefa,
                    blend_mode: 5,
                    blend_fix: 128,
                    depth_test: false,
                    depth_write: false,
                    uv_clamp: false,
                    vertices: refl_verts,
                });
            }
        }
    }

    fn transform_mesh(
        &self,
        mdl: &MeshGeometry,
        cam: &Mat4,
        world: &Mat4,
        scale: [f32; 3],
        flat_cube_z: bool,
    ) -> (Vec<TransformedMeshFace>, f32, f32) {
        let m = mat_mul(cam, world);
        let origin_cam = mat_apply(&m, [0.0, 0.0, 0.0, 1.0]);
        let origin_proj = self.project_cam_vertex(origin_cam);
        let obj_x = origin_proj.gs_xyz[0] - 2048.0;
        let obj_y = origin_proj.gs_xyz[1] - 2048.0;

        let mut out = Vec::with_capacity(mdl.nfaces);
        for i in 0..mdl.nfaces {
            let nrm = mat_apply(&m, mdl.norms[i]);
            let mut cam_v = [Vec4::default(); 4];
            let mut proj_v = [GsProjectedVertex::default(); 4];
            let mut uv_v = [[0.0_f32; 2]; 4];

            for k in 0..4 {
                let src = mdl.verts[i * 4 + k];
                let scaled = [src[0] * scale[0], src[1] * scale[1], src[2] * scale[2], 1.0];
                let cv = mat_apply(&m, scaled);
                cam_v[k] = cv;
                let mut pv = self.project_cam_vertex(cv);
                if flat_cube_z {
                    // Retail w=0 flat depth (0x00FFF010 => near-max Z)
                    pv.w = cv[2].max(1.0);
                    pv.q = 1.0 / pv.w;
                }
                proj_v[k] = pv;
                let uv = mdl.uvs[i * 4 + k];
                uv_v[k] = [uv[0], uv[1] * scale[1]];
            }

            let e2x = proj_v[2].gs_xyz[0] - proj_v[0].gs_xyz[0];
            let e2y = proj_v[2].gs_xyz[1] - proj_v[0].gs_xyz[1];
            let e1x = proj_v[1].gs_xyz[0] - proj_v[0].gs_xyz[0];
            let e1y = proj_v[1].gs_xyz[1] - proj_v[0].gs_xyz[1];
            let det = e2x * e1y - e2y * e1x;
            let cull = det <= 0.0 || det.is_nan();

            out.push(TransformedMeshFace {
                normal: nrm,
                cam: cam_v,
                proj: proj_v,
                uv: uv_v,
                cull,
            });
        }
        (out, obj_x, obj_y)
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_mesh_refract_faces(
        faces: &[TransformedMeshFace],
        mask: u32,
        back: bool,
        col: [i32; 4],
        size: f32,
        ref_x: f32,
        ref_y: f32,
        screen_w: f32,
        screen_h: f32,
        out: &mut Vec<PackedTexturedVertex>,
    ) {
        for (i, f) in faces.iter().enumerate() {
            if ((mask >> i) & 1) == 0 || f.cull != back {
                continue;
            }
            let fres = Self::mesh_fresnel(f);
            if fres < 0.0 {
                continue;
            }
            let bx = size * 10.0 * fres * fres * fres * fres;
            let mut bright = bx as i32;
            if fres > 0.9 {
                let t = ((1.0 - fres) * 32768.0 / 0.1) as i32;
                bright = ((bright as f32) * (1.0 - md_cos(t)) * 0.5) as i32;
            }
            let r = (bright + col[0]).clamp(0, 255) as f32;
            let g = (bright + col[1]).clamp(0, 255) as f32;
            let b = (bright + col[2]).clamp(0, 255) as f32;

            let mut qv = [PackedTexturedVertex::default(); 4];
            for (k, dst_v) in qv.iter_mut().enumerate() {
                let pv = &f.proj[k];
                let nx = f.normal[0] * 1000.0 * pv.q;
                let ny = f.normal[1] * 500.0 * pv.q;
                let u_px = (pv.gs_xyz[0] - 2048.0 - ref_x) * 0.95 + ref_x - nx + screen_w * 0.5;
                let v_px = (pv.gs_xyz[1] - 2048.0 - ref_y) * 0.95 + ref_y - ny + screen_h * 0.5;
                let u = clamp_f32(u_px / screen_w, 0.01, 0.99);
                let v = clamp_f32(v_px / screen_h, 0.01, 0.99);
                *dst_v = PackedTexturedVertex {
                    clip_pos: gs_projected_to_webgpu_clip(pv, screen_w, screen_h, 1.0),
                    uv_q: [u, v, 1.0, 0.0],
                    gs_rgba: [r, g, b, 110.0],
                };
            }
            out.push(qv[0]);
            out.push(qv[1]);
            out.push(qv[2]);
            out.push(qv[2]);
            out.push(qv[1]);
            out.push(qv[3]);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_mesh_bump_faces(
        faces: &[TransformedMeshFace],
        mask: u32,
        back: bool,
        col: [i32; 4],
        ofsx: f32,
        ofsy: f32,
        step: f32,
        screen_w: f32,
        screen_h: f32,
        out: &mut Vec<PackedTexturedVertex>,
    ) {
        for (i, f) in faces.iter().enumerate() {
            if ((mask >> i) & 1) == 0 || f.cull != back {
                continue;
            }
            let fx = ofsx + (i as f32) * step;
            let fy = ofsy + (i as f32) * step;
            let mut qv = [PackedTexturedVertex::default(); 4];
            for (k, dst_v) in qv.iter_mut().enumerate() {
                let pv = &f.proj[k];
                *dst_v = PackedTexturedVertex {
                    clip_pos: gs_projected_to_webgpu_clip(pv, screen_w, screen_h, 1.0),
                    uv_q: [f.uv[k][0] + fx, f.uv[k][1] + fy, pv.q, 0.0],
                    gs_rgba: [col[0] as f32, col[1] as f32, col[2] as f32, col[3] as f32],
                };
            }
            out.push(qv[0]);
            out.push(qv[1]);
            out.push(qv[2]);
            out.push(qv[2]);
            out.push(qv[1]);
            out.push(qv[3]);
        }
    }

    fn emit_mesh_refl_faces(
        faces: &[TransformedMeshFace],
        mask: u32,
        back: bool,
        col: [i32; 4],
        screen_w: f32,
        screen_h: f32,
        out: &mut Vec<PackedTexturedVertex>,
    ) {
        for (i, f) in faces.iter().enumerate() {
            if ((mask >> i) & 1) == 0 || f.cull != back {
                continue;
            }
            let mut qv = [PackedTexturedVertex::default(); 4];
            for (k, dst_v) in qv.iter_mut().enumerate() {
                let cv = f.cam[k];
                let len = (cv[0] * cv[0] + cv[1] * cv[1] + cv[2] * cv[2])
                    .sqrt()
                    .max(1e-5);
                let n = [cv[0] / len, cv[1] / len, cv[2] / len];
                let d =
                    (2.0 * (n[0] * f.normal[0] + n[1] * f.normal[1] + n[2] * f.normal[2])).abs();
                let rx = n[0] + f.normal[0] * d;
                let ry = n[1] + f.normal[1] * d;
                let u = (rx + 1.0) * 0.5;
                let v = (ry + 1.0) * 0.25;
                let pv = &f.proj[k];
                *dst_v = PackedTexturedVertex {
                    clip_pos: gs_projected_to_webgpu_clip(pv, screen_w, screen_h, 1.0),
                    uv_q: [u, v, 1.0, 0.0],
                    gs_rgba: [col[0] as f32, col[1] as f32, col[2] as f32, col[3] as f32],
                };
            }
            out.push(qv[0]);
            out.push(qv[1]);
            out.push(qv[2]);
            out.push(qv[2]);
            out.push(qv[1]);
            out.push(qv[3]);
        }
    }

    fn mesh_fresnel(f: &TransformedMeshFace) -> f32 {
        let c = f.cam[0];
        let len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
        if len <= 1e-6 {
            return -1.0;
        }
        let d = (c[0] * f.normal[0] + c[1] * f.normal[1] + c[2] * f.normal[2]) / len;
        1.0 - d.abs()
    }

    fn emit_vignette(&self, screen_w: f32, screen_h: f32, out: &mut MenuRenderList) {
        let a = self.vig_timer.interp(128).clamp(0, 128) as f32;
        if a <= 0.0 {
            return;
        }
        let vig_pt = |ang: i32, s: f32, clamp_edge: bool| -> [f32; 2] {
            let mut px = 266.0 + md_sin(ang) * 74.0 * s;
            let mut py = screen_h * 0.5 + md_cos(ang) * 37.0 * s;
            if clamp_edge {
                px = clamp_f32(px, 0.0, screen_w);
                py = clamp_f32(py, 0.0, screen_h);
            }
            pixel_to_ndc(px, py, screen_w, screen_h)
        };

        for seg in 0..16 {
            let ang = seg << 12;
            let ang2 = ang + 4096;
            let p_in0 = vig_pt(ang, 1.0, false);
            let p_in1 = vig_pt(ang2, 1.0, false);
            let p_mid0 = vig_pt(ang, 1.5, false);
            let p_mid1 = vig_pt(ang2, 1.5, false);
            let p_out0 = vig_pt(ang, 10.0, true);
            let p_out1 = vig_pt(ang2, 10.0, true);

            let mk = |xy: [f32; 2], alpha: f32| PackedColorVertex {
                clip_pos: [xy[0], xy[1], 0.0, 1.0],
                gs_rgba: [0.0, 0.0, 0.0, alpha],
            };
            // Inner -> mid ring
            out.vignette_vertices.extend_from_slice(&[
                mk(p_in0, 0.0),
                mk(p_in1, 0.0),
                mk(p_mid0, a),
                mk(p_mid0, a),
                mk(p_in1, 0.0),
                mk(p_mid1, a),
                // Mid -> outer ring
                mk(p_mid0, a),
                mk(p_mid1, a),
                mk(p_out0, a),
                mk(p_out0, a),
                mk(p_mid1, a),
                mk(p_out1, a),
            ]);
        }
    }

    fn step_config_state_machine(&mut self) {
        self.cfg_anim.step();
        let count = self.cfg_anim.count;
        if self.cfg_anim.state == 1 {
            if count == self.cfg_dur40b {
                self.cfg_cube_timer.open();
            }
            return;
        }
        if self.cfg_anim.state != 3 {
            if self.cfg_anim.state == 2 {
                let fold = (self.standard.fps() as i32 * 31400) / 60;
                self.config_phase += 310;
                if self.config_phase > fold {
                    self.config_phase -= 2 * fold;
                }
            }
            return;
        }

        if self.cfg_anim.duration - count == self.cfg_dur10 {
            self.cfg_cube_timer.close();
        }
        if count == self.cfg_dur40b + self.cfg_dur40 {
            self.vig_timer.open();
        } else if count == self.cfg_dur40b {
            self.bg_timer.close();
        } else if count == 1 && self.carousel_timer.state == 2 {
            self.carousel_timer.close();
            for s in &mut self.ring {
                s.progress = 0.0;
            }
        }
    }

    fn step_main_menu_anim(&mut self) {
        if self.cfg_anim.state != 0 {
            return;
        }
        if self.main_menu_anim.state == 0 {
            if self.fade_mode == 2 && self.fade_alpha >= 128 - self.cfg_dur10 {
                self.main_menu_anim.open();
            }
        } else if self.main_menu_anim.state == 2 && self.fade_mode == 3 && self.fade_alpha == 128 {
            self.main_menu_anim.close();
        }
        self.main_menu_anim.step();
    }

    fn step_carousel(&mut self) {
        self.carousel_timer.step();
        self.ring_offset = usize::from(self.clock.hour % 12);

        let target_tilt = ((self.ring_offset << 16) / NUM_RODS) as i32 as i16;
        if self.ring_spin_y > -201 && self.ring_spin_y < 201 {
            self.ring_tilt_z = target_tilt;
        } else {
            let diff = target_tilt.wrapping_sub(self.ring_tilt_z);
            self.ring_tilt_z = self.ring_tilt_z.wrapping_add(((diff as f32) * 0.1) as i16);
        }

        let target_spin = (self.clock.seconds_float() * (65536.0 / 60.0)) as i32 as i16;
        let spin_diff = target_spin.wrapping_sub(self.ring_spin_y);
        self.ring_spin_y = self
            .ring_spin_y
            .wrapping_add(((spin_diff as f32) * 0.1) as i16);

        self.ring_split_max = clamp_f32(1.0 - self.clock.minutes_float() / 60.0, 0.0, 1.0);

        for i in 0..NUM_RODS {
            if i == self.ring_offset {
                Self::color_ease(&mut self.ring[i].col0, CFG_COL_FRONT, 1);
                Self::color_ease(&mut self.ring[i].col1, CFG_COL_RING_B, 1);
            } else {
                Self::color_ease(&mut self.ring[i].col0, CFG_COL_BODY, 1);
                Self::color_ease(&mut self.ring[i].col1, CFG_COL_EDGE, 1);
            }
        }
        Self::color_ease(&mut self.ring_col_a, CFG_COL_FIXED, 1);
        Self::color_ease(&mut self.ring_col_b, CFG_COL_RING_A, 1);

        let prog = if self.carousel_timer.duration > 0 {
            ((self.carousel_timer.count * 128 / self.carousel_timer.duration) as f32) * 0.007_812_5
        } else {
            0.0
        };

        for (i, slot) in self.ring.iter_mut().enumerate() {
            if i != self.ring_offset {
                slot.split = 0.0;
            } else if slot.split < self.ring_split_max {
                slot.split = (slot.split + 0.004).min(self.ring_split_max);
            } else {
                // Instant snap down when minute increases (`menuconfig.c:1759`)
                slot.split = self.ring_split_max;
            }
            slot.progress = prog;
        }
    }

    fn step_fade(&mut self) {
        if self.fade_mode == 0 {
            return;
        }
        if self.fade_mode < 3 {
            self.fade_alpha += 1;
            if self.fade_alpha > 128 {
                self.fade_alpha = 128;
                self.fade_mode = 0;
            }
        } else if self.fade_mode == 3 {
            self.fade_alpha = (self.fade_alpha - 1).max(0);
        }
    }

    fn build_ui_snapshot(&self, out: &mut MenuRenderList) {
        let screen = self.current_screen();
        out.ui.screen = Some(screen);
        // Items fade in during the last `cfgDur10` frames of the open transition,
        // but `dispatch_action` keeps ignoring input until `cfgAnim` reaches state
        // 2. Publish that gap so the shell can disable the controls instead of
        // dropping pointer clicks (`menu.c:0x227D08`).
        out.ui.input_ready = self.input_ready();
        out.ui.date_text = self.clock.format_date();
        out.ui.time_text = self.clock.format_time();
        if self.confirm_cancel_swap {
            out.ui.hint_confirm_label = "Cross: Enter";
            out.ui.hint_cancel_label = "Circle: Back";
        } else {
            out.ui.hint_confirm_label = "Circle: Enter";
            out.ui.hint_cancel_label = "Cross: Back";
        }
        out.ui.show_options_hint = matches!(
            screen,
            MenuScreenState::MainMenu | MenuScreenState::SystemConfigList
        ) && (screen != MenuScreenState::SystemConfigList
            || self.config_cursor == 0);

        match screen {
            MenuScreenState::MainMenu => {
                if self.main_menu_anim.state == 2 {
                    let alpha = ((self.main_menu_anim.interp(128) * self.fade_alpha) / 128)
                        .clamp(0, 128) as u8;
                    out.ui.items.push(MenuUiItem {
                        label: "Browser".to_owned(),
                        value_text: String::new(),
                        selected: self.main_menu_cursor == 0,
                        alpha,
                        provenance: MenuEntryProvenance::ApproximatedSubscreen,
                    });
                    out.ui.items.push(MenuUiItem {
                        label: "System Configuration".to_owned(),
                        value_text: String::new(),
                        selected: self.main_menu_cursor == 1,
                        alpha,
                        provenance: MenuEntryProvenance::SourcedReconstruction,
                    });
                    for (idx, title) in self.custom_main_entries.iter().enumerate() {
                        out.ui.items.push(MenuUiItem {
                            label: title.clone(),
                            value_text: String::new(),
                            selected: self.main_menu_cursor == 2 + idx,
                            alpha,
                            provenance: MenuEntryProvenance::CleanRoomExtension,
                        });
                    }
                }
            }
            MenuScreenState::BrowserApproxPanel => {
                out.ui.title = "Browser (Clean-Room Approximation)".to_owned();
                out.ui.items.push(MenuUiItem {
                    label: "Memory Card / Disc icon grid (mcman) is outside osdbits Module U scope"
                        .to_owned(),
                    value_text: "Press Back to return".to_owned(),
                    selected: true,
                    alpha: 128,
                    provenance: MenuEntryProvenance::ApproximatedSubscreen,
                });
            }
            MenuScreenState::VersionApproxPanel => {
                out.ui.title = "Version Information (Clean-Room Inspector)".to_owned();
                out.ui.items.push(MenuUiItem {
                    label: "Reconstruction".to_owned(),
                    value_text: "osdbits-wasm (clean-room WebGPU + Rust WASM)".to_owned(),
                    selected: true,
                    alpha: 128,
                    provenance: MenuEntryProvenance::ApproximatedSubscreen,
                });
            }
            _ => {
                // System Configuration
                let v = (self.cfg_anim.count - (self.cfg_dur40b + self.cfg_dur40))
                    .clamp(0, self.cfg_dur10);
                let page_alpha = if self.cfg_dur10 > 0 {
                    (((v << 7) / self.cfg_dur10) * self.fade_alpha / 128).clamp(0, 128)
                } else {
                    0
                };
                out.ui.title = "System Configuration".to_owned();
                if self.config_mode != 1 && page_alpha >= 128 {
                    let pulse = (128.0 * ((self.config_phase as f32) / 10_000.0).sin()).abs();
                    out.ui.page_marker_alpha = pulse.clamp(0.0, 128.0) as u8;
                }

                let labels = [
                    "Clock Adjustment",
                    "Screen Size",
                    "Digital Out (Optical)",
                    "Component Video Out",
                    "Language",
                ];
                let values = [
                    self.clock.format_full(),
                    self.screen_aspect.label().to_owned(),
                    if self.digital_out_on { "On" } else { "Off" }.to_owned(),
                    if self.component_rgb {
                        "RGB"
                    } else {
                        "Y Cb/Pb Cr/Pr"
                    }
                    .to_owned(),
                    self.language.label().to_owned(),
                ];

                for i in 0..NUM_CFG_CUBES {
                    let item_a =
                        ((self.config_item_alpha[i] * page_alpha) / 128).clamp(0, 128) as u8;
                    if item_a > 0 || (self.config_mode == 1 && i == self.config_cursor) {
                        out.ui.items.push(MenuUiItem {
                            label: labels[i].to_owned(),
                            value_text: values[i].clone(),
                            selected: i == self.config_cursor,
                            alpha: item_a,
                            provenance: MenuEntryProvenance::SourcedReconstruction,
                        });
                    }
                }

                if self.config_mode == 1 && self.config_cursor == 0 {
                    out.ui.clock_edit_cursor = self.clock_edit_cursor as u8;
                    out.ui.clock_edit_fields = Some([
                        format!("{:04}", self.clock_edit_vals[0]),
                        format!("{:02}", self.clock_edit_vals[1]),
                        format!("{:02}", self.clock_edit_vals[2]),
                        format!("{:2}", self.clock_edit_vals[3]),
                        format!("{:02}", self.clock_edit_vals[4]),
                        format!("{:02}", self.clock_edit_vals[5]),
                    ]);
                }
            }
        }
    }

    fn project_cam_vertex(&self, cam_v: Vec4) -> GsProjectedVertex {
        let b = mat_apply(&self.view_screen_matrix, cam_v);
        let w = b[3];
        let q = if w.abs() > 1e-6 { 1.0 / w } else { 0.0 };
        let gx = b[0] * q;
        let gy = b[1] * q;
        let gz = b[2] * q;
        GsProjectedVertex {
            gs_xyz: [gx, gy, gz],
            fixed_xyz: [
                clamp_f32(gx * 16.0, -2_147_483_520.0, 2_147_483_520.0) as i32,
                clamp_f32(gy * 16.0, -2_147_483_520.0, 2_147_483_520.0) as i32,
                clamp_f32(gz, 0.0, 16_777_215.0) as i32,
            ],
            q,
            w,
        }
    }

    fn color_ease(dst: &mut [i32; 4], src: [i32; 4], rate: i32) {
        for k in 0..4 {
            if dst[k] < src[k] {
                dst[k] = (dst[k] + rate).min(src[k]);
            } else if dst[k] > src[k] {
                dst[k] = (dst[k] - rate).max(src[k]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_leap_year_and_midnight_rollover_match_menutext_rules() {
        assert!(MenuClock::is_leap_year(2000));
        assert!(MenuClock::is_leap_year(2024));
        assert!(!MenuClock::is_leap_year(2026));
        assert!(MenuClock::is_leap_year(2028));
        assert_eq!(MenuClock::days_in_month(2024, 2), 29);
        assert_eq!(MenuClock::days_in_month(2026, 2), 28);

        let mut clk = MenuClock::new(2024, 2, 28, 23, 59, 59);
        clk.advance_ms(1000.0);
        assert_eq!(
            (clk.year, clk.month, clk.day, clk.hour, clk.minute, clk.second),
            (2024, 2, 29, 0, 0, 0)
        );

        let mut clk_nye = MenuClock::new(2026, 12, 31, 23, 59, 59);
        clk_nye.advance_ms(1000.0);
        assert_eq!(
            (
                clk_nye.year,
                clk_nye.month,
                clk_nye.day,
                clk_nye.hour,
                clk_nye.minute,
                clk_nye.second
            ),
            (2027, 1, 1, 0, 0, 0)
        );
    }

    #[test]
    fn orbs_are_deterministic_wrap_150_subframes_and_converge_scatter() {
        let clk = MenuClock::new(2026, 10, 31, 9, 15, 30);
        let mut s1 = MenuSim::new(VideoStandard::Ntsc, clk, OsdLanguage::English, 42, true);
        let mut s2 = MenuSim::new(VideoStandard::Ntsc, clk, OsdLanguage::English, 42, true);
        let mut r1 = MenuRenderList::default();
        let mut r2 = MenuRenderList::default();
        let mut a1 = Vec::new();
        let mut a2 = Vec::new();

        for _ in 0..165 {
            s1.step(&mut r1, &mut a1);
            s2.step(&mut r2, &mut a2);
        }

        // After 165 frames (> 150 subframes = 50 * 3), all 7 orb rings have wrapped!
        assert!(s1.orbs.iter().all(|o| o.wrapped));
        assert_eq!(s1.fade_mode, 0);
        assert_eq!(s1.fade_alpha, 128);
        assert_eq!(r1.orb_trail_vertices.len(), r2.orb_trail_vertices.len());
        assert!(!r1.orb_trail_vertices.is_empty());
        assert_eq!(
            r1.orb_trail_vertices[0].clip_pos[0].to_bits(),
            r2.orb_trail_vertices[0].clip_pos[0].to_bits()
        );
        // Provenance tags explicitly distinguish Browser (Approximated) vs System Config (Sourced) vs Custom
        assert_eq!(r1.ui.items.len(), 3);
        assert_eq!(
            r1.ui.items[0].provenance,
            MenuEntryProvenance::ApproximatedSubscreen
        );
        assert_eq!(
            r1.ui.items[1].provenance,
            MenuEntryProvenance::SourcedReconstruction
        );
        assert_eq!(
            r1.ui.items[2].provenance,
            MenuEntryProvenance::CleanRoomExtension
        );
    }

    #[test]
    fn input_readiness_gates_items_that_fade_in_before_the_transition_finishes() {
        let clk = MenuClock::new(2026, 10, 31, 21, 45, 15);
        let mut sim = MenuSim::new(VideoStandard::Ntsc, clk, OsdLanguage::English, 1, true);
        let mut render = MenuRenderList::default();
        let mut audio = Vec::new();

        // During the opening fade-in the main menu refuses input.
        assert!(!sim.input_ready());

        let mut main_menu_ready_frames = 0;
        for _ in 0..135 {
            sim.step(&mut render, &mut audio);
            if render.ui.input_ready {
                main_menu_ready_frames += 1;
            }
            // Never advertise clickable main-menu entries while input is ignored.
            if !render.ui.input_ready {
                assert!(render.ui.items.is_empty());
            }
        }
        assert!(main_menu_ready_frames > 0);
        assert_eq!(sim.current_screen(), MenuScreenState::MainMenu);
        assert!(sim.input_ready());

        sim.dispatch_action(MenuAction::Down, &mut audio);
        sim.dispatch_action(MenuAction::Confirm, &mut audio);
        assert_eq!(
            sim.current_screen(),
            MenuScreenState::SystemConfigTransitionIn
        );
        assert!(
            !sim.input_ready(),
            "config transition-in must swallow input"
        );

        // The config items fade in over the last frames of the open transition, so
        // there is a window where rows are visible but the core still ignores input.
        let mut visible_but_not_ready = 0;
        for _ in 0..90 {
            sim.step(&mut render, &mut audio);
            if sim.current_screen() == MenuScreenState::SystemConfigList {
                assert!(
                    render.ui.input_ready,
                    "config list must be interactive as soon as it is displayed"
                );
                assert!(!render.ui.items.is_empty());
            } else if !render.ui.items.is_empty() && !render.ui.input_ready {
                visible_but_not_ready += 1;
            }
        }
        assert!(
            visible_but_not_ready > 0,
            "expected a fade-in window where rows are drawn before input is accepted"
        );
        assert_eq!(sim.current_screen(), MenuScreenState::SystemConfigList);
        assert!(sim.input_ready());

        // Closing the config screen blocks input again until the choreography ends.
        sim.dispatch_action(MenuAction::Cancel, &mut audio);
        assert_eq!(
            sim.current_screen(),
            MenuScreenState::SystemConfigTransitionOut
        );
        assert!(!sim.input_ready());
        for _ in 0..90 {
            sim.step(&mut render, &mut audio);
        }
        assert_eq!(sim.current_screen(), MenuScreenState::MainMenu);
        assert!(sim.input_ready());
    }

    #[test]
    fn system_config_90_frame_choreography_rod_split_and_clock_editor_work() {
        let clk = MenuClock::new(2026, 10, 31, 3, 30, 15);
        let mut sim = MenuSim::new(VideoStandard::Ntsc, clk, OsdLanguage::English, 1, true);
        let mut render = MenuRenderList::default();
        let mut audio = Vec::new();

        // Advance past initial 128-frame fade-in so main menu is interactive
        for _ in 0..135 {
            sim.step(&mut render, &mut audio);
        }
        assert_eq!(sim.current_screen(), MenuScreenState::MainMenu);

        // Move down to "System Configuration" (item 1) and confirm
        sim.dispatch_action(MenuAction::Down, &mut audio);
        sim.dispatch_action(MenuAction::Confirm, &mut audio);
        assert_eq!(
            sim.current_screen(),
            MenuScreenState::SystemConfigTransitionIn
        );

        // Step 90 frames of cfgAnim opening choreography
        for _ in 0..90 {
            sim.step(&mut render, &mut audio);
        }
        assert_eq!(sim.current_screen(), MenuScreenState::SystemConfigList);
        assert!(
            render.kabe_tunnel_batch.is_some(),
            "expected KABE tunnel in System Config"
        );
        assert!(
            !render.rod_refract_batches.is_empty(),
            "expected 12 crystal clock rods"
        );
        assert_eq!(
            render.cube_refract_batches.len(),
            5,
            "expected 5 config glass cubes"
        );

        // Let front rod split grow for 150 frames toward 1 - 30/60 = 0.5
        for _ in 0..150 {
            sim.step(&mut render, &mut audio);
        }
        let front = sim.ring_offset;
        assert!(
            (sim.ring[front].split - sim.ring_split_max).abs() < 0.02,
            "expected front rod split near {}, got {}",
            sim.ring_split_max,
            sim.ring[front].split
        );

        // Open Clock Adjustment (item 0): orbs sink (`menu_scale_target == 0.0`) & seconds zeroed!
        sim.dispatch_action(MenuAction::Confirm, &mut audio);
        assert_eq!(
            sim.current_screen(),
            MenuScreenState::SystemConfigEditingClock
        );
        assert_eq!(sim.menu_scale_target, 0.0);
        assert_eq!(sim.clock.second, 0);

        for _ in 0..40 {
            sim.step(&mut render, &mut audio);
        }
        assert!(sim.menu_scale < 0.1, "expected orbs to sink into center");

        // Navigate right to minute field (index 4: Y=0, M=1, D=2, h=3, m=4), step Up (30 -> 45), verify split snaps down!
        for _ in 0..4 {
            sim.dispatch_action(MenuAction::Right, &mut audio);
        }
        for _ in 0..15 {
            sim.dispatch_action(MenuAction::Up, &mut audio);
        }
        sim.step(&mut render, &mut audio);
        assert_eq!(sim.clock.minute, 45);
        assert!((sim.ring[front].split - 0.25).abs() < 0.02);

        // Confirm Clock Adjustment: cube 0 receives -0.1 scale kick and orbs restore (`menu_scale_target == 1.0`)!
        sim.dispatch_action(MenuAction::Confirm, &mut audio);
        assert!((sim.cube_bias[0] - (-0.1)).abs() < 1e-6);
        assert_eq!(sim.menu_scale_target, 1.0);

        // Leave System Configuration and step 90 frames back to Main Menu
        sim.dispatch_action(MenuAction::Cancel, &mut audio);
        assert_eq!(
            sim.current_screen(),
            MenuScreenState::SystemConfigTransitionOut
        );
        for _ in 0..90 {
            sim.step(&mut render, &mut audio);
        }
        assert_eq!(sim.current_screen(), MenuScreenState::MainMenu);
    }
}
