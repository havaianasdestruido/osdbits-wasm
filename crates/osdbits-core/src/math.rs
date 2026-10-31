//! Deterministic PS2 math, `osdRand` LCG, `libvu0` matrix/vector operations,
//! and Module U's 16-bit integer angle `MatrixDrive` stack.
//!
//! Directly ported from `osdbits/opening.c`, `osdbits/menu.c`, and `osdbits/vucode_1.vsm`.

use std::f32::consts::TAU;

pub type Vec4 = [f32; 4];
pub type Mat4 = [[f32; 4]; 4];

pub const VEC4_ZERO: Vec4 = [0.0, 0.0, 0.0, 0.0];
pub const VEC4_UNIT_W: Vec4 = [0.0, 0.0, 0.0, 1.0];

pub const MAT4_IDENTITY: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// Exact port of `osdRand()` and `osdSrand()` from `osdbits/opening.c` (real `0x215798` / `0x2157b8`).
///
/// ```c
/// static u32 randSeed = 1;
/// int osdRand(void) {
///     randSeed = randSeed * 1103515245 + 12345;
///     return (randSeed >> 16) & 0x7fff;
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OsdRng {
    pub seed: u32,
}

impl Default for OsdRng {
    fn default() -> Self {
        Self { seed: 1 }
    }
}

impl OsdRng {
    pub const fn new(seed: u32) -> Self {
        Self { seed }
    }

    #[inline]
    pub fn srand(&mut self, seed: u32) {
        self.seed = seed;
    }

    #[inline]
    pub fn rand(&mut self) -> i32 {
        self.seed = self.seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        ((self.seed >> 16) & 0x7fff) as i32
    }

    #[inline]
    pub fn rand_f32_01(&mut self) -> f32 {
        (self.rand() as f32) / 32767.0
    }

    #[inline]
    pub fn rand_signed(&mut self) -> f32 {
        (self.rand() as f32) / 16384.0 - 1.0
    }
}

/// Deterministic SplitMix64 generator used for property-based and fuzz tests.
#[derive(Debug, Clone, Copy)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    pub fn next_f32_range(&mut self, min: f32, max: f32) -> f32 {
        let t = ((self.next_u32() >> 8) as f32) / ((1u32 << 24) as f32);
        min + (max - min) * t
    }
}

/// PS2 GS 12.4 fixed-point conversion (`FTOI4` on VU0/VU1).
#[inline]
pub fn ftoi4(v: f32) -> i32 {
    (v * 16.0).round() as i32
}

/// PS2 GS 12.4 fixed-point back to float (`ITOF4`).
#[inline]
pub fn itof4(v: i32) -> f32 {
    (v as f32) * 0.0625
}

/// Module U 16-bit integer angle sine (`65536 = 2*pi`, `0x2304C0`).
#[inline]
pub fn md_sin(angle: i32) -> f32 {
    let wrapped = (angle as i16) as f32;
    (wrapped * (TAU / 65536.0)).sin()
}

/// Module U 16-bit integer angle cosine (`65536 = 2*pi`, `0x230440`).
#[inline]
pub fn md_cos(angle: i32) -> f32 {
    let wrapped = (angle as i16) as f32;
    (wrapped * (TAU / 65536.0)).cos()
}

/// Opening fixed-point angle cosine (`0x2121f0`: `ang` in `0..4096` or `0..65536`).
#[inline]
pub fn osd_sin_u16(angle: u16) -> f32 {
    ((angle as f32) * (TAU / 65536.0)).sin()
}

#[inline]
pub fn osd_cos_u16(angle: u16) -> f32 {
    ((angle as f32) * (TAU / 65536.0)).cos()
}

#[inline]
pub fn vec4_add(a: Vec4, b: Vec4) -> Vec4 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]]
}

#[inline]
pub fn vec4_sub(a: Vec4, b: Vec4) -> Vec4 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]]
}

#[inline]
pub fn vec4_scale(v: Vec4, s: f32) -> Vec4 {
    [v[0] * s, v[1] * s, v[2] * s, v[3] * s]
}

#[inline]
pub fn vec4_dot3(a: Vec4, b: Vec4) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
pub fn vec4_cross3(a: Vec4, b: Vec4) -> Vec4 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
        0.0,
    ]
}

#[inline]
pub fn vec4_len3(v: Vec4) -> f32 {
    vec4_dot3(v, v).sqrt()
}

#[inline]
pub fn vec4_normalize3(v: Vec4) -> Vec4 {
    let len = vec4_len3(v);
    if len > 1.0e-7 {
        [v[0] / len, v[1] / len, v[2] / len, v[3]]
    } else {
        [0.0, 0.0, 0.0, v[3]]
    }
}

#[inline]
pub fn vec4_clamp(v: Vec4, min_val: f32, max_val: f32) -> Vec4 {
    [
        v[0].clamp(min_val, max_val),
        v[1].clamp(min_val, max_val),
        v[2].clamp(min_val, max_val),
        v[3].clamp(min_val, max_val),
    ]
}

/// `sceVu0UnitMatrix`
#[inline]
pub const fn mat4_identity() -> Mat4 {
    MAT4_IDENTITY
}

/// `sceVu0MulMatrix(dst, a, b)` / `MulMatrix` in `vucode_1.vsm`.
/// In PS2 `libvu0`, matrices are stored as 4 row vectors (`m[row][col]`), where `m[3]` is the translation row,
/// and `sceVu0ApplyMatrix(v, m, v_in)` computes `v_out[col] = sum_row(m[row][col] * v_in[row])`.
/// Thus `sceVu0MulMatrix(a, b)` applies `a` to each row of `b`: `out[i] = apply(a, b[i])`.
#[inline]
pub fn mat4_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [[0.0f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            out[i][j] =
                b[i][0] * a[0][j] + b[i][1] * a[1][j] + b[i][2] * a[2][j] + b[i][3] * a[3][j];
        }
    }
    out
}

/// `sceVu0ApplyMatrix`: transforms vector `v` by PS2 row-major matrix `m`:
/// `out[j] = v[0]*m[0][j] + v[1]*m[1][j] + v[2]*m[2][j] + v[3]*m[3][j]`.
#[inline]
pub fn mat4_apply(m: &Mat4, v: Vec4) -> Vec4 {
    [
        v[0] * m[0][0] + v[1] * m[1][0] + v[2] * m[2][0] + v[3] * m[3][0],
        v[0] * m[0][1] + v[1] * m[1][1] + v[2] * m[2][1] + v[3] * m[3][1],
        v[0] * m[0][2] + v[1] * m[1][2] + v[2] * m[2][2] + v[3] * m[3][2],
        v[0] * m[0][3] + v[1] * m[1][3] + v[2] * m[2][3] + v[3] * m[3][3],
    ]
}

/// `sceVu0TransMatrix`: sets the translation row `m[3][0..3]` of `m`.
#[inline]
pub fn mat4_translate(mut m: Mat4, t: Vec4) -> Mat4 {
    m[3][0] = t[0];
    m[3][1] = t[1];
    m[3][2] = t[2];
    m[3][3] = 1.0;
    m
}

/// `sceVu0RotMatrixX`: multiplies `m` by an X rotation of `rx` radians.
#[inline]
pub fn mat4_rot_x(m: &Mat4, rx: f32) -> Mat4 {
    let (s, c) = rx.sin_cos();
    let r = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, c, s, 0.0],
        [0.0, -s, c, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    mat4_mul(m, &r)
}

/// `sceVu0RotMatrixY`: multiplies `m` by a Y rotation of `ry` radians.
#[inline]
pub fn mat4_rot_y(m: &Mat4, ry: f32) -> Mat4 {
    let (s, c) = ry.sin_cos();
    let r = [
        [c, 0.0, -s, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [s, 0.0, c, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    mat4_mul(m, &r)
}

/// `sceVu0RotMatrixZ`: multiplies `m` by a Z rotation of `rz` radians.
#[inline]
pub fn mat4_rot_z(m: &Mat4, rz: f32) -> Mat4 {
    let (s, c) = rz.sin_cos();
    let r = [
        [c, s, 0.0, 0.0],
        [-s, c, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    mat4_mul(m, &r)
}

/// `sceVu0CameraMatrix(m, pos, zd, yd)` where `zd` is the forward view vector (or `target - pos`)
/// and `yd` is the up vector. Matches `libvu0` camera convention used in `opening.c` and `menu.c`.
pub fn sce_vu0_camera_matrix(pos: Vec4, zd: Vec4, yd: Vec4) -> Mat4 {
    let z = vec4_normalize3(zd);
    let x = vec4_normalize3(vec4_cross3(yd, z));
    let y = vec4_cross3(z, x);
    let tx = -vec4_dot3(pos, x);
    let ty = -vec4_dot3(pos, y);
    let tz = -vec4_dot3(pos, z);
    [
        [x[0], y[0], z[0], 0.0],
        [x[1], y[1], z[1], 0.0],
        [x[2], y[2], z[2], 0.0],
        [tx, ty, tz, 1.0],
    ]
}

/// `sceVu0ViewScreenMatrix(m, scrz, ax, ay, cx, cy, zmin, zmax, nearz, farz)`
/// Matches the PS2 GS screen projection matrix in `opening.c` (`scrz=512, ax=1, ay=0.5, cx=2048, cy=2048`)
/// and `menu.c`.
pub fn sce_vu0_view_screen_matrix(
    scrz: f32,
    ax: f32,
    ay: f32,
    cx: f32,
    cy: f32,
    zmin: f32,
    zmax: f32,
    nearz: f32,
    farz: f32,
) -> Mat4 {
    let denom = farz - nearz;
    let az = if denom.abs() > 1.0e-6 {
        (-farz * nearz) * (zmax - zmin) / denom
    } else {
        0.0
    };
    let cz = if denom.abs() > 1.0e-6 {
        (farz * zmax - nearz * zmin) / denom
    } else {
        zmax
    };
    [
        [scrz * ax, 0.0, 0.0, 0.0],
        [0.0, scrz * ay, 0.0, 0.0],
        [cx, cy, cz, 1.0],
        [0.0, 0.0, az, 0.0],
    ]
}

/// Projects a world-space or view-space vertex through `world_view` and `view_screen`.
/// Returns `Some((screen_x, screen_y, z_norm, q))` where `screen_x` and `screen_y` are in
/// `0..640` and `0..448` (or `0..224` field space) relative to the screen origin (`cx - w/2`, `cy - h/2`).
pub fn project_point(
    view_mat: &Mat4,
    view_screen: &Mat4,
    pos: Vec4,
    screen_w: f32,
    screen_h: f32,
) -> Option<(f32, f32, f32, f32)> {
    let cam = mat4_apply(view_mat, [pos[0], pos[1], pos[2], 1.0]);
    if cam[2] <= 1.0 {
        return None;
    }
    let clip = mat4_apply(view_screen, cam);
    if clip[3].abs() < 1.0e-5 {
        return None;
    }
    let q = 1.0 / clip[3];
    let gs_x = clip[0] * q;
    let gs_y = clip[1] * q;
    let gs_z = clip[2] * q;
    let sx = gs_x - (2048.0 - screen_w * 0.5);
    let sy = gs_y - (2048.0 - screen_h * 0.5);
    // Normalize Z into 0.0..1.0 for WebGPU depth buffer (higher GS Z = closer on PS2, so we map near->far)
    let z_norm = (1.0 - (cam[2] / 4000.0)).clamp(0.0001, 0.9999);
    let _ = gs_z;
    Some((sx, sy, z_norm, q))
}

/// Module U's 8-level matrix stack (`MatrixDrive`, `0x230000`, `osdbits/menu.c` lines 165-275).
#[derive(Debug, Clone)]
pub struct MatrixDrive {
    stack: [Mat4; 8],
    top: usize,
}

impl Default for MatrixDrive {
    fn default() -> Self {
        Self::new()
    }
}

impl MatrixDrive {
    pub const fn new() -> Self {
        Self {
            stack: [MAT4_IDENTITY; 8],
            top: 0,
        }
    }

    #[inline]
    pub fn reset(&mut self) {
        self.stack = [MAT4_IDENTITY; 8];
        self.top = 0;
    }

    #[inline]
    pub fn top(&self) -> &Mat4 {
        &self.stack[self.top]
    }

    #[inline]
    pub fn top_mut(&mut self) -> &mut Mat4 {
        &mut self.stack[self.top]
    }

    #[inline]
    pub fn unit(&mut self) {
        self.stack[self.top] = MAT4_IDENTITY;
    }

    #[inline]
    pub fn push(&mut self) {
        if self.top + 1 < self.stack.len() {
            let cur = self.stack[self.top];
            self.top += 1;
            self.stack[self.top] = cur;
        }
    }

    #[inline]
    pub fn pop(&mut self) {
        if self.top > 0 {
            self.top -= 1;
        }
    }

    /// `mdRotX(ang16)`: pre-multiplies top matrix by 16-bit angle X rotation (`0x2301C0`).
    #[inline]
    pub fn rot_x(&mut self, ang16: i32) {
        let s = md_sin(ang16);
        let c = md_cos(ang16);
        let r = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, c, s, 0.0],
            [0.0, -s, c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        self.stack[self.top] = mat4_mul(&self.stack[self.top], &r);
    }

    /// `mdRotY(ang16)`: pre-multiplies top matrix by 16-bit angle Y rotation (`0x230268`).
    #[inline]
    pub fn rot_y(&mut self, ang16: i32) {
        let s = md_sin(ang16);
        let c = md_cos(ang16);
        let r = [
            [c, 0.0, -s, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [s, 0.0, c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        self.stack[self.top] = mat4_mul(&self.stack[self.top], &r);
    }

    /// `mdRotZ(ang16)`: pre-multiplies top matrix by 16-bit angle Z rotation (`0x230310`).
    #[inline]
    pub fn rot_z(&mut self, ang16: i32) {
        let s = md_sin(ang16);
        let c = md_cos(ang16);
        let r = [
            [c, s, 0.0, 0.0],
            [-s, c, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        self.stack[self.top] = mat4_mul(&self.stack[self.top], &r);
    }

    /// `mdTranslatef(x, y, z)`: pre-multiplies top matrix by translation (`0x2300F8`).
    #[inline]
    pub fn translate(&mut self, x: f32, y: f32, z: f32) {
        let t = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [x, y, z, 1.0],
        ];
        self.stack[self.top] = mat4_mul(&self.stack[self.top], &t);
    }

    /// `mdScalef(x, y, z)`: pre-multiplies top matrix by scale (`0x230150`).
    #[inline]
    pub fn scale(&mut self, x: f32, y: f32, z: f32) {
        let s = [
            [x, 0.0, 0.0, 0.0],
            [0.0, y, 0.0, 0.0],
            [0.0, 0.0, z, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        self.stack[self.top] = mat4_mul(&self.stack[self.top], &s);
    }
}

/// VU1 `vucode_1.vsm` 4-directional light + ambient shading for a world-space normal.
/// Matches `vucode_1.vsm` (`START0` / `START1`):
/// 1. Transforms `normal` by `light_dirs` (4 light directions).
/// 2. Clamps dot products to `>= 0.0`.
/// 3. Multiplies by `light_colors` (4 light RGB columns + ambient column) and clamps to `0..255`.
pub fn vucode1_shade_normal(
    normal: Vec4,
    light_dirs: &[Vec4; 4],
    light_colors: &[Vec4; 4],
    ambient: Vec4,
) -> [u8; 4] {
    let n = vec4_normalize3(normal);
    let mut r = ambient[0];
    let mut g = ambient[1];
    let mut b = ambient[2];

    for i in 0..4 {
        let d = vec4_dot3(n, light_dirs[i]).max(0.0);
        r += d * light_colors[i][0];
        g += d * light_colors[i][1];
        b += d * light_colors[i][2];
    }

    [
        (r * 128.0).clamp(0.0, 255.0) as u8,
        (g * 128.0).clamp(0.0, 255.0) as u8,
        (b * 128.0).clamp(0.0, 255.0) as u8,
        0x80,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_osd_rand_known_sequence() {
        let mut rng = OsdRng::new(1);
        // First 5 values of seed * 1103515245 + 12345 >> 16 & 0x7fff starting at seed=1
        assert_eq!(rng.rand(), 16838);
        assert_eq!(rng.rand(), 5758);
        assert_eq!(rng.rand(), 10113);
        assert_eq!(rng.rand(), 17515);
        assert_eq!(rng.rand(), 31051);
    }

    #[test]
    fn test_md_sin_cos_identities() {
        assert!((md_sin(0) - 0.0).abs() < 1e-6);
        assert!((md_cos(0) - 1.0).abs() < 1e-6);
        assert!((md_sin(16384) - 1.0).abs() < 1e-5);
        assert!((md_cos(16384) - 0.0).abs() < 1e-5);
        assert!((md_cos(32768) + 1.0).abs() < 1e-5);

        let mut sm = SplitMix64::new(0x1234_5678_9ABC_DEF0);
        for _ in 0..500 {
            let ang = sm.next_u32() as i32;
            let s = md_sin(ang);
            let c = md_cos(ang);
            assert!((s * s + c * c - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn test_matrix_drive_stack_and_transforms() {
        let mut md = MatrixDrive::new();
        md.translate(10.0, 20.0, 30.0);
        md.push();
        md.translate(5.0, -5.0, 2.0);
        let p = mat4_apply(md.top(), [0.0, 0.0, 0.0, 1.0]);
        assert!((p[0] - 15.0).abs() < 1e-5);
        assert!((p[1] - 15.0).abs() < 1e-5);
        assert!((p[2] - 32.0).abs() < 1e-5);
        md.pop();
        let p2 = mat4_apply(md.top(), [0.0, 0.0, 0.0, 1.0]);
        assert!((p2[0] - 10.0).abs() < 1e-5);
        assert!((p2[1] - 20.0).abs() < 1e-5);
        assert!((p2[2] - 30.0).abs() < 1e-5);
    }

    #[test]
    fn test_camera_and_view_screen_projection() {
        let cam = sce_vu0_camera_matrix(
            [0.0, 0.0, -50.0, 1.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
        );
        let vs =
            sce_vu0_view_screen_matrix(512.0, 1.0, 1.0, 2048.0, 2048.0, 1.0, 65535.0, 1.0, 2000.0);
        let projected =
            project_point(&cam, &vs, [0.0, 0.0, 0.0, 1.0], 640.0, 448.0).expect("on screen");
        assert!((projected.0 - 320.0).abs() < 1e-3);
        assert!((projected.1 - 224.0).abs() < 1e-3);
    }
}
