//! Clean-room deterministic procedural texture, glyph-atlas, and `MENUGEOM`
//! mesh generator (`P02-002`).
//!
//! Contains zero proprietary Sony BIOS bytes, ROM textures, font tables, or
//! trademarks. Every asset is synthesized mathematically at startup.

use crate::math::{PI, TAU, Vec3, clampf, cosf, sinf, sqrtf};
use crate::render_list::TextureSlot;

#[derive(Clone, Debug)]
pub struct RgbaTexture {
    pub slot: TextureSlot,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaTexture {
    #[must_use]
    pub fn fnv1a64(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &b in &self.pixels {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MeshQuad {
    pub verts: [Vec3; 4],
    pub normal: Vec3,
    pub uvs: [[f32; 2]; 4],
}

/// Deterministic integer hash for clean-room procedural value noise.
#[inline]
fn hash2d(ix: i32, iy: i32, seed: u32) -> f32 {
    let mut n = (ix as u32)
        .wrapping_mul(374_761_393)
        .wrapping_add((iy as u32).wrapping_mul(668_265_263))
        ^ seed.wrapping_mul(2_246_822_519);
    n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    n ^= n >> 16;
    ((n & 0xffff) as f32) * (1.0 / 65_535.0)
}

/// Smooth periodic value noise tileable over period `period`.
fn tileable_noise(u: f32, v: f32, period: i32, seed: u32) -> f32 {
    let x = u * (period as f32);
    let y = v * (period as f32);
    let ix0 = (x.floor() as i32).rem_euclid(period);
    let iy0 = (y.floor() as i32).rem_euclid(period);
    let ix1 = (ix0 + 1).rem_euclid(period);
    let iy1 = (iy0 + 1).rem_euclid(period);
    let fx = x - x.floor();
    let fy = y - y.floor();
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);

    let n00 = hash2d(ix0, iy0, seed);
    let n10 = hash2d(ix1, iy0, seed);
    let n01 = hash2d(ix0, iy1, seed);
    let n11 = hash2d(ix1, iy1, seed);
    let nx0 = n00 + (n10 - n00) * sx;
    let nx1 = n01 + (n11 - n01) * sx;
    nx0 + (nx1 - nx0) * sy
}

fn tileable_fbm(u: f32, v: f32, base_period: i32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut p = base_period;
    for oct in 0..4u32 {
        sum += amp * tileable_noise(u, v, p, seed.wrapping_add(oct * 977));
        p *= 2;
        amp *= 0.5;
    }
    sum
}

#[inline]
fn to_u8(v: f32) -> u8 {
    (clampf(v, 0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Generate all 15 clean-room RGBA8 textures deterministically.
#[must_use]
pub fn generate_procedural_textures(seed: u32) -> Vec<RgbaTexture> {
    let mut out = Vec::with_capacity(15);

    // 1. TowerWall (128x128) — brushed cool slate monolith texture with subtle bevelled seams
    {
        let (w, h) = (128u32, 128u32);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let v = (y as f32 + 0.5) / (h as f32);
            let strata = tileable_fbm(0.15, v, 8, seed ^ 0x101);
            let top_cap_rim = if y < 6 {
                (6 - y) as f32 / 6.0 * 0.14
            } else {
                0.0
            };
            for x in 0..w {
                let u = (x as f32 + 0.5) / (w as f32);
                let fine = tileable_fbm(u, v, 16, seed ^ 0x202);
                let edge_x = (u * 2.0 - 1.0).abs();
                let bevel = 1.0 - 0.16 * edge_x.powi(4);
                let lum = clampf(
                    (0.56 + 0.24 * fine + 0.14 * strata + top_cap_rim) * bevel,
                    0.12,
                    0.96,
                );
                let idx = ((y * w + x) * 4) as usize;
                pixels[idx] = to_u8(lum * 0.84);
                pixels[idx + 1] = to_u8(lum * 0.90);
                pixels[idx + 2] = to_u8(lum * 1.00);
                pixels[idx + 3] = 255;
            }
        }
        out.push(RgbaTexture {
            slot: TextureSlot::TowerWall,
            width: w,
            height: h,
            pixels,
        });
    }

    // 2. FogCloud (64x64) — soft billowy cloud puff fading cleanly to zero at borders
    {
        let (w, h) = (64u32, 64u32);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let v = (y as f32 + 0.5) / (h as f32);
            let dy = v * 2.0 - 1.0;
            for x in 0..w {
                let u = (x as f32 + 0.5) / (w as f32);
                let dx = u * 2.0 - 1.0;
                let r = sqrtf(dx * dx + dy * dy);
                let radial = if r >= 1.0 {
                    0.0
                } else {
                    0.5 * (1.0 + cosf(r * PI))
                };
                let n = tileable_fbm(u, v, 6, seed ^ 0x303);
                let a = clampf(radial * (0.55 + 0.65 * n), 0.0, 1.0);
                let idx = ((y * w + x) * 4) as usize;
                pixels[idx] = 220;
                pixels[idx + 1] = 232;
                pixels[idx + 2] = 255;
                pixels[idx + 3] = to_u8(a);
            }
        }
        out.push(RgbaTexture {
            slot: TextureSlot::FogCloud,
            width: w,
            height: h,
            pixels,
        });
    }

    // 3. LightStreak (64x64) — soft radial/elliptical orb & ribbon glow
    {
        let (w, h) = (64u32, 64u32);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let dy = ((y as f32 + 0.5) / (h as f32)) * 2.0 - 1.0;
            for x in 0..w {
                let dx = ((x as f32 + 0.5) / (w as f32)) * 2.0 - 1.0;
                let r2 = dx * dx + dy * dy;
                let glow = clampf((1.0 - r2).max(0.0).powi(3), 0.0, 1.0);
                let idx = ((y * w + x) * 4) as usize;
                pixels[idx] = to_u8(0.75 + 0.25 * glow);
                pixels[idx + 1] = to_u8(0.88 + 0.12 * glow);
                pixels[idx + 2] = 255;
                pixels[idx + 3] = to_u8(glow);
            }
        }
        out.push(RgbaTexture {
            slot: TextureSlot::LightStreak,
            width: w,
            height: h,
            pixels,
        });
    }

    // 4. LensFlare (64x64) — multi-ring corona + soft center for Illegal-Disc & light glints
    {
        let (w, h) = (64u32, 64u32);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let dy = ((y as f32 + 0.5) / (h as f32)) * 2.0 - 1.0;
            for x in 0..w {
                let dx = ((x as f32 + 0.5) / (w as f32)) * 2.0 - 1.0;
                let r = sqrtf(dx * dx + dy * dy);
                let core = (1.0 - (r / 0.45).min(1.0)).powi(2);
                let ring = (1.0 - ((r - 0.68).abs() / 0.14).min(1.0)).powi(2) * 0.45;
                let outer = (1.0 - r.min(1.0)).powi(2) * 0.25;
                let v = clampf(core + ring + outer, 0.0, 1.0);
                let idx = ((y * w + x) * 4) as usize;
                pixels[idx] = to_u8(v);
                pixels[idx + 1] = to_u8(v * 0.85);
                pixels[idx + 2] = to_u8(v * 0.80);
                pixels[idx + 3] = to_u8(v);
            }
        }
        out.push(RgbaTexture {
            slot: TextureSlot::LensFlare,
            width: w,
            height: h,
            pixels,
        });
    }

    // 5. CubeBluePurple (`TEXOBLPR`, 64x64)
    {
        let (w, h) = (64u32, 64u32);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let v = (y as f32 + 0.5) / (h as f32);
            for x in 0..w {
                let u = (x as f32 + 0.5) / (w as f32);
                let wave = 0.5 + 0.5 * sinf((u + v) * TAU + 1.4 * sinf(u * TAU));
                let n = tileable_fbm(u, v, 4, seed ^ 0x505);
                let idx = ((y * w + x) * 4) as usize;
                pixels[idx] = to_u8(0.28 + 0.36 * wave + 0.15 * n);
                pixels[idx + 1] = to_u8(0.22 + 0.32 * (1.0 - wave) + 0.18 * n);
                pixels[idx + 2] = to_u8(0.62 + 0.34 * wave);
                pixels[idx + 3] = 220;
            }
        }
        out.push(RgbaTexture {
            slot: TextureSlot::CubeBluePurple,
            width: w,
            height: h,
            pixels,
        });
    }

    // 6. CubeBlueDeep (`TEXOBLP`, 64x64)
    {
        let (w, h) = (64u32, 64u32);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let v = (y as f32 + 0.5) / (h as f32);
            for x in 0..w {
                let u = (x as f32 + 0.5) / (w as f32);
                let caustic = 0.5 + 0.5 * cosf((u * 2.0 - v) * TAU);
                let n = tileable_fbm(u, v, 6, seed ^ 0x606);
                let idx = ((y * w + x) * 4) as usize;
                pixels[idx] = to_u8(0.10 + 0.20 * n);
                pixels[idx + 1] = to_u8(0.36 + 0.36 * caustic);
                pixels[idx + 2] = to_u8(0.68 + 0.30 * caustic);
                pixels[idx + 3] = 220;
            }
        }
        out.push(RgbaTexture {
            slot: TextureSlot::CubeBlueDeep,
            width: w,
            height: h,
            pixels,
        });
    }

    // 7. CubeReflect (`TEXOREF`, 64x64) — crisp diagonal glass specular reflection
    {
        let (w, h) = (64u32, 64u32);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let v = (y as f32 + 0.5) / (h as f32);
            for x in 0..w {
                let u = (x as f32 + 0.5) / (w as f32);
                let d1 = ((u * 0.8 + v * 0.6 - 0.72) * PI * 2.0)
                    .cos()
                    .max(0.0)
                    .powi(6);
                let d2 = ((u * 0.6 - v * 0.8 + 0.15) * PI * 2.0)
                    .cos()
                    .max(0.0)
                    .powi(10)
                    * 0.55;
                let spec = clampf(0.12 + 0.88 * (d1 + d2), 0.0, 1.0);
                let idx = ((y * w + x) * 4) as usize;
                pixels[idx] = to_u8(spec * 0.92);
                pixels[idx + 1] = to_u8(spec * 0.96);
                pixels[idx + 2] = to_u8(spec);
                pixels[idx + 3] = to_u8(spec);
            }
        }
        out.push(RgbaTexture {
            slot: TextureSlot::CubeReflect,
            width: w,
            height: h,
            pixels,
        });
    }

    // 8. SmokeTunnel (`TEXCKABE`, 64x64) — seamless domain-warped smoke for System Config cylinder
    {
        let (w, h) = (64u32, 64u32);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let v = (y as f32 + 0.5) / (h as f32);
            for x in 0..w {
                let u = (x as f32 + 0.5) / (w as f32);
                let warp_u = u + 0.12 * sinf(v * TAU);
                let warp_v = v + 0.12 * cosf(u * TAU);
                let smoke = tileable_fbm(
                    warp_u.rem_euclid(1.0),
                    warp_v.rem_euclid(1.0),
                    4,
                    seed ^ 0x808,
                );
                let swirl = 0.5 + 0.5 * sinf((u + v * 2.0 + smoke * 0.8) * TAU);
                let lum = clampf(0.28 + 0.52 * smoke + 0.22 * swirl, 0.08, 0.98);
                let idx = ((y * w + x) * 4) as usize;
                pixels[idx] = to_u8(lum * 0.92);
                pixels[idx + 1] = to_u8(lum * 0.95);
                pixels[idx + 2] = to_u8(lum);
                pixels[idx + 3] = 255;
            }
        }
        out.push(RgbaTexture {
            slot: TextureSlot::SmokeTunnel,
            width: w,
            height: h,
            pixels,
        });
    }

    // 9. OrbHalo (`TEXCBLUR`, 64x64)
    {
        let (w, h) = (64u32, 64u32);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let dy = ((y as f32 + 0.5) / (h as f32)) * 2.0 - 1.0;
            for x in 0..w {
                let dx = ((x as f32 + 0.5) / (w as f32)) * 2.0 - 1.0;
                let r = sqrtf(dx * dx + dy * dy);
                let halo = (1.0 - r.min(1.0)).powi(2);
                let idx = ((y * w + x) * 4) as usize;
                pixels[idx] = to_u8(halo);
                pixels[idx + 1] = to_u8(halo);
                pixels[idx + 2] = to_u8(halo);
                pixels[idx + 3] = to_u8(halo);
            }
        }
        out.push(RgbaTexture {
            slot: TextureSlot::OrbHalo,
            width: w,
            height: h,
            pixels,
        });
    }

    // 10. OrbCore (`TEXCNAVI`, 64x64)
    {
        let (w, h) = (64u32, 64u32);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let dy = ((y as f32 + 0.5) / (h as f32)) * 2.0 - 1.0;
            for x in 0..w {
                let dx = ((x as f32 + 0.5) / (w as f32)) * 2.0 - 1.0;
                let r = sqrtf(dx * dx + dy * dy);
                let core = (1.0 - (r / 0.55).min(1.0)).powi(3);
                let cross = ((1.0 - dx.abs() * 6.0).max(0.0) + (1.0 - dy.abs() * 6.0).max(0.0))
                    * (1.0 - r.min(1.0))
                    * 0.35;
                let v = clampf(core + cross, 0.0, 1.0);
                let idx = ((y * w + x) * 4) as usize;
                pixels[idx] = 255;
                pixels[idx + 1] = 255;
                pixels[idx + 2] = 255;
                pixels[idx + 3] = to_u8(v);
            }
        }
        out.push(RgbaTexture {
            slot: TextureSlot::OrbCore,
            width: w,
            height: h,
            pixels,
        });
    }

    // 11. GlassBump (`TEXCBUMP`, 64x64) & 12. GlassBumpInv (`TEXCBINV == ~TEXCBUMP`, 64x64)
    {
        let (w, h) = (64u32, 64u32);
        let mut bump = vec![0u8; (w * h * 4) as usize];
        let mut binv = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let v = (y as f32 + 0.5) / (h as f32);
            for x in 0..w {
                let u = (x as f32 + 0.5) / (w as f32);
                let bevel_u = sinf(u * PI);
                let bevel_v = sinf(v * PI);
                let ripple = 0.5 + 0.5 * sinf((u * 2.0 + v) * TAU);
                let n = tileable_fbm(u, v, 8, seed ^ 0xb0b);
                let val = to_u8(0.25 + 0.45 * (bevel_u * bevel_v) + 0.18 * ripple + 0.12 * n);
                let idx = ((y * w + x) * 4) as usize;
                bump[idx] = val;
                bump[idx + 1] = val;
                bump[idx + 2] = val;
                bump[idx + 3] = 255;

                // Exact bitwise complement per menuconfig.c:112 (`~TEXCBUMP`)
                binv[idx] = !val;
                binv[idx + 1] = !val;
                binv[idx + 2] = !val;
                binv[idx + 3] = 255;
            }
        }
        out.push(RgbaTexture {
            slot: TextureSlot::GlassBump,
            width: w,
            height: h,
            pixels: bump,
        });
        out.push(RgbaTexture {
            slot: TextureSlot::GlassBumpInv,
            width: w,
            height: h,
            pixels: binv,
        });
    }

    // 13. GlassFlow (`TEXCFLOW`, 64x64) — diagonal silk sheen across clock rods
    {
        let (w, h) = (64u32, 64u32);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let v = (y as f32 + 0.5) / (h as f32);
            for x in 0..w {
                let u = (x as f32 + 0.5) / (w as f32);
                let sheen = (0.5 + 0.5 * sinf((u + v) * TAU)).powi(3);
                let idx = ((y * w + x) * 4) as usize;
                pixels[idx] = to_u8(0.65 * sheen);
                pixels[idx + 1] = to_u8(0.84 * sheen);
                pixels[idx + 2] = to_u8(1.00 * sheen);
                pixels[idx + 3] = to_u8(sheen);
            }
        }
        out.push(RgbaTexture {
            slot: TextureSlot::GlassFlow,
            width: w,
            height: h,
            pixels,
        });
    }

    // 14. ConfigCubeRef (`TEXCREFA`, 64x64) — spherical environment highlight for config cubes
    {
        let (w, h) = (64u32, 64u32);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let dy = ((y as f32 + 0.5) / (h as f32)) * 2.0 - 1.0 + 0.25;
            for x in 0..w {
                let dx = ((x as f32 + 0.5) / (w as f32)) * 2.0 - 1.0 + 0.22;
                let r = sqrtf(dx * dx + dy * dy);
                let lobe = (1.0 - (r / 0.85).min(1.0)).powi(3);
                let band = (0.5 + 0.5 * cosf((dx - dy) * PI * 1.5)).powi(8) * 0.45;
                let spec = clampf(lobe * 0.75 + band, 0.0, 1.0);
                let idx = ((y * w + x) * 4) as usize;
                pixels[idx] = to_u8(spec * 0.88);
                pixels[idx + 1] = to_u8(spec * 0.95);
                pixels[idx + 2] = to_u8(spec);
                pixels[idx + 3] = to_u8(spec);
            }
        }
        out.push(RgbaTexture {
            slot: TextureSlot::ConfigCubeRef,
            width: w,
            height: h,
            pixels,
        });
    }

    // 15. UiGlyphAtlas (256x128) — 16x8 grid of 16x16 glyph cells (ASCII 32..=126 + 4 geometric button outlines)
    out.push(generate_clean_room_glyph_atlas());

    out
}

/// Compact clean-room 5x7 vector-stroke / bitmap glyph patterns for ASCII `32..=95`
/// (lowercase `a..z` map cleanly with lowercase x-height metrics) plus 4 geometric button icons (`128..=131`).
fn glyph_rows_5x7(code: u8) -> [u8; 7] {
    let ch = if code.is_ascii_lowercase() {
        code.to_ascii_uppercase()
    } else {
        code
    };
    match ch {
        b' ' => [0, 0, 0, 0, 0, 0, 0],
        b'!' => [0x04, 0x04, 0x04, 0x04, 0x04, 0x00, 0x04],
        b'"' => [0x0A, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00],
        b'#' => [0x0A, 0x1F, 0x0A, 0x0A, 0x1F, 0x0A, 0x00],
        b'%' => [0x18, 0x19, 0x02, 0x04, 0x08, 0x13, 0x03],
        b'(' => [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
        b')' => [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
        b'*' => [0x00, 0x04, 0x15, 0x0E, 0x15, 0x04, 0x00],
        b'+' => [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00],
        b',' => [0x00, 0x00, 0x00, 0x00, 0x04, 0x04, 0x08],
        b'-' => [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
        b'.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x06],
        b'/' => [0x01, 0x02, 0x02, 0x04, 0x08, 0x08, 0x10],
        b'0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        b'1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        b'2' => [0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F],
        b'3' => [0x1F, 0x02, 0x04, 0x06, 0x01, 0x11, 0x0E],
        b'4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        b'5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        b'6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        b'7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        b'8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        b'9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        b':' => [0x00, 0x06, 0x06, 0x00, 0x06, 0x06, 0x00],
        b';' => [0x00, 0x06, 0x06, 0x00, 0x06, 0x04, 0x08],
        b'<' => [0x02, 0x04, 0x08, 0x10, 0x08, 0x04, 0x02],
        b'=' => [0x00, 0x00, 0x1F, 0x00, 0x1F, 0x00, 0x00],
        b'>' => [0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08],
        b'?' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04],
        b'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        b'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        b'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        b'D' => [0x1C, 0x12, 0x11, 0x11, 0x11, 0x12, 0x1C],
        b'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        b'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        b'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        b'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        b'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        b'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        b'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        b'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        b'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        b'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        b'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        b'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        b'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        b'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        b'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        b'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        b'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        b'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        b'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11],
        b'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        b'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        b'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        b'[' => [0x0E, 0x08, 0x08, 0x08, 0x08, 0x08, 0x0E],
        b']' => [0x0E, 0x02, 0x02, 0x02, 0x02, 0x02, 0x0E],
        b'_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1F],
        _ => [0x0E, 0x11, 0x01, 0x06, 0x04, 0x00, 0x04],
    }
}

fn generate_clean_room_glyph_atlas() -> RgbaTexture {
    let (w, h) = (256u32, 128u32); // 16 cols x 8 rows of 16x16 cells -> 128 glyph slots
    let mut pixels = vec![0u8; (w * h * 4) as usize];

    let put_px = |buf: &mut [u8], px: u32, py: u32, alpha: u8| {
        if px < w && py < h {
            let idx = ((py * w + px) * 4) as usize;
            buf[idx] = 255;
            buf[idx + 1] = 255;
            buf[idx + 2] = 255;
            buf[idx + 3] = buf[idx + 3].max(alpha);
        }
    };

    // Slots 0..95 -> ASCII 32..=126
    for code in 32u8..=126u8 {
        let slot = (code - 32) as u32;
        let cell_x = (slot % 16) * 16;
        let cell_y = (slot / 16) * 16;
        let rows = glyph_rows_5x7(code);
        let is_lower = code.is_ascii_lowercase();

        for (ry, &bits) in rows.iter().enumerate() {
            for rx in 0..5u32 {
                if ((bits >> (4 - rx)) & 1) != 0 {
                    let gx = cell_x + 2 + rx * 2;
                    // Align both uppercase (height 13, y=1..14) and small-caps lowercase (height 10, y=4..14) on baseline y=14
                    let gy = if is_lower {
                        cell_y + 4 + ((ry as u32) * 10) / 6
                    } else {
                        cell_y + 1 + (ry as u32) * 2
                    };
                    put_px(&mut pixels, gx, gy, 255);
                    put_px(&mut pixels, gx + 1, gy, 240);
                    put_px(&mut pixels, gx, (gy + 1).min(cell_y + 14), 220);
                    put_px(&mut pixels, gx + 1, (gy + 1).min(cell_y + 14), 175);
                }
            }
        }
    }

    // Slots 96..=99 -> Clean geometric button outlines: Circle (96), Cross (97), Triangle (98), Square (99)
    for icon in 0..4u32 {
        let slot = 96 + icon;
        let cx = (slot % 16) * 16;
        let cy = (slot / 16) * 16;
        for py in 0..16u32 {
            let dy = (py as f32) - 7.5;
            for px in 0..16u32 {
                let dx = (px as f32) - 7.5;
                let dist = match icon {
                    0 => (sqrtf(dx * dx + dy * dy) - 5.2).abs(), // Circle outline
                    1 => {
                        // Cross outline
                        if dx.abs() <= 5.2 && dy.abs() <= 5.2 {
                            let inv_rt2 = core::f32::consts::FRAC_1_SQRT_2;
                            ((dx - dy).abs() * inv_rt2).min((dx + dy).abs() * inv_rt2)
                        } else {
                            9.0
                        }
                    }
                    2 => {
                        // Triangle outline
                        let top = dy + 5.2;
                        let bot = (4.8 - dy).abs();
                        let left = ((dx * 1.75 + dy - 1.5).abs()) * 0.49;
                        let right = ((-dx * 1.75 + dy - 1.5).abs()) * 0.49;
                        if top >= -0.8 && dy <= 5.6 && dx.abs() <= (dy + 6.0) * 0.62 {
                            bot.min(left).min(right)
                        } else {
                            9.0
                        }
                    }
                    _ => {
                        // Square outline
                        let mx = dx.abs().max(dy.abs());
                        (mx - 5.0).abs()
                    }
                };
                if dist < 1.35 {
                    let a = to_u8(1.0 - (dist / 1.35));
                    put_px(&mut pixels, cx + px, cy + py, a);
                }
            }
        }
    }

    RgbaTexture {
        slot: TextureSlot::UiGlyphAtlas,
        width: w,
        height: h,
        pixels,
    }
}

/// Build the exact 16-quad bevelled hexagonal prism rod mesh documented in `docs/menu-config.md`:
/// - Regular hexagon of circumradius `2.6` (`y = 0..26.0`)
/// - Bevelled cap hexagon of circumradius `2.34` at `y = 26.39`
/// - Faces `0..=1`: top cap (`y = 26.39`), `2..=7`: 6 bevel quads (`26.0..26.39`),
///   `8..=9`: bottom cap (`y = 0.0`), `10..=15`: 6 prism side quads (`0.0..26.0`, split by active-hour minute fill!).
#[must_use]
pub fn build_hex_rod_mesh() -> [MeshQuad; 16] {
    let r_outer = 2.6_f32;
    let r_bevel = 2.34_f32;
    let y_bot = 0.0_f32;
    let y_mid = 26.0_f32;
    let y_top = 26.39_f32;

    let mut hex_outer = [Vec3::default(); 6];
    let mut hex_bevel = [Vec3::default(); 6];
    for i in 0..6 {
        let ang = (i as f32) * (TAU / 6.0);
        let (c, s) = (cosf(ang), sinf(ang));
        hex_outer[i] = Vec3::new(r_outer * c, 0.0, r_outer * s);
        hex_bevel[i] = Vec3::new(r_bevel * c, 0.0, r_bevel * s);
    }

    let with_y = |v: Vec3, y: f32| Vec3::new(v.x, y, v.z);
    let quad_uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

    let mut quads = [MeshQuad {
        verts: [Vec3::default(); 4],
        normal: Vec3::new(0.0, 1.0, 0.0),
        uvs: quad_uv,
    }; 16];

    // 0..=1: top cap (split hexagon 0-1-2-3 and 0-3-4-5)
    quads[0].verts = [
        with_y(hex_bevel[0], y_top),
        with_y(hex_bevel[1], y_top),
        with_y(hex_bevel[2], y_top),
        with_y(hex_bevel[3], y_top),
    ];
    quads[0].normal = Vec3::new(0.0, 1.0, 0.0);

    quads[1].verts = [
        with_y(hex_bevel[0], y_top),
        with_y(hex_bevel[3], y_top),
        with_y(hex_bevel[4], y_top),
        with_y(hex_bevel[5], y_top),
    ];
    quads[1].normal = Vec3::new(0.0, 1.0, 0.0);

    // 2..=7: 6 bevel faces (y_mid -> y_top)
    for i in 0..6 {
        let j = (i + 1) % 6;
        let a = with_y(hex_outer[i], y_mid);
        let b = with_y(hex_outer[j], y_mid);
        let c = with_y(hex_bevel[j], y_top);
        let d = with_y(hex_bevel[i], y_top);
        let e1 = Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z);
        let e2 = Vec3::new(d.x - a.x, d.y - a.y, d.z - a.z);
        quads[2 + i] = MeshQuad {
            verts: [a, b, c, d],
            normal: e2.cross(e1).normalized(),
            uvs: quad_uv,
        };
    }

    // 8..=9: bottom cap (y = 0)
    quads[8].verts = [
        with_y(hex_outer[3], y_bot),
        with_y(hex_outer[2], y_bot),
        with_y(hex_outer[1], y_bot),
        with_y(hex_outer[0], y_bot),
    ];
    quads[8].normal = Vec3::new(0.0, -1.0, 0.0);

    quads[9].verts = [
        with_y(hex_outer[5], y_bot),
        with_y(hex_outer[4], y_bot),
        with_y(hex_outer[3], y_bot),
        with_y(hex_outer[0], y_bot),
    ];
    quads[9].normal = Vec3::new(0.0, -1.0, 0.0);

    // 10..=15: 6 main prism side faces (verts[0..1] at y=0, verts[2..3] at y=26.0)
    for i in 0..6 {
        let j = (i + 1) % 6;
        let a = with_y(hex_outer[i], y_bot);
        let b = with_y(hex_outer[j], y_bot);
        let c = with_y(hex_outer[j], y_mid);
        let d = with_y(hex_outer[i], y_mid);
        let mid_n = Vec3::new(
            hex_outer[i].x + hex_outer[j].x,
            0.0,
            hex_outer[i].z + hex_outer[j].z,
        )
        .normalized();
        quads[10 + i] = MeshQuad {
            verts: [a, b, c, d],
            normal: mid_n,
            uvs: [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        };
    }

    quads
}

/// Build an axis-aligned 6-quad cube mesh of half-extent `h` (`1.8` Opening, `1.2` Illegal, `2.64` Config).
#[must_use]
pub fn build_cube_mesh(h: f32, uv_max: f32) -> [MeshQuad; 6] {
    let uvs = [[0.0, 0.0], [uv_max, 0.0], [uv_max, uv_max], [0.0, uv_max]];
    [
        // +Z
        MeshQuad {
            verts: [
                Vec3::new(-h, -h, h),
                Vec3::new(h, -h, h),
                Vec3::new(h, h, h),
                Vec3::new(-h, h, h),
            ],
            normal: Vec3::new(0.0, 0.0, 1.0),
            uvs,
        },
        // -Z
        MeshQuad {
            verts: [
                Vec3::new(h, -h, -h),
                Vec3::new(-h, -h, -h),
                Vec3::new(-h, h, -h),
                Vec3::new(h, h, -h),
            ],
            normal: Vec3::new(0.0, 0.0, -1.0),
            uvs,
        },
        // +X
        MeshQuad {
            verts: [
                Vec3::new(h, -h, h),
                Vec3::new(h, -h, -h),
                Vec3::new(h, h, -h),
                Vec3::new(h, h, h),
            ],
            normal: Vec3::new(1.0, 0.0, 0.0),
            uvs,
        },
        // -X
        MeshQuad {
            verts: [
                Vec3::new(-h, -h, -h),
                Vec3::new(-h, -h, h),
                Vec3::new(-h, h, h),
                Vec3::new(-h, h, -h),
            ],
            normal: Vec3::new(-1.0, 0.0, 0.0),
            uvs,
        },
        // +Y
        MeshQuad {
            verts: [
                Vec3::new(-h, h, h),
                Vec3::new(h, h, h),
                Vec3::new(h, h, -h),
                Vec3::new(-h, h, -h),
            ],
            normal: Vec3::new(0.0, 1.0, 0.0),
            uvs,
        },
        // -Y
        MeshQuad {
            verts: [
                Vec3::new(-h, -h, -h),
                Vec3::new(h, -h, -h),
                Vec3::new(h, -h, h),
                Vec3::new(-h, -h, h),
            ],
            normal: Vec3::new(0.0, -1.0, 0.0),
            uvs,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procedural_textures_are_deterministic_and_satisfy_complement_law() {
        let a = generate_procedural_textures(0x05d5_2000);
        let b = generate_procedural_textures(0x05d5_2000);
        assert_eq!(a.len(), 15);
        for (ta, tb) in a.iter().zip(b.iter()) {
            assert_eq!(ta.slot, tb.slot);
            assert_eq!(ta.pixels.len(), (ta.width * ta.height * 4) as usize);
            assert_eq!(ta.fnv1a64(), tb.fnv1a64());
        }

        let bump = a.iter().find(|t| t.slot == TextureSlot::GlassBump).unwrap();
        let binv = a
            .iter()
            .find(|t| t.slot == TextureSlot::GlassBumpInv)
            .unwrap();
        for (cb, ci) in bump
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(binv.pixels.as_chunks::<4>().0)
        {
            assert_eq!(ci[0], !cb[0]);
            assert_eq!(ci[1], !cb[1]);
            assert_eq!(ci[2], !cb[2]);
            assert_eq!(ci[3], 255);
        }
    }

    #[test]
    fn hex_rod_and_cube_meshes_have_unit_normals_and_exact_dimensions() {
        let rod = build_hex_rod_mesh();
        assert_eq!(rod.len(), 16);
        for q in &rod {
            assert!((q.normal.length() - 1.0).abs() < 1e-3);
        }
        // Side faces span y = 0..26.0
        assert!((rod[10].verts[0].y - 0.0).abs() < 1e-5);
        assert!((rod[10].verts[2].y - 26.0).abs() < 1e-5);
        // Top cap sits at y = 26.39
        assert!((rod[0].verts[0].y - 26.39).abs() < 1e-5);

        let cube = build_cube_mesh(2.64, 1.5);
        assert_eq!(cube.len(), 6);
        for q in &cube {
            assert!((q.normal.length() - 1.0).abs() < 1e-5);
        }
    }
}
