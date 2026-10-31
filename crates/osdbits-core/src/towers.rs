//! PlayStation 2 OSDSYS boot-history tower field (`14x9` slots over `20x20` height grid)
//! and `vucode_1` / `towerchain.dsm` 6-quad vertex & lighting pipeline.

use crate::math::{
    clamp_f32, gs_all_behind_or_outside_guard, gs_projected_to_webgpu_clip, gs_transform_vertex,
    mat_apply, mat_mul, mat_rot_x, mat_rot_y, mat_rot_z, mat_trans, mat_unit, Mat4, TowerRng, Vec4,
    DEG2RAD_PS2, HALF_PI_PS2,
};

pub const TOWER_ROWS: usize = 14;
pub const TOWER_COLS: usize = 9;
pub const TOWER_SLOTS: usize = TOWER_ROWS * TOWER_COLS;

const TOWER_L: f32 = 0.707_106_77;

const HIST_TABLE_C: [f32; 14] = [
    0.2, 0.4, 0.6, 0.8, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
];

const HIST_TABLE_D: [f32; 14] = [
    0.1, 0.1, 0.1, 0.1, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0,
];

const HIST_CELLS: [[[u8; 2]; 6]; 21] = [
    [[0, 0], [0, 1], [0, 2], [0, 3], [1, 0], [1, 1]],
    [[1, 2], [2, 0], [2, 1], [2, 2], [3, 0], [4, 0]],
    [[4, 1], [5, 0], [5, 1], [6, 0], [6, 1], [6, 2]],
    [[7, 0], [7, 1], [8, 0], [8, 1], [8, 2], [8, 3]],
    [[9, 0], [9, 1], [10, 0], [10, 1], [10, 2], [11, 0]],
    [[12, 0], [12, 1], [13, 0], [13, 1], [13, 2], [13, 3]],
    [[1, 3], [2, 3], [3, 1], [3, 2], [3, 3], [4, 2]],
    [[5, 2], [5, 3], [5, 4], [6, 3], [7, 2], [7, 3]],
    [[9, 2], [9, 3], [9, 5], [10, 3], [10, 4], [10, 5]],
    [[11, 1], [11, 2], [11, 3], [11, 4], [12, 2], [12, 3]],
    [[0, 4], [1, 4], [2, 4], [2, 5], [2, 6], [3, 4]],
    [[3, 5], [3, 6], [4, 3], [4, 4], [4, 5], [4, 6]],
    [[5, 5], [5, 6], [6, 4], [6, 5], [7, 4], [7, 5]],
    [[8, 4], [8, 5], [8, 6], [9, 4], [9, 6], [9, 7]],
    [[12, 4], [13, 4], [13, 5], [13, 6], [13, 7], [13, 8]],
    [[0, 5], [0, 6], [1, 5], [1, 6], [1, 7], [2, 7]],
    [[0, 7], [0, 8], [1, 8], [2, 8], [3, 7], [3, 8]],
    [[4, 7], [4, 8], [5, 7], [5, 8], [6, 6], [6, 7]],
    [[6, 8], [7, 6], [7, 7], [7, 8], [8, 7], [8, 8]],
    [[9, 8], [10, 6], [10, 7], [10, 8], [11, 7], [11, 8]],
    [[11, 5], [11, 6], [12, 5], [12, 6], [12, 7], [12, 8]],
];

#[rustfmt::skip]
const HIST_SRC_POS: [[[f32; 3]; 9]; 14] = [
    [[-13.7428,11.7512,-2.5366],[-13.7428,10.4512,-2.5366],[-13.7428,9.1512,-2.5366],[-13.1428,7.8512,-2.5366],[-13.7428,6.5512,-6.5517],[-13.7428,5.2512,-2.5366],[-13.7428,3.9512,-2.5366],[-13.7428,2.6512,-2.5366],[-13.7428,1.3512,-2.5366]],
    [[-12.4428,11.7512,-2.5366],[-12.4428,10.4512,-2.5366],[-12.4428,9.1512,-2.5366],[-12.1428,7.8512,-0.933],[-12.4428,6.5512,-2.5366],[-12.4428,5.2512,-2.5366],[-12.4428,3.9512,-2.5366],[-12.4428,2.6512,-2.5366],[-12.4428,1.3512,-2.5366]],
    [[-11.1428,11.7512,-2.5366],[-11.1428,10.4512,-2.5366],[-11.1428,9.1512,-3.2422],[-11.1428,7.8512,-2.5366],[-11.1428,6.5512,-3.1781],[-11.1428,5.2512,-2.5366],[-11.1428,3.9512,-2.5366],[-11.1428,2.6512,-2.5366],[-11.1428,1.3512,-2.5366]],
    [[-9.8428,11.7512,-2.5366],[-9.8428,10.4512,-2.5366],[-9.8428,9.1512,-2.5366],[-9.8428,7.8512,-3.3705],[-9.8428,6.5512,-4.0119],[-9.8428,5.2512,-2.5366],[-9.8428,3.9512,-2.5366],[-9.8428,2.6512,-2.5366],[-9.8428,1.3512,-3.2422]],
    [[-8.5428,11.7512,-2.5366],[-8.5428,10.4512,-2.5366],[-8.5428,9.1512,-2.5366],[-8.5428,7.8512,-5.4054],[-8.5428,6.5512,-4.4491],[-8.5428,5.2512,-2.5366],[-8.5428,3.9512,-3.5629],[-8.5428,2.6512,-2.5366],[-8.5428,1.3512,-2.5366]],
    [[-7.2428,11.7512,-2.5366],[-7.2428,10.4512,-2.5366],[-7.2428,9.1512,-2.5366],[-7.2428,7.8512,-5.4097],[-7.2428,6.5512,-3.8195],[-7.2428,5.2512,-2.5366],[-7.2428,3.9512,-3.5629],[-7.2428,2.6512,-2.5366],[-7.2428,1.3512,-2.5366]],
    [[-5.9428,11.7512,-2.5366],[-5.9428,10.4512,-2.5366],[-5.9428,9.1512,-2.5366],[-5.9428,7.8512,-3.3705],[-5.9428,6.5512,-4.4491],[-5.9428,5.2512,-3.8195],[-5.9428,3.9512,-0.0309],[-5.9428,2.6512,-2.5366],[-5.9428,1.3512,-2.5366]],
    [[-4.6428,11.7512,-2.5366],[-4.6428,10.4512,-2.5366],[-4.6428,9.1512,-2.5366],[-4.6428,7.8512,-2.5366],[-4.6428,6.5512,-3.8195],[-4.6428,5.2512,-2.5366],[-4.6428,3.9512,-2.5366],[-4.6428,2.6512,-4.5251],[-4.6428,1.3512,-2.5366]],
    [[-3.3428,11.7512,-2.5366],[-3.3428,10.4512,-2.5366],[-3.3428,9.1512,-5.0383],[-3.3428,7.8512,-4.5892],[-3.3428,6.5512,-3.5629],[-3.3428,5.2512,-4.5766],[-3.3428,3.9512,-2.8536],[-3.3428,2.6512,-0.619],[-3.3428,1.3512,-2.5366]],
    [[-2.0428,11.7512,-2.5366],[-2.0428,10.4512,-2.5366],[-2.0428,9.1512,-2.5366],[-2.0428,7.8512,-2.5366],[-2.0428,6.5512,-3.3654],[-2.0428,5.2512,-2.8799],[-2.0428,3.9512,-2.5366],[-2.0428,2.6512,-2.5366],[-2.0428,1.3512,-2.5366]],
    [[-0.7428,11.7512,-2.5366],[-0.7428,10.4512,-2.5366],[-0.7428,9.1512,0.7348],[-0.7428,7.8512,-2.5366],[-0.7428,6.5512,-2.5366],[-0.7428,5.2512,-2.8554],[-0.7428,3.9512,-4.267],[-0.7428,2.6512,-2.5366],[-0.7428,1.3512,-2.5366]],
    [[0.5572,11.7512,-2.5366],[0.5572,10.4512,-2.5366],[0.5572,9.1512,-2.5366],[0.5572,7.8512,-3.0466],[0.5572,6.5512,-3.6204],[0.5572,5.2512,-2.8554],[0.5572,3.9512,-2.5366],[0.5572,2.6512,-2.5366],[0.5572,1.3512,-2.5366]],
    [[1.8572,11.7512,-2.5366],[1.8572,10.4512,-2.5366],[1.8572,9.1512,-2.5366],[1.8572,7.8512,-2.5366],[1.8572,6.5512,-2.5366],[1.8572,5.2512,-3.1741],[1.8572,3.9512,-2.5366],[1.8572,2.6512,-2.5366],[1.8572,1.3512,-2.5366]],
    [[3.1572,11.7512,-2.5366],[3.1572,10.4512,-2.5366],[3.1572,9.1512,-2.5366],[3.1572,7.8512,-2.5366],[3.1572,6.5512,-2.5366],[3.1572,5.2512,-2.5366],[3.1572,3.9512,-2.5366],[3.1572,2.6512,-2.5366],[3.1572,1.3512,-2.5366]],
];

/// Captured 21-slot retail memory-card boot history (`osdbits/opening.c:3305`) —
/// anonymized to count/mask/own/date triples (the title strings are never used by OSDSYS geometry).
const CAPTURED_RETAIL_SLOTS: [(u8, u8, u8); 21] = [
    (3, 1, 0),
    (1, 1, 0),
    (1, 1, 0),
    (1, 1, 0),
    (2, 1, 0),
    (6, 1, 0),
    (1, 1, 0),
    (1, 1, 0),
    (1, 1, 0),
    (1, 1, 0),
    (1, 1, 0),
    (1, 1, 0),
    (1, 1, 0),
    (1, 1, 0),
    (1, 1, 0),
    (1, 1, 0),
    (1, 1, 0),
    (1, 1, 0),
    (1, 1, 0),
    (1, 1, 0),
    (1, 1, 0),
];

/// Static geometry of the 5 non-degenerate faces of a tower (`TowerBlock5`, `1`, `2`, `3`, `4`
/// in `osdbits/towerchain.dsm`). `TowerBlock0` (`towerBlocks[5]`) has all `(0,0)` X/Y vertices
/// and zero area, so emitting the 5 visible faces (`0..5`) is 100% raster-identical.
struct TowerFaceTemplate {
    xy: [[f32; 2]; 4],
    far_sign: [bool; 4],
    normal: Vec4,
    is_cap: bool,
}

const TOWER_FACES: [TowerFaceTemplate; 5] = [
    // Block 5 (towerBlocks[0]): camera-facing cap at z = -f0
    TowerFaceTemplate {
        xy: [[2.0, -2.0], [-2.0, -2.0], [2.0, 2.0], [-2.0, 2.0]],
        far_sign: [false, false, false, false],
        normal: [0.0, 0.0, 1.0, 1.0],
        is_cap: true,
    },
    // Block 1 (towerBlocks[1]): -Y side wall with +Y normal
    TowerFaceTemplate {
        xy: [[2.0, -2.0], [2.0, -2.0], [-2.0, -2.0], [-2.0, -2.0]],
        far_sign: [false, true, false, true],
        normal: [0.0, 1.0, 0.0, 1.0],
        is_cap: false,
    },
    // Block 2 (towerBlocks[2]): +Y side wall with -Y normal
    TowerFaceTemplate {
        xy: [[2.0, 2.0], [2.0, 2.0], [-2.0, 2.0], [-2.0, 2.0]],
        far_sign: [true, false, true, false],
        normal: [0.0, -1.0, 0.0, 1.0],
        is_cap: false,
    },
    // Block 3 (towerBlocks[3]): +X side wall with -X normal
    TowerFaceTemplate {
        xy: [[2.0, -2.0], [2.0, 2.0], [2.0, -2.0], [2.0, 2.0]],
        far_sign: [true, true, false, false],
        normal: [-1.0, 0.0, 0.0, 1.0],
        is_cap: false,
    },
    // Block 4 (towerBlocks[4]): -X side wall with +X normal
    TowerFaceTemplate {
        xy: [[-2.0, -2.0], [-2.0, 2.0], [-2.0, -2.0], [-2.0, 2.0]],
        far_sign: [false, false, true, true],
        normal: [1.0, 0.0, 0.0, 1.0],
        is_cap: false,
    },
];

/// Configuration for how the 14x9 memory-card tower field is populated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TowerFieldPreset {
    /// Deterministic simulation using real memory-card sprawl & eviction rules (`opening.c:3380`).
    Simulated {
        seed: u32,
        ngames: u16,
        nboots: Option<u16>,
    },
    /// Captured 21-slot retail memory card preset (`TOWER_FIELD_CAPTURED = 1`).
    CapturedRetail,
    /// Diagnostic full-field mode (`ngames == 0`, all 126 towers at maximum height).
    FullField,
}

impl Default for TowerFieldPreset {
    fn default() -> Self {
        Self::Simulated {
            seed: 42,
            ngames: 16,
            nboots: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TowerSlot {
    pub row: u8,
    pub col: u8,
    pub enabled: bool,
    pub pos: Vec4,
    pub tower_a: f32,
    pub tower_b: f32,
    pub tower_c: f32,
    pub tower_d: f32,
    pub grid_val: f32,
    pub st_u_offset: f32,
    pub cell_rot_term: f32,
}

/// Packed vertex ready for WebGPU `tower.wgsl` (`12` floats = `48` bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PackedTowerVertex {
    /// WebGPU clip-space `(x, y, z, w)` with reverse-Z `[0, 1]`.
    pub clip_pos: [f32; 4],
    /// Normalized texture coordinates `(u, v)` (`[u_off, u_off + 1]`, clamped in sampler/shader) + `(q, pad)`.
    pub uv_q: [f32; 4],
    /// PS2 GS vertex color `(r, g, b, a)` in GS integer units (`0..255`, where `128 = 1.0`).
    pub gs_rgba: [f32; 4],
}

/// Computes `sub_217e30` (`HeightGrid`, `opening.c:3239`): 20x20 radial brightness grid in `[32, 220]`.
pub fn compute_height_grid() -> [[f32; 20]; 20] {
    let sq = 5202.0_f32.sqrt();
    let mut grid = [[32.0_f32; 20]; 20];
    for j in 0..20 {
        let gy = (j as f32 - 10.0) * 5.1 + 2.55;
        for (i, row) in grid.iter_mut().enumerate() {
            let gx = (i as f32 - 10.0) * 5.1 + 2.55;
            let d1 = 2.0 * ((-5.1 - gx) * (-5.1 - gx) + gy * gy).sqrt();
            let d2 = 4.0 * ((10.2 - gx) * (10.2 - gx) + (5.1 - gy) * (5.1 - gy)).sqrt();
            let mut v = clamp_f32(255.0 * (sq - d1) / sq, 32.0, 255.0)
                + clamp_f32(255.0 * (sq - d2) / sq * 0.5, 32.0, 255.0);
            v *= 0.85;
            let hash_term = 10 * ((((i + j) * i) / (j + 1)) % 11) as i32 - 50;
            v -= hash_term as f32;
            row[j] = clamp_f32(v, 32.0, 220.0);
        }
    }
    grid
}

/// Computes `vucode_1` directional lighting factor `(r, g, b, a)` for a given `light_matrix` and normal.
#[inline]
pub fn compute_vu1_tower_light(light_mat: &Mat4, normal: Vec4) -> Vec4 {
    let mut l = mat_apply(light_mat, normal);
    for comp in &mut l {
        if *comp < 0.0 {
            *comp = 0.0;
        }
    }
    // VF09=(1,1,1,0), VF10=(0.8,0.8,0.8,0), VF11=(0.8,0.8,0.8,0), VF12=(0.4,0.4,0.4,1)
    let rgb = l[0] * 1.0 + l[1] * 0.8 + l[2] * 0.8 + l[3] * 0.4;
    [rgb, rgb, rgb, l[3]]
}

#[derive(Clone, Debug)]
pub struct TowerField {
    pub grid: [[f32; 20]; 20],
    pub slots: [[TowerSlot; TOWER_COLS]; TOWER_ROWS],
    pub active_count: usize,
    light_source_quad: Mat4,
}

impl TowerField {
    pub fn new(preset: TowerFieldPreset) -> Self {
        let grid = compute_height_grid();
        let mut flags = [[false; TOWER_COLS]; TOWER_ROWS];
        let mut tower_c = [[0.0_f32; TOWER_COLS]; TOWER_ROWS];
        let mut tower_d = [[0.0_f32; TOWER_COLS]; TOWER_ROWS];

        match preset {
            TowerFieldPreset::CapturedRetail => {
                for (slot_idx, &(count, mask, own)) in CAPTURED_RETAIL_SLOTS.iter().enumerate() {
                    Self::apply_history_slot(
                        slot_idx,
                        count,
                        mask,
                        own,
                        &mut flags,
                        &mut tower_c,
                        &mut tower_d,
                    );
                }
            }
            TowerFieldPreset::FullField => {
                for r in 0..TOWER_ROWS {
                    for c in 0..TOWER_COLS {
                        flags[r][c] = true;
                        tower_c[r][c] = 0.0;
                        tower_d[r][c] = 1.0;
                    }
                }
            }
            TowerFieldPreset::Simulated {
                seed,
                ngames,
                nboots,
            } => {
                if ngames == 0 {
                    for r in 0..TOWER_ROWS {
                        for c in 0..TOWER_COLS {
                            flags[r][c] = true;
                            tower_c[r][c] = 0.0;
                            tower_d[r][c] = 1.0;
                        }
                    }
                } else {
                    let sim = Self::simulate_history(seed, ngames.max(1), nboots);
                    for (slot_idx, entry) in sim.iter().enumerate() {
                        if entry.occupied {
                            Self::apply_history_slot(
                                slot_idx,
                                entry.count,
                                entry.mask,
                                entry.own,
                                &mut flags,
                                &mut tower_c,
                                &mut tower_d,
                            );
                        }
                    }
                }
            }
        }

        let mut slots = [[TowerSlot::default(); TOWER_COLS]; TOWER_ROWS];
        let mut active_count = 0;

        for r in 0..TOWER_ROWS {
            for c in 0..TOWER_COLS {
                let mut pos = [
                    (HIST_SRC_POS[r][c][0] + 4.8) * 4.0,
                    (HIST_SRC_POS[r][c][1] - 6.5) * 4.0,
                    (HIST_SRC_POS[r][c][2] + 4.0) * 12.0 + 150.0,
                    0.0,
                ];
                let mut f0 = tower_d[r][c] * 30.0;
                if f0 < 3.0 {
                    f0 = 3.0;
                }
                let f3 = tower_c[r][c];
                pos[2] += f3 * 30.0 - f0;
                let (final_c, final_b) = if f3 >= 1.0 {
                    (1.0, 0.0)
                } else {
                    (f3, (((1.0 - f3) * 128.0) as i32) as f32)
                };

                let a0 = ((r + c + 9) * (r + 8)) / (c + 7) + ((r + c + 9) * (r + 7)) / (c + 9);
                let q = ((r + c + 9) * (r + 3)) / 7;
                let cell_rot_term = ((q % 4) as f32) * HALF_PI_PS2;

                if flags[r][c] {
                    active_count += 1;
                }

                slots[r][c] = TowerSlot {
                    row: r as u8,
                    col: c as u8,
                    enabled: flags[r][c],
                    pos,
                    tower_a: f0,
                    tower_b: final_b,
                    tower_c: final_c,
                    tower_d: tower_d[r][c],
                    grid_val: grid[r + 3][c + 6],
                    st_u_offset: (a0 as f32) / 256.0,
                    cell_rot_term,
                };
            }
        }

        let light_source_quad: Mat4 = [
            [0.0, TOWER_L, -TOWER_L, 0.0],
            [0.0, TOWER_L, -TOWER_L, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];

        Self {
            grid,
            slots,
            active_count,
            light_source_quad,
        }
    }

    fn apply_history_slot(
        slot_idx: usize,
        count: u8,
        mask: u8,
        own: u8,
        flags: &mut [[bool; TOWER_COLS]; TOWER_ROWS],
        tower_c: &mut [[f32; TOWER_COLS]; TOWER_ROWS],
        tower_d: &mut [[f32; TOWER_COLS]; TOWER_ROWS],
    ) {
        let count_usize = usize::from(count);
        let tidx = if count_usize < 14 {
            count_usize
        } else {
            (count_usize - 14) % 10 + 4
        };
        for t in 0..6 {
            let r = usize::from(HIST_CELLS[slot_idx][t][0]);
            let c = usize::from(HIST_CELLS[slot_idx][t][1]);
            if t == usize::from(own) {
                flags[r][c] = true;
                tower_c[r][c] = HIST_TABLE_C[tidx];
                tower_d[r][c] = HIST_TABLE_D[tidx];
            } else if ((mask >> t) & 1) != 0 {
                flags[r][c] = true;
                tower_c[r][c] = 1.0;
                tower_d[r][c] = 1.0;
            }
        }
    }

    fn simulate_history(
        seed: u32,
        ngames: u16,
        nboots_override: Option<u16>,
    ) -> [SimHistEntry; 21] {
        let mut rng = TowerRng::from_user_seed(seed);
        let ng = i32::from(ngames.max(1));
        let nboots = match nboots_override {
            Some(nb) => i32::from(nb),
            None => 80 + (rng.next_i15() % 80),
        };

        let mut sim = [SimHistEntry::default(); 21];
        for b in 0..nboots {
            let r1 = rng.next_i15() % ng;
            let r2 = rng.next_i15() % ng;
            let title = r1.min(r2) as u16;

            if let Some(existing) = sim.iter_mut().find(|e| e.occupied && e.title == title) {
                existing.date = b as u16;
                if (existing.mask & 0x3F) == 0x3F {
                    if existing.count < 63 {
                        existing.count += 1;
                    } else {
                        existing.own = 7;
                    }
                    continue;
                }
                if existing.count < 127 {
                    existing.count += 1;
                }
                if existing.count >= 14 && (existing.count - 14) % 10 == 0 {
                    let mut bit = (rng.next_i15() % 6) as u8;
                    while ((existing.mask >> bit) & 1) != 0 {
                        bit = (rng.next_i15() % 6) as u8;
                    }
                    existing.own = bit;
                    existing.mask |= 1 << bit;
                }
                continue;
            }

            let empty_count = sim.iter().filter(|e| !e.occupied).count();
            let slot_idx = if empty_count > 0 {
                let mut pick = (rng.next_i15() as usize) % empty_count;
                let mut chosen = 0;
                for (idx, e) in sim.iter().enumerate() {
                    if !e.occupied {
                        if pick == 0 {
                            chosen = idx;
                            break;
                        }
                        pick -= 1;
                    }
                }
                chosen
            } else {
                let mut evict_idx = 0;
                for idx in 1..21 {
                    if sim[idx].count < sim[evict_idx].count
                        || (sim[idx].count == sim[evict_idx].count
                            && sim[idx].date < sim[evict_idx].date)
                    {
                        evict_idx = idx;
                    }
                }
                evict_idx
            };

            let own = (rng.next_i15() % 6) as u8;
            sim[slot_idx] = SimHistEntry {
                occupied: true,
                title,
                count: 1,
                own,
                mask: 1 << own,
                date: b as u16,
            };
        }
        sim
    }

    /// Emits triangle vertices (`6` vertices per visible quad, `5` quads per visible tower) into `out`.
    /// Returns the number of towers drawn after frustum/guard-band culling.
    pub fn emit_towers(
        &self,
        frame_count: u32,
        camera_screen: &Mat4,
        screen_w: f32,
        screen_h: f32,
        reduced_motion: bool,
        out: &mut Vec<PackedTowerVertex>,
    ) -> u32 {
        let sway = if reduced_motion {
            0.0
        } else {
            (((frame_count % 360) as i32 - 180) as f32 * DEG2RAD_PS2).sin() * 10.0 * DEG2RAD_PS2
        };

        let base = mat_unit();
        let mut drawn_towers = 0_u32;

        for r in 0..TOWER_ROWS {
            for c in 0..TOWER_COLS {
                let slot = &self.slots[r][c];
                if !slot.enabled {
                    continue;
                }

                let angle_z = slot.cell_rot_term
                    + if (slot.tower_c - 1.0).abs() < 1e-6 {
                        0.0
                    } else {
                        sway
                    };
                let rotated = mat_mul(
                    &mat_rot_x(0.0),
                    &mat_mul(&mat_rot_y(0.0), &mat_mul(&mat_rot_z(angle_z), &base)),
                );
                let translated = mat_trans(&rotated, slot.pos);
                let obj_to_screen = mat_mul(camera_screen, &translated);

                let f0 = slot.tower_a;
                let bbox: [Vec4; 8] = [
                    [-2.0, -2.0, -f0, 1.0],
                    [2.0, -2.0, -f0, 1.0],
                    [-2.0, 2.0, -f0, 1.0],
                    [2.0, 2.0, -f0, 1.0],
                    [-2.0, -2.0, f0, 1.0],
                    [2.0, -2.0, f0, 1.0],
                    [-2.0, 2.0, f0, 1.0],
                    [2.0, 2.0, f0, 1.0],
                ];
                if gs_all_behind_or_outside_guard(&obj_to_screen, &bbox, 1.0, screen_w, screen_h) {
                    continue;
                }

                let mut light_mat = [[0.0_f32; 4]; 4];
                for k in 0..4 {
                    light_mat[k] = mat_apply(&self.light_source_quad, rotated[k]);
                }

                let bint = slot.tower_b as i32;
                let f12 = slot.grid_val
                    * if bint != 0 {
                        (bint as f32) / 128.0
                    } else {
                        slot.tower_a / 30.0
                    };
                let u_base = slot.st_u_offset;

                for face in &TOWER_FACES {
                    let s_col = if face.is_cap { f12 } else { f12 * 0.8 };
                    let light_factor = compute_vu1_tower_light(&light_mat, face.normal);

                    let mut quad_verts = [PackedTowerVertex::default(); 4];
                    for (v_idx, dst_v) in quad_verts.iter_mut().enumerate() {
                        let far = face.far_sign[v_idx];
                        let z = if far { f0 } else { -f0 };
                        let local_v = [face.xy[v_idx][0], face.xy[v_idx][1], z, 1.0];
                        let proj = gs_transform_vertex(&obj_to_screen, local_v);
                        let clip_pos = gs_projected_to_webgpu_clip(&proj, screen_w, screen_h, 1.0);

                        let base_rgb = if far { 0.0 } else { s_col };
                        let lit_r = (base_rgb * light_factor[0]).trunc();
                        let lit_g = (base_rgb * light_factor[1]).trunc();
                        let lit_b = (base_rgb * light_factor[2]).trunc();
                        let lit_a = (128.0 * light_factor[3]).trunc();

                        let u = u_base + ((v_idx & 1) as f32);
                        let v = u_base + ((v_idx >> 1) as f32);

                        *dst_v = PackedTowerVertex {
                            clip_pos,
                            uv_q: [u, v, proj.q, 0.0],
                            gs_rgba: [lit_r, lit_g, lit_b, lit_a],
                        };
                    }

                    // Expand 4-vertex GS TRISTRIP (0, 1, 2, 3) into 2 triangles: (0,1,2) and (2,1,3).
                    out.push(quad_verts[0]);
                    out.push(quad_verts[1]);
                    out.push(quad_verts[2]);
                    out.push(quad_verts[2]);
                    out.push(quad_verts[1]);
                    out.push(quad_verts[3]);
                }

                drawn_towers += 1;
            }
        }
        drawn_towers
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct SimHistEntry {
    occupied: bool,
    title: u16,
    count: u8,
    own: u8,
    mask: u8,
    date: u16,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{mat_camera, mat_view_screen};

    #[test]
    fn height_grid_matches_bounds_and_center_brightness() {
        let g = compute_height_grid();
        for row in &g {
            for &val in row {
                assert!((32.0..=220.0).contains(&val), "grid out of range: {val}");
            }
        }
        // Center of cluster around (9, 10) should be significantly brighter than outer edge (19, 19)
        assert!(g[9][10] > 170.0, "expected bright core, got {}", g[9][10]);
        assert!(
            g[19][19] < 90.0,
            "expected dim outer edge, got {}",
            g[19][19]
        );
        // Verify exact sub_217e30 dither at (0, 0): (32 + 32) * 0.85 - (-50) = 104.4
        assert!((g[0][0] - 104.4).abs() < 1e-3);
    }

    #[test]
    fn vu1_tower_lighting_matches_exact_cap_and_side_constants() {
        let field = TowerField::new(TowerFieldPreset::CapturedRetail);
        let cap = compute_vu1_tower_light(&field.light_source_quad, [0.0, 0.0, 1.0, 1.0]);
        assert!((cap[0] - 1.4).abs() < 1e-5);
        assert!((cap[3] - 1.0).abs() < 1e-5);

        for side_n in [
            [0.0, 1.0, 0.0, 1.0],
            [0.0, -1.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            [-1.0, 0.0, 0.0, 1.0],
        ] {
            let side = compute_vu1_tower_light(&field.light_source_quad, side_n);
            assert!(
                (side[0] - 0.965_685_4).abs() < 1e-4,
                "unexpected side lighting {} for {:?}",
                side[0],
                side_n
            );
        }
    }

    #[test]
    fn hist_cells_partition_all_126_cells_bijectively() {
        let mut seen = [[false; TOWER_COLS]; TOWER_ROWS];
        let mut total = 0;
        for slot in &HIST_CELLS {
            for &[r, c] in slot {
                let ru = usize::from(r);
                let cu = usize::from(c);
                assert!(!seen[ru][cu], "duplicate cell ({ru}, {cu})");
                seen[ru][cu] = true;
                total += 1;
            }
        }
        assert_eq!(total, 126);
    }

    #[test]
    fn simulated_captured_and_full_fields_are_deterministic_and_emit_valid_vertices() {
        let sim1 = TowerField::new(TowerFieldPreset::Simulated {
            seed: 1337,
            ngames: 18,
            nboots: Some(140),
        });
        let sim2 = TowerField::new(TowerFieldPreset::Simulated {
            seed: 1337,
            ngames: 18,
            nboots: Some(140),
        });
        assert_eq!(sim1.active_count, sim2.active_count);
        assert!(sim1.active_count >= 18);

        let full = TowerField::new(TowerFieldPreset::FullField);
        assert_eq!(full.active_count, 126);

        let retail = TowerField::new(TowerFieldPreset::CapturedRetail);
        assert_eq!(retail.active_count, 21);

        let cam = mat_camera(
            [0.0, 0.0, 16.0, 0.0],
            [0.0, 0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
        );
        let vs = mat_view_screen(
            1024.0,
            1.0,
            0.457_627,
            2048.0,
            2048.0,
            1.0,
            16_777_215.0,
            1.0,
            65536.0,
        );
        let cs = mat_mul(&vs, &cam);
        let mut out = Vec::new();
        let drawn = sim1.emit_towers(45, &cs, 640.0, 224.0, false, &mut out);
        assert!(drawn > 0);
        assert_eq!(out.len(), (drawn as usize) * 30);
        for v in &out {
            assert!(v.clip_pos.iter().all(|x| x.is_finite()));
            assert!(v.uv_q.iter().all(|x| x.is_finite()));
            assert!(v.gs_rgba.iter().all(|x| x.is_finite()));
        }
    }

    #[test]
    fn sprawl_eviction_sway_suppression_and_culling_invariants_hold() {
        // 1. Sprawl: 1 game played 35 times must sprawl at plays 14, 24, 34 -> 4 occupied cells:
        //    3 previously maxed cells (`tower_d == 1.0`, `tower_a == 30.0`) + 1 growing home cell (`tidx = 5`, `tower_d == 0.2`, `tower_a == 6.0`)!
        let sprawl_field = TowerField::new(TowerFieldPreset::Simulated {
            seed: 42,
            ngames: 1,
            nboots: Some(35),
        });
        assert_eq!(
            sprawl_field.active_count, 4,
            "expected home cell + 3 sprawled maxed towers at 35 plays"
        );
        let mut full_height_count = 0;
        let mut growing_sprawl_count = 0;
        for r in 0..TOWER_ROWS {
            for c in 0..TOWER_COLS {
                let s = &sprawl_field.slots[r][c];
                if s.enabled {
                    assert!(s.st_u_offset > 0.0 && s.st_u_offset < 1.0);
                    if (s.tower_d - 1.0).abs() < 1e-6 {
                        full_height_count += 1;
                    } else {
                        assert!((s.tower_d - 0.2).abs() < 1e-6);
                        growing_sprawl_count += 1;
                    }
                }
            }
        }
        assert_eq!(full_height_count, 3);
        assert_eq!(growing_sprawl_count, 1);

        // 2. Eviction: 40 distinct titles across 250 boots stays within 21 history slots cleanly
        let evicted = TowerField::new(TowerFieldPreset::Simulated {
            seed: 99,
            ngames: 40,
            nboots: Some(250),
        });
        assert!(evicted.active_count >= 21 && evicted.active_count <= 126);

        // 3. Sway suppression on maxed (`towerC == 1.0`) vs active sway on `FullField` (`towerC == 0.0`)
        // Build a field where ONLY maxed towers exist (`1` game played `5` times => `count=5` => `HIST_TABLE_C[5] == 1.0`!)
        let single_maxed = TowerField::new(TowerFieldPreset::Simulated {
            seed: 7,
            ngames: 1,
            nboots: Some(5),
        });
        assert_eq!(single_maxed.active_count, 1);

        let cam = mat_camera(
            [0.0, 0.0, 16.0, 0.0],
            [0.0, 0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
        );
        let vs = mat_view_screen(
            1024.0,
            1.0,
            0.457_627,
            2048.0,
            2048.0,
            1.0,
            16_777_215.0,
            1.0,
            65536.0,
        );
        let cs = mat_mul(&vs, &cam);

        let mut f90 = Vec::new();
        let mut f270 = Vec::new();
        single_maxed.emit_towers(90, &cs, 640.0, 224.0, false, &mut f90);
        single_maxed.emit_towers(270, &cs, 640.0, 224.0, false, &mut f270);
        assert_eq!(f90.len(), 30);
        // Maxed tower (`towerC == 1.0`) MUST stand completely still across frames 90 and 270!
        for (a, b) in f90.iter().zip(f270.iter()) {
            assert_eq!(a.clip_pos[0].to_bits(), b.clip_pos[0].to_bits());
            assert_eq!(a.clip_pos[1].to_bits(), b.clip_pos[1].to_bits());
        }
        // Check CLAMP_1 UV offset structure (`u_base` and `u_base + 1.0`)
        assert!((f90[1].uv_q[0] - f90[0].uv_q[0] - 1.0).abs() < 1e-6);

        // Non-maxed towers (`FullField`, `towerC == 0.0`) MUST sway between frames 90 (-10 deg) and 270 (+10 deg)!
        let full = TowerField::new(TowerFieldPreset::FullField);
        let mut sw90 = Vec::new();
        let mut sw270 = Vec::new();
        full.emit_towers(90, &cs, 640.0, 224.0, false, &mut sw90);
        full.emit_towers(270, &cs, 640.0, 224.0, false, &mut sw270);
        assert_ne!(
            sw90[0].clip_pos[0].to_bits(),
            sw270[0].clip_pos[0].to_bits()
        );

        // 4. Camera placed past the entire tower cluster (`z = 500`, looking `+z`) culls 100% of towers!
        let cam_past = mat_camera(
            [0.0, 0.0, 500.0, 0.0],
            [0.0, 0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
        );
        let cs_past = mat_mul(&vs, &cam_past);
        let mut culled = Vec::new();
        let drawn_past = full.emit_towers(0, &cs_past, 640.0, 224.0, false, &mut culled);
        assert_eq!(drawn_past, 0);
        assert!(culled.is_empty());
    }
}
