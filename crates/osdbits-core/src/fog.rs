//! The opening's fog: six scrolling layers of 16x16 additive patches.
//!
//! Ported from `InitFog` / `DrawFog` in `osdbits/opening.c`. The 17x17
//! vertex grid is fixed for the whole scene; only the texture phase
//! `fogAnimation[6]` advances, and each layer is pushed back by `l*5` in z.

use crate::math::clamp;

pub const GRID: usize = 17;
pub const LAYERS: usize = 6;

/// Initial vertex grid (`InitFog`).
#[derive(Clone, Debug)]
pub struct Fog {
    /// `fogAnimation[6]` — per-layer UV phase in [0,1).
    pub animation: [f32; LAYERS],
    /// `fogVertices[17][17]` (x, y, z, w).
    pub vertices: [[[f32; 4]; GRID]; GRID],
    /// `fogColors[17][17]`, RGBA 0..255; only the blue channel is used.
    pub colors: [[[u8; 4]; GRID]; GRID],
}

impl Default for Fog {
    fn default() -> Self {
        Self::new()
    }
}

impl Fog {
    pub fn new() -> Self {
        let sq = 5202.0f32.sqrt(); // `sqrtf(5202.0f)` in InitFog
        let mut vertices = [[[0.0f32; 4]; GRID]; GRID];
        let mut colors = [[[0u8; 4]; GRID]; GRID];
        // The reconstruction multiplies the vertex offset by a dead
        // cos/sin pair (c = 1, s = 0); kept as the same literal values.
        let (c, s) = (1.0f32, 0.0f32);
        for y in 0..GRID {
            for x in 0..GRID {
                vertices[x][y][0] = (x as f32 - 8.0) * 6.0 + 2.0 * c;
                vertices[x][y][1] = (y as f32 - 8.0) * 6.0 + 2.0 * s;
                vertices[x][y][2] = 134.0;
                vertices[x][y][3] = 1.0;

                let dx = -5.1 - ((2.0 * x as f32 - 16.0) * 6.0 * 0.5 + 3.0);
                let dy = 0.0 - ((2.0 * y as f32 - 16.0) * 6.0 * 0.5 + 3.0);
                let d = (dx * dx + dy * dy).sqrt();
                let d = clamp((sq - d * 4.0) * 96.0 / sq, 0.0, 127.0);
                colors[x][y][0] = 0;
                colors[x][y][1] = 0;
                colors[x][y][2] = d as u8;
                colors[x][y][3] = 128;
            }
        }
        Fog {
            animation: [0.0; LAYERS],
            vertices,
            colors,
        }
    }

    /// Per-frame phase advance (`DrawFog`'s loop head).
    pub fn advance(&mut self) {
        for l in 0..LAYERS {
            self.animation[l] += (14.0 - l as f32) * 0.0001 * (l as f32 + 1.0) * 0.5;
            if self.animation[l] > 1.0 {
                self.animation[l] -= 1.0;
            }
        }
    }

    /// The z offset applied to layer `l` (`dz = l*5.0f`).
    pub fn layer_depth(l: usize) -> f32 {
        l as f32 * 5.0
    }

    /// Texture slot for layer `l`: `{FOG4, FOG2, FOG1}` cycling.
    pub fn layer_texture(l: usize) -> u32 {
        match l % 3 {
            0 => crate::frame::TEX_FOG4,
            1 => crate::frame::TEX_FOG2,
            _ => crate::frame::TEX_FOG1,
        }
    }

    /// RGBA of grid vertex `(x, y)` as the GS receives it
    /// (`RGBAQ(c/4, c*2/5, c, 128)`), normalised to 0..1.
    pub fn vertex_color(&self, x: usize, y: usize) -> [f32; 4] {
        let c = self.colors[x][y][2] as f32;
        [c / 4.0 / 255.0, c * 0.4 / 255.0, c / 255.0, 128.0 / 255.0]
    }

    /// UV of the four corners of cell `(x, y)` on layer `l`, in the order
    /// the GS tristrip receives them (`k = 0..3`).
    pub fn cell_uv(&self, l: usize, x: usize, y: usize, k: usize) -> [f32; 2] {
        let u = (k % 2) as f32 * 0.5 + (x % 2) as f32 * 0.5 - self.animation[l];
        let v = (k / 2) as f32 * 0.5 + (y % 2) as f32 * 0.5;
        [u, v]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_is_centred_and_at_the_documented_depth() {
        let f = Fog::new();
        assert_eq!(f.vertices[8][8][0], 2.0);
        assert_eq!(f.vertices[8][8][1], 0.0);
        assert_eq!(f.vertices[8][8][2], 134.0);
        assert_eq!(f.vertices[0][0][0], -48.0 + 2.0);
        assert_eq!(f.vertices[16][16][0], 48.0 + 2.0);
        assert_eq!(f.vertices[16][16][1], 48.0);
    }

    #[test]
    fn colours_are_blue_and_clamped() {
        let f = Fog::new();
        for x in 0..GRID {
            for y in 0..GRID {
                assert_eq!(f.colors[x][y][0], 0);
                assert_eq!(f.colors[x][y][1], 0);
                assert!(f.colors[x][y][2] <= 127);
                assert_eq!(f.colors[x][y][3], 128);
            }
        }
        // Centre is brighter than a corner.
        assert!(f.colors[8][8][2] > f.colors[0][16][2]);
    }

    #[test]
    fn vertex_colour_matches_gs_scaling() {
        let f = Fog::new();
        let c = f.colors[8][8][2] as f32;
        let col = f.vertex_color(8, 8);
        assert!((col[0] - c / 4.0 / 255.0).abs() < 1e-6);
        assert!((col[2] - c / 255.0).abs() < 1e-6);
        assert!((col[3] - 128.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn animation_advances_per_layer_and_wraps() {
        let mut f = Fog::new();
        f.advance();
        // Layer 0: (14-0)*0.0001*1*0.5 = 0.0007; layer 1: 13*0.0001*2*0.5 = 0.0013.
        assert!((f.animation[0] - 0.0007).abs() < 1e-7);
        assert!((f.animation[1] - 0.0013).abs() < 1e-7);
        // (14-l) shrinks but (l+1) grows faster, so higher layers scroll
        // faster: 0.0007, 0.0013, ... 0.0027.
        assert!((f.animation[5] - 0.0027).abs() < 1e-7);
        assert!(f.animation[5] > f.animation[0]);
        for _ in 0..5000 {
            f.advance();
            for a in f.animation {
                assert!((0.0..1.0).contains(&a), "phase out of range: {a}");
            }
        }
    }

    #[test]
    fn cell_uv_covers_half_texel_steps() {
        let mut f = Fog::new();
        f.animation[0] = 0.25;
        assert_eq!(f.cell_uv(0, 0, 0, 0), [-0.25, 0.0]);
        assert_eq!(f.cell_uv(0, 1, 0, 1), [0.75, 0.0]);
        assert_eq!(f.cell_uv(0, 1, 1, 2), [0.25, 1.0]);
        assert_eq!(f.cell_uv(0, 1, 1, 3), [0.75, 1.0]);
        // The per-cell offset depends on the cell's *parity*: the texture
        // repeats every two cells in both axes.
        assert_eq!(f.cell_uv(0, 2, 2, 0), f.cell_uv(0, 0, 0, 0));
        assert_eq!(f.cell_uv(0, 3, 3, 3), f.cell_uv(0, 1, 1, 3));
    }

    #[test]
    fn layer_textures_and_depths_follow_the_table() {
        assert_eq!(Fog::layer_texture(0), crate::frame::TEX_FOG4);
        assert_eq!(Fog::layer_texture(1), crate::frame::TEX_FOG2);
        assert_eq!(Fog::layer_texture(2), crate::frame::TEX_FOG1);
        assert_eq!(Fog::layer_texture(5), crate::frame::TEX_FOG1);
        assert_eq!(Fog::layer_depth(5), 25.0);
    }
}
