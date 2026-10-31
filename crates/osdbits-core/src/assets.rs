//! PS2 OSDSYS texture/font decoders, `MENUGEOM` / `FONTDATA` tables, and
//! deterministic clean-room procedural asset generator.
//!
//! Directly ported from `osdbits/opening.c`, `osdbits/menuback.c`,
//! `osdbits/menuconfig.c`, `osdbits/menutext.c`, and `osdbits/tools/extract-res.py`.

use crate::math::{OsdRng, Vec4};
use crate::resource::{extract_romdir_container, parse_romdir, ResourceError};

/// Texture slot IDs uploaded to the WebGPU texture array / bind group.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureId {
    WhiteDummy = 0,
    TexoSce = 1,
    TexoFog0 = 2,
    TexoFog1 = 3,
    TexoFog2 = 4,
    TexoFog3 = 5,
    TexoFog4 = 6,
    TexoWal0 = 7,
    TexoCrle = 8,
    TexoCrbl = 9,
    TexoFlar = 10,
    TexoRef = 11,
    TexoBlp = 12,
    TexoBlpr = 13,
    TexoPngE = 14,
    TexcKabe = 15,
    TexcBlur = 16,
    TexcNavi = 17,
    TexcBump = 18,
    TexcRefa = 19,
    TexcFlow = 20,
    TexcBinv = 21,
    TexcMaru = 22,
    FntAscii = 23,
    FntExOsd = 24,
    // Dynamic offscreen framebuffers (bound by ID in shader passes)
    ScreenCopy = 25,
    WorkBuf3 = 26,
    WorkBuf4 = 27,
}

pub const STATIC_TEXTURE_COUNT: usize = 25;

#[derive(Debug, Clone)]
pub struct DecodedTexture {
    pub id: TextureId,
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// 16-entry grey-alpha CLUT from `0x2715E0` (`docs/menu-text.md` §1.2).
pub const FONT_CLUT: [[u8; 4]; 16] = [
    [0x00, 0x00, 0x00, 0x00],
    [0x00, 0x00, 0x00, 0x19],
    [0x00, 0x00, 0x00, 0x32],
    [0x00, 0x00, 0x00, 0x4B],
    [0x00, 0x00, 0x00, 0x64],
    [0x33, 0x33, 0x33, 0x32],
    [0x33, 0x33, 0x33, 0x4B],
    [0x33, 0x33, 0x33, 0x64],
    [0x33, 0x33, 0x33, 0x7D],
    [0x66, 0x66, 0x66, 0x4B],
    [0x66, 0x66, 0x66, 0x64],
    [0x66, 0x66, 0x66, 0x7D],
    [0x99, 0x99, 0x99, 0x64],
    [0x99, 0x99, 0x99, 0x7D],
    [0xCC, 0xCC, 0xCC, 0x7D],
    [0xFF, 0xFF, 0xFF, 0x7D],
];

/// Opening format 3 decoder (`osdbits/opening.c` line 701):
/// 8-bit alpha input -> white RGB `(0xFF, 0xFF, 0xFF)` with `A = src[i]`.
pub fn decode_format3_white_alpha(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    let count = w * h;
    let mut out = vec![0u8; count * 4];
    for i in 0..count.min(src.len()) {
        let base = i * 4;
        out[base] = 0xFF;
        out[base + 1] = 0xFF;
        out[base + 2] = 0xFF;
        out[base + 3] = src[i];
    }
    out
}

/// Opening format 4 decoder (`osdbits/opening.c` line 706):
/// 8-bit alpha input -> black RGB `(0x00, 0x00, 0x00)` with `A = src[i]`.
pub fn decode_format4_black_alpha(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    let count = w * h;
    let mut out = vec![0u8; count * 4];
    for i in 0..count.min(src.len()) {
        let base = i * 4;
        out[base] = 0x00;
        out[base + 1] = 0x00;
        out[base + 2] = 0x00;
        out[base + 3] = src[i];
    }
    out
}

/// Opening format 5 decoder (`osdbits/opening.c` line 710):
/// 16-bit luminance + alpha (`u16`: low byte = luminance, high byte = alpha) -> RGBA32.
pub fn decode_format5_lum_alpha(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    let count = w * h;
    let mut out = vec![0u8; count * 4];
    for i in 0..count.min(src.len() / 2) {
        let lum = src[i * 2];
        let alpha = src[i * 2 + 1];
        let base = i * 4;
        out[base] = lum;
        out[base + 1] = lum;
        out[base + 2] = lum;
        out[base + 3] = alpha;
    }
    out
}

/// Decodes PS2 `PSMCT16` (`1555` ABGR) to RGBA8 (`TEXOWAL0`, `TEXCMARU`).
pub fn decode_psmct16_rgba(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    let count = w * h;
    let mut out = vec![0u8; count * 4];
    for i in 0..count.min(src.len() / 2) {
        let px = u16::from_le_bytes([src[i * 2], src[i * 2 + 1]]);
        let r = ((px & 0x1F) as u8) << 3;
        let g = (((px >> 5) & 0x1F) as u8) << 3;
        let b = (((px >> 10) & 0x1F) as u8) << 3;
        let a = if (px & 0x8000) != 0 { 0x80 } else { 0x00 };
        let base = i * 4;
        out[base] = r;
        out[base + 1] = g;
        out[base + 2] = b;
        out[base + 3] = a;
    }
    out
}

/// Exact port of `opening.c` `InitTexture` software 2-level mipmap box filter for `PSMCT16` (`TEXOWAL0`).
pub fn build_psmct16_mipmaps(src_u16: &[u16], w: usize, h: usize) -> (Vec<u16>, Vec<u16>) {
    let downsample = |input: &[u16], sw: usize, sh: usize| -> Vec<u16> {
        let dw = sw / 2;
        let dh = sh / 2;
        let mut out = vec![0u16; dw * dh];
        for y in 0..dh {
            for x in 0..dw {
                let p00 = input[(y * 2) * sw + (x * 2)] as u32;
                let p10 = input[(y * 2) * sw + (x * 2 + 1)] as u32;
                let p01 = input[(y * 2 + 1) * sw + (x * 2)] as u32;
                let p11 = input[(y * 2 + 1) * sw + (x * 2 + 1)] as u32;
                let r = (((p00 & 0x1f) + (p10 & 0x1f) + (p01 & 0x1f) + (p11 & 0x1f)) >> 2) & 0x1f;
                let g = ((((p00 >> 5) & 0x1f)
                    + ((p10 >> 5) & 0x1f)
                    + ((p01 >> 5) & 0x1f)
                    + ((p11 >> 5) & 0x1f))
                    >> 2)
                    & 0x1f;
                let b = ((((p00 >> 10) & 0x1f)
                    + ((p10 >> 10) & 0x1f)
                    + ((p01 >> 10) & 0x1f)
                    + ((p11 >> 10) & 0x1f))
                    >> 2)
                    & 0x1f;
                let a = ((((p00 >> 15) & 1)
                    + ((p10 >> 15) & 1)
                    + ((p01 >> 15) & 1)
                    + ((p11 >> 15) & 1))
                    >> 2)
                    & 1;
                out[y * dw + x] = (r | (g << 5) | (b << 10) | (a << 15)) as u16;
            }
        }
        out
    };
    let mip1 = downsample(src_u16, w, h);
    let mip2 = downsample(&mip1, w / 2, h / 2);
    (mip1, mip2)
}

/// Decodes a 4-bit indexed `PSMT4` texture with a 16-entry RGBA CLUT (`FNTASCII`, `FNTEXOSD`, `TEXOSCE`, `TEXOPNG*`).
pub fn decode_psmt4_clut(src: &[u8], clut: &[[u8; 4]; 16], w: usize, h: usize) -> Vec<u8> {
    let total_px = w * h;
    let mut out = vec![0u8; total_px * 4];
    for i in 0..(total_px / 2).min(src.len()) {
        let byte = src[i];
        let lo = (byte & 0x0F) as usize;
        let hi = (byte >> 4) as usize;
        let idx0 = i * 2 * 4;
        out[idx0..idx0 + 4].copy_from_slice(&clut[lo]);
        let idx1 = (i * 2 + 1) * 4;
        out[idx1..idx1 + 4].copy_from_slice(&clut[hi]);
    }
    out
}

/// Exact port of `DecodeKabeTile` (`0x22A5D8`, `osdbits/menuback.c` lines 95-145):
/// Decodes an 8,192-byte tiled 4-bit `TEXCKABE` resource into a 128×128 RGBA32 image
/// (8×8 grid of 16×16 pixel tiles, each nibble scaled by 16 with alpha `0x80`).
pub fn decode_kabe_tiled(src_8kb: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; 128 * 128 * 4];
    if src_8kb.len() < 8192 {
        return out;
    }
    let mut sp = 0usize;
    for ty in 0..8usize {
        for tx in 0..8usize {
            for py in 0..16usize {
                for px_pair in 0..8usize {
                    let b = src_8kb[sp];
                    sp += 1;
                    let v0 = (b & 0x0F) << 4;
                    let v1 = b & 0xF0;
                    let x0 = tx * 16 + px_pair * 2;
                    let y = ty * 16 + py;
                    let dst0 = (y * 128 + x0) * 4;
                    out[dst0] = v0;
                    out[dst0 + 1] = v0;
                    out[dst0 + 2] = v0;
                    out[dst0 + 3] = 0x80;
                    let dst1 = (y * 128 + x0 + 1) * 4;
                    out[dst1] = v1;
                    out[dst1 + 1] = v1;
                    out[dst1 + 2] = v1;
                    out[dst1 + 3] = 0x80;
                }
            }
        }
    }
    out
}

/// Exact port of `0x22A720` (`osdbits/menuconfig.c` lines 166-195):
/// Decodes a 4,096-byte 64×64 8-bit grey page (`TEXCBUMP`, `TEXCREFA`, `TEXCFLOW`, `TEXCBINV`)
/// into `b | (b << 8) | (b << 16) | 0x7F000000` (with bitwise negation `!b` when `invert == true`).
pub fn decode_grey_page_64x64(src_4kb: &[u8], invert: bool) -> Vec<u8> {
    let mut out = vec![0u8; 64 * 64 * 4];
    for i in 0..4096.min(src_4kb.len()) {
        let b = if invert { !src_4kb[i] } else { src_4kb[i] };
        let base = i * 4;
        out[base] = b;
        out[base + 1] = b;
        out[base + 2] = b;
        out[base + 3] = 0x7F;
    }
    out
}

/// Static 3D mesh definition for System Configuration rods and cubes (`MENUGEOM`, `osdbits/menuconfig.c`).
#[derive(Debug, Clone)]
pub struct MenuMeshModel {
    pub face_count: usize,
    pub verts: Vec<[Vec4; 4]>,
    pub normals: Vec<Vec4>,
    pub uvs: Vec<[Vec4; 4]>,
}

/// Cube placement entry in System Configuration (`0x27F090`, `docs/menu-config.md` §5).
#[derive(Debug, Clone, Copy)]
pub struct ConfigCubePlacement {
    pub pos: Vec4,
    pub color: [i32; 4],
    pub bias: f32,
}

/// Exact 5-cube placement table from `0x27F090` (`docs/menu-config.md` §5).
pub const MENU_CUBE_ENTRIES: [ConfigCubePlacement; 5] = [
    ConfigCubePlacement {
        pos: [-11.50, -11.50, 47.5, 1.0],
        color: [128, 128, 128, 128],
        bias: 0.0,
    },
    ConfigCubePlacement {
        pos: [-22.50, -5.50, 47.5, 1.0],
        color: [128, 128, 128, 128],
        bias: 0.0,
    },
    ConfigCubePlacement {
        pos: [-10.75, -0.25, 47.5, 1.0],
        color: [128, 128, 128, 128],
        bias: 0.0,
    },
    ConfigCubePlacement {
        pos: [-21.75, 6.00, 47.5, 1.0],
        color: [128, 128, 128, 128],
        bias: 0.0,
    },
    ConfigCubePlacement {
        pos: [-11.25, 11.50, 47.5, 1.0],
        color: [128, 128, 128, 128],
        bias: 0.0,
    },
];

/// Builds the 16-face hexagonal bevelled prism rod model (`0x27E950`, `docs/menu-config.md` §6):
/// - Circumradius `2.6` (`(±1.3, y, ±2.251666)` and `(±2.6, y, 0)`), height `y = 0.0 .. 26.0`
/// - Bevel cap hexagon of radius `2.34` at `y = 26.39`
/// - Faces `0..=1`: flat top cap (`y = 26.39`)
/// - Faces `2..=7`: 6 bevel ring quads (`y = 26.0 .. 26.39`)
/// - Faces `8..=9`: flat bottom cap (`y = 0.0`)
/// - Faces `10..=15`: 6 vertical shaft side quads (`y = 0.0 .. 26.0`)
pub fn build_rod_mesh_model() -> MenuMeshModel {
    let r_outer = 2.6f32;
    let r_bevel = 2.34f32;
    let y_bot = 0.0f32;
    let y_shaft_top = 26.0f32;
    let y_cap = 26.39f32;

    let hex_ring = |r: f32, y: f32| -> [Vec4; 6] {
        let mut pts = [[0.0f32; 4]; 6];
        for i in 0..6 {
            let a = (i as f32) * (std::f32::consts::TAU / 6.0);
            pts[i] = [r * a.cos(), y, r * a.sin(), 1.0];
        }
        pts
    };

    let bot_ring = hex_ring(r_outer, y_bot);
    let top_ring = hex_ring(r_outer, y_shaft_top);
    let cap_ring = hex_ring(r_bevel, y_cap);

    let mut verts = Vec::with_capacity(16);
    let mut normals = Vec::with_capacity(16);
    let mut uvs = Vec::with_capacity(16);

    let unit_uv = [
        [0.0, 0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0, 0.0],
    ];

    // 0..=1: top cap (y = 26.39)
    verts.push([cap_ring[0], cap_ring[1], cap_ring[5], cap_ring[2]]);
    normals.push([0.0, 1.0, 0.0, 0.0]);
    uvs.push(unit_uv);

    verts.push([cap_ring[5], cap_ring[2], cap_ring[4], cap_ring[3]]);
    normals.push([0.0, 1.0, 0.0, 0.0]);
    uvs.push(unit_uv);

    // 2..=7: 6 bevel quads (26.0 .. 26.39)
    for i in 0..6 {
        let next = (i + 1) % 6;
        let mid_ang = ((i as f32) + 0.5) * (std::f32::consts::TAU / 6.0);
        verts.push([top_ring[i], top_ring[next], cap_ring[i], cap_ring[next]]);
        normals.push([0.6 * mid_ang.cos(), 0.8, 0.6 * mid_ang.sin(), 0.0]);
        uvs.push(unit_uv);
    }

    // 8..=9: bottom cap (y = 0.0)
    verts.push([bot_ring[1], bot_ring[0], bot_ring[2], bot_ring[5]]);
    normals.push([0.0, -1.0, 0.0, 0.0]);
    uvs.push(unit_uv);

    verts.push([bot_ring[2], bot_ring[5], bot_ring[3], bot_ring[4]]);
    normals.push([0.0, -1.0, 0.0, 0.0]);
    uvs.push(unit_uv);

    // 10..=15: 6 vertical shaft sides (0.0 .. 26.0)
    // Note: in `menuconfig.c` `MeshTransform` scales `v[1] * scene->scale[1]` and `uv[1] * scene->scale[1]`.
    // Each quad is ordered as a 4-vertex tristrip `[bot_i, bot_next, top_i, top_next]`.
    for i in 0..6 {
        let next = (i + 1) % 6;
        let mid_ang = ((i as f32) + 0.5) * (std::f32::consts::TAU / 6.0);
        verts.push([bot_ring[i], bot_ring[next], top_ring[i], top_ring[next]]);
        normals.push([mid_ang.cos(), 0.0, mid_ang.sin(), 0.0]);
        uvs.push([
            [0.0, 0.0, 0.0, 0.0],
            [0.5, 0.0, 0.0, 0.0],
            [0.0, 2.0, 0.0, 0.0],
            [0.5, 2.0, 0.0, 0.0],
        ]);
    }

    MenuMeshModel {
        face_count: 16,
        verts,
        normals,
        uvs,
    }
}

/// Builds the 6-face System Configuration cube model (`0x27EFB0`, `docs/menu-config.md` §6):
/// Half-extent `2.64`, 6 axis-aligned quad tristrips, UVs `0.0 .. 1.5`.
pub fn build_cube_mesh_model() -> MenuMeshModel {
    let h = 2.64f32;
    let faces: [([Vec4; 4], Vec4); 6] = [
        // +Z front
        (
            [
                [-h, -h, h, 1.0],
                [h, -h, h, 1.0],
                [-h, h, h, 1.0],
                [h, h, h, 1.0],
            ],
            [0.0, 0.0, 1.0, 0.0],
        ),
        // -Z back
        (
            [
                [h, -h, -h, 1.0],
                [-h, -h, -h, 1.0],
                [h, h, -h, 1.0],
                [-h, h, -h, 1.0],
            ],
            [0.0, 0.0, -1.0, 0.0],
        ),
        // +X right
        (
            [
                [h, -h, h, 1.0],
                [h, -h, -h, 1.0],
                [h, h, h, 1.0],
                [h, h, -h, 1.0],
            ],
            [1.0, 0.0, 0.0, 0.0],
        ),
        // -X left
        (
            [
                [-h, -h, -h, 1.0],
                [-h, -h, h, 1.0],
                [-h, h, -h, 1.0],
                [-h, h, h, 1.0],
            ],
            [-1.0, 0.0, 0.0, 0.0],
        ),
        // +Y top
        (
            [
                [-h, h, h, 1.0],
                [h, h, h, 1.0],
                [-h, h, -h, 1.0],
                [h, h, -h, 1.0],
            ],
            [0.0, 1.0, 0.0, 0.0],
        ),
        // -Y bottom
        (
            [
                [-h, -h, -h, 1.0],
                [h, -h, -h, 1.0],
                [-h, -h, h, 1.0],
                [h, -h, h, 1.0],
            ],
            [0.0, -1.0, 0.0, 0.0],
        ),
    ];

    let mut verts = Vec::with_capacity(6);
    let mut normals = Vec::with_capacity(6);
    let mut uvs = Vec::with_capacity(6);

    for (i, (v, n)) in faces.into_iter().enumerate() {
        let u0 = ((i % 2) as f32) * 0.5;
        let v0 = ((i / 2) as f32) * 0.5;
        verts.push(v);
        normals.push(n);
        uvs.push([
            [u0, v0, 0.0, 0.0],
            [u0 + 0.75, v0, 0.0, 0.0],
            [u0, v0 + 0.75, 0.0, 0.0],
            [u0 + 0.75, v0 + 0.75, 0.0, 0.0],
        ]);
    }

    MenuMeshModel {
        face_count: 6,
        verts,
        normals,
        uvs,
    }
}

/// Compact 8x12 bitmap glyph patterns for ASCII 32..=126 used by the clean-room procedural font
/// generator to rasterize crisp, anti-aliased glyphs into the 256×480 `FNTASCII` atlas (`8×12` grid of `32×40` cells)
/// and `FNTEXOSD` (`512×80`).
fn glyph_bitmap_8x8(ch: char) -> [u8; 8] {
    match ch {
        ' ' => [0x00; 8],
        '!' => [0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x00],
        '"' => [0x66, 0x66, 0x24, 0x00, 0x00, 0x00, 0x00, 0x00],
        '#' => [0x24, 0x7E, 0x24, 0x24, 0x7E, 0x24, 0x00, 0x00],
        '$' => [0x18, 0x3E, 0x58, 0x3C, 0x1A, 0x7C, 0x18, 0x00],
        '%' => [0x62, 0x64, 0x08, 0x10, 0x26, 0x46, 0x00, 0x00],
        '&' => [0x38, 0x44, 0x38, 0x54, 0x48, 0x34, 0x0A, 0x00],
        '\'' => [0x18, 0x18, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00],
        '(' => [0x0C, 0x18, 0x30, 0x30, 0x30, 0x18, 0x0C, 0x00],
        ')' => [0x30, 0x18, 0x0C, 0x0C, 0x0C, 0x18, 0x30, 0x00],
        '*' => [0x00, 0x24, 0x18, 0x7E, 0x18, 0x24, 0x00, 0x00],
        '+' => [0x00, 0x18, 0x18, 0x7E, 0x18, 0x18, 0x00, 0x00],
        ',' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x30],
        '-' => [0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00],
        '/' => [0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x00, 0x00],
        '0' => [0x3C, 0x66, 0x6E, 0x76, 0x66, 0x66, 0x3C, 0x00],
        '1' => [0x18, 0x38, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00],
        '2' => [0x3C, 0x66, 0x06, 0x0C, 0x30, 0x60, 0x7E, 0x00],
        '3' => [0x3C, 0x66, 0x06, 0x1C, 0x06, 0x66, 0x3C, 0x00],
        '4' => [0x0C, 0x1C, 0x3C, 0x6C, 0x7E, 0x0C, 0x0C, 0x00],
        '5' => [0x7E, 0x60, 0x7C, 0x06, 0x06, 0x66, 0x3C, 0x00],
        '6' => [0x3C, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x3C, 0x00],
        '7' => [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x30, 0x30, 0x00],
        '8' => [0x3C, 0x66, 0x66, 0x3C, 0x66, 0x66, 0x3C, 0x00],
        '9' => [0x3C, 0x66, 0x66, 0x3E, 0x06, 0x0C, 0x38, 0x00],
        ':' => [0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00],
        ';' => [0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x30, 0x00],
        '<' => [0x0C, 0x18, 0x30, 0x60, 0x30, 0x18, 0x0C, 0x00],
        '=' => [0x00, 0x00, 0x7E, 0x00, 0x7E, 0x00, 0x00, 0x00],
        '>' => [0x30, 0x18, 0x0C, 0x06, 0x0C, 0x18, 0x30, 0x00],
        '?' => [0x3C, 0x66, 0x0C, 0x18, 0x18, 0x00, 0x18, 0x00],
        '@' => [0x3C, 0x66, 0x6E, 0x6E, 0x60, 0x62, 0x3C, 0x00],
        'A' => [0x18, 0x3C, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x00],
        'B' => [0x7C, 0x66, 0x66, 0x7C, 0x66, 0x66, 0x7C, 0x00],
        'C' => [0x3C, 0x66, 0x60, 0x60, 0x60, 0x66, 0x3C, 0x00],
        'D' => [0x78, 0x6C, 0x66, 0x66, 0x66, 0x6C, 0x78, 0x00],
        'E' => [0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x7E, 0x00],
        'F' => [0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x60, 0x00],
        'G' => [0x3C, 0x66, 0x60, 0x6E, 0x66, 0x66, 0x3E, 0x00],
        'H' => [0x66, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00],
        'I' => [0x3C, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'J' => [0x1E, 0x0C, 0x0C, 0x0C, 0x6C, 0x6C, 0x38, 0x00],
        'K' => [0x66, 0x6C, 0x78, 0x70, 0x78, 0x6C, 0x66, 0x00],
        'L' => [0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x7E, 0x00],
        'M' => [0x63, 0x77, 0x7F, 0x6B, 0x63, 0x63, 0x63, 0x00],
        'N' => [0x66, 0x76, 0x7E, 0x7E, 0x6E, 0x66, 0x66, 0x00],
        'O' => [0x3C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'P' => [0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60, 0x60, 0x00],
        'Q' => [0x3C, 0x66, 0x66, 0x66, 0x6A, 0x6C, 0x36, 0x00],
        'R' => [0x7C, 0x66, 0x66, 0x7C, 0x6C, 0x66, 0x66, 0x00],
        'S' => [0x3C, 0x66, 0x60, 0x3C, 0x06, 0x66, 0x3C, 0x00],
        'T' => [0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
        'U' => [0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'V' => [0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
        'W' => [0x63, 0x63, 0x63, 0x6B, 0x7F, 0x77, 0x63, 0x00],
        'X' => [0x66, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x66, 0x00],
        'Y' => [0x66, 0x66, 0x66, 0x3C, 0x18, 0x18, 0x18, 0x00],
        'Z' => [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x60, 0x7E, 0x00],
        '[' => [0x3C, 0x30, 0x30, 0x30, 0x30, 0x30, 0x3C, 0x00],
        '\\' => [0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x00, 0x00],
        ']' => [0x3C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x3C, 0x00],
        '^' => [0x18, 0x3C, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00],
        '_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7E, 0x00],
        '`' => [0x30, 0x18, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00],
        'a' => [0x00, 0x00, 0x3C, 0x06, 0x3E, 0x66, 0x3E, 0x00],
        'b' => [0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x7C, 0x00],
        'c' => [0x00, 0x00, 0x3C, 0x66, 0x60, 0x66, 0x3C, 0x00],
        'd' => [0x06, 0x06, 0x3E, 0x66, 0x66, 0x66, 0x3E, 0x00],
        'e' => [0x00, 0x00, 0x3C, 0x66, 0x7E, 0x60, 0x3C, 0x00],
        'f' => [0x1C, 0x30, 0x7C, 0x30, 0x30, 0x30, 0x30, 0x00],
        'g' => [0x00, 0x00, 0x3E, 0x66, 0x66, 0x3E, 0x06, 0x3C],
        'h' => [0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00],
        'i' => [0x18, 0x00, 0x38, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'j' => [0x0C, 0x00, 0x1C, 0x0C, 0x0C, 0x0C, 0x6C, 0x38],
        'k' => [0x60, 0x60, 0x66, 0x6C, 0x78, 0x6C, 0x66, 0x00],
        'l' => [0x38, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'm' => [0x00, 0x00, 0x66, 0x7F, 0x7F, 0x6B, 0x63, 0x00],
        'n' => [0x00, 0x00, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00],
        'o' => [0x00, 0x00, 0x3C, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'p' => [0x00, 0x00, 0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60],
        'q' => [0x00, 0x00, 0x3E, 0x66, 0x66, 0x3E, 0x06, 0x06],
        'r' => [0x00, 0x00, 0x7C, 0x66, 0x60, 0x60, 0x60, 0x00],
        's' => [0x00, 0x00, 0x3E, 0x60, 0x3C, 0x06, 0x7C, 0x00],
        't' => [0x30, 0x30, 0x7C, 0x30, 0x30, 0x36, 0x1C, 0x00],
        'u' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x66, 0x3E, 0x00],
        'v' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
        'w' => [0x00, 0x00, 0x63, 0x6B, 0x7F, 0x7F, 0x36, 0x00],
        'x' => [0x00, 0x00, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x00],
        'y' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x3E, 0x06, 0x3C],
        'z' => [0x00, 0x00, 0x7E, 0x0C, 0x18, 0x30, 0x7E, 0x00],
        _ => [0x3C, 0x42, 0x99, 0xA1, 0xA1, 0x99, 0x42, 0x3C],
    }
}

/// Returns `[left_inset, width]` for each of the 97 ASCII glyphs (`32..=128`), matching
/// `0x26FE60` (`docs/menu-text.md` §1.1 spot-checks: `' '={0,13}`, `'1'={5,23}`, `'A'={4,25}`, `'W'={0,32}`, `'l'={12,9}`).
pub fn default_font_ascii_metrics() -> [[i32; 2]; 97] {
    let mut m = [[4i32, 22i32]; 97];
    for idx in 0..97 {
        let ch = (32u8 + idx as u8) as char;
        m[idx] = match ch {
            ' ' => [0, 13],
            '1' => [5, 23],
            'A' => [4, 25],
            'W' => [0, 32],
            'M' => [1, 30],
            'l' | 'i' | '!' | '|' => [12, 9],
            'I' | 'j' | '.' | ',' | ':' | ';' | '\'' => [10, 12],
            '(' | ')' | '[' | ']' | 'f' | 'r' | 't' => [8, 15],
            'm' | 'w' => [2, 28],
            'A'..='Z' => [4, 24],
            '0'..='9' => [5, 22],
            _ => [5, 20],
        };
    }
    m
}

/// Returns `[left_inset, width]` for the 35 `FNTEXOSD` symbol glyphs (`0x271460`).
pub fn default_font_osd_metrics() -> [[i32; 2]; 35] {
    let mut m = [[4i32, 24i32]; 35];
    // Slot 4 (`\7o004`) is the registered trademark symbol `®`
    m[4] = [4, 22];
    m
}

/// Complete AssetPack holding all 25 static RGBA textures, glyph metrics, string tables, and 3D meshes.
#[derive(Debug, Clone)]
pub struct AssetPack {
    pub is_user_imported: bool,
    pub source_label: String,
    pub textures: Vec<DecodedTexture>,
    pub font_ascii_metrics: [[i32; 2]; 97],
    pub font_osd_metrics: [[i32; 2]; 35],
    pub rod_mesh: MenuMeshModel,
    pub cube_mesh: MenuMeshModel,
    pub cube_entries: [ConfigCubePlacement; 5],
}

impl AssetPack {
    /// Generates a complete, deterministic clean-room procedural asset pack so `osdbits-wasm`
    /// runs out-of-the-box with zero copyrighted Sony files.
    pub fn new_procedural() -> Self {
        let font_ascii_metrics = default_font_ascii_metrics();
        let font_osd_metrics = default_font_osd_metrics();

        let mut textures = Vec::with_capacity(STATIC_TEXTURE_COUNT);

        // 0: WhiteDummy (8x8 solid white with PS2 alpha 0x80)
        textures.push(DecodedTexture {
            id: TextureId::WhiteDummy,
            name: "WHITE_DUMMY",
            width: 8,
            height: 8,
            rgba: [255u8, 255, 255, 128].repeat(64),
        });

        // 1: TEXOSCE (256x64 "Sony Computer Entertainment" banner)
        textures.push(DecodedTexture {
            id: TextureId::TexoSce,
            name: "TEXOSCE",
            width: 256,
            height: 64,
            rgba: render_text_banner_rgba(
                256,
                64,
                &[(18, 24, "Sony Computer Entertainment", 1)],
                &font_ascii_metrics,
            ),
        });

        // 2..=6: TEXOFOG0..4 (128x64 billowing cloud alpha layers)
        let fog_names = ["TEXOFOG0", "TEXOFOG1", "TEXOFOG2", "TEXOFOG3", "TEXOFOG4"];
        let fog_ids = [
            TextureId::TexoFog0,
            TextureId::TexoFog1,
            TextureId::TexoFog2,
            TextureId::TexoFog3,
            TextureId::TexoFog4,
        ];
        for i in 0..5 {
            textures.push(DecodedTexture {
                id: fog_ids[i],
                name: fog_names[i],
                width: 128,
                height: 64,
                rgba: generate_fog_texture_rgba(128, 64, (i as u32 + 1) * 1337),
            });
        }

        // 7: TEXOWAL0 (128x128 brushed architectural monolith stone/metal wall texture)
        textures.push(DecodedTexture {
            id: TextureId::TexoWal0,
            name: "TEXOWAL0",
            width: 128,
            height: 128,
            rgba: generate_tower_wall_rgba(128, 128),
        });

        // 8: TEXOCRLE (32x32 bright orb light core)
        textures.push(DecodedTexture {
            id: TextureId::TexoCrle,
            name: "TEXOCRLE",
            width: 32,
            height: 32,
            rgba: generate_radial_sprite_rgba(32, 32, 2.2, true),
        });

        // 9: TEXOCRBL (32x32 soft orb light halo)
        textures.push(DecodedTexture {
            id: TextureId::TexoCrbl,
            name: "TEXOCRBL",
            width: 32,
            height: 32,
            rgba: generate_radial_sprite_rgba(32, 32, 1.15, false),
        });

        // 10: TEXOFLAR (32x32 red flare lens glow)
        textures.push(DecodedTexture {
            id: TextureId::TexoFlar,
            name: "TEXOFLAR",
            width: 32,
            height: 32,
            rgba: generate_radial_sprite_rgba(32, 32, 1.4, true),
        });

        // 11..=13: TEXOREF, TEXOBLP, TEXOBLPR (64x64 glass cube specular reflection maps)
        textures.push(DecodedTexture {
            id: TextureId::TexoRef,
            name: "TEXOREF",
            width: 64,
            height: 64,
            rgba: generate_env_reflection_rgba(64, 64, 0),
        });
        textures.push(DecodedTexture {
            id: TextureId::TexoBlp,
            name: "TEXOBLP",
            width: 64,
            height: 64,
            rgba: generate_env_reflection_rgba(64, 64, 1),
        });
        textures.push(DecodedTexture {
            id: TextureId::TexoBlpr,
            name: "TEXOBLPR",
            width: 64,
            height: 64,
            rgba: generate_env_reflection_rgba(64, 64, 2),
        });

        // 14: TEXOPNGE (512x64 Illegal Disc text banner)
        textures.push(DecodedTexture {
            id: TextureId::TexoPngE,
            name: "TEXOPNGE",
            width: 512,
            height: 64,
            rgba: render_text_banner_rgba(
                512,
                64,
                &[
                    (46, 10, "Please insert a PlayStation", 1),
                    (36, 36, "or PlayStation 2 format disc.", 1),
                ],
                &font_ascii_metrics,
            ),
        });

        // 15: TEXCKABE (128x128 tiled cloud/nebula backdrop for System Configuration tunnel)
        let kabe_raw = generate_kabe_raw_8kb();
        textures.push(DecodedTexture {
            id: TextureId::TexcKabe,
            name: "TEXCKABE",
            width: 128,
            height: 128,
            rgba: decode_kabe_tiled(&kabe_raw),
        });

        // 16: TEXCBLUR (32x32 menu orb soft outer glow)
        textures.push(DecodedTexture {
            id: TextureId::TexcBlur,
            name: "TEXCBLUR",
            width: 32,
            height: 32,
            rgba: generate_radial_sprite_rgba(32, 32, 1.25, false),
        });

        // 17: TEXCNAVI (32x32 menu orb intense core)
        textures.push(DecodedTexture {
            id: TextureId::TexcNavi,
            name: "TEXCNAVI",
            width: 32,
            height: 32,
            rgba: generate_radial_sprite_rgba(32, 32, 2.5, true),
        });

        // 18 & 21: TEXCBUMP & TEXCBINV (64x64 grey emboss bump pair, where TEXCBINV == ~TEXCBUMP)
        let bump_raw = generate_bump_raw_4kb();
        let refa_raw = generate_refa_raw_4kb(false);
        let flow_raw = generate_refa_raw_4kb(true);

        textures.push(DecodedTexture {
            id: TextureId::TexcBump,
            name: "TEXCBUMP",
            width: 64,
            height: 64,
            rgba: decode_grey_page_64x64(&bump_raw, false),
        });

        // 19: TEXCREFA (64x64 spherical reflection map for config cubes)
        textures.push(DecodedTexture {
            id: TextureId::TexcRefa,
            name: "TEXCREFA",
            width: 64,
            height: 64,
            rgba: decode_grey_page_64x64(&refa_raw, false),
        });

        // 20: TEXCFLOW (64x64 spherical flow map for clock rod bloom)
        textures.push(DecodedTexture {
            id: TextureId::TexcFlow,
            name: "TEXCFLOW",
            width: 64,
            height: 64,
            rgba: decode_grey_page_64x64(&flow_raw, false),
        });

        // 21: TEXCBINV (64x64 bitwise complement of TEXCBUMP)
        textures.push(DecodedTexture {
            id: TextureId::TexcBinv,
            name: "TEXCBINV",
            width: 64,
            height: 64,
            rgba: decode_grey_page_64x64(&bump_raw, true),
        });

        // 22: TEXCMARU (64x64 PlayStation button icons & config up/down arrows)
        textures.push(DecodedTexture {
            id: TextureId::TexcMaru,
            name: "TEXCMARU",
            width: 64,
            height: 64,
            rgba: generate_texcmaru_rgba(),
        });

        // 23: FNTASCII (256x480 8x12 grid of 32x40 ASCII glyph cells)
        textures.push(DecodedTexture {
            id: TextureId::FntAscii,
            name: "FNTASCII",
            width: 256,
            height: 480,
            rgba: generate_fntascii_rgba(&font_ascii_metrics),
        });

        // 24: FNTEXOSD (512x80 16x2 grid of 32x40 OSD special glyph cells including ® at slot 4)
        textures.push(DecodedTexture {
            id: TextureId::FntExOsd,
            name: "FNTEXOSD",
            width: 512,
            height: 80,
            rgba: generate_fntexosd_rgba(),
        });

        Self {
            is_user_imported: false,
            source_label: "Procedural Clean-Room Asset Pack".to_string(),
            textures,
            font_ascii_metrics,
            font_osd_metrics,
            rod_mesh: build_rod_mesh_model(),
            cube_mesh: build_cube_mesh_model(),
            cube_entries: MENU_CUBE_ENTRIES,
        }
    }

    /// Parses a user-supplied PS2 BIOS image or standalone ROMDIR container (`TEXIMAGE` / `FNTIMAGE`),
    /// decompresses any matching OSDSYS resources via `expand_lzss`, decodes them into textures,
    /// and merges them over the procedural pack.
    pub fn import_from_romdir_blob(&mut self, blob: &[u8]) -> Result<usize, ResourceError> {
        let top_entries = parse_romdir(blob)?;
        let mut extracted_files: Vec<(String, Vec<u8>)> = Vec::new();

        // Check if this is a full BIOS containing nested containers (TEXIMAGE, FNTIMAGE) or a direct container
        let has_nested = top_entries
            .iter()
            .any(|e| e.name == "TEXIMAGE" || e.name == "FNTIMAGE");

        if has_nested {
            for entry in &top_entries {
                if entry.name == "TEXIMAGE" || entry.name == "FNTIMAGE" {
                    let end = entry.data_offset.saturating_add(entry.file_size as usize);
                    if end <= blob.len() {
                        if let Ok(sub) = extract_romdir_container(&blob[entry.data_offset..end]) {
                            extracted_files.extend(sub);
                        }
                    }
                }
            }
        } else {
            extracted_files = extract_romdir_container(blob)?;
        }

        if extracted_files.is_empty() {
            return Err(ResourceError::RomDirNotFound);
        }

        let mut updated_count = 0usize;
        let mut bump_raw_copy: Option<Vec<u8>> = None;

        for (name, data) in &extracted_files {
            match name.as_str() {
                "TEXOFOG0" | "TEXOFOG1" | "TEXOFOG2" | "TEXOFOG3" | "TEXOFOG4" => {
                    if data.len() >= 128 * 64 {
                        let idx = match name.as_str() {
                            "TEXOFOG0" => TextureId::TexoFog0 as usize,
                            "TEXOFOG1" => TextureId::TexoFog1 as usize,
                            "TEXOFOG2" => TextureId::TexoFog2 as usize,
                            "TEXOFOG3" => TextureId::TexoFog3 as usize,
                            _ => TextureId::TexoFog4 as usize,
                        };
                        self.textures[idx].rgba = decode_format3_white_alpha(data, 128, 64);
                        updated_count += 1;
                    }
                }
                "TEXOWAL0" => {
                    if data.len() >= 128 * 128 * 2 {
                        self.textures[TextureId::TexoWal0 as usize].rgba =
                            decode_psmct16_rgba(data, 128, 128);
                        updated_count += 1;
                    }
                }
                "TEXOCRLE" if data.len() >= 32 * 32 * 2 => {
                    self.textures[TextureId::TexoCrle as usize].rgba =
                        decode_format5_lum_alpha(data, 32, 32);
                    updated_count += 1;
                }
                "TEXOCRBL" if data.len() >= 32 * 32 => {
                    self.textures[TextureId::TexoCrbl as usize].rgba =
                        decode_format3_white_alpha(data, 32, 32);
                    updated_count += 1;
                }
                "TEXOFLAR" if data.len() >= 32 * 32 => {
                    self.textures[TextureId::TexoFlar as usize].rgba =
                        decode_format3_white_alpha(data, 32, 32);
                    updated_count += 1;
                }
                "TEXOREF" if data.len() >= 64 * 64 => {
                    self.textures[TextureId::TexoRef as usize].rgba =
                        decode_format3_white_alpha(data, 64, 64);
                    updated_count += 1;
                }
                "TEXOBLP" if data.len() >= 64 * 64 => {
                    self.textures[TextureId::TexoBlp as usize].rgba =
                        decode_format3_white_alpha(data, 64, 64);
                    updated_count += 1;
                }
                "TEXOBLPR" if data.len() >= 64 * 64 => {
                    self.textures[TextureId::TexoBlpr as usize].rgba =
                        decode_format4_black_alpha(data, 64, 64);
                    updated_count += 1;
                }
                "TEXCKABE" if data.len() >= 8192 => {
                    self.textures[TextureId::TexcKabe as usize].rgba = decode_kabe_tiled(data);
                    updated_count += 1;
                }
                "TEXCBLUR" if data.len() >= 32 * 32 * 4 => {
                    self.textures[TextureId::TexcBlur as usize].rgba = data[..32 * 32 * 4].to_vec();
                    updated_count += 1;
                }
                "TEXCNAVI" if data.len() >= 32 * 32 * 4 => {
                    self.textures[TextureId::TexcNavi as usize].rgba = data[..32 * 32 * 4].to_vec();
                    updated_count += 1;
                }
                "TEXCBUMP" if data.len() >= 4096 => {
                    self.textures[TextureId::TexcBump as usize].rgba =
                        decode_grey_page_64x64(data, false);
                    bump_raw_copy = Some(data[..4096].to_vec());
                    updated_count += 1;
                }
                "TEXCREFA" if data.len() >= 4096 => {
                    self.textures[TextureId::TexcRefa as usize].rgba =
                        decode_grey_page_64x64(data, false);
                    updated_count += 1;
                }
                "TEXCFLOW" if data.len() >= 4096 => {
                    self.textures[TextureId::TexcFlow as usize].rgba =
                        decode_grey_page_64x64(data, false);
                    updated_count += 1;
                }
                "TEXCMARU" if data.len() >= 64 * 64 * 2 => {
                    self.textures[TextureId::TexcMaru as usize].rgba =
                        decode_psmct16_rgba(data, 64, 64);
                    updated_count += 1;
                }
                "FNTASCII" if data.len() >= 256 * 480 / 2 => {
                    self.textures[TextureId::FntAscii as usize].rgba =
                        decode_psmt4_clut(data, &FONT_CLUT, 256, 480);
                    updated_count += 1;
                }
                "FNTEXOSD" if data.len() >= 512 * 80 / 2 => {
                    self.textures[TextureId::FntExOsd as usize].rgba =
                        decode_psmt4_clut(data, &FONT_CLUT, 512, 80);
                    updated_count += 1;
                }
                _ => {}
            }
        }

        if let Some(bump) = bump_raw_copy {
            self.textures[TextureId::TexcBinv as usize].rgba = decode_grey_page_64x64(&bump, true);
        }

        if updated_count > 0 {
            self.is_user_imported = true;
            self.source_label = format!("User Imported ROMDIR ({updated_count} resources)");
        }

        Ok(updated_count)
    }
}

fn generate_fog_texture_rgba(w: usize, h: usize, seed: u32) -> Vec<u8> {
    let mut rng = OsdRng::new(seed);
    // Generate a low-res 16x8 grid of random values and bilinearly interpolate with radial falloff
    let gw = 16usize;
    let gh = 8usize;
    let mut grid = vec![0.0f32; gw * gh];
    for v in &mut grid {
        *v = rng.rand_f32_01();
    }

    let mut raw_alpha = vec![0u8; w * h];
    for y in 0..h {
        let ny = (y as f32) / (h as f32) * 2.0 - 1.0;
        let fy = (y as f32) / (h as f32) * ((gh - 1) as f32);
        let y0 = (fy as usize).min(gh - 2);
        let ty = fy - (y0 as f32);

        for x in 0..w {
            let nx = (x as f32) / (w as f32) * 2.0 - 1.0;
            let fx = (x as f32) / (w as f32) * ((gw - 1) as f32);
            let x0 = (fx as usize).min(gw - 2);
            let tx = fx - (x0 as f32);

            let v00 = grid[y0 * gw + x0];
            let v10 = grid[y0 * gw + x0 + 1];
            let v01 = grid[(y0 + 1) * gw + x0];
            let v11 = grid[(y0 + 1) * gw + x0 + 1];

            let v0 = v00 + (v10 - v00) * tx;
            let v1 = v01 + (v11 - v01) * tx;
            let noise = v0 + (v1 - v0) * ty;

            let dist2 = nx * nx + ny * ny;
            let env = (1.0 - dist2).max(0.0).powi(2);
            let a = (noise * env * 96.0).clamp(0.0, 128.0) as u8;
            raw_alpha[y * w + x] = a;
        }
    }

    decode_format3_white_alpha(&raw_alpha, w, h)
}

fn generate_tower_wall_rgba(w: usize, h: usize) -> Vec<u8> {
    let mut rng = OsdRng::new(0x5053_3200);
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        let panel_y = if y % 32 == 0 || y % 32 == 31 {
            0.78
        } else {
            1.0
        };
        for x in 0..w {
            let panel_x = if x % 16 == 0 { 0.82 } else { 1.0 };
            let streak = (((x * 7 + y / 4) % 13) as f32) / 13.0 * 18.0;
            let grain = (rng.rand() & 0x0F) as f32;
            let base = (168.0 + streak + grain) * panel_x * panel_y;
            let c = base.clamp(60.0, 245.0) as u8;
            let idx = (y * w + x) * 4;
            out[idx] = c;
            out[idx + 1] = c;
            out[idx + 2] = (c as u16 + 10).min(255) as u8;
            out[idx + 3] = 0x80;
        }
    }
    out
}

fn generate_radial_sprite_rgba(w: usize, h: usize, exponent: f32, bright_core: bool) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    let cx = (w as f32 - 1.0) * 0.5;
    let cy = (h as f32 - 1.0) * 0.5;
    let inv_r = 1.0 / cx.max(1.0);
    for y in 0..h {
        for x in 0..w {
            let dx = (x as f32 - cx) * inv_r;
            let dy = (y as f32 - cy) * inv_r;
            let d = (dx * dx + dy * dy).sqrt().min(1.0);
            let falloff = (1.0 - d).powf(exponent);
            let lum = if bright_core {
                (falloff * 255.0).clamp(0.0, 255.0) as u8
            } else {
                255
            };
            let alpha = (falloff * 128.0).clamp(0.0, 128.0) as u8;
            let idx = (y * w + x) * 4;
            out[idx] = lum;
            out[idx + 1] = lum;
            out[idx + 2] = lum;
            out[idx + 3] = alpha;
        }
    }
    out
}

fn generate_env_reflection_rgba(w: usize, h: usize, variant: u32) -> Vec<u8> {
    let mut raw = vec![0u8; w * h];
    for y in 0..h {
        let fy = (y as f32) / (h as f32);
        for x in 0..w {
            let fx = (x as f32) / (w as f32);
            let wave = ((fx * std::f32::consts::TAU * 2.0 + fy * 4.0 + variant as f32).sin() * 0.5
                + 0.5)
                * ((fy * std::f32::consts::PI).sin());
            raw[y * w + x] = (wave * 110.0).clamp(0.0, 128.0) as u8;
        }
    }
    if variant == 2 {
        decode_format4_black_alpha(&raw, w, h)
    } else {
        decode_format3_white_alpha(&raw, w, h)
    }
}

fn generate_kabe_raw_8kb() -> Vec<u8> {
    // Generate 128x128 4-bit cloud intensities packed into 8x8 tiles of 16x16 pixels
    let mut linear = [0u8; 128 * 128];
    for y in 0..128usize {
        let fy = (y as f32) / 128.0 * std::f32::consts::TAU;
        for x in 0..128usize {
            let fx = (x as f32) / 128.0 * std::f32::consts::TAU;
            let v = (fx.sin() * fy.cos() * 0.35
                + (fx * 2.0 + fy).sin() * 0.25
                + (fx * 3.0 - fy * 2.0).cos() * 0.2
                + 0.55)
                .clamp(0.1, 0.95);
            linear[y * 128 + x] = (v * 15.0) as u8;
        }
    }

    let mut tiled = Vec::with_capacity(8192);
    for ty in 0..8usize {
        for tx in 0..8usize {
            for py in 0..16usize {
                for px_pair in 0..8usize {
                    let x0 = tx * 16 + px_pair * 2;
                    let y = ty * 16 + py;
                    let n0 = linear[y * 128 + x0] & 0x0F;
                    let n1 = linear[y * 128 + x0 + 1] & 0x0F;
                    tiled.push(n0 | (n1 << 4));
                }
            }
        }
    }
    tiled
}

fn generate_bump_raw_4kb() -> Vec<u8> {
    let mut rng = OsdRng::new(0x22A7_2000);
    let mut out = vec![0u8; 64 * 64];
    for y in 0..64usize {
        for x in 0..64usize {
            let ripple = (((x as f32) * 0.35).sin() * ((y as f32) * 0.35).cos() * 48.0) as i32;
            let grain = (rng.rand() & 0x3F) - 32;
            out[y * 64 + x] = (128 + ripple + grain).clamp(16, 240) as u8;
        }
    }
    out
}

fn generate_refa_raw_4kb(flow: bool) -> Vec<u8> {
    let mut out = vec![0u8; 64 * 64];
    for y in 0..64usize {
        let ny = (y as f32 - 31.5) / 31.5;
        for x in 0..64usize {
            let nx = (x as f32 - 31.5) / 31.5;
            let r2 = (nx * nx + ny * ny).min(1.0);
            let band = if flow {
                ((nx * 5.0 + ny * 3.0).sin() * 0.5 + 0.5) * (1.0 - r2 * 0.6)
            } else {
                (1.0 - r2).powf(1.5)
            };
            out[y * 64 + x] = (band * 220.0).clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// Generates the 64×64 `TEXCMARU` button & arrow icon sheet matching `menutext.c`:
/// - Row 0 (`y = 0..16`):
///   - `x = 0..16`: Circle button (red circle inside dark badge)
///   - `x = 16..32`: Cross button (blue cross inside dark badge)
///   - `x = 32..48`: Square button (pink square inside dark badge)
///   - `x = 48..64`: Triangle button (green triangle inside dark badge)
/// - `x = 0..16, y = 16..24`: Up arrow for System Configuration
/// - `x = 0..16, y = 24..32`: Down arrow for System Configuration
fn generate_texcmaru_rgba() -> Vec<u8> {
    let mut out = vec![0u8; 64 * 64 * 4];
    let set_px = |buf: &mut [u8], x: usize, y: usize, r: u8, g: u8, b: u8, a: u8| {
        if x < 64 && y < 64 {
            let idx = (y * 64 + x) * 4;
            buf[idx] = r;
            buf[idx + 1] = g;
            buf[idx + 2] = b;
            buf[idx + 3] = a;
        }
    };

    for icon in 0..4usize {
        let ox = icon * 16;
        for y in 0..16usize {
            for x in 0..16usize {
                let dx = (x as f32) - 7.5;
                let dy = (y as f32) - 7.5;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist <= 7.0 {
                    // Dark button disc background
                    set_px(&mut out, ox + x, y, 36, 40, 52, 0x80);
                    match icon {
                        0 => {
                            // Circle (red ring)
                            if (dist - 4.3).abs() <= 1.2 {
                                set_px(&mut out, ox + x, y, 235, 75, 75, 0x80);
                            }
                        }
                        1 => {
                            // Cross (blue X)
                            if dx.abs() <= 4.5
                                && dy.abs() <= 4.5
                                && ((dx - dy).abs() <= 1.25 || (dx + dy).abs() <= 1.25)
                            {
                                set_px(&mut out, ox + x, y, 110, 145, 245, 0x80);
                            }
                        }
                        2 => {
                            // Square (pink box)
                            if (dx.abs() <= 4.2 && dy.abs() <= 4.2)
                                && (dx.abs() >= 2.8 || dy.abs() >= 2.8)
                            {
                                set_px(&mut out, ox + x, y, 225, 115, 195, 0x80);
                            }
                        }
                        _ => {
                            // Triangle (green triangle)
                            if (-4.2..=3.8).contains(&dy) {
                                let half_w = (dy + 4.2) * 0.58;
                                if (dx.abs() - half_w).abs() <= 1.15
                                    || ((dy - 3.2).abs() <= 0.9 && dx.abs() <= half_w)
                                {
                                    set_px(&mut out, ox + x, y, 75, 215, 135, 0x80);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Up arrow in (0..16, 16..24) and Down arrow in (0..16, 24..32)
    for row in 0..7usize {
        let half = row;
        for x in (7usize.saturating_sub(half))..=(7 + half).min(14) {
            set_px(&mut out, x, 17 + row, 210, 215, 230, 0x80);
            set_px(&mut out, x, 30 - row, 210, 215, 230, 0x80);
        }
    }

    out
}

/// Rasterizes the 97 ASCII glyphs (`32..=128`) into a 256×480 RGBA texture (`8 cols × 12 rows` of `32×40` cells)
/// using anti-aliased CLUT alpha values and exact `metrics[idx]` `[left_inset, width]` bounds.
fn generate_fntascii_rgba(metrics: &[[i32; 2]; 97]) -> Vec<u8> {
    let w = 256usize;
    let h = 480usize;
    let mut out = vec![0u8; w * h * 4];

    for idx in 0..97usize {
        let ch = (32u8 + idx as u8) as char;
        let bmp = glyph_bitmap_8x8(ch);
        let col = idx % 8;
        let row = idx / 8;
        let cell_x = col * 32;
        let cell_y = row * 40;
        // The atlas is 256x480 = 8 columns x 12 rows of 32x40 cells, i.e. 96 cells,
        // while the metrics table has 97 entries (characters 32..=128). The last
        // character has no cell in the original page either - skip it rather than
        // writing out of bounds.
        if cell_y + 40 > h || cell_x + 32 > w {
            continue;
        }
        let left_inset = metrics[idx][0].max(0) as usize;
        let glyph_w = metrics[idx][1].clamp(4, 30) as usize;

        for py in 4..34usize {
            let by = ((py - 4) * 8) / 30;
            let bits = bmp[by.min(7)];
            for px in 0..glyph_w {
                let bx = (px * 8) / glyph_w.max(1);
                let is_on = ((bits >> (7 - bx.min(7))) & 1) != 0;
                if is_on {
                    let gx = (cell_x + left_inset + px).min(w - 1);
                    let gy = (cell_y + py).min(h - 1);
                    let dst = (gy * w + gx) * 4;
                    out[dst] = 0xFF;
                    out[dst + 1] = 0xFF;
                    out[dst + 2] = 0xFF;
                    out[dst + 3] = 0x7D;

                    // Soft anti-aliased outline around lit pixels
                    for (ox, oy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                        let nx = gx as i32 + ox;
                        let ny = gy as i32 + oy;
                        if nx >= cell_x as i32
                            && nx < (cell_x + 32) as i32
                            && ny >= cell_y as i32
                            && ny < (cell_y + 40) as i32
                        {
                            let ndst = (ny as usize * w + nx as usize) * 4;
                            if out[ndst + 3] == 0 {
                                out[ndst] = 0x66;
                                out[ndst + 1] = 0x66;
                                out[ndst + 2] = 0x66;
                                out[ndst + 3] = 0x4B;
                            }
                        }
                    }
                }
            }
        }
    }

    out
}

/// Rasterizes the 512×80 `FNTEXOSD` special symbol page (`16 cols × 2 rows` of `32×40` cells),
/// including slot 4 (`®` registered trademark symbol used by `\7o004`).
fn generate_fntexosd_rgba() -> Vec<u8> {
    let w = 512usize;
    let h = 80usize;
    let mut out = vec![0u8; w * h * 4];

    for slot in 0..32usize {
        let col = slot % 16;
        let row = slot / 16;
        let cx = col * 32 + 15;
        let cy = row * 40 + 20;

        for y in 0..40usize {
            for x in 0..32usize {
                let gx = col * 32 + x;
                let gy = row * 40 + y;
                let dx = (gx as i32 - cx as i32) as f32;
                let dy = (gy as i32 - cy as i32) as f32 * 0.75;
                let dist = (dx * dx + dy * dy).sqrt();
                if (dist - 8.5).abs() <= 1.2 || (dx.abs() <= 3.0 && dy.abs() <= 4.0) {
                    let dst = (gy * w + gx) * 4;
                    out[dst] = 0xFF;
                    out[dst + 1] = 0xFF;
                    out[dst + 2] = 0xFF;
                    out[dst + 3] = 0x7D;
                }
            }
        }
    }

    out
}

fn render_text_banner_rgba(
    w: usize,
    h: usize,
    lines: &[(usize, usize, &str, usize)],
    metrics: &[[i32; 2]; 97],
) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    for &(start_x, start_y, text, scale) in lines {
        let mut pen_x = start_x;
        for ch in text.chars() {
            let code = (ch as usize).saturating_sub(32).min(96);
            let bmp = glyph_bitmap_8x8(ch);
            let adv = ((metrics[code][1] as usize) * scale) / 2;
            for py in 0..(16 * scale) {
                let by = py / (2 * scale);
                let bits = bmp[by.min(7)];
                for px in 0..(8 * scale) {
                    let bx = px / scale;
                    if ((bits >> (7 - bx.min(7))) & 1) != 0 {
                        let gx = pen_x + px;
                        let gy = start_y + py;
                        if gx < w && gy < h {
                            let dst = (gy * w + gx) * 4;
                            out[dst] = 0xFF;
                            out[dst + 1] = 0xFF;
                            out[dst + 2] = 0xFF;
                            out[dst + 3] = 0x80;
                        }
                    }
                }
            }
            pen_x += adv.max(6);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::build_synthetic_romdir_container;

    #[test]
    fn test_procedural_asset_pack_completeness_and_invariants() {
        let pack = AssetPack::new_procedural();
        assert_eq!(pack.textures.len(), STATIC_TEXTURE_COUNT);
        for (idx, tex) in pack.textures.iter().enumerate() {
            assert_eq!(tex.id as usize, idx);
            assert_eq!(tex.rgba.len(), (tex.width * tex.height * 4) as usize);
        }

        // Verify TEXCBINV is the exact bitwise RGB complement of TEXCBUMP (`docs/glass-refraction.md` §3)
        let bump = &pack.textures[TextureId::TexcBump as usize].rgba;
        let binv = &pack.textures[TextureId::TexcBinv as usize].rgba;
        assert_eq!(bump.len(), 64 * 64 * 4);
        assert_eq!(binv.len(), 64 * 64 * 4);
        for i in 0..(64 * 64) {
            assert_eq!(bump[i * 4], !binv[i * 4]);
            assert_eq!(bump[i * 4 + 1], !binv[i * 4 + 1]);
            assert_eq!(bump[i * 4 + 2], !binv[i * 4 + 2]);
            assert_eq!(bump[i * 4 + 3], 0x7F);
            assert_eq!(binv[i * 4 + 3], 0x7F);
        }

        // Verify fontAsciiMetrics spot-checks from `docs/menu-text.md` §1.1
        assert_eq!(pack.font_ascii_metrics[('1' as usize) - 32], [5, 23]);
        assert_eq!(pack.font_ascii_metrics[('A' as usize) - 32], [4, 25]);
        assert_eq!(pack.font_ascii_metrics[('W' as usize) - 32], [0, 32]);
        assert_eq!(pack.font_ascii_metrics[('l' as usize) - 32], [12, 9]);
        assert_eq!(pack.font_ascii_metrics[(' ' as usize) - 32], [0, 13]);

        // Verify rod (16 faces) and cube (6 faces) meshes
        assert_eq!(pack.rod_mesh.face_count, 16);
        assert_eq!(pack.cube_mesh.face_count, 6);
    }

    #[test]
    fn test_romdir_asset_import_updates_textures_and_rebuilds_binv() {
        let mut pack = AssetPack::new_procedural();
        let custom_bump = vec![0x42u8; 4096];
        let container = build_synthetic_romdir_container(&[("TEXCBUMP", &custom_bump)]);
        let count = pack
            .import_from_romdir_blob(&container)
            .expect("import should succeed");
        assert_eq!(count, 1);
        assert!(pack.is_user_imported);
        assert_eq!(pack.textures[TextureId::TexcBump as usize].rgba[0], 0x42);
        assert_eq!(pack.textures[TextureId::TexcBinv as usize].rgba[0], !0x42);
    }
}
