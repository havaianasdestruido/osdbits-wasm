//! The opening's camera state machine and motion integration.
//!
//! Ported from `ProcessOpeningAnimation` (`0x215fd0`), `OpeningInitAnimation`
//! (`0x215f18`), `InitIllegalDisc` (`0x215f68`), `InitOpeningScene` and
//! `InitIllegalScene`. The arithmetic — semi-implicit integration with the
//! odd `2*a + b` / `2` half-step form, PAL's `timestep = 1.2` / `fps = 50`,
//! the roll wrap, and the `up = (sin r, cos r, 0)` camera basis — is
//! reproduced exactly; only the *sources* of hardware state (disc presence,
//! boot parameter) are simulated, and that is labelled everywhere.

use crate::math::{camera_matrix, Mat4, Vec4, PI, TAU};

/// `openingStateLevels`, real `0x27a0d8`.
pub const OPENING_STATE_LEVELS: [f32; 8] = [16.0, 56.0, 104.0, 320.0, 672.0, 800.0, 1160.0, 1160.0];

/// Simulated hardware state. The real OSDSYS reads this from CDVD; the
/// browser has no disc drive, so the user (or a test) selects it and the UI
/// labels it "simulated".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HwState {
    NoDisc,
    /// A PlayStation 2 disc is present and detected (`discType == 1`).
    DiscPs2,
    /// A disc the original rejects (the illegal-disc screen).
    DiscIllegal,
}

impl HwState {
    pub fn has_disc(self) -> bool {
        !matches!(self, HwState::NoDisc)
    }

    /// `GetDiscType()`; 0 = unknown, 1 = PS2 format in the reconstruction.
    pub fn disc_type(self) -> i32 {
        match self {
            HwState::NoDisc => 0,
            HwState::DiscPs2 => 1,
            HwState::DiscIllegal => 2,
        }
    }

    pub fn id(self) -> u32 {
        match self {
            HwState::NoDisc => 0,
            HwState::DiscPs2 => 1,
            HwState::DiscIllegal => 2,
        }
    }

    pub fn from_id(id: u32) -> Self {
        match id {
            1 => HwState::DiscPs2,
            2 => HwState::DiscIllegal,
            _ => HwState::NoDisc,
        }
    }
}

/// Scene-relevant events the camera emits (sound dispatch, scene end). The
/// browser shell maps these onto its own audio and lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraEvent {
    /// `OSDDispatch(20500, record, 0, 0)` — start a BGM/boot record.
    PlayRecord(i32),
    /// `OSDDispatch(20501, 0, 0, fade)` — fade the current record out.
    FadeRecord(i32),
    /// `OSDDispatch2(1, 20501, 6, 0, 15)` in state 6.
    EndSceneSound,
}

#[derive(Clone, Debug)]
pub struct Camera {
    pub position: Vec4,
    pub fwd: Vec4,
    pub up: Vec4,
    pub light1: Vec4,
    pub light2: Vec4,
    pub light3: Vec4,
    pub position_accel1: Vec4,
    pub position_accel2: Vec4,
    pub position_speed: Vec4,
    pub rotation_accel: Vec4,
    pub rotation_speed: Vec4,
    pub rotation: f32,

    pub opening_state: i32,
    pub opening_go: i32,
    pub boot_request: i32,
    pub opening_type: i32,
    pub next_opening_type: i32,
    pub opening_end_flag: i32,
    pub opening_end_frame: i32,
    pub draw_black_bars: i32,

    /// Simulated boot parameter, real `*(0x1f0010)` (values 100..116).
    pub boot_param: i32,
    pub boot_param_c: i32,
    pub boot_param2: i32,
    /// Simulated CDVD state.
    pub hw: HwState,
    /// `discReady` once the (simulated) poller has answered.
    pub disc_ready: bool,
    pub pal: bool,
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}

impl Camera {
    pub fn new() -> Self {
        Camera {
            position: [0.0; 4],
            fwd: [0.0; 4],
            up: [0.0; 4],
            light1: [0.0; 4],
            light2: [0.0; 4],
            light3: [0.0; 4],
            position_accel1: [0.0; 4],
            position_accel2: [0.0; 4],
            position_speed: [0.0; 4],
            rotation_accel: [0.0; 4],
            rotation_speed: [0.0; 4],
            rotation: 0.0,
            opening_state: 0,
            opening_go: 0,
            boot_request: 1,
            opening_type: 0,
            next_opening_type: 0,
            opening_end_flag: 0,
            opening_end_frame: 0,
            draw_black_bars: 0,
            boot_param: 100,
            boot_param_c: 0,
            boot_param2: 0,
            hw: HwState::NoDisc,
            disc_ready: false,
            pal: false,
        }
    }

    pub fn fps(&self) -> i32 {
        if self.pal {
            50
        } else {
            60
        }
    }

    fn timestep(&self) -> f32 {
        if self.pal {
            1.2
        } else {
            1.0
        }
    }

    /// `InitOpeningScene` (the towers scene's fixed starting state).
    pub fn init_opening_scene(&mut self) {
        self.up = [0.0, 1.0, 0.0, 1.0];
        self.fwd = [0.0, 0.0, 1.0, 1.0];
        self.position = [0.0, 0.0, 16.0, 0.0];
        self.rotation = -0.12;
        self.light1 = [0.0, 0.0, -1.0, 0.0];
        self.light2 = [0.5, 0.5, 0.0, 0.0];
        self.light3 = [-0.5, -0.5, 0.0, 0.0];
    }

    /// `OpeningInitAnimation` (`0x215f18`): drift across the field.
    pub fn init_animation(&mut self) {
        self.opening_state = 0;
        self.position_speed = [0.0; 4];
        self.rotation_speed = [0.0; 4];
        self.position_speed[2] = 0.04;
        self.rotation_speed[2] = 0.001;
        self.position_accel1 = [0.0; 4];
        self.position_accel2 = [0.0; 4];
        self.rotation_accel = [0.0; 4];
        self.opening_go = 0;
    }

    /// `InitIllegalDisc` (`0x215f68`): state 4 at height 672, rising and
    /// braking.
    pub fn init_illegal_disc(&mut self) {
        self.position[2] = 672.0;
        self.opening_state = 4;
        self.position_accel2[2] = -0.0178;
        self.position_speed[2] = 2.159_994;
        self.rotation_speed[2] = 0.004_62;
        self.position_accel2[0] = 0.0;
        self.position_accel2[1] = 0.0;
        self.position_speed[0] = 0.0;
        self.position_speed[1] = 0.0;
        self.rotation_accel = [0.0; 4];
        self.rotation_speed[0] = 0.0;
        self.rotation_speed[1] = 0.0;
        self.opening_go = 0;
    }

    /// `InitIllegalScene` (`0x21b570`): camera/lights for the red scene.
    pub fn init_illegal_scene(&mut self) {
        self.position = [0.0, 0.0, 320.0, 0.0];
        self.fwd = [0.0, -0.03, 1.0, 1.0];
        self.up = [0.0, 1.0, 0.0, 1.0];
        self.rotation = 0.0;
        self.init_illegal_disc();
        self.light1 = [0.0, 0.0, -1.0, 0.0];
        self.light2 = [0.5, 0.5, 0.0, 0.0];
        self.light3 = [-0.5, -0.5, 0.0, 0.0];
    }

    /// The original's `HasDisc()`: the sim treats the selected hardware state
    /// as "disc answers the poller immediately".
    pub fn has_disc(&self) -> bool {
        self.disc_ready && self.hw.has_disc()
    }

    pub fn disc_type(&self) -> i32 {
        if self.disc_ready {
            self.hw.disc_type()
        } else {
            0
        }
    }

    /// World -> view matrix for this frame, using the same basis the
    /// reconstruction builds (`upDir = (sin r, cos r, 0)`).
    pub fn view_matrix(&self) -> Mat4 {
        camera_matrix(&self.position, &self.fwd, &self.up)
    }

    /// Advance one frame. Returns the opening type the caller should switch
    /// to (`openingType` in the C); `events` collects sound dispatches.
    ///
    /// `frame` is the reconstruction's `frameCount` (1-based).
    pub fn step(&mut self, frame: u32, events: &mut Vec<CameraEvent>) -> i32 {
        let timestep = self.timestep();
        let fps = self.fps();

        if OPENING_STATE_LEVELS[self.opening_state as usize] < self.position[2] {
            self.opening_state += 1;
        }
        let mut ty = self.opening_type;

        match self.opening_state {
            1 => {
                if self.has_disc() {
                    self.rotation_speed[2] = 0.0004;
                    if (frame as i32) < fps * 20 / 6 {
                        self.position_accel2[2] = -0.000_14;
                    } else {
                        self.position_accel2[2] = 0.000_025;
                    }
                    if self.disc_type() != 0 {
                        self.position_accel2[2] = 0.003;
                        self.opening_go = 1;
                        self.opening_state += 1;
                    } else if frame as i32 > fps * 20 {
                        self.disc_ready = false; // give up on the disc
                    }
                } else {
                    self.position_accel1[2] = 0.000_000_4;
                    // Only these boot params proceed without a disc.
                    if matches!(
                        self.boot_param,
                        100 | 106 | 107 | 108 | 109 | 110 | 111 | 112 | 114 | 115 | 116
                    ) && frame as i32 > fps * 2
                    {
                        self.opening_go = 1;
                        self.opening_state += 1;
                    }
                }
            }
            2 => {
                if frame as i32 > fps * 10 {
                    self.opening_go = 1;
                }
                if self.opening_go != 0 {
                    if self.boot_request == 1 {
                        if self.boot_param_c == 0 {
                            // BootLatchClear() is always true in the sim.
                            events.push(CameraEvent::PlayRecord(1));
                        } else if self.has_disc() && self.disc_type() == 1 {
                            events.push(CameraEvent::FadeRecord(15));
                        } else {
                            match self.boot_param {
                                106 | 107 | 114 | 115 => events.push(CameraEvent::FadeRecord(15)),
                                108..=110 => {
                                    events.push(CameraEvent::FadeRecord(15));
                                    events.push(CameraEvent::PlayRecord(7));
                                }
                                _ => events.push(CameraEvent::PlayRecord(1)),
                            }
                        }
                        self.boot_request = -1;
                    }
                    if self.has_disc() || self.disc_type() != 0 {
                        self.position_accel1[2] = 0.0004;
                        self.rotation_accel[2] = 0.000_08;
                    } else {
                        self.position_accel2[2] = 0.0099;
                        self.rotation_accel[2] = 0.000_195;
                    }
                    self.position_accel2[0] = 0.0;
                    self.position_accel2[1] = 0.0;
                    self.rotation_accel[0] = 0.0;
                    self.rotation_accel[1] = 0.0;
                }
            }
            3 => {
                ty += 1;
            }
            6 => {
                self.position_accel1 = [0.0; 4];
                self.position_accel2 = [0.0; 4];
                self.position_speed = [0.0; 4];
                self.rotation_accel = [0.0; 4];
                if self.boot_param_c == 0 {
                    match self.boot_param {
                        100 | 106 | 107 | 108 | 109 | 110 | 111 | 112 | 115 => {
                            self.opening_end_flag = 1;
                            if self.opening_end_frame == 0 {
                                events.push(CameraEvent::EndSceneSound);
                                self.opening_end_frame = frame as i32;
                            } else if (frame as i32) > self.opening_end_frame + 128 {
                                ty = 2;
                            }
                        }
                        114 => {
                            if self.boot_param2 > 0 {
                                self.opening_end_flag = 1;
                                if self.opening_end_frame == 0 {
                                    self.opening_end_frame = frame as i32;
                                } else if (frame as i32) > self.opening_end_frame + 128 {
                                    ty = 2;
                                }
                            }
                        }
                        _ => {
                            if self.opening_end_flag != 0
                                && (frame as i32) > self.opening_end_frame + 128
                            {
                                ty = 2;
                            }
                        }
                    }
                }
            }
            7 => {
                self.init_animation();
                ty = 2;
            }
            _ => {}
        }

        // --- motion integration (verbatim) ---
        for i in 0..3 {
            self.rotation_speed[i] += (2.0 * self.rotation_accel[i] + 0.0) * 0.5 * timestep;
        }
        for i in 0..3 {
            self.position_speed[i] +=
                (2.0 * self.position_accel2[i] + self.position_accel1[i]) * 0.5 * timestep;
        }
        self.position_accel2[2] += self.position_accel1[2] * timestep;
        self.rotation += (2.0 * self.rotation_speed[2] + self.rotation_accel[2]) * 0.5 * timestep;
        for i in 0..3 {
            self.position[i] +=
                (2.0 * self.position_speed[i] + self.position_accel2[i]) * 0.5 * timestep;
        }

        if self.rotation > PI {
            self.rotation -= TAU;
        }
        if self.rotation < -PI {
            self.rotation += TAU;
        }
        self.up[0] = self.rotation.sin();
        self.up[1] = self.rotation.cos();
        self.up[3] = 1.0;
        self.fwd[3] = 1.0;

        // Guard against non-finite state injected by a corrupt host call.
        if !self.position.iter().all(|v| v.is_finite()) {
            self.position = [0.0, 0.0, 16.0, 0.0];
            self.position_speed = [0.0; 4];
            self.position_accel1 = [0.0; 4];
            self.position_accel2 = [0.0; 4];
        }
        if !self.rotation.is_finite() {
            self.rotation = 0.0;
        }

        ty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boot_camera() -> Camera {
        let mut c = Camera::new();
        c.init_opening_scene();
        c.init_animation();
        c.opening_type = 0;
        c.next_opening_type = 0;
        c.hw = HwState::NoDisc;
        c
    }

    #[test]
    fn opening_scene_initial_state_matches_reconstruction() {
        let mut c = Camera::new();
        c.init_opening_scene();
        assert_eq!(c.position, [0.0, 0.0, 16.0, 0.0]);
        assert_eq!(c.fwd, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(c.up, [0.0, 1.0, 0.0, 1.0]);
        assert!((c.rotation + 0.12).abs() < 1e-6);
        assert_eq!(c.light1, [0.0, 0.0, -1.0, 0.0]);
    }

    #[test]
    fn init_animation_sets_the_documented_drift() {
        let mut c = Camera::new();
        c.init_animation();
        assert_eq!(c.position_speed[2], 0.04);
        assert_eq!(c.rotation_speed[2], 0.001);
        assert_eq!(c.opening_state, 0);
        assert_eq!(c.opening_go, 0);
    }

    #[test]
    fn init_illegal_disc_starts_at_height_672_state_4() {
        let mut c = Camera::new();
        c.init_illegal_disc();
        assert_eq!(c.position[2], 672.0);
        assert_eq!(c.opening_state, 4);
        assert_eq!(c.position_accel2[2], -0.0178);
        assert_eq!(c.position_speed[2], 2.159_994);
        assert_eq!(c.rotation_speed[2], 0.004_62);
    }

    #[test]
    fn first_frame_advances_from_state_0_to_1_but_not_further() {
        let mut c = boot_camera();
        let mut ev = Vec::new();
        let ty = c.step(1, &mut ev);
        assert_eq!(ty, 0);
        // 16 -> just over 16 after the drift step, so state becomes 1 only
        // after the threshold test of the *next* frame.
        assert!(c.position[2] > 16.0);
        let _ = c.step(2, &mut ev);
        assert_eq!(c.opening_state, 1);
        assert!(ev.is_empty(), "state 1 dispatches no sound");
    }

    #[test]
    fn boot_without_disc_goes_after_two_seconds() {
        let mut c = boot_camera();
        let mut ev = Vec::new();
        let mut state_at_go = None;
        for f in 1..600 {
            c.step(f as u32, &mut ev);
            if c.opening_go == 1 && state_at_go.is_none() {
                state_at_go = Some((f, c.opening_state));
            }
        }
        let (f, st) = state_at_go.expect("openingGo must be set");
        assert!(f >= 120, "go happened at frame {f}");
        assert!(st >= 1);
    }

    #[test]
    fn boot_reaches_scene_end_in_bounded_frames() {
        let mut c = boot_camera();
        let mut ev = Vec::new();
        let mut ty = 0;
        let mut frames = 0;
        for f in 1..6000 {
            ty = c.step(f as u32, &mut ev);
            frames = f;
            if ty == 2 {
                break;
            }
        }
        assert_eq!(ty, 2, "scene never ended");
        assert!(frames < 4000, "end took {frames} frames");
        assert_eq!(c.opening_end_flag, 1);
        // The end latch is 128 frames after the state-6 entry.
        assert!(c.opening_end_frame > 0);
        assert!(ev.contains(&CameraEvent::EndSceneSound));
    }

    #[test]
    fn state_6_end_latch_is_128_frames() {
        let mut c = boot_camera();
        let mut ev = Vec::new();
        // Run until state 6 is entered and the latch is set.
        let mut f = 1u32;
        while c.opening_state != 6 && f < 5000 {
            c.step(f, &mut ev);
            f += 1;
        }
        assert_eq!(c.opening_state, 6);
        c.step(f, &mut ev);
        let latch = c.opening_end_frame;
        assert!(latch > 0);
        // 128 frames later the type must be 2.
        let mut ty = 0;
        for _ in 0..200 {
            f += 1;
            ty = c.step(f, &mut ev);
            if ty == 2 {
                break;
            }
        }
        assert_eq!(ty, 2);
        assert!((f as i32) > latch + 128);
    }

    #[test]
    fn idle_mode_never_leaves_state_zero() {
        let mut c = boot_camera();
        c.position_speed[2] = 0.0; // MODE_IDLE override
        let mut ev = Vec::new();
        for f in 1..30_000 {
            c.step(f, &mut ev);
            assert_eq!(c.opening_state, 0, "state advanced at frame {f}");
        }
        assert!((c.position[2] - 16.0).abs() < 1e-6);
        assert!(ev.is_empty());
    }

    #[test]
    fn illegal_scene_climbs_and_ends_by_state_7() {
        let mut c = Camera::new();
        c.init_illegal_scene();
        c.opening_type = 1;
        c.next_opening_type = 1;
        c.boot_param = 100; // end-capable, so the climb terminates
        let mut ev = Vec::new();
        let mut ty = 1;
        let mut last_z = c.position[2];
        for f in 1..4000 {
            ty = c.step(f as u32, &mut ev);
            assert!(c.position[2] >= last_z - 1e-3, "camera fell at frame {f}");
            last_z = c.position[2];
            if ty == 2 {
                break;
            }
        }
        assert_eq!(ty, 2, "illegal scene never ended");
        assert!(c.opening_state >= 6);
    }

    #[test]
    fn illegal_scene_with_boot_param_101_idles_forever() {
        // The port sets 101 for the illegal screen: the state-6 default arm
        // leaves openingEndFlag 0, so nothing ends.
        let mut c = Camera::new();
        c.init_illegal_scene();
        c.opening_type = 1;
        c.next_opening_type = 1;
        c.boot_param = 101;
        let mut ev = Vec::new();
        let mut ty = 1;
        for f in 1..4000 {
            ty = c.step(f as u32, &mut ev);
        }
        assert_eq!(ty, 1);
        assert_eq!(c.opening_end_flag, 0);
    }

    #[test]
    fn pal_timestep_moves_faster_per_frame() {
        let mut ntsc = boot_camera();
        let mut pal = boot_camera();
        pal.pal = true;
        let mut ev = Vec::new();
        ntsc.step(1, &mut ev);
        pal.step(1, &mut ev);
        assert!(pal.position[2] > ntsc.position[2]);
    }

    #[test]
    fn disc_state_changes_state_1_acceleration() {
        let mut c = boot_camera();
        c.hw = HwState::DiscPs2;
        c.disc_ready = true;
        // Force entry to state 1.
        c.position[2] = 17.0;
        let mut ev = Vec::new();
        c.step(1, &mut ev);
        assert_eq!(c.opening_state, 2, "disc path should skip the wait");
        assert_eq!(c.position_accel2[2], 0.003);
        assert_eq!(c.opening_go, 1);
    }

    #[test]
    fn view_matrix_is_finite_and_oriented() {
        let c = boot_camera();
        let m = c.view_matrix();
        for row in m.iter() {
            for v in row.iter() {
                assert!(v.is_finite());
            }
        }
        // Camera at (0,0,16) looking +z: the world point z=26 is +10 ahead.
        let v = crate::math::apply(&m, &[0.0, 0.0, 26.0, 1.0]);
        assert!((v[2] - 10.0).abs() < 1e-5);
    }

    #[test]
    fn non_finite_position_is_recovered() {
        let mut c = boot_camera();
        c.position[2] = f32::NAN;
        let mut ev = Vec::new();
        c.step(10, &mut ev);
        assert!(c.position.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn hw_state_ids_round_trip() {
        for s in [HwState::NoDisc, HwState::DiscPs2, HwState::DiscIllegal] {
            assert_eq!(HwState::from_id(s.id()), s);
        }
        assert!(!HwState::NoDisc.has_disc());
        assert!(HwState::DiscIllegal.has_disc());
    }
}
