//! The four orbiting light streaks (the coloured "streaks" of the opening).
//!
//! Ported from `InitLightsCubes` / `DrawLights` in `osdbits/opening.c`:
//!
//! * `lightsSeed = osdRand()%2345 + 3456` — draw #386 of the osdRand stream
//!   (savestate-verified upstream; that is what the RNG ordering in
//!   `Scene::init_lights_cubes` reproduces).
//! * Per light and frame, two stacked billboards (coloured + white) are drawn
//!   at each of the four most recent history slots, with alpha ramping
//!   `lightAlphas[j]*(i+1)/5`.
//! * A 128-entry ring of projected trail vertices is drawn as a spaced
//!   line strip; the renderer draws it as a polyline of the same samples.

use crate::frame::{TEX_CRBL, TEX_NONE};
use crate::math::{Vec4, PI, TAU};

pub const COUNT: usize = 4;
pub const TRAIL_LEN: usize = 128;
pub const HISTORY: usize = 4;
/// `lightAlphas[]` — the two stacked quads' base alphas.
pub const LIGHT_ALPHAS: [f32; 2] = [24.0, 12.0];
/// `lightColors[]`, 0..255.
pub const LIGHT_COLORS: [[f32; 4]; 4] = [
    [32.0, 128.0, 0.0, 0.0],
    [128.0, 32.0, 64.0, 0.0],
    [128.0, 0.0, 0.0, 0.0],
    [64.0, 32.0, 128.0, 0.0],
];
/// `lightVertices[]` — the billboard's corner offsets (world units).
pub const LIGHT_VERTICES: [[f32; 3]; 4] = [
    [-0.8, -0.8, 0.0],
    [0.8, -0.8, 0.0],
    [-0.8, 0.8, 0.0],
    [0.8, 0.8, 0.0],
];
/// `lightTexCoords[]` — 0..1 UV quad.
pub const LIGHT_TEX_COORDS: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
/// How many ring entries apart the trail's polyline samples are (`j & 7`).
pub const TRAIL_SAMPLE_STEP: usize = 8;

#[derive(Clone, Debug)]
pub struct Lights {
    /// Most recent world position per light (index 3 of the history).
    pub positions: [[f32; 4]; COUNT],
    /// `lightMatrixHistory[4][4]`, stored as world positions per history slot.
    pub history: [[[f32; 4]; HISTORY]; COUNT],
    /// Ring buffer of trail sample positions.
    pub trail: [[[f32; 4]; TRAIL_LEN]; COUNT],
    pub trail_start: usize,
    pub trail_end: usize,
    /// Real `lightsSeed` (`0x217d80`), pinned by tests.
    pub seed: i32,
}

impl Lights {
    /// `lightTrailStart = lightTrailEnd = 0` and the seeded history.
    pub fn new(seed: i32) -> Self {
        Lights {
            positions: [[0.0; 4]; COUNT],
            history: [[[0.0; 4]; HISTORY]; COUNT],
            trail: [[[0.0; 4]; TRAIL_LEN]; COUNT],
            trail_start: 0,
            trail_end: 0,
            seed,
        }
    }

    /// Advance one frame (`DrawLights`' outer loop). Returns nothing: the
    /// draw list is produced by [`Lights::emit`].
    pub fn advance(&mut self, frame: u32) {
        for l in 0..COUNT {
            // Verbatim from DrawLights:
            //   c = cosf((frameCount + lightsSeed + l*17)*0.01f*(l+10)*0.1f)
            //   s = sinf((frameCount + lightsSeed + l*15)*0.005f*(l+10)*0.1f)
            let c = ((frame as f32 + self.seed as f32 + l as f32 * 17.0)
                * 0.01
                * (l as f32 + 10.0)
                * 0.1)
                .cos();
            let s = ((frame as f32 + self.seed as f32 + l as f32 * 15.0)
                * 0.005
                * (l as f32 + 10.0)
                * 0.1)
                .sin();
            let pos = [
                (10.0 - l as f32) * c,
                (3.0 + l as f32) * s,
                c * 12.0 + 88.0,
                0.0,
            ];
            self.positions[l] = pos;
            // Shift the history down, newest at index HISTORY-1.
            for i in 0..HISTORY - 1 {
                self.history[l][i] = self.history[l][i + 1];
            }
            self.history[l][HISTORY - 1] = pos;
            self.trail[l][self.trail_end] = pos;
        }
        self.trail_end = (self.trail_end + 1) % TRAIL_LEN;
        if self.trail_end == self.trail_start {
            self.trail_start = (self.trail_start + 1) % TRAIL_LEN;
        }
    }

    /// Number of live ring entries.
    pub fn trail_count(&self) -> usize {
        if self.trail_end >= self.trail_start {
            self.trail_end - self.trail_start
        } else {
            TRAIL_LEN - self.trail_start + self.trail_end
        }
    }

    /// The trail polyline for light `l`, newest last, sampled every
    /// `TRAIL_SAMPLE_STEP` ring entries exactly like the original's
    /// `(j & 7) != 0` skip. Bounded to 16 points.
    pub fn trail_points(&self, l: usize) -> Vec<[f32; 3]> {
        let mut out = Vec::with_capacity(16);
        let start = (self.trail_start + 1) % TRAIL_LEN;
        let end = (self.trail_end + TRAIL_LEN - 1) % TRAIL_LEN;
        let mut k = end;
        let mut j = 0usize;
        loop {
            if k == start || out.len() >= 16 {
                break;
            }
            if j % TRAIL_SAMPLE_STEP == 0 {
                let p = self.trail[l][k];
                out.push([p[0], p[1], p[2]]);
            }
            k = (k + TRAIL_LEN - 1) % TRAIL_LEN;
            j += 1;
        }
        out
    }

    /// Alpha of trail sample `j` in the original:
    /// `a = (intensity - j)*64/intensity`, where `intensity` is the live
    /// ring length minus one.
    pub fn trail_alpha(&self, j: usize) -> f32 {
        let intensity = self.trail_count().saturating_sub(1);
        if intensity == 0 {
            0.0
        } else {
            ((intensity as f32 - j as f32) * 64.0 / intensity as f32).max(0.0)
        }
    }
}

/// One emitted billboard quad, marker for the draw list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightSprite {
    pub pos: [f32; 3],
    pub half: f32,
    pub color: [f32; 3],
    pub alpha: f32,
    pub tex: u32,
}

/// Produce the 8 billboards of light `l` for the current history
/// (2 stacked quads x 4 history slots), matching DrawLights' inner loop.
pub fn light_sprites(lights: &Lights, l: usize) -> [LightSprite; 2 * HISTORY] {
    let mut out = [LightSprite {
        pos: [0.0; 3],
        half: 0.8,
        color: [0.0; 3],
        alpha: 0.0,
        tex: TEX_NONE,
    }; 2 * HISTORY];
    for (i, slot) in out.chunks_exact_mut(2).enumerate() {
        let p = lights.history[l][i];
        for (j, s) in slot.iter_mut().enumerate() {
            let base = LIGHT_COLORS[l];
            let color = if j == 0 {
                [
                    base[0] * 0.5 / 255.0,
                    base[1] * 0.5 / 255.0,
                    base[2] * 0.5 / 255.0,
                ]
            } else {
                [128.0 / 255.0; 3]
            };
            let alpha = LIGHT_ALPHAS[j] * (i as f32 + 1.0) / 5.0 / 128.0;
            *s = LightSprite {
                pos: [p[0], p[1], p[2]],
                half: 0.8,
                color,
                alpha,
                tex: TEX_CRBL,
            };
        }
    }
    out
}

/// Trail colour of light `l` (the original scales the light colour by the
/// per-sample alpha: `r = color[0]*a/128`).
pub fn trail_color(l: usize, alpha: f32) -> [f32; 4] {
    let c = LIGHT_COLORS[l];
    let a = (alpha / 128.0).clamp(0.0, 1.0);
    [
        c[0] * a / 255.0,
        c[1] * a / 255.0,
        c[2] * a / 255.0,
        (alpha * 2.0 / 255.0).clamp(0.0, 1.0),
    ]
}

/// Wrap an angle into (-PI, PI], as the original's `while` loops do.
pub fn wrap_pi(mut a: f32) -> f32 {
    while a > PI {
        a -= TAU;
    }
    while a < -PI {
        a += TAU;
    }
    a
}

/// Position of light `l` at a frame count, without mutating state — used by
/// tests and by the deterministic frame hash.
pub fn position_at(seed: i32, l: usize, frame: u32) -> Vec4 {
    let c = ((frame as f32 + seed as f32 + l as f32 * 17.0) * 0.01 * (l as f32 + 10.0) * 0.1).cos();
    let s =
        ((frame as f32 + seed as f32 + l as f32 * 15.0) * 0.005 * (l as f32 + 10.0) * 0.1).sin();
    [
        (10.0 - l as f32) * c,
        (3.0 + l as f32) * s,
        c * 12.0 + 88.0,
        0.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lights_history_fills_over_four_frames() {
        let mut l = Lights::new(4002);
        for f in 1..=4 {
            l.advance(f);
        }
        for i in 0..HISTORY {
            let want = position_at(4002, 0, (i + 1) as u32);
            assert!((l.history[0][i][0] - want[0]).abs() < 1e-5);
            assert!((l.history[0][i][2] - want[2]).abs() < 1e-5);
        }
    }

    #[test]
    fn trail_ring_is_bounded() {
        let mut l = Lights::new(4002);
        for f in 1..=10_000 {
            l.advance(f);
            assert!(l.trail_count() <= TRAIL_LEN);
            assert!(l.trail_start < TRAIL_LEN);
            assert!(l.trail_end < TRAIL_LEN);
        }
        assert_eq!(l.trail_count(), TRAIL_LEN - 1);
    }

    #[test]
    fn trail_points_are_sampled_and_bounded() {
        let mut l = Lights::new(4002);
        for f in 1..=TRAIL_LEN as u32 * 2 {
            l.advance(f);
        }
        for light in 0..COUNT {
            let pts = l.trail_points(light);
            assert!(pts.len() <= 16, "{} points", pts.len());
            assert!(pts.len() >= 8);
        }
    }

    #[test]
    fn trail_alpha_ramps_down() {
        let mut l = Lights::new(4002);
        for f in 1..=100 {
            l.advance(f);
        }
        let a0 = l.trail_alpha(0);
        let a8 = l.trail_alpha(8);
        assert!(a0 > a8, "{a0} {a8}");
        assert!(a0 <= 64.0);
    }

    #[test]
    fn sprites_have_monotonic_alpha_per_slot() {
        let mut l = Lights::new(4002);
        for f in 1..=4 {
            l.advance(f);
        }
        let s = light_sprites(&l, 1);
        assert_eq!(s.len(), 8);
        // Within a slot the white quad is dimmer than the coloured one; across
        // slots alpha grows with i.
        assert!(s[0].alpha >= s[1].alpha);
        assert!(s[6].alpha > s[0].alpha);
        assert_eq!(s[2].tex, TEX_CRBL);
    }

    #[test]
    fn positions_stay_inside_the_expected_band() {
        for f in 1..=2000u32 {
            for l in 0..COUNT {
                let p = position_at(4002, l, f);
                assert!(p[0].abs() <= 10.0 + 1e-3);
                assert!(p[1].abs() <= 6.0 + 1e-3);
                assert!((p[2] - 88.0).abs() <= 12.0 + 1e-3);
            }
        }
    }

    #[test]
    fn wrap_pi_is_idempotent() {
        for k in -10..10 {
            let a = k as f32 * 3.0;
            let w = wrap_pi(a);
            assert!((-PI..=PI).contains(&w));
            assert!((wrap_pi(w) - w).abs() < 1e-6);
        }
    }

    #[test]
    fn trail_colour_scales_with_alpha() {
        let c = trail_color(0, 64.0);
        assert!((c[0] - 32.0 * 0.5 / 255.0).abs() < 1e-6);
        let c = trail_color(0, 128.0);
        assert!((c[0] - 32.0 / 255.0).abs() < 1e-6);
        assert!((c[3] - 1.0).abs() < 1e-6);
    }
}
