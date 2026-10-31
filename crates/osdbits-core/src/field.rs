//! Tower field: boot-history simulation, per-cell tower tables and the
//! radial brightness grid.
//!
//! Ported 1:1 from `osdbits/opening.c`:
//!
//! * `HeightGrid` (`0x217e30`) — the 20x20 radial brightness grid.
//! * `SimulateBootHistory` + `GenerateTowerField` — the memory-card history
//!   model: 21 entries, each owning a fixed 6-cell neighbourhood; the launch
//!   count selects tower height/brightness from `histTableC`/`histTableD`,
//!   and at count 14, 24, 34, … the title "sprawls" into another cell.
//! * `InitTowersFog` step 5 — alpha/halo tables and the `towerPos.z`
//!   adjustment.
//!
//! Deviations are called out inline; every one is recorded in
//! `docs/PORTING-NOTES.md`.

use crate::math::clamp;
use crate::rng::TowerRand;

pub const ROWS: usize = 14;
pub const COLS: usize = 9;
pub const GRID: usize = 20;
pub const HISTORY_SLOTS: usize = 21;
pub const NEIGHBOURHOOD: usize = 6;

/// `histTableC`, real `0x27aee0`.
pub const HIST_TABLE_C: [f32; 14] = [
    0.2, 0.4, 0.6, 0.8, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
];
/// `histTableD`, real `0x27af18`.
pub const HIST_TABLE_D: [f32; 14] = [
    0.1, 0.1, 0.1, 0.1, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0,
];

/// `histCells`, real `0x27a260`: 21 slots x 6 `(row, col)` cells, tiling all
/// 126 field cells exactly once.
pub const HIST_CELLS: [[[u8; 2]; NEIGHBOURHOOD]; HISTORY_SLOTS] = [
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

/// `histSrcPos`, real `0x27a670`: static per-cell source positions.
pub const HIST_SRC_POS: [[[f32; 3]; COLS]; ROWS] = [
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

/// One simulated memory-card history entry (`HistEntry` in the C).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HistEntry {
    /// `name[0..3]`: 'S' + the 16-bit title id (little endian).
    pub name: [u8; 16],
    pub count: u8,
    pub mask: u8,
    pub own: u8,
    pub pad: u8,
    pub date: u16,
}

impl HistEntry {
    pub fn occupied(&self) -> bool {
        self.name[0] != 0
    }

    pub fn title(&self) -> i32 {
        i32::from(self.name[1]) | (i32::from(self.name[2]) << 8)
    }
}

/// Simulated boot history: the real updater's rules, deterministically
/// driven by `TowerRand`.
///
/// `games` is the number of distinct titles the simulated card saw
/// (0 = "full field" test mode: the caller lights every cell),
/// `boots` the number of boot events applied.
pub fn simulate_boot_history(seed: u32, games: i32, boots: i32) -> [HistEntry; HISTORY_SLOTS] {
    let games = games.max(1);
    let mut rng = TowerRand::new(TowerRand::DEFAULT_SEED);
    rng.xor_seed(seed);
    let mut history = [HistEntry::default(); HISTORY_SLOTS];

    for b in 0..boots.max(0) {
        let r1 = rng.below(games);
        let r2 = rng.below(games);
        let title = r1.min(r2); // favourites-skew: the smaller draw wins
        let mut slot: Option<usize> = None;

        for (i, e) in history.iter().enumerate() {
            if e.occupied() && i32::from(e.name[1]) == title && i32::from(e.name[2]) == title >> 8 {
                slot = Some(i);
                break;
            }
        }

        let idx = match slot {
            None => {
                // Place in a RANDOM empty slot. (Not original: the real
                // updater fills empty slots front to back; the reconstruction
                // deliberately randomises so few-game fields spread out.)
                let empty: Vec<usize> = (0..HISTORY_SLOTS)
                    .filter(|&i| !history[i].occupied())
                    .collect();
                let idx = if empty.is_empty() {
                    // Full card: evict lowest count, tie-break oldest date.
                    let mut victim = 0usize;
                    for i in 1..HISTORY_SLOTS {
                        let a = &history[i];
                        let b = &history[victim];
                        if a.count < b.count || (a.count == b.count && a.date < b.date) {
                            victim = i;
                        }
                    }
                    victim
                } else {
                    empty[rng.below(empty.len() as i32) as usize]
                };
                let e = &mut history[idx];
                *e = HistEntry::default();
                e.name[0] = b'S';
                e.name[1] = (title & 0xff) as u8;
                e.name[2] = ((title >> 8) & 0xff) as u8;
                e.count = 1;
                // Random initial home cell too (real: always bit 0).
                e.own = rng.below(NEIGHBOURHOOD as i32) as u8;
                e.mask = 1 << e.own;
                e.date = b as u16;
                continue;
            }
            Some(i) => i,
        };

        let e = &mut history[idx];
        e.date = b as u16;
        if (e.mask & 0x3f) == 0x3f {
            if e.count < 63 {
                e.count += 1;
            } else {
                e.own = 7; // frozen: all six cells maxed
            }
            continue;
        }
        if e.count < 127 {
            e.count += 1;
        }
        if e.count >= 14 && (e.count - 14) % 10 == 0 {
            // The sprawl: a random unset neighbourhood bit becomes home.
            let mut bit;
            loop {
                bit = rng.below(NEIGHBOURHOOD as i32) as u8;
                if (e.mask >> bit) & 1 == 0 {
                    break;
                }
            }
            e.own = bit;
            e.mask |= 1 << bit;
        }
    }

    history
}

/// The tower field: per-cell flags and the four per-cell float tables from
/// `InitTowersFog` steps 2, 4 and 5, plus the 20x20 brightness grid.
pub struct TowerField {
    pub flags: [[u8; GRID]; GRID],
    pub grid: [[f32; GRID]; GRID],
    pub pos: [[[f32; 4]; COLS]; ROWS],
    pub a: [[f32; COLS]; ROWS],
    pub b: [[f32; COLS]; ROWS],
    pub c: [[f32; COLS]; ROWS],
    pub d: [[f32; COLS]; ROWS],
}

impl TowerField {
    /// `HeightGrid` (`0x217e30`), bit-exact per the reconstruction notes:
    /// a two-centre radial distance field, clamped to [32, 220], with a
    /// per-cell integer-hash dither.
    pub fn height_grid() -> [[f32; GRID]; GRID] {
        let sq = 5202.0f32.sqrt();
        let mut grid = [[0.0f32; GRID]; GRID];
        for j in 0..GRID {
            let gy = (j as f32 - 10.0) * 5.1 + 2.55;
            for i in 0..GRID {
                let gx = (i as f32 - 10.0) * 5.1 + 2.55;
                let d1 = 2.0 * ((-5.1 - gx) * (-5.1 - gx) + gy * gy).sqrt();
                let d2 = 4.0 * ((10.2 - gx) * (10.2 - gx) + (5.1 - gy) * (5.1 - gy)).sqrt();
                let mut v = clamp(255.0 * (sq - d1) / sq, 32.0, 255.0)
                    + clamp(255.0 * (sq - d2) / sq * 0.5, 32.0, 255.0);
                v *= 0.85;
                // Integer dither, C semantics: 10*(((i+j)*i)/(j+1) % 11 - 5).
                let hash = (10 * ((((i + j) * i) / (j + 1)) % 11) as i32 - 50) as f32;
                v -= hash;
                grid[i][j] = clamp(v, 32.0, 220.0);
            }
        }
        grid
    }

    /// Parse a boot history into the field, then apply the step-5
    /// alpha/halo tables exactly as `InitTowersFog` does.
    ///
    /// `full_field` reproduces the reconstruction's `ngames 0` test mode
    /// (every cell lit, C=0, D=1) — used by the "all towers" demo setting.
    pub fn generate(history: &[HistEntry; HISTORY_SLOTS], full_field: bool) -> Self {
        let mut f = TowerField {
            flags: [[0; GRID]; GRID],
            grid: Self::height_grid(),
            pos: [[[0.0; 4]; COLS]; ROWS],
            a: [[0.0; COLS]; ROWS],
            b: [[0.0; COLS]; ROWS],
            c: [[0.0; COLS]; ROWS],
            d: [[0.0; COLS]; ROWS],
        };

        for (i, e) in history.iter().enumerate() {
            if !e.occupied() {
                continue;
            }
            let count = e.count as i32;
            let mask = e.mask;
            let own = e.own;
            let tidx: usize = if count < 14 {
                count as usize
            } else {
                ((count - 14) % 10 + 4) as usize
            };
            for t in 0..NEIGHBOURHOOD {
                let r = HIST_CELLS[i][t][0] as usize;
                let c = HIST_CELLS[i][t][1] as usize;
                if t as u8 == own {
                    f.flags[r][c] = 1;
                    f.c[r][c] = HIST_TABLE_C[tidx];
                    f.d[r][c] = HIST_TABLE_D[tidx];
                } else if (mask >> t) & 1 != 0 {
                    f.flags[r][c] = 1;
                    f.c[r][c] = 1.0;
                    f.d[r][c] = 1.0;
                }
            }
        }

        if full_field {
            for r in 0..ROWS {
                for c in 0..COLS {
                    f.flags[r][c] = 1;
                    f.c[r][c] = 0.0;
                    f.d[r][c] = 1.0;
                }
            }
        }

        // Step 4: transform the static source positions.
        for r in 0..ROWS {
            for c in 0..COLS {
                let s = &HIST_SRC_POS[r][c];
                f.pos[r][c] = [
                    (s[0] + 4.8) * 4.0,
                    (s[1] - 6.5) * 4.0,
                    (s[2] + 4.0) * 12.0 + 150.0,
                    0.0,
                ];
            }
        }

        // Step 5: alpha/halo tables.
        for r in 0..ROWS {
            for c in 0..COLS {
                let mut f0 = f.d[r][c] * 30.0;
                if f0 < 3.0 {
                    f0 = 3.0;
                }
                f.a[r][c] = f0;
                let f3 = f.c[r][c];
                f.pos[r][c][2] += f3 * 30.0 - f0;
                if 1.0 <= f3 {
                    f.c[r][c] = 1.0;
                    f.b[r][c] = 0.0;
                } else {
                    f.b[r][c] = ((1.0 - f3) * 128.0) as i32 as f32;
                }
            }
        }

        f
    }

    /// Convenience: simulate the history for `seed`/`games`/`boots` and build
    /// the field.
    pub fn simulated(
        seed: u32,
        games: i32,
        boots: i32,
        full_field: bool,
    ) -> (Self, [HistEntry; 21]) {
        let history = simulate_boot_history(seed, games, boots);
        (Self::generate(&history, full_field), history)
    }

    pub fn lit_count(&self) -> usize {
        let mut n = 0;
        for r in 0..ROWS {
            for c in 0..COLS {
                if self.flags[r][c] != 0 {
                    n += 1;
                }
            }
        }
        n
    }

    /// `towerPatchST`'s per-tower pseudo-hash: `a0` in `0..=114`.
    pub fn st_offset(r: usize, c: usize) -> u32 {
        let (r, c) = (r as u32, c as u32);
        let a0 = (r + c + 9) * (r + 8) / (c + 7) + (r + c + 9) * (r + 7) / (c + 9);
        a0 % 256
    }

    /// `towerPatchVertices`'s colour numerator: `f12`.
    ///
    /// `gridval` is `towerGrid[r+3][c+6]`; `bint = (int)towerB`; when `bint`
    /// is zero the fallback is `towerA/30`.
    pub fn f12(&self, r: usize, c: usize) -> f32 {
        let gridval = self.grid[r + 3][c + 6];
        let bint = self.b[r][c] as i32;
        let scale = if bint != 0 {
            bint as f32 / 128.0
        } else {
            self.a[r][c] / 30.0
        };
        gridval * scale
    }

    /// Per-cell quantised 90-degree rotation term (`DrawTowers` `0x218830`).
    pub fn cell_rotation_term(r: usize, c: usize) -> f32 {
        let q = ((r + c + 9) * (r + 3)) / 7;
        (q % 4) as f32 * crate::math::PI_2
    }

    pub fn is_flat(&self, r: usize, c: usize) -> bool {
        self.c[r][c] == 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbourhoods_tile_the_field_exactly_once() {
        let mut seen = [[0u8; COLS]; ROWS];
        for slot in HIST_CELLS.iter() {
            for cell in slot.iter() {
                let (r, c) = (cell[0] as usize, cell[1] as usize);
                assert!(r < ROWS && c < COLS, "cell {r},{c} out of range");
                seen[r][c] += 1;
            }
        }
        for r in 0..ROWS {
            for c in 0..COLS {
                assert_eq!(seen[r][c], 1, "cell {r},{c} visited {} times", seen[r][c]);
            }
        }
    }

    #[test]
    fn history_table_sizes_and_ranges() {
        assert_eq!(HIST_TABLE_C.len(), 14);
        assert_eq!(HIST_TABLE_D.len(), 14);
        assert_eq!(HIST_CELLS.len(), HISTORY_SLOTS);
        assert_eq!(HIST_SRC_POS.len(), ROWS);
        for c in HIST_TABLE_C {
            assert!((0.0..=1.0).contains(&c));
        }
        for d in HIST_TABLE_D {
            assert!((0.0..=1.0).contains(&d));
        }
        // The count tables only start growing at index 4/5, as documented.
        assert_eq!(HIST_TABLE_D[0], 0.1);
        assert_eq!(HIST_TABLE_D[5], 0.2);
        assert_eq!(HIST_TABLE_C[0], 0.2);
        assert_eq!(HIST_TABLE_C[4], 1.0);
    }

    #[test]
    fn height_grid_is_clamped_and_has_two_bumps() {
        let g = TowerField::height_grid();
        let mut lo = f32::INFINITY;
        let mut hi = f32::NEG_INFINITY;
        for col in g.iter() {
            for &v in col.iter() {
                assert!((32.0..=220.0).contains(&v), "grid value {v}");
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        assert!(lo < 40.0, "min {lo}");
        assert!(hi > 200.0, "max {hi}");
        // The integer dither makes some neighbouring cells differ.
        let mut differing = 0;
        for i in 0..GRID - 1 {
            if g[i][0] != g[i + 1][0] {
                differing += 1;
            }
        }
        assert!(differing > 10, "dither had no effect ({differing})");
    }

    #[test]
    fn height_grid_matches_reference_cells() {
        // Reference values produced by an independent transcription of
        // sub_217e30 (see docs/PORTING-NOTES.md); tolerance 1e-3 covers the
        // f32 evaluations in a different order.
        let g = TowerField::height_grid();
        let cases = [
            (0usize, 0usize, 104.4f32),
            (10, 10, 208.191_45),
            (19, 19, 74.4),
            (5, 12, 102.106_12),
        ];
        for (i, j, want) in cases {
            let got = g[i][j];
            assert!(
                (got - want).abs() < 1e-3,
                "grid[{i}][{j}] = {got}, want {want}"
            );
        }
    }

    #[test]
    fn full_field_lights_every_cell() {
        let (field, _) = TowerField::simulated(7, 3, 5, true);
        assert_eq!(field.lit_count(), ROWS * COLS);
        for r in 0..ROWS {
            for c in 0..COLS {
                assert_eq!(field.d[r][c], 1.0);
                assert_eq!(field.c[r][c], 0.0);
                assert_eq!(field.a[r][c], 30.0);
            }
        }
    }

    #[test]
    fn fields_are_deterministic_for_the_same_seed() {
        let (a, _) = TowerField::simulated(4242, 8, 64, false);
        let (b, _) = TowerField::simulated(4242, 8, 64, false);
        assert_eq!(a.lit_count(), b.lit_count());
        for r in 0..ROWS {
            for c in 0..COLS {
                assert_eq!(a.flags[r][c], b.flags[r][c]);
                assert_eq!(a.a[r][c].to_bits(), b.a[r][c].to_bits());
                assert_eq!(a.b[r][c].to_bits(), b.b[r][c].to_bits());
                assert_eq!(a.pos[r][c][2].to_bits(), b.pos[r][c][2].to_bits());
            }
        }
    }

    #[test]
    fn different_seeds_produce_different_fields() {
        let (a, _) = TowerField::simulated(1, 8, 64, false);
        let (b, _) = TowerField::simulated(2, 8, 64, false);
        let mut same = 0;
        for r in 0..ROWS {
            for c in 0..COLS {
                if a.flags[r][c] == b.flags[r][c] && a.d[r][c] == b.d[r][c] {
                    same += 1;
                }
            }
        }
        assert!(same < ROWS * COLS, "seed had no effect");
    }

    #[test]
    fn history_respects_slot_and_count_bounds() {
        let history = simulate_boot_history(99, 16, 400);
        let mut occupied = 0;
        for e in history.iter() {
            assert!(e.own <= 7);
            assert!(e.count <= 127 || e.count == 63);
            if e.occupied() {
                occupied += 1;
                assert_eq!(e.name[0], b'S');
                assert!(e.mask != 0);
            }
        }
        assert!((1..=HISTORY_SLOTS).contains(&occupied));
    }

    #[test]
    fn full_card_evicts_lowest_count() {
        // One game booted many times fills at most one slot; 21 slots with a
        // single title must keep exactly one occupied entry.
        let history = simulate_boot_history(3, 1, 500);
        let occupied: Vec<_> = history.iter().filter(|e| e.occupied()).collect();
        assert_eq!(occupied.len(), 1);
        // Mask eventually saturates all six cells and own becomes 7.
        assert_eq!(occupied[0].mask, 0x3f);
        assert_eq!(occupied[0].own, 7);
    }

    #[test]
    fn st_offset_is_bounded_and_matches_formula() {
        for r in 0..ROWS {
            for c in 0..COLS {
                let a0 = (r + c + 9) * (r + 8) / (c + 7) + (r + c + 9) * (r + 7) / (c + 9);
                assert!(a0 <= 255);
                assert_eq!(TowerField::st_offset(r, c) as usize, a0);
            }
        }
        assert_eq!(TowerField::st_offset(0, 0), 9 * 8 / 7 + 9 * 7 / 9);
    }

    #[test]
    fn cell_rotation_term_is_quantised() {
        for r in 0..ROWS {
            for c in 0..COLS {
                let t = TowerField::cell_rotation_term(r, c);
                assert!((0.0..crate::math::TAU).contains(&t));
                let k = (t / crate::math::PI_2).round();
                assert!((t - k * crate::math::PI_2).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn flat_cells_are_marked_and_not_swayed() {
        let (field, _) = TowerField::simulated(5, 4, 200, false);
        let mut flat = 0;
        for r in 0..ROWS {
            for c in 0..COLS {
                if field.flags[r][c] != 0 {
                    assert!(field.a[r][c] >= 3.0);
                    if field.c[r][c] == 1.0 {
                        flat += 1;
                        assert_eq!(field.b[r][c], 0.0);
                    }
                }
            }
        }
        assert!(flat > 0, "expected some C==1 cells in a saturated field");
    }

    #[test]
    fn f12_uses_bint_or_a_fallback() {
        let (mut field, _) = TowerField::simulated(11, 4, 120, false);
        field.grid[3][6] = 100.0;
        field.b[0][0] = 0.0;
        field.a[0][0] = 30.0;
        assert!((field.f12(0, 0) - 100.0).abs() < 1e-4);
        field.b[0][0] = 64.0;
        assert!((field.f12(0, 0) - 50.0).abs() < 1e-4);
    }

    #[test]
    fn zero_boots_yields_empty_field() {
        let (field, history) = TowerField::simulated(1, 0, 0, false);
        assert!(history.iter().all(|e| !e.occupied()));
        assert_eq!(field.lit_count(), 0);
        // The static positions are still populated.
        assert!(field.pos[0][0][2] > 100.0);
    }
}
