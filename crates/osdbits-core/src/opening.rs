//! Deterministic reconstruction of `osdbits/opening.c` + `vucode_1.vsm`:
//! - **Mode A (`SceneVariant::Opening`)**: Boot tower field, 6-layer fog, 4 orbiting light streaks,
//!   5 10-pass refractive glass cubes, 8-state camera dive/fly-up, bloom, fades, and clean-room banner.
//! - **Mode B (`SceneVariant::Idle`)**: Indefinite canopy hover/drift over the live tower field.
//! - **Mode C (`SceneVariant::Illegal`)**: Red warning flare rings, lens sprites, 128 falling red
//!   cloud patches, 5 red refractive glass cubes, fades, and localized warning overlay.

use crate::math::{Mat4, TAU, Vec3, Vec4, clampf, cosf, sinf, sqrtf, wrap_pi};
use crate::osd_font::{emit_osd_text, measure_osd_text};
use crate::procedural_assets::{MeshQuad, build_cube_mesh};
use crate::render_list::{
    BatchKey, BlendMode, DepthMode, RenderList, RenderVertex, TargetId, TextureSlot, pass_action,
    vertex_flags,
};
use crate::rng::{OsdRng, TowerRng};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpeningVariant {
    Boot = 0,
    Idle = 1,
    Illegal = 2,
}

#[derive(Clone, Copy, Debug)]
pub struct OpeningTuning {
    pub is_pal: bool,
    pub speed_scale: f32,
    pub idle_drift: f32,
    pub fog_enabled: bool,
    pub bloom_enabled: bool,
    pub refraction_enabled: bool,
    pub reduced_motion: bool,
    pub letterbox_16_9: bool,
    pub show_banner: bool,
    pub language: u8,
    pub tower_density_games: u8,
}

impl Default for OpeningTuning {
    fn default() -> Self {
        Self {
            is_pal: false,
            speed_scale: 1.0,
            idle_drift: 1.0,
            fog_enabled: true,
            bloom_enabled: true,
            refraction_enabled: true,
            reduced_motion: false,
            letterbox_16_9: false,
            show_banner: true,
            language: 0,
            tower_density_games: 16,
        }
    }
}

/// Exact `count -> towerC / towerD` tables from `opening.c:3249`.
const HIST_TABLE_C: [f32; 14] = [
    0.2, 0.4, 0.6, 0.8, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
];
const HIST_TABLE_D: [f32; 14] = [
    0.1, 0.1, 0.1, 0.1, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0,
];

/// Exact 21-slot x 6-cell neighbourhood partition from `opening.c:3257` (`0x27a260`).
const HIST_CELLS: [[(u8, u8); 6]; 21] = [
    [(0, 0), (0, 1), (0, 2), (0, 3), (1, 0), (1, 1)],
    [(1, 2), (2, 0), (2, 1), (2, 2), (3, 0), (4, 0)],
    [(4, 1), (5, 0), (5, 1), (6, 0), (6, 1), (6, 2)],
    [(7, 0), (7, 1), (8, 0), (8, 1), (8, 2), (8, 3)],
    [(9, 0), (9, 1), (10, 0), (10, 1), (10, 2), (11, 0)],
    [(12, 0), (12, 1), (13, 0), (13, 1), (13, 2), (13, 3)],
    [(1, 3), (2, 3), (3, 1), (3, 2), (3, 3), (4, 2)],
    [(5, 2), (5, 3), (5, 4), (6, 3), (7, 2), (7, 3)],
    [(9, 2), (9, 3), (9, 5), (10, 3), (10, 4), (10, 5)],
    [(11, 1), (11, 2), (11, 3), (11, 4), (12, 2), (12, 3)],
    [(0, 4), (1, 4), (2, 4), (2, 5), (2, 6), (3, 4)],
    [(3, 5), (3, 6), (4, 3), (4, 4), (4, 5), (4, 6)],
    [(5, 5), (5, 6), (6, 4), (6, 5), (7, 4), (7, 5)],
    [(8, 4), (8, 5), (8, 6), (9, 4), (9, 6), (9, 7)],
    [(12, 4), (13, 4), (13, 5), (13, 6), (13, 7), (13, 8)],
    [(0, 5), (0, 6), (1, 5), (1, 6), (1, 7), (2, 7)],
    [(0, 7), (0, 8), (1, 8), (2, 8), (3, 7), (3, 8)],
    [(4, 7), (4, 8), (5, 7), (5, 8), (6, 6), (6, 7)],
    [(6, 8), (7, 6), (7, 7), (7, 8), (8, 7), (8, 8)],
    [(9, 8), (10, 6), (10, 7), (10, 8), (11, 7), (11, 8)],
    [(11, 5), (11, 6), (12, 5), (12, 6), (12, 7), (12, 8)],
];

/// Exact `[14][9]` source positions from `opening.c:3281` (`0x27a670`).
const HIST_SRC_POS: [[[f32; 3]; 9]; 14] = [
    [
        [-13.7428, 11.7512, -2.5366],
        [-13.7428, 10.4512, -2.5366],
        [-13.7428, 9.1512, -2.5366],
        [-13.1428, 7.8512, -2.5366],
        [-13.7428, 6.5512, -6.5517],
        [-13.7428, 5.2512, -2.5366],
        [-13.7428, 3.9512, -2.5366],
        [-13.7428, 2.6512, -2.5366],
        [-13.7428, 1.3512, -2.5366],
    ],
    [
        [-12.4428, 11.7512, -2.5366],
        [-12.4428, 10.4512, -2.5366],
        [-12.4428, 9.1512, -2.5366],
        [-12.1428, 7.8512, -0.933],
        [-12.4428, 6.5512, -2.5366],
        [-12.4428, 5.2512, -2.5366],
        [-12.4428, 3.9512, -2.5366],
        [-12.4428, 2.6512, -2.5366],
        [-12.4428, 1.3512, -2.5366],
    ],
    [
        [-11.1428, 11.7512, -2.5366],
        [-11.1428, 10.4512, -2.5366],
        [-11.1428, 9.1512, -3.2422],
        [-11.1428, 7.8512, -2.5366],
        [-11.1428, 6.5512, -3.1781],
        [-11.1428, 5.2512, -2.5366],
        [-11.1428, 3.9512, -2.5366],
        [-11.1428, 2.6512, -2.5366],
        [-11.1428, 1.3512, -2.5366],
    ],
    [
        [-9.8428, 11.7512, -2.5366],
        [-9.8428, 10.4512, -2.5366],
        [-9.8428, 9.1512, -2.5366],
        [-9.8428, 7.8512, -3.3705],
        [-9.8428, 6.5512, -4.0119],
        [-9.8428, 5.2512, -2.5366],
        [-9.8428, 3.9512, -2.5366],
        [-9.8428, 2.6512, -2.5366],
        [-9.8428, 1.3512, -3.2422],
    ],
    [
        [-8.5428, 11.7512, -2.5366],
        [-8.5428, 10.4512, -2.5366],
        [-8.5428, 9.1512, -2.5366],
        [-8.5428, 7.8512, -5.4054],
        [-8.5428, 6.5512, -4.4491],
        [-8.5428, 5.2512, -2.5366],
        [-8.5428, 3.9512, -3.5629],
        [-8.5428, 2.6512, -2.5366],
        [-8.5428, 1.3512, -2.5366],
    ],
    [
        [-7.2428, 11.7512, -2.5366],
        [-7.2428, 10.4512, -2.5366],
        [-7.2428, 9.1512, -2.5366],
        [-7.2428, 7.8512, -5.4097],
        [-7.2428, 6.5512, -3.8195],
        [-7.2428, 5.2512, -2.5366],
        [-7.2428, 3.9512, -3.5629],
        [-7.2428, 2.6512, -2.5366],
        [-7.2428, 1.3512, -2.5366],
    ],
    [
        [-5.9428, 11.7512, -2.5366],
        [-5.9428, 10.4512, -2.5366],
        [-5.9428, 9.1512, -2.5366],
        [-5.9428, 7.8512, -3.3705],
        [-5.9428, 6.5512, -4.4491],
        [-5.9428, 5.2512, -3.8195],
        [-5.9428, 3.9512, -0.0309],
        [-5.9428, 2.6512, -2.5366],
        [-5.9428, 1.3512, -2.5366],
    ],
    [
        [-4.6428, 11.7512, -2.5366],
        [-4.6428, 10.4512, -2.5366],
        [-4.6428, 9.1512, -2.5366],
        [-4.6428, 7.8512, -2.5366],
        [-4.6428, 6.5512, -3.8195],
        [-4.6428, 5.2512, -2.5366],
        [-4.6428, 3.9512, -2.5366],
        [-4.6428, 2.6512, -4.5251],
        [-4.6428, 1.3512, -2.5366],
    ],
    [
        [-3.3428, 11.7512, -2.5366],
        [-3.3428, 10.4512, -2.5366],
        [-3.3428, 9.1512, -5.0383],
        [-3.3428, 7.8512, -4.5892],
        [-3.3428, 6.5512, -3.5629],
        [-3.3428, 5.2512, -4.5766],
        [-3.3428, 3.9512, -2.8536],
        [-3.3428, 2.6512, -0.619],
        [-3.3428, 1.3512, -2.5366],
    ],
    [
        [-2.0428, 11.7512, -2.5366],
        [-2.0428, 10.4512, -2.5366],
        [-2.0428, 9.1512, -2.5366],
        [-2.0428, 7.8512, -2.5366],
        [-2.0428, 6.5512, -3.3654],
        [-2.0428, 5.2512, -2.8799],
        [-2.0428, 3.9512, -2.5366],
        [-2.0428, 2.6512, -2.5366],
        [-2.0428, 1.3512, -2.5366],
    ],
    [
        [-0.7428, 11.7512, -2.5366],
        [-0.7428, 10.4512, -2.5366],
        [-0.7428, 9.1512, 0.7348],
        [-0.7428, 7.8512, -2.5366],
        [-0.7428, 6.5512, -2.5366],
        [-0.7428, 5.2512, -2.8554],
        [-0.7428, 3.9512, -4.267],
        [-0.7428, 2.6512, -2.5366],
        [-0.7428, 1.3512, -2.5366],
    ],
    [
        [0.5572, 11.7512, -2.5366],
        [0.5572, 10.4512, -2.5366],
        [0.5572, 9.1512, -2.5366],
        [0.5572, 7.8512, -3.0466],
        [0.5572, 6.5512, -3.6204],
        [0.5572, 5.2512, -2.8554],
        [0.5572, 3.9512, -2.5366],
        [0.5572, 2.6512, -2.5366],
        [0.5572, 1.3512, -2.5366],
    ],
    [
        [1.8572, 11.7512, -2.5366],
        [1.8572, 10.4512, -2.5366],
        [1.8572, 9.1512, -2.5366],
        [1.8572, 7.8512, -2.5366],
        [1.8572, 6.5512, -2.5366],
        [1.8572, 5.2512, -3.1741],
        [1.8572, 3.9512, -2.5366],
        [1.8572, 2.6512, -2.5366],
        [1.8572, 1.3512, -2.5366],
    ],
    [
        [3.1572, 11.7512, -2.5366],
        [3.1572, 10.4512, -2.5366],
        [3.1572, 9.1512, -2.5366],
        [3.1572, 7.8512, -2.5366],
        [3.1572, 6.5512, -2.5366],
        [3.1572, 5.2512, -2.5366],
        [3.1572, 3.9512, -2.5366],
        [3.1572, 2.6512, -2.5366],
        [3.1572, 1.3512, -2.5366],
    ],
];

/// Exact `cubeSeedTable` (`opening.c:1321`, `0x27a210`).
const CUBE_SEED_TABLE: [[f32; 3]; 5] = [
    [3.5679, 0.5447, 2.5932],
    [-0.9042, -1.1173, 3.7952],
    [3.2639, -2.6491, 4.1075],
    [-3.7296, -2.3677, 4.3654],
    [-3.1017, 2.2409, 4.5429],
];

/// Exact `illegalCubeSeedTable` (`opening.c:1332`, `0x27af50`).
const ILLEGAL_CUBE_SEED_TABLE: [[f32; 3]; 5] = [
    [-10.4068, 4.1636, 5.0429],
    [-12.9184, -4.2708, 4.3654],
    [2.7639, 0.1509, 4.1075],
    [4.0958, -1.3173, 3.0952],
    [-2.4321, 0.5447, 2.7732],
];

/// Exact camera state height levels (`opening.c:2809`).
const OPENING_STATE_LEVELS: [f32; 8] = [16.0, 56.0, 104.0, 320.0, 672.0, 800.0, 1160.0, 1160.0];

const LIGHT_COLORS: [[u8; 3]; 4] = [
    [130, 185, 255],
    [155, 140, 255],
    [110, 215, 255],
    [190, 225, 255],
];

#[derive(Clone, Copy, Debug, Default)]
struct HistSlot {
    used: bool,
    title_id: u16,
    count: u8,
    mask: u8,
    own: u8,
    date: u16,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TowerCell {
    pub active: bool,
    pub brightness: f32,
    pub height_factor: f32,
    pub base_pos: Vec3,
    pub phase_a: f32,
    pub phase_b: f32,
}

#[derive(Clone, Copy, Debug, Default)]
struct CubeInstance {
    anchor: Vec3,
    angles: Vec3,
    rates: Vec3,
}

#[derive(Clone, Debug)]
struct OrbitLight {
    trail: [Vec3; 64],
    trail_head: usize,
    trail_len: usize,
    phase: f32,
    speed: f32,
    radius_xy: f32,
    base_z: f32,
}

#[derive(Clone, Copy, Debug, Default)]
struct RedFogPatch {
    pos: Vec3,
    rot_z: f32,
    center_lum: u8,
}

pub struct OpeningScene {
    pub variant: OpeningVariant,
    pub seed: u32,
    pub osd_rng: OsdRng,
    pub frame_count: u32,
    pub sim_time: f32,
    pub opening_state: usize,
    pub opening_go: bool,
    pub completed: bool,

    pub cam_pos: Vec3,
    pub cam_speed: Vec3,
    pub cam_accel1: Vec3,
    pub cam_accel2: Vec3,
    pub cam_rot: f32,
    pub cam_rot_speed: f32,
    pub cam_rot_accel: f32,
    pub fwd_dir: Vec3,

    pub towers: [[TowerCell; 9]; 14],
    pub active_tower_count: u32,
    pub fog_phases: [f32; 6],
    cubes: [CubeInstance; 5],
    cube_mesh_boot: [MeshQuad; 6],
    cube_mesh_illegal: [MeshQuad; 6],
    lights: [OrbitLight; 4],
    red_fog_patches: Vec<RedFogPatch>,

    banner_state: i8,
    banner_alpha: f32,
    banner_step: f32,
    illegal_text_alpha: f32,
}

impl OpeningScene {
    #[must_use]
    pub fn new(variant: OpeningVariant, seed: u32, tuning: OpeningTuning) -> Self {
        let mut s = Self {
            variant,
            seed,
            osd_rng: OsdRng::new(OsdRng::DEFAULT_SEED),
            frame_count: 0,
            sim_time: 0.0,
            opening_state: 0,
            opening_go: false,
            completed: false,
            cam_pos: Vec3::new(0.0, 0.0, 16.0),
            cam_speed: Vec3::default(),
            cam_accel1: Vec3::default(),
            cam_accel2: Vec3::default(),
            cam_rot: -0.12,
            cam_rot_speed: 0.001,
            cam_rot_accel: 0.0,
            fwd_dir: Vec3::new(0.0, 0.0, 1.0),
            towers: [[TowerCell::default(); 9]; 14],
            active_tower_count: 0,
            fog_phases: [0.0, 0.17, 0.34, 0.51, 0.68, 0.85],
            cubes: [CubeInstance::default(); 5],
            cube_mesh_boot: build_cube_mesh(1.8, 1.0),
            cube_mesh_illegal: build_cube_mesh(1.2, 1.0),
            lights: core::array::from_fn(|_| OrbitLight {
                trail: [Vec3::default(); 64],
                trail_head: 0,
                trail_len: 0,
                phase: 0.0,
                speed: 0.02,
                radius_xy: 12.0,
                base_z: 112.0,
            }),
            red_fog_patches: Vec::with_capacity(128),
            banner_state: 0,
            banner_alpha: 0.0,
            banner_step: 4.0,
            illegal_text_alpha: 0.0,
        };
        s.reset(variant, seed, tuning);
        s
    }

    pub fn reset(&mut self, variant: OpeningVariant, seed: u32, tuning: OpeningTuning) {
        self.variant = variant;
        self.seed = seed;
        self.osd_rng
            .reset(OsdRng::DEFAULT_SEED.wrapping_add(seed.wrapping_mul(7919)));
        self.frame_count = 0;
        self.sim_time = 0.0;
        self.completed = false;
        self.opening_go = false;
        self.banner_state = 0;
        self.banner_alpha = 0.0;
        self.banner_step = 4.0;
        self.illegal_text_alpha = 0.0;
        self.fog_phases = [0.0, 0.17, 0.34, 0.51, 0.68, 0.85];

        self.generate_tower_field(seed, tuning.tower_density_games);
        self.init_lights_and_cubes(variant);
        self.init_illegal_fog();

        match variant {
            OpeningVariant::Boot => {
                self.opening_state = 0;
                self.cam_pos = Vec3::new(0.0, 0.0, 16.0);
                self.fwd_dir = Vec3::new(0.0, 0.0, 1.0);
                self.cam_speed = Vec3::new(0.0, 0.0, 0.04);
                self.cam_accel1 = Vec3::default();
                self.cam_accel2 = Vec3::default();
                self.cam_rot = -0.12;
                self.cam_rot_speed = 0.001;
                self.cam_rot_accel = 0.0;
            }
            OpeningVariant::Idle => {
                self.opening_state = 0;
                self.cam_pos = Vec3::new(0.0, 0.0, 16.0);
                self.fwd_dir = Vec3::new(0.0, 0.0, 1.0);
                // Exact opening.c:4295: positionSpeed[2] = 0.0f so state machine never advances!
                self.cam_speed = Vec3::default();
                self.cam_accel1 = Vec3::default();
                self.cam_accel2 = Vec3::default();
                self.cam_rot = -0.12;
                self.cam_rot_speed = 0.001;
                self.cam_rot_accel = 0.0;
            }
            OpeningVariant::Illegal => {
                // Exact opening.c:981 InitIllegalDisc + InitIllegalScene
                self.opening_state = 4;
                self.cam_pos = Vec3::new(0.0, 0.0, 672.0);
                self.fwd_dir = Vec3::new(0.0, -0.03, 1.0);
                self.cam_speed = Vec3::new(0.0, 0.0, 2.159_994);
                self.cam_accel1 = Vec3::default();
                self.cam_accel2 = Vec3::new(0.0, 0.0, -0.0178);
                self.cam_rot = 0.0;
                self.cam_rot_speed = 0.00462;
                self.cam_rot_accel = 0.0;
            }
        }
    }

    fn generate_tower_field(&mut self, seed: u32, ngames_cfg: u8) {
        let mut trng = TowerRng::new(seed);
        let full_field = ngames_cfg == 0;
        let ngames = u32::from(ngames_cfg.max(1));
        let nboots = 96 + trng.next_mod(80);
        let mut hist = [HistSlot::default(); 21];

        for b in 0..nboots {
            let r1 = trng.next_mod(ngames);
            let r2 = trng.next_mod(ngames);
            let title = r1.min(r2) as u16;

            let existing = hist.iter_mut().find(|e| e.used && e.title_id == title);
            if let Some(e) = existing {
                e.date = b as u16;
                if (e.mask & 0x3f) == 0x3f {
                    if e.count < 63 {
                        e.count += 1;
                    } else {
                        e.own = 7;
                    }
                    continue;
                }
                if e.count < 127 {
                    e.count += 1;
                }
                if e.count >= 14 && (e.count - 14).is_multiple_of(10) {
                    let mut bit = (trng.next_mod(6)) as u8;
                    for _ in 0..6 {
                        if ((e.mask >> bit) & 1) == 0 {
                            break;
                        }
                        bit = (bit + 1) % 6;
                    }
                    e.own = bit;
                    e.mask |= 1 << bit;
                }
            } else {
                let empty_count = hist.iter().filter(|e| !e.used).count() as u32;
                let slot_idx = if empty_count > 0 {
                    let mut pick = trng.next_mod(empty_count);
                    let mut found = 0usize;
                    for (idx, item) in hist.iter().enumerate() {
                        if !item.used {
                            if pick == 0 {
                                found = idx;
                                break;
                            }
                            pick -= 1;
                        }
                    }
                    found
                } else {
                    // Authentic eviction rule: lowest count, tie-break oldest date
                    let mut worst = 0usize;
                    for idx in 1..21 {
                        if hist[idx].count < hist[worst].count
                            || (hist[idx].count == hist[worst].count
                                && hist[idx].date < hist[worst].date)
                        {
                            worst = idx;
                        }
                    }
                    worst
                };
                let own = (trng.next_mod(6)) as u8;
                hist[slot_idx] = HistSlot {
                    used: true,
                    title_id: title,
                    count: 1,
                    mask: 1 << own,
                    own,
                    date: b as u16,
                };
            }
        }

        // Reset all 126 cells to base positions (`opening.c:3568`)
        for (r, src_row) in HIST_SRC_POS.iter().enumerate() {
            for (c, &src) in src_row.iter().enumerate() {
                let base_x = (src[0] + 4.8) * 4.0;
                let base_y = (src[1] - 6.5) * 4.0;
                let base_z = (src[2] + 4.0) * 12.0 + 150.0;
                let ph = ((r * 9 + c) as f32) * 0.43;
                self.towers[r][c] = TowerCell {
                    active: full_field,
                    brightness: if full_field { 0.85 } else { 0.0 },
                    height_factor: if full_field { 0.85 } else { 0.0 },
                    base_pos: Vec3::new(base_x, base_y, base_z),
                    phase_a: ph,
                    phase_b: ph * 0.71 + 1.1,
                };
            }
        }

        for (slot_i, entry) in hist.iter().enumerate() {
            if !entry.used {
                continue;
            }
            for (bit, &(r_u8, c_u8)) in HIST_CELLS[slot_i].iter().enumerate() {
                if ((entry.mask >> bit) & 1) == 0 {
                    continue;
                }
                let r = r_u8 as usize;
                let c = c_u8 as usize;
                let (tc, td) = if entry.own as usize == bit {
                    let idx = if entry.count >= 14 {
                        ((entry.count - 14) % 10 + 4) as usize
                    } else {
                        (entry.count.saturating_sub(1) as usize).min(13)
                    };
                    (HIST_TABLE_C[idx.min(13)], HIST_TABLE_D[idx.min(13)])
                } else {
                    // Older sprawled cells stay maxed at 1.0 (`opening.c:3226`)
                    (1.0, 1.0)
                };
                self.towers[r][c].active = true;
                self.towers[r][c].brightness = tc;
                self.towers[r][c].height_factor = td;
            }
        }

        let mut active_count = 0u32;
        for row in &self.towers {
            for cell in row {
                if cell.active {
                    active_count += 1;
                }
            }
        }
        self.active_tower_count = active_count;
    }

    fn init_lights_and_cubes(&mut self, variant: OpeningVariant) {
        for (l, light) in self.lights.iter_mut().enumerate() {
            let lf = l as f32;
            light.trail_head = 0;
            light.trail_len = 0;
            light.phase = lf * (TAU * 0.25) + 0.35 * (self.osd_rng.next_f32());
            light.speed = 0.026 + 0.006 * lf;
            light.radius_xy = 10.5 + 3.8 * lf;
            light.base_z = 92.0 + 11.0 * lf;
        }

        for i in 0..5 {
            match variant {
                OpeningVariant::Boot | OpeningVariant::Idle => {
                    let d = if i == 2 {
                        0.9
                    } else {
                        ((i as i32 - 2) as f32) * 0.8
                    };
                    let s = CUBE_SEED_TABLE[i];
                    let v = d * ((i % 3) as f32) * 3.7 + 0.2856;
                    self.cubes[i] = CubeInstance {
                        anchor: Vec3::new(s[0] * 3.5, s[1] * 3.5, s[2] * -15.0 + 150.0),
                        angles: Vec3::new(wrap_pi(v), wrap_pi(v), wrap_pi(v)),
                        rates: Vec3::new(0.0031 / d, d * 0.0022, d / 1000.0 + 0.0013),
                    };
                }
                OpeningVariant::Illegal => {
                    let mut f = ((i as i32 - 2) as f32) * 0.8;
                    if f == 0.0 {
                        f = 0.9;
                    }
                    let s = ILLEGAL_CUBE_SEED_TABLE[i];
                    self.cubes[i] = CubeInstance {
                        anchor: Vec3::new(s[0], s[1], (s[2] - 2.5) * 128.0 + 800.0 - 12.0),
                        angles: Vec3::new(
                            wrap_pi(f * (((i * 2) % 9) as f32) * 0.25),
                            wrap_pi(f * (((i * 2) & 7) as f32) / 5.0),
                            wrap_pi(f * (((i * 2) % 7) as f32) / 6.0),
                        ),
                        rates: Vec3::new(0.004 / f, f * 0.003, f / 800.0 + 0.002),
                    };
                }
            }
        }
    }

    fn init_illegal_fog(&mut self) {
        self.red_fog_patches.clear();
        for _ in 0..128 {
            let lum = (self.osd_rng.next_mod(64) + 64) as u8;
            let x = (self.osd_rng.next_mod(4800) as f32 - 2400.0) * 0.01;
            let y = (self.osd_rng.next_mod(4800) as f32 - 2400.0) * 0.01;
            let z = self.osd_rng.next_mod(805) as f32 + 477.0;
            let rot_z = self.osd_rng.next_f32() * TAU;
            self.red_fog_patches.push(RedFogPatch {
                pos: Vec3::new(x, y, z),
                rot_z,
                center_lum: lum,
            });
        }
    }

    pub fn step(&mut self, dt_scale: f32, tuning: OpeningTuning) {
        let base_step = if tuning.is_pal { 1.2 } else { 1.0 };
        let motion_damp = if tuning.reduced_motion { 0.45 } else { 1.0 };
        let dt = clampf(base_step * dt_scale * tuning.speed_scale, 0.0, 4.0);
        let fps = if tuning.is_pal { 50u32 } else { 60u32 };

        self.frame_count = self.frame_count.saturating_add(1);
        self.sim_time += dt;

        match self.variant {
            OpeningVariant::Boot => {
                if self.opening_state < 7
                    && self.cam_pos.z > OPENING_STATE_LEVELS[self.opening_state]
                {
                    self.opening_state += 1;
                }
                match self.opening_state {
                    1 => {
                        self.cam_accel1.z = 0.000_000_4;
                        if self.frame_count > fps * 2 {
                            self.opening_go = true;
                            self.opening_state = 2;
                        }
                    }
                    2 => {
                        if self.frame_count > fps * 10 {
                            self.opening_go = true;
                        }
                        if self.opening_go {
                            self.cam_accel2.z = 0.0099 * motion_damp;
                            self.cam_rot_accel = 0.000_195 * motion_damp;
                        }
                    }
                    3..=7 if self.cam_pos.z >= 148.0 => {
                        self.completed = true;
                    }
                    _ => {}
                }

                self.cam_rot_speed += self.cam_rot_accel * dt;
                self.cam_speed.z += (2.0 * self.cam_accel2.z + self.cam_accel1.z) * 0.5 * dt;
                self.cam_accel2.z += self.cam_accel1.z * dt;
                self.cam_rot = wrap_pi(
                    self.cam_rot + (2.0 * self.cam_rot_speed + self.cam_rot_accel) * 0.5 * dt,
                );
                self.cam_pos.z += (2.0 * self.cam_speed.z + self.cam_accel2.z) * 0.5 * dt;

                // Conservative clean-room banner state machine (`DoSCEText` timing)
                if self.cam_pos.z > 18.0 && self.banner_state == 0 {
                    self.banner_state = 1;
                }
                if self.banner_state == 1 {
                    self.banner_alpha += self.banner_step * dt;
                    if self.banner_alpha >= 240.0 {
                        self.banner_alpha = 240.0;
                        self.banner_step = -4.0;
                    } else if self.banner_alpha <= 0.0 {
                        self.banner_alpha = 0.0;
                        self.banner_step = 4.0;
                        self.banner_state = -1;
                    }
                }
            }
            OpeningVariant::Idle => {
                // Indefinite canopy hover: Z stays pinned around 16.0 while gentle sway/roll runs forever
                let drift = clampf(tuning.idle_drift, 0.0, 2.5) * motion_damp;
                self.cam_rot = wrap_pi(self.cam_rot + 0.001 * dt * drift.max(0.25));
                let t = self.sim_time * 0.008;
                self.cam_pos.x = sinf(t) * 5.2 * drift;
                self.cam_pos.y = cosf(t * 0.77) * 3.8 * drift;
                self.cam_pos.z = 16.0 + sinf(t * 0.53) * 3.0 * drift;
            }
            OpeningVariant::Illegal => {
                if self.opening_state < 6
                    && self.cam_pos.z > OPENING_STATE_LEVELS[self.opening_state]
                {
                    self.opening_state += 1;
                }
                if self.opening_state >= 6 {
                    self.cam_accel1 = Vec3::default();
                    self.cam_accel2 = Vec3::default();
                    self.cam_speed = Vec3::default();
                    self.cam_rot_accel = 0.0;
                    self.cam_pos.z = self.cam_pos.z.max(800.5);
                } else {
                    self.cam_speed.z += self.cam_accel2.z * dt;
                    self.cam_pos.z += (2.0 * self.cam_speed.z + self.cam_accel2.z) * 0.5 * dt;
                    if self.cam_speed.z < 0.08 && self.cam_pos.z < 801.0 {
                        self.cam_speed.z = 0.08;
                        self.cam_accel2.z = 0.0;
                    }
                }
                self.cam_rot = wrap_pi(self.cam_rot + self.cam_rot_speed * dt * motion_damp);

                if self.cam_pos.z > 800.0 {
                    self.illegal_text_alpha = (self.illegal_text_alpha + 1.5 * dt).min(112.0);
                }

                for (i, patch) in self.red_fog_patches.iter_mut().enumerate() {
                    patch.rot_z =
                        wrap_pi(patch.rot_z + 0.003 * dt * if i % 2 == 0 { 1.0 } else { -1.0 });
                    patch.pos.z -= 0.42 * dt * motion_damp;
                    if patch.pos.z < 690.0 {
                        patch.pos.z += 520.0;
                    }
                }
            }
        }

        // Advance fog layers
        for (i, ph) in self.fog_phases.iter_mut().enumerate() {
            *ph = (*ph + (0.0022 + 0.0005 * (i as f32)) * dt * motion_damp).rem_euclid(1.0);
        }

        // Advance orbiting lights & ribbon trails
        for (l, light) in self.lights.iter_mut().enumerate() {
            light.phase = wrap_pi(light.phase + light.speed * dt * motion_damp);
            let lf = l as f32;
            let p = Vec3::new(
                cosf(light.phase) * light.radius_xy + sinf(light.phase * 1.7 + lf) * 4.5,
                sinf(light.phase * 0.9 + lf) * (light.radius_xy * 0.78),
                light.base_z + sinf(light.phase * 0.65 + lf * 1.3) * 14.0,
            );
            light.trail[light.trail_head] = p;
            light.trail_head = (light.trail_head + 1) % light.trail.len();
            if light.trail_len < light.trail.len() {
                light.trail_len += 1;
            }
        }

        // Advance tumbling refractive glass cubes
        for cube in &mut self.cubes {
            cube.angles.x = wrap_pi(cube.angles.x + cube.rates.x * dt * motion_damp);
            cube.angles.y = wrap_pi(cube.angles.y + cube.rates.y * dt * motion_damp);
            cube.angles.z = wrap_pi(cube.angles.z + cube.rates.z * dt * motion_damp);
        }
    }

    pub fn emit_frame(&self, rl: &mut RenderList, tuning: OpeningTuning) {
        let screen_w = 640.0_f32;
        let screen_h = if tuning.is_pal { 256.0_f32 } else { 224.0_f32 };
        let screen_ay = if tuning.is_pal {
            0.526_271_05_f32
        } else {
            0.457_627_f32
        };

        let up_dir = Vec3::new(sinf(self.cam_rot), cosf(self.cam_rot), 0.0);
        let cam_mat = Mat4::ps2_camera(self.cam_pos, self.fwd_dir, up_dir);
        let proj_mat =
            Mat4::ps2_webgpu_projection(1024.0, 1.0, screen_ay, screen_w, screen_h, 1.0, 4096.0);
        let vp = proj_mat * cam_mat;

        match self.variant {
            OpeningVariant::Boot | OpeningVariant::Idle => {
                // Deep midnight-indigo PS2 void background
                rl.clear([0.012, 0.018, 0.048, 1.0]);

                self.emit_sky_glow(rl, screen_w, screen_h, false);
                self.emit_towers(rl, vp, tuning);

                if tuning.bloom_enabled {
                    rl.emit_pass_action(
                        pass_action::CAPTURE_BLOOM_DOWNSAMPLE,
                        TargetId::MainScreen,
                    );
                    rl.begin_batch(BatchKey {
                        texture: TextureSlot::ExtraBloom,
                        blend: BlendMode::Additive,
                        depth: DepthMode::None,
                        action: pass_action::CLAMP_UV,
                        target: TargetId::MainScreen,
                    });
                    let bloom_alpha = if tuning.reduced_motion { 42 } else { 72 };
                    rl.push_screen_rect(
                        -6.0,
                        -3.0,
                        screen_w + 12.0,
                        screen_h + 6.0,
                        screen_w,
                        screen_h,
                        [0.0, 0.0, 1.0, 1.0],
                        [255, 255, 255, bloom_alpha],
                        0,
                    );
                }

                if tuning.fog_enabled {
                    self.emit_opening_fog(rl, vp);
                }
                self.emit_orbit_lights(rl, vp);
                self.emit_glass_cubes(rl, cam_mat, proj_mat, false, tuning.refraction_enabled);

                // Fly-up atmospheric flash & fade at end of Boot timeline
                if self.variant == OpeningVariant::Boot && self.cam_pos.z > 98.0 {
                    let t = clampf((self.cam_pos.z - 98.0) / 48.0, 0.0, 1.0);
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
                        [0.0, 0.0, 1.0, 1.0],
                        [0, 0, 0, (t * 255.0) as u8],
                        0,
                    );
                }

                // Conservative clean-room boot banner (never Sony trademarks)
                if self.variant == OpeningVariant::Boot
                    && tuning.show_banner
                    && self.banner_state == 1
                    && self.banner_alpha > 1.0
                {
                    let a = ((self.banner_alpha.min(112.0) / 112.0) * 230.0) as u8;
                    let label = "OSDSYS // CLEAN-ROOM RECONSTRUCTION";
                    let tw = measure_osd_text(label, 1.05);
                    emit_osd_text(
                        rl,
                        label,
                        (screen_w - tw) * 0.5,
                        screen_h * 0.46,
                        1.05,
                        [225, 236, 255, a],
                        screen_w,
                        screen_h,
                    );
                }
            }
            OpeningVariant::Illegal => {
                // Deep crimson-black illegal-disc void
                rl.clear([0.045, 0.005, 0.008, 1.0]);
                self.emit_sky_glow(rl, screen_w, screen_h, true);
                self.emit_red_flare(rl, screen_w, screen_h);
                if tuning.fog_enabled {
                    self.emit_illegal_fog(rl, vp);
                }
                self.emit_glass_cubes(rl, cam_mat, proj_mat, true, tuning.refraction_enabled);

                if tuning.bloom_enabled {
                    rl.emit_pass_action(
                        pass_action::CAPTURE_BLOOM_DOWNSAMPLE,
                        TargetId::MainScreen,
                    );
                    rl.begin_batch(BatchKey {
                        texture: TextureSlot::ExtraBloom,
                        blend: BlendMode::Additive,
                        depth: DepthMode::None,
                        action: pass_action::CLAMP_UV,
                        target: TargetId::MainScreen,
                    });
                    rl.push_screen_rect(
                        -4.0,
                        -2.0,
                        screen_w + 8.0,
                        screen_h + 4.0,
                        screen_w,
                        screen_h,
                        [0.0, 0.0, 1.0, 1.0],
                        [255, 190, 190, 88],
                        0,
                    );
                }

                // DrawIllegalFades: fade in from black as z rises 672 -> 800
                if self.cam_pos.z < 800.0 {
                    let fade = clampf(1.0 - (self.cam_pos.z - 672.0) / 128.0, 0.0, 1.0);
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
                        [0.0, 0.0, 1.0, 1.0],
                        [0, 0, 0, (fade * 255.0) as u8],
                        0,
                    );
                }

                if tuning.show_banner && self.illegal_text_alpha > 1.0 {
                    let msg = match tuning.language {
                        1 => "Veuillez inserer un disque au format valide.",
                        2 => "Inserte un disco de formato compatible.",
                        3 => "Bitte legen Sie ein kompatibles Medium ein.",
                        4 => "Inserire un disco di formato compatibile.",
                        5 => "Plaats een schijf van geldig formaat.",
                        6 => "Insira um disco de formato compativel.",
                        _ => "Please insert a compatible format disc.",
                    };
                    let a = ((self.illegal_text_alpha / 112.0) * 235.0) as u8;
                    let tw = measure_osd_text(msg, 1.0);
                    emit_osd_text(
                        rl,
                        msg,
                        (screen_w - tw) * 0.5,
                        screen_h * 0.44,
                        1.0,
                        [255, 224, 224, a],
                        screen_w,
                        screen_h,
                    );
                }
            }
        }

        if tuning.letterbox_16_9 {
            let imgh = screen_w * 9.0 * screen_ay / 16.0;
            let bar = ((screen_h - imgh) * 0.5).max(0.0);
            if bar > 1.0 {
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
    }

    fn emit_sky_glow(&self, rl: &mut RenderList, screen_w: f32, screen_h: f32, crimson: bool) {
        rl.begin_batch(BatchKey {
            texture: TextureSlot::FogCloud,
            blend: BlendMode::Additive,
            depth: DepthMode::None,
            action: pass_action::CLAMP_UV,
            target: TargetId::MainScreen,
        });
        let col = if crimson {
            [185, 24, 32, 110]
        } else {
            [42, 74, 168, 95]
        };
        rl.push_screen_rect(
            screen_w * 0.08,
            screen_h * 0.02,
            screen_w * 0.84,
            screen_h * 0.96,
            screen_w,
            screen_h,
            [0.0, 0.0, 1.0, 1.0],
            col,
            0,
        );
    }

    fn emit_towers(&self, rl: &mut RenderList, vp: Mat4, tuning: OpeningTuning) {
        rl.begin_batch(BatchKey {
            texture: TextureSlot::TowerWall,
            blend: BlendMode::SrcAlpha,
            depth: DepthMode::ReadWrite,
            action: 0,
            target: TargetId::MainScreen,
        });

        let light1 = Vec3::new(0.0, 0.0, -1.0);
        let light2 = Vec3::new(0.5, 0.5, -0.35).normalized();
        let light3 = Vec3::new(-0.5, -0.45, -0.25).normalized();
        let sway_amp = if tuning.reduced_motion { 0.15 } else { 0.55 };
        let half = 1.68_f32;

        for row in &self.towers {
            for cell in row {
                if !cell.active {
                    continue;
                }
                let height = 3.0 + cell.height_factor * 29.0;
                let sway_x = sinf(self.sim_time * 0.024 + cell.phase_a)
                    * cosf(self.sim_time * 0.017 + cell.phase_b)
                    * sway_amp
                    * cell.height_factor;
                let sway_y = cosf(self.sim_time * 0.021 + cell.phase_b)
                    * sway_amp
                    * 0.7
                    * cell.height_factor;

                let z_base = cell.base_pos.z;
                let z_top = z_base - height;
                let bx = cell.base_pos.x;
                let by = cell.base_pos.y;
                let tx = bx + sway_x;
                let ty = by + sway_y;

                // Top corners (nearer camera) & bottom corners (deeper into void)
                let t0 = Vec4::new(tx - half, ty - half, z_top, 1.0);
                let t1 = Vec4::new(tx + half, ty - half, z_top, 1.0);
                let t2 = Vec4::new(tx + half, ty + half, z_top, 1.0);
                let t3 = Vec4::new(tx - half, ty + half, z_top, 1.0);

                let b0 = Vec4::new(bx - half, by - half, z_base, 1.0);
                let b1 = Vec4::new(bx + half, by - half, z_base, 1.0);
                let b2 = Vec4::new(bx + half, by + half, z_base, 1.0);
                let b3 = Vec4::new(bx - half, by + half, z_base, 1.0);

                let shade = |n: Vec3, is_base: bool| -> [u8; 4] {
                    let d1 = n.dot(light1).max(0.0);
                    let d2 = n.dot(light2).max(0.0);
                    let d3 = n.dot(light3).max(0.0);
                    let lum = (0.28 + 0.52 * d1 + 0.34 * d2 + 0.20 * d3)
                        * (0.45 + 0.55 * cell.brightness);
                    let depth_fade = if is_base { 0.22 } else { 1.0 };
                    let r = clampf(lum * depth_fade * 148.0, 8.0, 235.0) as u8;
                    let g = clampf(lum * depth_fade * 172.0, 12.0, 245.0) as u8;
                    let b = clampf(lum * depth_fade * 224.0, 24.0, 255.0) as u8;
                    let a = if is_base { 90 } else { 248 };
                    [r, g, b, a]
                };

                // Top cap (-Z normal facing camera)
                let top_col = shade(Vec3::new(0.0, 0.0, -1.0), false);
                rl.push_clipped_quad([
                    RenderVertex::new(vp.apply(t0).into_arr(), [0.0, 0.0], top_col, 0),
                    RenderVertex::new(vp.apply(t1).into_arr(), [1.0, 0.0], top_col, 0),
                    RenderVertex::new(vp.apply(t2).into_arr(), [1.0, 1.0], top_col, 0),
                    RenderVertex::new(vp.apply(t3).into_arr(), [0.0, 1.0], top_col, 0),
                ]);

                // 4 monolith sides with base column fade (`vucode_1` column fade)
                let v_len = (height / 8.0).max(1.0);
                let sides = [
                    (t0, t1, b1, b0, Vec3::new(0.0, -1.0, 0.0)),
                    (t1, t2, b2, b1, Vec3::new(1.0, 0.0, 0.0)),
                    (t2, t3, b3, b2, Vec3::new(0.0, 1.0, 0.0)),
                    (t3, t0, b0, b3, Vec3::new(-1.0, 0.0, 0.0)),
                ];
                for (sa, sb, sc, sd, n) in sides {
                    let c_top = shade(n, false);
                    let c_bot = shade(n, true);
                    rl.push_clipped_quad([
                        RenderVertex::new(vp.apply(sa).into_arr(), [0.0, 0.0], c_top, 0),
                        RenderVertex::new(vp.apply(sb).into_arr(), [1.0, 0.0], c_top, 0),
                        RenderVertex::new(vp.apply(sc).into_arr(), [1.0, v_len], c_bot, 0),
                        RenderVertex::new(vp.apply(sd).into_arr(), [0.0, v_len], c_bot, 0),
                    ]);
                }
            }
        }
    }

    fn emit_opening_fog(&self, rl: &mut RenderList, vp: Mat4) {
        rl.begin_batch(BatchKey {
            texture: TextureSlot::FogCloud,
            blend: BlendMode::Additive,
            depth: DepthMode::ReadOnly,
            action: 0,
            target: TargetId::MainScreen,
        });

        let fog_radius = sqrtf(5202.0);
        for (layer, &phase) in self.fog_phases.iter().enumerate() {
            let z = 122.0 + (layer as f32) * 5.5;
            let u_scroll = phase + (layer as f32) * 0.13;
            let v_scroll = phase * 0.7 + (layer as f32) * 0.21;
            // 8x8 coarse grid matching the 17x17 `InitFog` span (-48..+48)
            for gy in 0_i32..8 {
                for gx in 0_i32..8 {
                    let x0 = ((gx - 4) as f32) * 12.0 + 2.0;
                    let y0 = ((gy - 4) as f32) * 12.0;
                    let x1 = x0 + 12.0;
                    let y1 = y0 + 12.0;

                    let fog_col = |wx: f32, wy: f32| -> [u8; 4] {
                        let dx = -5.1 - wx;
                        let dy = -wy;
                        let dist = sqrtf(dx * dx + dy * dy);
                        let d = clampf((fog_radius - dist * 1.15) * 62.0 / fog_radius, 0.0, 74.0);
                        [
                            (d * 0.28) as u8,
                            (d * 0.55) as u8,
                            (d * 1.35).min(120.0) as u8,
                            68,
                        ]
                    };

                    let p0 = vp.apply(Vec4::new(x0, y0, z, 1.0)).into_arr();
                    let p1 = vp.apply(Vec4::new(x1, y0, z, 1.0)).into_arr();
                    let p2 = vp.apply(Vec4::new(x1, y1, z, 1.0)).into_arr();
                    let p3 = vp.apply(Vec4::new(x0, y1, z, 1.0)).into_arr();

                    rl.push_clipped_quad([
                        RenderVertex::new(p0, [u_scroll, v_scroll], fog_col(x0, y0), 0),
                        RenderVertex::new(p1, [u_scroll + 0.5, v_scroll], fog_col(x1, y0), 0),
                        RenderVertex::new(p2, [u_scroll + 0.5, v_scroll + 0.5], fog_col(x1, y1), 0),
                        RenderVertex::new(p3, [u_scroll, v_scroll + 0.5], fog_col(x0, y1), 0),
                    ]);
                }
            }
        }
    }

    fn emit_orbit_lights(&self, rl: &mut RenderList, vp: Mat4) {
        rl.begin_batch(BatchKey {
            texture: TextureSlot::LightStreak,
            blend: BlendMode::Additive,
            depth: DepthMode::ReadOnly,
            action: pass_action::CLAMP_UV,
            target: TargetId::MainScreen,
        });

        for (l, light) in self.lights.iter().enumerate() {
            let base_rgb = LIGHT_COLORS[l % 4];
            if light.trail_len >= 2 {
                for step in 0..(light.trail_len - 1) {
                    let idx_a =
                        (light.trail_head + light.trail.len() - 1 - step) % light.trail.len();
                    let idx_b =
                        (light.trail_head + light.trail.len() - 2 - step) % light.trail.len();
                    let pa = light.trail[idx_a];
                    let pb = light.trail[idx_b];
                    let fade = 1.0 - (step as f32) / (light.trail_len as f32);
                    let half_w = 0.22 + 0.45 * fade;
                    let col = [
                        base_rgb[0],
                        base_rgb[1],
                        base_rgb[2],
                        (fade * fade * 155.0) as u8,
                    ];
                    let a0 = vp
                        .apply(Vec4::new(pa.x, pa.y - half_w, pa.z, 1.0))
                        .into_arr();
                    let a1 = vp
                        .apply(Vec4::new(pa.x, pa.y + half_w, pa.z, 1.0))
                        .into_arr();
                    let b1 = vp
                        .apply(Vec4::new(pb.x, pb.y + half_w, pb.z, 1.0))
                        .into_arr();
                    let b0 = vp
                        .apply(Vec4::new(pb.x, pb.y - half_w, pb.z, 1.0))
                        .into_arr();
                    rl.push_clipped_quad([
                        RenderVertex::new(a0, [0.0, 0.0], col, 0),
                        RenderVertex::new(a1, [0.0, 1.0], col, 0),
                        RenderVertex::new(b1, [1.0, 1.0], col, 0),
                        RenderVertex::new(b0, [1.0, 0.0], col, 0),
                    ]);
                }
            }
            if light.trail_len > 0 {
                let head_idx = (light.trail_head + light.trail.len() - 1) % light.trail.len();
                let p = light.trail[head_idx];
                let r = 2.6_f32;
                let col = [base_rgb[0], base_rgb[1], base_rgb[2], 235];
                rl.push_clipped_quad([
                    RenderVertex::new(
                        vp.apply(Vec4::new(p.x - r, p.y - r, p.z, 1.0)).into_arr(),
                        [0.0, 0.0],
                        col,
                        0,
                    ),
                    RenderVertex::new(
                        vp.apply(Vec4::new(p.x + r, p.y - r, p.z, 1.0)).into_arr(),
                        [1.0, 0.0],
                        col,
                        0,
                    ),
                    RenderVertex::new(
                        vp.apply(Vec4::new(p.x + r, p.y + r, p.z, 1.0)).into_arr(),
                        [1.0, 1.0],
                        col,
                        0,
                    ),
                    RenderVertex::new(
                        vp.apply(Vec4::new(p.x - r, p.y + r, p.z, 1.0)).into_arr(),
                        [0.0, 1.0],
                        col,
                        0,
                    ),
                ]);
            }
        }
    }

    fn emit_glass_cubes(
        &self,
        rl: &mut RenderList,
        cam_mat: Mat4,
        proj_mat: Mat4,
        illegal_red: bool,
        refraction_enabled: bool,
    ) {
        let mesh = if illegal_red {
            &self.cube_mesh_illegal
        } else {
            &self.cube_mesh_boot
        };
        let scroll = (self.sim_time * 0.0045).rem_euclid(1.0);

        for cube in &self.cubes {
            let model = Mat4::translate(cube.anchor.x, cube.anchor.y, cube.anchor.z)
                * Mat4::rot_z(cube.angles.z)
                * Mat4::rot_y(cube.angles.y)
                * Mat4::rot_x(cube.angles.x);
            let mv = cam_mat * model;
            let mvp = proj_mat * mv;

            // Capture current screen into BackdropTinted before each cube (`CubeCaptureBuffer`, `opening.c:1443`)
            if refraction_enabled {
                rl.emit_pass_action(pass_action::CAPTURE_TINTED_BACKDROP, TargetId::MainScreen);
            }

            // Pass 1: Back faces then front faces refracting the captured screen (`uv_offset = -0.084 * normal_xy`)
            for front_pass in [false, true] {
                if refraction_enabled {
                    rl.begin_batch(BatchKey {
                        texture: TextureSlot::BackdropTinted,
                        blend: BlendMode::SrcAlpha,
                        depth: DepthMode::ReadOnly,
                        action: pass_action::CLAMP_UV,
                        target: TargetId::MainScreen,
                    });
                } else {
                    rl.begin_batch(BatchKey {
                        texture: TextureSlot::CubeBlueDeep,
                        blend: BlendMode::SrcAlpha,
                        depth: DepthMode::ReadOnly,
                        action: 0,
                        target: TargetId::MainScreen,
                    });
                }

                for quad in mesh {
                    let vn = mv
                        .apply(Vec4::new(quad.normal.x, quad.normal.y, quad.normal.z, 0.0))
                        .xyz()
                        .normalized();
                    let center = quad.verts[0];
                    let view_pos = mv.apply(Vec4::new(center.x, center.y, center.z, 1.0)).xyz();
                    let is_front = vn.dot(view_pos) < 0.0;
                    if is_front != front_pass {
                        continue;
                    }

                    let fresnel = (1.0 - vn.z.abs()).powi(2);
                    let tint = if illegal_red {
                        [245, 96, 108, if is_front { 188 } else { 132 }]
                    } else {
                        [188, 212, 255, if is_front { 182 } else { 126 }]
                    };

                    let mut out_q = [RenderVertex::default(); 4];
                    for (vi, &v) in quad.verts.iter().enumerate() {
                        let clip = mvp.apply(Vec4::new(v.x, v.y, v.z, 1.0));
                        let mut suv = RenderList::clip_to_screen_uv(clip);
                        // Exact opening.c `-0.084f` screen-space normal refraction offset!
                        suv[0] = clampf(suv[0] - 0.084 * vn.x * (0.65 + 0.5 * fresnel), 0.01, 0.99);
                        suv[1] = clampf(suv[1] + 0.084 * vn.y * (0.65 + 0.5 * fresnel), 0.01, 0.99);
                        out_q[vi] = RenderVertex::new(clip.into_arr(), suv, tint, 0);
                    }
                    rl.push_clipped_quad(out_q);
                }
            }

            // Pass 2: Scrolling internal plasma (`TEXOBLPR` / `TEXOBLP`) + specular Fresnel reflection (`TEXOREF`)
            let plasma_tex = if illegal_red {
                TextureSlot::LensFlare
            } else {
                TextureSlot::CubeBluePurple
            };
            rl.begin_batch(BatchKey {
                texture: plasma_tex,
                blend: BlendMode::Additive,
                depth: DepthMode::ReadOnly,
                action: 0,
                target: TargetId::MainScreen,
            });
            for quad in mesh {
                let vn = mv
                    .apply(Vec4::new(quad.normal.x, quad.normal.y, quad.normal.z, 0.0))
                    .xyz()
                    .normalized();
                let fresnel = (1.0 - vn.z.abs()).powi(2);
                let col = if illegal_red {
                    [235, 42, 54, (55.0 + 95.0 * fresnel) as u8]
                } else {
                    [110, 148, 255, (48.0 + 88.0 * fresnel) as u8]
                };
                let mut out_q = [RenderVertex::default(); 4];
                for (vi, &v) in quad.verts.iter().enumerate() {
                    let clip = mvp.apply(Vec4::new(v.x, v.y, v.z, 1.0));
                    let uv = [quad.uvs[vi][0] + scroll, quad.uvs[vi][1] - scroll * 0.7];
                    out_q[vi] = RenderVertex::new(clip.into_arr(), uv, col, 0);
                }
                rl.push_clipped_quad(out_q);
            }

            rl.begin_batch(BatchKey {
                texture: TextureSlot::CubeReflect,
                blend: BlendMode::Additive,
                depth: DepthMode::ReadOnly,
                action: 0,
                target: TargetId::MainScreen,
            });
            for quad in mesh {
                let vn = mv
                    .apply(Vec4::new(quad.normal.x, quad.normal.y, quad.normal.z, 0.0))
                    .xyz()
                    .normalized();
                let view_pos = mv
                    .apply(Vec4::new(
                        quad.verts[0].x,
                        quad.verts[0].y,
                        quad.verts[0].z,
                        1.0,
                    ))
                    .xyz();
                if vn.dot(view_pos) >= 0.0 {
                    continue;
                }
                let fresnel = (1.0 - vn.z.abs()).powi(2);
                let spec_a = (45.0 + 145.0 * fresnel) as u8;
                let col = if illegal_red {
                    [255, 165, 170, spec_a]
                } else {
                    [225, 242, 255, spec_a]
                };
                let mut out_q = [RenderVertex::default(); 4];
                for (vi, &v) in quad.verts.iter().enumerate() {
                    let clip = mvp.apply(Vec4::new(v.x, v.y, v.z, 1.0));
                    let env_uv = [0.5 + 0.45 * vn.x + scroll * 0.25, 0.5 - 0.45 * vn.y];
                    out_q[vi] = RenderVertex::new(
                        clip.into_arr(),
                        env_uv,
                        col,
                        vertex_flags::PS2_MODULATE_2X,
                    );
                }
                rl.push_clipped_quad(out_q);
            }
        }
    }

    fn emit_red_flare(&self, rl: &mut RenderList, screen_w: f32, screen_h: f32) {
        let intensity = clampf((self.cam_pos.z - 420.0) / 740.0, 0.15, 1.0);
        let cx = screen_w * 0.5;
        let cy = screen_h * 0.48;

        // 7 spinning 16-gon flare rings + 5 `TEXOFLAR` lens sprites (`DrawRedFlare`)
        rl.begin_batch(BatchKey {
            texture: TextureSlot::LensFlare,
            blend: BlendMode::Additive,
            depth: DepthMode::None,
            action: pass_action::CLAMP_UV,
            target: TargetId::MainScreen,
        });

        for ring in 0..7 {
            let rf = ring as f32;
            let spin =
                self.sim_time * (0.008 + 0.003 * rf) * if ring % 2 == 0 { 1.0 } else { -1.0 };
            let rad_x = 46.0 + rf * 28.0 + sinf(spin * 2.0) * 6.0;
            let rad_y = rad_x * 0.52;
            let a = ((84.0 - rf * 8.5) * intensity) as u8;
            rl.push_screen_rect(
                cx - rad_x,
                cy - rad_y,
                rad_x * 2.0,
                rad_y * 2.0,
                screen_w,
                screen_h,
                [0.0, 0.0, 1.0, 1.0],
                [255, (48 + ring * 10) as u8, 44, a],
                0,
            );
        }

        for sp in 0_i32..5 {
            let sf = (sp - 2) as f32;
            let sx = cx + sf * 54.0 * cosf(self.cam_rot * 2.0);
            let sy = cy + sf * 22.0 * sinf(self.cam_rot * 2.0);
            let sz = 28.0 + (4 - sp) as f32 * 10.0;
            rl.push_screen_rect(
                sx - sz,
                sy - sz * 0.65,
                sz * 2.0,
                sz * 1.3,
                screen_w,
                screen_h,
                [0.0, 0.0, 1.0, 1.0],
                [255, 92, 76, (115.0 * intensity) as u8],
                0,
            );
        }
    }

    fn emit_illegal_fog(&self, rl: &mut RenderList, vp: Mat4) {
        rl.begin_batch(BatchKey {
            texture: TextureSlot::FogCloud,
            blend: BlendMode::Additive,
            depth: DepthMode::None,
            action: pass_action::CLAMP_UV,
            target: TargetId::MainScreen,
        });

        for patch in &self.red_fog_patches {
            let dz = patch.pos.z - self.cam_pos.z;
            if !(6.0..=520.0).contains(&dz) {
                continue;
            }
            let dist_fade = clampf(1.0 - (dz - 80.0).abs() / 420.0, 0.1, 1.0);
            let s = 26.0_f32;
            let (sn, cs) = (sinf(patch.rot_z), cosf(patch.rot_z));
            let col = [
                patch.center_lum.saturating_add(70),
                (u16::from(patch.center_lum) * 28 / 128) as u8,
                (u16::from(patch.center_lum) * 32 / 128) as u8,
                (dist_fade * 92.0) as u8,
            ];
            let corners = [(-s, -s), (s, -s), (s, s), (-s, s)];
            let mut q = [RenderVertex::default(); 4];
            let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
            for (i, &(lx, ly)) in corners.iter().enumerate() {
                let wx = patch.pos.x + lx * cs - ly * sn;
                let wy = patch.pos.y + lx * sn + ly * cs;
                let clip = vp.apply(Vec4::new(wx, wy, patch.pos.z, 1.0));
                q[i] = RenderVertex::new(clip.into_arr(), uvs[i], col, 0);
            }
            rl.push_clipped_quad(q);
        }
    }
}

trait Vec4Arr {
    fn into_arr(self) -> [f32; 4];
}

impl Vec4Arr for Vec4 {
    #[inline]
    fn into_arr(self) -> [f32; 4] {
        [self.x, self.y, self.z, self.w]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::SplitMix64;

    #[test]
    fn boot_mode_advances_through_states_and_reaches_completion() {
        let tuning = OpeningTuning::default();
        let mut scene = OpeningScene::new(OpeningVariant::Boot, 1234, tuning);
        let mut rl = RenderList::default();
        assert!(scene.active_tower_count > 0 && scene.active_tower_count <= 126);

        for f in 0..420 {
            scene.step(1.0, tuning);
            if f % 60 == 0 {
                scene.emit_frame(&mut rl, tuning);
                assert!(!rl.vertices.is_empty());
            }
            if scene.completed {
                break;
            }
        }
        assert!(
            scene.completed,
            "Boot timeline must reach completion within 420 frames"
        );
        assert!(scene.opening_state >= 3);
    }

    #[test]
    fn idle_mode_remains_bounded_over_ten_thousand_frames() {
        let tuning = OpeningTuning::default();
        let mut scene = OpeningScene::new(OpeningVariant::Idle, 99, tuning);
        for _ in 0..10_000 {
            scene.step(1.0, tuning);
        }
        assert!(!scene.completed);
        assert_eq!(scene.opening_state, 0);
        assert!(scene.cam_pos.x.abs() <= 10.0);
        assert!(scene.cam_pos.y.abs() <= 10.0);
        assert!((scene.cam_pos.z - 16.0).abs() <= 6.0);

        let mut rl = RenderList::default();
        scene.emit_frame(&mut rl, tuning);
        assert!(rl.vertices.len() > 300);
        for v in &rl.vertices {
            assert!(v.clip_pos.iter().all(|c| c.is_finite()));
        }
    }

    #[test]
    fn illegal_mode_climbs_from_672_to_800_and_emits_red_scene() {
        let tuning = OpeningTuning::default();
        let mut scene = OpeningScene::new(OpeningVariant::Illegal, 7, tuning);
        assert!((scene.cam_pos.z - 672.0).abs() < 1e-4);
        for _ in 0..180 {
            scene.step(1.0, tuning);
        }
        assert!(scene.cam_pos.z >= 800.0);
        assert!(scene.illegal_text_alpha > 0.0);

        let mut rl = RenderList::default();
        scene.emit_frame(&mut rl, tuning);
        assert!(rl.vertices.len() > 200);
    }

    #[test]
    fn property_fixed_seed_replays_bit_identically() {
        let mut sm = SplitMix64::new(0x05d5_7070_2026);
        let tuning = OpeningTuning::default();
        for _ in 0..32 {
            let seed = sm.next_u32();
            let mut a = OpeningScene::new(OpeningVariant::Boot, seed, tuning);
            let mut b = OpeningScene::new(OpeningVariant::Boot, seed, tuning);
            for _ in 0..45 {
                a.step(1.0, tuning);
                b.step(1.0, tuning);
            }
            let mut rla = RenderList::default();
            let mut rlb = RenderList::default();
            a.emit_frame(&mut rla, tuning);
            b.emit_frame(&mut rlb, tuning);
            assert_eq!(rla.vertices, rlb.vertices);
            assert_eq!(rla.batches, rlb.batches);
        }
    }
}
