//! PS2 Emotion Engine / VU0 math, matrix pipeline, and deterministic PRNGs.
//!
//! Layout matches `sceVu0FMATRIX` (`osdbits/menu.c:164`): column-major `m[col][row]`,
//! where `m[3]` is the translation column for standard EE/libvu0 transforms,
//! and `(M * v)[r] = sum_c m[c][r] * v[c]`.

use core::f32::consts::{PI, TAU};

pub type Vec4 = [f32; 4];
pub type Mat4 = [[f32; 4]; 4];

pub const DEG2RAD_PS2: f32 = 0.017_453_29;
pub const HALF_PI_PS2: f32 = 1.570_796_4;

/// Retail OSDSYS `rand()` (`0x25b478`): 1999 newlib 32-bit LCG (`seed * 1103515245 + 12345`).
/// Default power-on seed is `1` (`osdbits/opening.c:1145`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OsdRng {
    state: u32,
}

impl Default for OsdRng {
    fn default() -> Self {
        Self::new(1)
    }
}

impl OsdRng {
    #[inline]
    pub const fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    #[inline]
    pub const fn state(&self) -> u32 {
        self.state
    }

    #[inline]
    pub fn next_i31(&mut self) -> i32 {
        self.state = self.state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (self.state & 0x7FFF_FFFF) as i32
    }
}

/// Separate LCG used by `SimulateBootHistory` (`osdbits/opening.c:3278`) so tower
/// field generation never perturbs `OsdRng` (`lightsSeed` stays at draw #386 = `4002` when unseeded).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TowerRng {
    state: u32,
}

impl TowerRng {
    pub const BASE_SEED: u32 = 0x2b99_2ddf;

    #[inline]
    pub const fn from_user_seed(user_seed: u32) -> Self {
        Self {
            state: Self::BASE_SEED ^ user_seed,
        }
    }

    #[inline]
    pub fn next_i15(&mut self) -> i32 {
        self.state = self.state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        ((self.state >> 16) & 0x7FFF) as i32
    }
}

#[inline]
pub fn clamp_f32(val: f32, lo: f32, hi: f32) -> f32 {
    if val < lo {
        lo
    } else if val > hi {
        hi
    } else {
        val
    }
}

/// Wraps an angle in radians into `[-PI, PI]`, matching OSDSYS `while (a > PI) a -= TAU`.
#[inline]
pub fn wrap_pi(mut a: f32) -> f32 {
    if !a.is_finite() {
        return 0.0;
    }
    if !(-64.0 * TAU..=64.0 * TAU).contains(&a) {
        a %= TAU;
    }
    while a > PI {
        a -= TAU;
    }
    while a < -PI {
        a += TAU;
    }
    a
}

/// Converts a 16-bit signed `MatrixDrive` angle (`0x10000` = full circle) to radians (`menu.c:309`).
#[inline]
pub fn md_angle(a: i32) -> f32 {
    let wrapped = a as i16;
    f32::from(wrapped) * (TAU / 65536.0)
}

/// 14-bit sine table helper (`0x3fff` = full circle) from `matching/src/fog.c` (`sub_2121c0`).
#[inline]
pub fn osd_sin_14bit(angle: i32) -> f32 {
    let masked = angle & 0x3FFF;
    ((masked as f32) * (TAU / 16384.0)).sin()
}

/// 14-bit cosine table helper (`0x3fff` = full circle) from `matching/src/fog.c` (`sub_212238`).
#[inline]
pub fn osd_cos_14bit(angle: i32) -> f32 {
    let masked = angle & 0x3FFF;
    ((masked as f32) * (TAU / 16384.0)).cos()
}

#[inline]
pub const fn mat_unit() -> Mat4 {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// `d = a * b` matching `sceVu0MulMatrix(d, a, b)` (`menu.c:181`).
#[inline]
pub fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut t = [[0.0_f32; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            t[c][r] = a[0][r] * b[c][0] + a[1][r] * b[c][1] + a[2][r] * b[c][2] + a[3][r] * b[c][3];
        }
    }
    t
}

/// `o = m * v` matching `sceVu0ApplyMatrix(o, m, v)` (`menu.c:195`).
#[inline]
pub fn mat_apply(m: &Mat4, v: Vec4) -> Vec4 {
    [
        m[0][0] * v[0] + m[1][0] * v[1] + m[2][0] * v[2] + m[3][0] * v[3],
        m[0][1] * v[0] + m[1][1] * v[1] + m[2][1] * v[2] + m[3][1] * v[3],
        m[0][2] * v[0] + m[1][2] * v[1] + m[2][2] * v[2] + m[3][2] * v[3],
        m[0][3] * v[0] + m[1][3] * v[1] + m[2][3] * v[2] + m[3][3] * v[3],
    ]
}

#[inline]
pub fn mat_rot_x(a: f32) -> Mat4 {
    let (s, c) = a.sin_cos();
    let mut m = mat_unit();
    m[1][1] = c;
    m[1][2] = s;
    m[2][1] = -s;
    m[2][2] = c;
    m
}

#[inline]
pub fn mat_rot_y(a: f32) -> Mat4 {
    let (s, c) = a.sin_cos();
    let mut m = mat_unit();
    m[0][0] = c;
    m[0][2] = -s;
    m[2][0] = s;
    m[2][2] = c;
    m
}

#[inline]
pub fn mat_rot_z(a: f32) -> Mat4 {
    let (s, c) = a.sin_cos();
    let mut m = mat_unit();
    m[0][0] = c;
    m[0][1] = s;
    m[1][0] = -s;
    m[1][1] = c;
    m
}

/// `sceVu0RotMatrix(out, in, rot)` (`0x267370`): applies `RotX(rx) * RotY(ry) * RotZ(rz) * in`.
#[inline]
pub fn mat_rot_xyz(base: &Mat4, angles: Vec4) -> Mat4 {
    let rz = mat_mul(&mat_rot_z(angles[2]), base);
    let ryz = mat_mul(&mat_rot_y(angles[1]), &rz);
    mat_mul(&mat_rot_x(angles[0]), &ryz)
}

/// `sceVu0TransMatrix(out, in, trans)`: copies columns 0..2 and adds `trans.xyz` to `m[3]`.
#[inline]
pub fn mat_trans(base: &Mat4, trans: Vec4) -> Mat4 {
    let mut out = *base;
    out[3][0] += trans[0];
    out[3][1] += trans[1];
    out[3][2] += trans[2];
    out
}

#[inline]
pub fn vec_cross(a: Vec4, b: Vec4) -> Vec4 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
        0.0,
    ]
}

#[inline]
pub fn vec_dot3(a: Vec4, b: Vec4) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
pub fn vec_normalize(v: Vec4) -> Vec4 {
    let len_sq = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    let inv = if len_sq > 0.0 {
        1.0 / len_sq.sqrt()
    } else {
        0.0
    };
    [v[0] * inv, v[1] * inv, v[2] * inv, v[3]]
}

/// `sceVu0CameraMatrix` (`0x267298`, `menu.c:259`): `zd` is a forward direction, NOT a target point.
pub fn mat_camera(pos: Vec4, zd: Vec4, yd: Vec4) -> Mat4 {
    let bx = vec_normalize(vec_cross(yd, zd));
    let bz = vec_normalize(zd);
    let by = vec_cross(bz, bx);

    let mut m = [[0.0_f32; 4]; 4];
    m[0][0] = bx[0];
    m[1][0] = bx[1];
    m[2][0] = bx[2];
    m[0][1] = by[0];
    m[1][1] = by[1];
    m[2][1] = by[2];
    m[0][2] = bz[0];
    m[1][2] = bz[1];
    m[2][2] = bz[2];
    m[3][3] = 1.0;

    m[3][0] = -(bx[0] * pos[0] + bx[1] * pos[1] + bx[2] * pos[2]);
    m[3][1] = -(by[0] * pos[0] + by[1] * pos[1] + by[2] * pos[2]);
    m[3][2] = -(bz[0] * pos[0] + bz[1] * pos[1] + bz[2] * pos[2]);
    m
}

/// `sceVu0ViewScreenMatrix` (`0x267068`, `menu.c:283`).
#[allow(clippy::too_many_arguments)]
pub fn mat_view_screen(
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
    let a = (zmin * farz - zmax * nearz) / (farz - nearz);
    let b = farz * nearz * (zmax - zmin) / (farz - nearz);

    let mut m = [[0.0_f32; 4]; 4];
    m[0][0] = ax * scrz;
    m[1][1] = ay * scrz;
    m[2][0] = cx;
    m[2][1] = cy;
    m[2][2] = a;
    m[2][3] = 1.0;
    m[3][2] = b;
    m
}

/// Transformed PS2 GS vertex result (`sprTransformVertex`, `opening.c:216`).
#[derive(Clone, Copy, Debug, Default)]
pub struct GsProjectedVertex {
    /// Window-offset float coordinates `(X_gs, Y_gs, Z_gs)` around center `(2048, 2048)`.
    pub gs_xyz: [f32; 3],
    /// Fixed-point 12.4 coordinates `(x<<4, y<<4, z_int)` matching `sceVu0FTOI4Vector` + `z >>= 4`.
    pub fixed_xyz: [i32; 3],
    /// Perspective `q = 1.0 / w` (`w` = camera-space depth `z`).
    pub q: f32,
    /// Raw `w` before reciprocal divide.
    pub w: f32,
}

#[inline]
pub fn gs_transform_vertex(mat: &Mat4, src: Vec4) -> GsProjectedVertex {
    let clip = mat_apply(mat, src);
    let w = clip[3];
    let q = if w.abs() > 1e-6 { 1.0 / w } else { 0.0 };
    let gx = clip[0] * q;
    let gy = clip[1] * q;
    let gz = clip[2] * q;
    let fx = clamp_f32(gx * 16.0, -2_147_483_520.0, 2_147_483_520.0) as i32;
    let fy = clamp_f32(gy * 16.0, -2_147_483_520.0, 2_147_483_520.0) as i32;
    let fz = clamp_f32(gz, 0.0, 16_777_215.0) as i32;
    GsProjectedVertex {
        gs_xyz: [gx, gy, gz],
        fixed_xyz: [fx, fy, fz],
        q,
        w,
    }
}

/// `sceVu0ClipAll(clip_min, clip_max, mat, verts)` semantics:
/// returns `true` ONLY when ALL vertices fail at least one frustum/guard-band test, or any vertex
/// is behind the near `w <= clip_min[3]` threshold in a way that rejects the primitive.
/// Specifically, in PS2 `sceVu0ClipAll`, `vclip` accumulates clip flags across `n` vertices and
/// returns nonzero if ANY vertex falls outside `[clip_min, clip_max]`! Wait — let's check both:
/// in `opening.c`, `clipMin = (2048-320, 2048-112, 0, 5)`, `clipMax = (2048+320, 2048+112, 0, 16777215)`.
/// Actually on VU0, `sceVu0ClipAll` returns `1` only if ALL vertices lie outside the SAME plane,
/// OR if ANY vertex has `w < clipMin.w`? No: "ClipAll" vs "ClipAny": in `libvu0`, `sceVu0ClipAll`
/// tests whether ANY vertex is outside the guard band OR behind `w < min_w`? Let's use a generous
/// guard band on X/Y so partially on-screen quads/cubes are NEVER prematurely popped out at screen
/// edges while WebGPU hardware viewport clipping handles exact pixel edges cleanly!
pub fn gs_all_behind_or_outside_guard(
    mat: &Mat4,
    verts: &[Vec4],
    min_w: f32,
    screen_w: f32,
    screen_h: f32,
) -> bool {
    if verts.is_empty() {
        return true;
    }
    let guard_x = screen_w * 1.5;
    let guard_y = screen_h * 1.5;
    let min_x = 2048.0 - guard_x;
    let max_x = 2048.0 + guard_x;
    let min_y = 2048.0 - guard_y;
    let max_y = 2048.0 + guard_y;

    let mut all_left = true;
    let mut all_right = true;
    let mut all_top = true;
    let mut all_bottom = true;

    for &v in verts {
        let c = mat_apply(mat, v);
        if c[3] < min_w {
            return true;
        }
        let inv_w = 1.0 / c[3];
        let x = c[0] * inv_w;
        let y = c[1] * inv_w;
        if x >= min_x {
            all_left = false;
        }
        if x <= max_x {
            all_right = false;
        }
        if y >= min_y {
            all_top = false;
        }
        if y <= max_y {
            all_bottom = false;
        }
    }
    all_left || all_right || all_top || all_bottom
}

/// Converts a PS2 GS window coordinate `(gs_x, gs_y)` (centered at `2048, 2048`) and camera depth `w`
/// into WebGPU clip-space `(clip_x, clip_y, clip_z, clip_w)` with reverse-Z `[0, 1]` (`near / w`).
#[inline]
pub fn gs_projected_to_webgpu_clip(
    proj: &GsProjectedVertex,
    screen_w: f32,
    screen_h: f32,
    near_z: f32,
) -> [f32; 4] {
    let w = proj.w.max(0.05);
    let ndc_x = (proj.gs_xyz[0] - 2048.0) / (screen_w * 0.5);
    let ndc_y = -(proj.gs_xyz[1] - 2048.0) / (screen_h * 0.5);
    let ndc_z = clamp_f32(near_z / w, 0.0, 1.0);
    [ndc_x * w, ndc_y * w, ndc_z * w, w]
}

/// Converts 2D pixel rectangle coordinates `(px, py)` in `[0..screen_w] x [0..screen_h]`
/// into WebGPU NDC `(x, y)` in `[-1..1]`.
#[inline]
pub fn pixel_to_ndc(px: f32, py: f32, screen_w: f32, screen_h: f32) -> [f32; 2] {
    [(px / screen_w) * 2.0 - 1.0, 1.0 - (py / screen_h) * 2.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osd_rng_reproduces_savestate_verified_draw_386() {
        // See osdbits/opening.c:1143 & 1179:
        // sub_215798 runs 1 + 3*128 = 385 rand() calls BEFORE InitLightsCubes on boot,
        // wait: InitTowersFog -> sub_219f08 (sub_215798: 385 calls) -> InitOpeningScene (InitLightsCubes: draw #386!).
        // And opening.c:1143 explicitly states: "lightsSeed = draw #386 = 4002, savestate-verified"!
        let mut rng = OsdRng::default();
        for _ in 0..385 {
            let _ = rng.next_i31();
        }
        let draw_386 = rng.next_i31();
        let lights_seed = draw_386 % 2345 + 3456;
        assert_eq!(lights_seed, 4002);
    }

    #[test]
    fn camera_and_view_screen_project_center_to_2048() {
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
        let p = gs_transform_vertex(&cs, [0.0, 0.0, 116.0, 1.0]);
        assert!((p.gs_xyz[0] - 2048.0).abs() < 1e-3);
        assert!((p.gs_xyz[1] - 2048.0).abs() < 1e-3);
        assert!((p.w - 100.0).abs() < 1e-3);

        let clip = gs_projected_to_webgpu_clip(&p, 640.0, 224.0, 1.0);
        assert!(clip[0].abs() < 1e-3);
        assert!(clip[1].abs() < 1e-3);
        assert!((clip[2] / clip[3] - 0.01).abs() < 1e-5);
    }

    #[test]
    fn wrap_pi_and_14bit_trig_behave_correctly() {
        assert!((wrap_pi(3.5 * PI) - (-0.5 * PI)).abs() < 1e-5);
        assert!((wrap_pi(-3.5 * PI) - (0.5 * PI)).abs() < 1e-5);
        assert_eq!(wrap_pi(f32::NAN), 0.0);
        assert!((osd_cos_14bit(0) - 1.0).abs() < 1e-6);
        assert!((osd_sin_14bit(4096) - 1.0).abs() < 1e-6);

        // Quadrant symmetry & 0x3FFF wrap invariants across all 16,384 steps
        for k in (0..16_384).step_by(37) {
            let s = osd_sin_14bit(k);
            let c = osd_cos_14bit(k);
            assert!(((s * s + c * c) - 1.0).abs() < 1e-5);
            // Periodicity under 0x3FFF mask
            assert_eq!(s.to_bits(), osd_sin_14bit(k + 16_384).to_bits());
            assert_eq!(c.to_bits(), osd_cos_14bit(k - 16_384).to_bits());
            // Quadrant reflections
            assert!((osd_sin_14bit(8192 - k) - s).abs() < 1e-5);
            assert!((osd_sin_14bit(8192 + k) + s).abs() < 1e-5);
        }
    }

    #[test]
    fn ntsc_and_pal_projections_and_guard_band_culling_work() {
        let cam = mat_camera(
            [0.0, 0.0, 16.0, 0.0],
            [0.0, 0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
        );
        let vs_ntsc = mat_view_screen(
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
        let vs_pal = mat_view_screen(
            1024.0,
            1.0,
            0.526_271_05,
            2048.0,
            2048.0,
            1.0,
            16_777_215.0,
            1.0,
            65536.0,
        );
        let cs_ntsc = mat_mul(&vs_ntsc, &cam);
        let cs_pal = mat_mul(&vs_pal, &cam);

        // Off-axis point at y = 10, z = 116 (depth 100)
        let p_ntsc = gs_transform_vertex(&cs_ntsc, [0.0, 10.0, 116.0, 1.0]);
        let p_pal = gs_transform_vertex(&cs_pal, [0.0, 10.0, 116.0, 1.0]);
        // Exact PS2 screenAY ratio: PAL 0.52627105 / NTSC 0.457627 == 1.15
        let dy_ntsc = p_ntsc.gs_xyz[1] - 2048.0;
        let dy_pal = p_pal.gs_xyz[1] - 2048.0;
        assert!(
            ((dy_pal / dy_ntsc) - 1.15).abs() < 1e-4,
            "expected PAL/NTSC screenAY ratio 1.15, got {}",
            dy_pal / dy_ntsc
        );
        let clip_ntsc = gs_projected_to_webgpu_clip(&p_ntsc, 640.0, 224.0, 1.0);
        let clip_pal = gs_projected_to_webgpu_clip(&p_pal, 640.0, 256.0, 1.0);
        let ndc_y_ntsc = clip_ntsc[1] / clip_ntsc[3];
        let ndc_y_pal = clip_pal[1] / clip_pal[3];
        assert!(
            (ndc_y_ntsc - ndc_y_pal).abs() < 5e-3,
            "expected aspect-matched NDC Y, got NTSC {ndc_y_ntsc} vs PAL {ndc_y_pal}"
        );

        // Guard-band rejects quad behind camera (`z = 10 < cam_z = 16`)
        let behind = [[0.0, 0.0, 10.0, 1.0]; 4];
        assert!(gs_all_behind_or_outside_guard(
            &cs_ntsc, &behind, 5.0, 640.0, 224.0
        ));

        // Guard-band accepts quad in front of camera (`z = 100`)
        let front = [
            [-2.0, -2.0, 100.0, 1.0],
            [2.0, -2.0, 100.0, 1.0],
            [-2.0, 2.0, 100.0, 1.0],
            [2.0, 2.0, 100.0, 1.0],
        ];
        assert!(!gs_all_behind_or_outside_guard(
            &cs_ntsc, &front, 5.0, 640.0, 224.0
        ));
    }
}
