//! Deterministic IEEE-754 `f32` math, PlayStation 2 `sceVu0FMATRIX` operations,
//! `MatrixDrive` 16-bit angle stack, and WebGPU clip-space projection.
//!
//! All trigonometric functions are implemented in pure Rust over `f32` so that
//! native `x86_64` tests and `wasm32-unknown-unknown` browser execution produce
//! bit-for-bit identical floating-point results without platform libc drift.

pub const PI: f32 = core::f32::consts::PI;
pub const TAU: f32 = core::f32::consts::TAU;
pub const FRAC_PI_2: f32 = core::f32::consts::FRAC_PI_2;

#[inline]
#[must_use]
pub fn absf(x: f32) -> f32 {
    f32::from_bits(x.to_bits() & 0x7fff_ffff)
}

#[inline]
#[must_use]
pub fn sqrtf(x: f32) -> f32 {
    if x <= 0.0 {
        0.0
    } else {
        // Maps to IEEE-754 hardware `sqrtss` on x86_64 and `f32.sqrt` in WASM
        // (both strictly rounded to 0.5 ULP per IEEE-754).
        x.sqrt()
    }
}

#[inline]
#[must_use]
pub fn clampf(x: f32, lo: f32, hi: f32) -> f32 {
    if x.is_nan() || x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

#[inline]
#[must_use]
pub fn floorf(x: f32) -> f32 {
    if !x.is_finite() || absf(x) >= 8_388_608.0 {
        return x;
    }
    let i = x as i32;
    let f = i as f32;
    if f > x { f - 1.0 } else { f }
}

/// Wrap angle in radians into `[-PI, PI]` using IEEE-754 `f64` reduction so large
/// multiples of `TAU` retain full `f32` precision with zero libm dependency.
#[inline]
#[must_use]
pub fn wrap_pi(a: f32) -> f32 {
    if !a.is_finite() {
        return 0.0;
    }
    let ad = f64::from(a);
    let tau = core::f64::consts::TAU;
    let pi = core::f64::consts::PI;
    let q = (ad + pi) / tau;
    let qi = q as i64;
    let turns = if (qi as f64) > q {
        (qi - 1) as f64
    } else {
        qi as f64
    };
    let mut r = (ad - turns * tau) as f32;
    if r > PI {
        r -= TAU;
    } else if r < -PI {
        r += TAU;
    }
    r
}

/// Minimax polynomial on `[-PI/4, PI/4]` for `sin(r)`.
#[inline]
fn poly_sin(r: f32) -> f32 {
    let r2 = r * r;
    let p = ((-0.000_195_072_7_f32 * r2 + 0.008_332_076_f32) * r2 - 0.166_666_55_f32) * r2;
    r + r * p
}

/// Minimax polynomial on `[-PI/4, PI/4]` for `cos(r)`.
#[inline]
fn poly_cos(r: f32) -> f32 {
    let r2 = r * r;
    1.0 + r2
        * (-0.5_f32
            + r2 * (0.041_666_64_f32 + r2 * (-0.001_388_731_6_f32 + r2 * 0.000_024_433_157_f32)))
}

/// Deterministic Cody-Waite reduced `sin(x)` in `f32`.
#[must_use]
pub fn sinf(x: f32) -> f32 {
    if !x.is_finite() {
        return 0.0;
    }
    let xr = wrap_pi(x);
    let k = floorf(xr * core::f32::consts::FRAC_2_PI + 0.5) as i32;
    let kf = k as f32;
    // Two-term Cody-Waite PI/2 split using exact IEEE-754 FRAC_PI_2 + residual
    let r = (xr - kf * FRAC_PI_2) + kf * 4.371_139e-8_f32;
    match k.rem_euclid(4) {
        0 => poly_sin(r),
        1 => poly_cos(r),
        2 => -poly_sin(r),
        _ => -poly_cos(r),
    }
}

/// Deterministic Cody-Waite reduced `cos(x)` in `f32`.
#[must_use]
pub fn cosf(x: f32) -> f32 {
    if !x.is_finite() {
        return 1.0;
    }
    let xr = wrap_pi(x);
    let k = floorf(xr * core::f32::consts::FRAC_2_PI + 0.5) as i32;
    let kf = k as f32;
    let r = (xr - kf * FRAC_PI_2) + kf * 4.371_139e-8_f32;
    match k.rem_euclid(4) {
        0 => poly_cos(r),
        1 => -poly_sin(r),
        2 => -poly_cos(r),
        _ => poly_sin(r),
    }
}

/// Deterministic `atan2(y, x)` in `f32`.
#[must_use]
pub fn atan2f(y: f32, x: f32) -> f32 {
    if !x.is_finite() || !y.is_finite() || (x == 0.0 && y == 0.0) {
        return 0.0;
    }
    let ax = absf(x);
    let ay = absf(y);
    let mn = if ax < ay { ax } else { ay };
    let mx = if ax < ay { ay } else { ax };
    let a = mn / mx;
    let s = a * a;
    let mut r = ((-0.046_496_473_f32 * s + 0.159_314_22_f32) * s - 0.327_622_77_f32) * s * a + a;
    if ay > ax {
        r = FRAC_PI_2 - r;
    }
    if x < 0.0 {
        r = PI - r;
    }
    if y < 0.0 { -r } else { r }
}

/// Convert a PS2 `MatrixDrive` 16-bit integer angle (`65536 == 2*PI`) to radians.
#[inline]
#[must_use]
pub fn md_angle_to_rad(angle: i32) -> f32 {
    let wrapped = angle.rem_euclid(65_536);
    (wrapped as f32) * (TAU / 65_536.0)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    #[must_use]
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    #[must_use]
    pub fn cross(self, rhs: Self) -> Self {
        Self {
            x: self.y * rhs.z - self.z * rhs.y,
            y: self.z * rhs.x - self.x * rhs.z,
            z: self.x * rhs.y - self.y * rhs.x,
        }
    }

    #[must_use]
    pub fn length(self) -> f32 {
        sqrtf(self.dot(self))
    }

    #[must_use]
    pub fn normalized(self) -> Self {
        let len = self.length();
        if len > 1e-8 {
            let inv = 1.0 / len;
            Self::new(self.x * inv, self.y * inv, self.z * inv)
        } else {
            Self::new(0.0, 0.0, 0.0)
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Vec4 {
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    #[must_use]
    pub const fn xyz(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }
}

/// Column-major 4x4 matrix matching `sceVu0FMATRIX` (`cols[col][row]`, so `cols[3]` is translation).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4 {
    pub cols: [[f32; 4]; 4],
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::identity()
    }
}

impl Mat4 {
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            cols: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    #[must_use]
    pub fn mul_mat(self, rhs: Self) -> Self {
        let mut out = [[0.0_f32; 4]; 4];
        for (c, out_col) in out.iter_mut().enumerate() {
            let v = rhs.cols[c];
            for (r, slot) in out_col.iter_mut().enumerate() {
                *slot = self.cols[0][r] * v[0]
                    + self.cols[1][r] * v[1]
                    + self.cols[2][r] * v[2]
                    + self.cols[3][r] * v[3];
            }
        }
        Self { cols: out }
    }

    #[must_use]
    pub fn apply(self, v: Vec4) -> Vec4 {
        Vec4::new(
            self.cols[0][0] * v.x
                + self.cols[1][0] * v.y
                + self.cols[2][0] * v.z
                + self.cols[3][0] * v.w,
            self.cols[0][1] * v.x
                + self.cols[1][1] * v.y
                + self.cols[2][1] * v.z
                + self.cols[3][1] * v.w,
            self.cols[0][2] * v.x
                + self.cols[1][2] * v.y
                + self.cols[2][2] * v.z
                + self.cols[3][2] * v.w,
            self.cols[0][3] * v.x
                + self.cols[1][3] * v.y
                + self.cols[2][3] * v.z
                + self.cols[3][3] * v.w,
        )
    }

    #[must_use]
    pub fn rot_x(a: f32) -> Self {
        let (s, c) = (sinf(a), cosf(a));
        let mut m = Self::identity();
        m.cols[1][1] = c;
        m.cols[1][2] = s;
        m.cols[2][1] = -s;
        m.cols[2][2] = c;
        m
    }

    #[must_use]
    pub fn rot_y(a: f32) -> Self {
        let (s, c) = (sinf(a), cosf(a));
        let mut m = Self::identity();
        m.cols[0][0] = c;
        m.cols[0][2] = -s;
        m.cols[2][0] = s;
        m.cols[2][2] = c;
        m
    }

    #[must_use]
    pub fn rot_z(a: f32) -> Self {
        let (s, c) = (sinf(a), cosf(a));
        let mut m = Self::identity();
        m.cols[0][0] = c;
        m.cols[0][1] = s;
        m.cols[1][0] = -s;
        m.cols[1][1] = c;
        m
    }

    #[must_use]
    pub fn translate(x: f32, y: f32, z: f32) -> Self {
        let mut m = Self::identity();
        m.cols[3] = [x, y, z, 1.0];
        m
    }

    /// Exact reconstruction of `sceVu0CameraMatrix` (`osdbits/menu.c:265`):
    /// `zd` is camera forward direction, `yd` is up reference, `pos` is world eye position.
    #[must_use]
    pub fn ps2_camera(pos: Vec3, zd: Vec3, yd: Vec3) -> Self {
        let bx = yd.cross(zd).normalized();
        let bz = zd.normalized();
        let by = bz.cross(bx);
        Self {
            cols: [
                [bx.x, by.x, bz.x, 0.0],
                [bx.y, by.y, bz.y, 0.0],
                [bx.z, by.z, bz.z, 0.0],
                [-bx.dot(pos), -by.dot(pos), -bz.dot(pos), 1.0],
            ],
        }
    }

    /// Build a WebGPU clip-space projection matrix directly from PS2 `sceVu0ViewScreenMatrix`
    /// parameters (`scrz`, `ax`, `ay`, `screen_w`, `screen_h`, `near`, `far`).
    ///
    /// Maps PS2 GS screen coordinates (`X_gs = ax*scrz*x/z + cx`, `Y_gs = ay*scrz*y/z + cy`
    /// where `Y_gs` increases downward) into WebGPU NDC (`x,y ∈ [-1, 1]`, `y` up, `z ∈ [0, 1]`).
    #[must_use]
    pub fn ps2_webgpu_projection(
        scrz: f32,
        ax: f32,
        ay: f32,
        screen_w: f32,
        screen_h: f32,
        near: f32,
        far: f32,
    ) -> Self {
        let half_w = (screen_w * 0.5).max(1.0);
        let half_h = (screen_h * 0.5).max(1.0);
        let depth_span = (far - near).max(1.0);
        let z_a = far / depth_span;
        let z_b = -(far * near) / depth_span;

        let mut m = Self {
            cols: [[0.0; 4]; 4],
        };
        m.cols[0][0] = (ax * scrz) / half_w;
        m.cols[1][1] = -(ay * scrz) / half_h;
        m.cols[2][2] = z_a;
        m.cols[2][3] = 1.0;
        m.cols[3][2] = z_b;
        m
    }
}

impl core::ops::Mul for Mat4 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        self.mul_mat(rhs)
    }
}

/// Module U's `MatrixDrive` post-multiplying matrix builder (`osdbits/menu.c:320`).
#[derive(Clone, Copy, Debug, Default)]
pub struct MatrixDrive {
    pub top: Mat4,
}

impl MatrixDrive {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            top: Mat4::identity(),
        }
    }

    pub fn unit(&mut self) {
        self.top = Mat4::identity();
    }

    pub fn rot_x(&mut self, angle_16: i32) {
        self.top = self.top * Mat4::rot_x(md_angle_to_rad(angle_16));
    }

    pub fn rot_y(&mut self, angle_16: i32) {
        self.top = self.top * Mat4::rot_y(md_angle_to_rad(angle_16));
    }

    pub fn rot_z(&mut self, angle_16: i32) {
        self.top = self.top * Mat4::rot_z(md_angle_to_rad(angle_16));
    }

    pub fn translatef(&mut self, x: f32, y: f32, z: f32) {
        self.top = self.top * Mat4::translate(x, y, z);
    }

    /// `mdTranslate(v4)` where `v4.w` can be `0.0` (used by Config Cubes to keep Z flat!).
    pub fn translate_vec4(&mut self, v: Vec4) {
        let mut t = Mat4::identity();
        t.cols[3] = [v.x, v.y, v.z, if v.w == 0.0 { 1.0 } else { v.w }];
        self.top = self.top * t;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::SplitMix64;

    #[test]
    fn trig_matches_f64_reference_across_full_range() {
        let mut sm = SplitMix64::new(0x1234_abcd_9876_5432);
        for i in 0..2_000 {
            let x = if i < 8 {
                [-TAU, -PI, -FRAC_PI_2, 0.0, FRAC_PI_2, PI, TAU, 100.0][i]
            } else {
                sm.next_f32_range(-500.0, 500.0)
            };
            let s = sinf(x);
            let c = cosf(x);
            let s_ref = (x as f64).sin() as f32;
            let c_ref = (x as f64).cos() as f32;
            assert!(s.is_finite() && c.is_finite());
            assert!(
                (s - s_ref).abs() < 2e-5,
                "sin mismatch at {x}: got {s}, ref {s_ref}"
            );
            assert!(
                (c - c_ref).abs() < 2e-5,
                "cos mismatch at {x}: got {c}, ref {c_ref}"
            );
            let unit_err = (s * s + c * c - 1.0).abs();
            assert!(unit_err < 5e-5, "sin^2+cos^2 drift {unit_err} at {x}");
        }
    }

    #[test]
    fn non_finite_inputs_are_sanitized_without_nan_propagation() {
        assert_eq!(sinf(f32::NAN), 0.0);
        assert_eq!(cosf(f32::INFINITY), 1.0);
        assert_eq!(atan2f(f32::NAN, 1.0), 0.0);
        assert_eq!(clampf(f32::NAN, -1.0, 1.0), -1.0);
        assert_eq!(sqrtf(-5.0), 0.0);
    }

    #[test]
    fn ps2_camera_and_webgpu_projection_map_forward_point_to_screen_center() {
        let cam = Mat4::ps2_camera(
            Vec3::new(0.0, 0.0, 16.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, 1.0, 0.0),
        );
        let proj = Mat4::ps2_webgpu_projection(1024.0, 1.0, 0.457_627, 640.0, 224.0, 1.0, 65536.0);
        let vp = proj * cam;

        let clip = vp.apply(Vec4::new(0.0, 0.0, 116.0, 1.0));
        assert!((clip.w - 100.0).abs() < 1e-4);
        assert!((clip.x / clip.w).abs() < 1e-5);
        assert!((clip.y / clip.w).abs() < 1e-5);
        let ndc_z = clip.z / clip.w;
        assert!(ndc_z > 0.0 && ndc_z < 1.0);
    }
}
