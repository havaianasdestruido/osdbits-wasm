//! Minimal 3D math for the OSDSYS reconstruction.
//!
//! Conventions (documented in `docs/RENDERING.md`):
//!
//! * Vectors are `[f32; 4]` with the usual `w` component. World space is
//!   right-handed, `+x` right, `+y` **up**, `+z` forward.
//! * `Mat4` is **row-major** (`m[row][col]`), matching the PS2 VU/libvu0
//!   layout used by the reconstruction, and `Mat4::apply` computes
//!   `out = m * v` treating `v` as a column vector.
//! * Translation therefore lives in **column 3** (`m[3][*]` in the PS2
//!   source is the transposed form; see `docs/PORTING-NOTES.md`).
//!
//! The projection helpers reproduce the GS window mapping used by the
//! original: a 2048-centred, y-down 640x224 (NTSC) virtual screen, with the
//! libvu0 view-screen scale factors `scrz`, `ax`, `ay`.

pub type Vec3 = [f32; 3];
pub type Vec4 = [f32; 4];
pub type Mat4 = [[f32; 4]; 4];

pub const PI: f32 = std::f32::consts::PI;
pub const TAU: f32 = 2.0 * PI;
/// PS2 libvu0 constant used by the opening (`0x2a7190`).
pub const DEG2RAD: f32 = 0.017_453_29;
/// PS2 libvu0 constant (`0x2a7194`).
pub const PI_2: f32 = 1.570_796_4;

/// `scrz`: distance from the camera to the projection plane, libvu0
/// `sceVu0ViewScreenMatrix(viewScreenMatrix, 1024.0f, ...)` in `opening.c`.
pub const VIEW_SCREEN_DISTANCE: f32 = 1024.0;
/// `ax` of the same call.
pub const VIEW_SCREEN_AX: f32 = 1.0;
/// `ay` for a 640x224 NTSC framebuffer (`opening.c` line ~361).
pub const VIEW_SCREEN_AY_NTSC: f32 = 0.526_271_05;
/// `ay` for a 640x256 PAL framebuffer (`opening.c` line ~363).
pub const VIEW_SCREEN_AY_PAL: f32 = 0.457_627;
/// GS window centre (`cx`, `cy`).
pub const GS_WINDOW_CENTRE: f32 = 2048.0;
/// NTSC virtual framebuffer.
pub const SCREEN_W_NTSC: u32 = 640;
pub const SCREEN_H_NTSC: u32 = 224;
/// PAL virtual framebuffer.
pub const SCREEN_W_PAL: u32 = 640;
pub const SCREEN_H_PAL: u32 = 256;

#[inline]
pub fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

#[inline]
pub fn v4(x: f32, y: f32, z: f32, w: f32) -> Vec4 {
    [x, y, z, w]
}

#[inline]
pub fn unit_matrix() -> Mat4 {
    let mut m = [[0.0f32; 4]; 4];
    m[0][0] = 1.0;
    m[1][1] = 1.0;
    m[2][2] = 1.0;
    m[3][3] = 1.0;
    m
}

#[inline]
pub fn copy_matrix(m: &Mat4) -> Mat4 {
    *m
}

/// `out = m * v` (column-vector convention, row-major storage).
#[inline]
pub fn apply(m: &Mat4, v: &Vec4) -> Vec4 {
    let mut out = [0.0f32; 4];
    for (r, row) in m.iter().enumerate() {
        out[r] = row[0] * v[0] + row[1] * v[1] + row[2] * v[2] + row[3] * v[3];
    }
    out
}

/// `out = a * b` (row-major product; the PS2 `sceVu0MulMatrix` order).
pub fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [[0.0f32; 4]; 4];
    for r in 0..4 {
        for c in 0..4 {
            let mut sum = 0.0f32;
            for k in 0..4 {
                sum += a[r][k] * b[k][c];
            }
            out[r][c] = sum;
        }
    }
    out
}

pub fn rot_x(m: &Mat4, angle: f32) -> Mat4 {
    let (s, c) = (angle.sin(), angle.cos());
    let mut r = unit_matrix();
    r[1][1] = c;
    r[1][2] = -s;
    r[2][1] = s;
    r[2][2] = c;
    mul(&r, m)
}

pub fn rot_y(m: &Mat4, angle: f32) -> Mat4 {
    let (s, c) = (angle.sin(), angle.cos());
    let mut r = unit_matrix();
    r[0][0] = c;
    r[0][2] = s;
    r[2][0] = -s;
    r[2][2] = c;
    mul(&r, m)
}

pub fn rot_z(m: &Mat4, angle: f32) -> Mat4 {
    let (s, c) = (angle.sin(), angle.cos());
    let mut r = unit_matrix();
    r[0][0] = c;
    r[0][1] = -s;
    r[1][0] = s;
    r[1][1] = c;
    mul(&r, m)
}

/// `sceVu0RotMatrix` as the reconstruction uses it: rotations applied
/// Z-then-Y-then-X onto `base` (`opening.c` DrawTowers, `0x218830`).
pub fn rot_matrix(base: &Mat4, rot: &Vec4) -> Mat4 {
    let m = rot_z(base, rot[2]);
    let m = rot_y(&m, rot[1]);
    rot_x(&m, rot[0])
}

/// Translation into column 3, `out = T * m`.
pub fn translate(m: &Mat4, t: &Vec4) -> Mat4 {
    let mut out = copy_matrix(m);
    out[0][3] += t[0];
    out[1][3] += t[1];
    out[2][3] += t[2];
    out
}

/// World -> view matrix, matching `sceVu0CameraMatrix(m, p, zd, yd)`:
/// the camera sits at `p`, looks along `zd`, `yd` is the up hint.
///
/// Rows are the camera basis expressed in world space, so the product
/// `view * v` yields "right, up, forward" coordinates (forward positive
/// into the screen, as the GS expects).
pub fn camera_matrix(p: &Vec4, zd: &Vec4, yd: &Vec4) -> Mat4 {
    let z = normalize3([zd[0], zd[1], zd[2]]);
    let y_hint = [yd[0], yd[1], yd[2]];
    let mut x = cross(y_hint, z);
    if len3(x) < 1e-6 {
        // Degenerate up vector: fall back to world up, then world x.
        x = cross([0.0, 1.0, 0.0], z);
        if len3(x) < 1e-6 {
            x = [1.0, 0.0, 0.0];
        }
    }
    let x = normalize3(x);
    let y = normalize3(cross(z, x));
    let mut m = unit_matrix();
    m[0][..3].copy_from_slice(&x[..3]);
    m[1][..3].copy_from_slice(&y[..3]);
    m[2][..3].copy_from_slice(&z[..3]);
    m[0][3] = -(x[0] * p[0] + x[1] * p[1] + x[2] * p[2]);
    m[1][3] = -(y[0] * p[0] + y[1] * p[1] + y[2] * p[2]);
    m[2][3] = -(z[0] * p[0] + z[1] * p[1] + z[2] * p[2]);
    m
}

/// View -> NDC, reproducing the GS window mapping implied by libvu0's
/// `sceVu0ViewScreenMatrix(scrz, ax, ay, cx, cy, ...)`:
///
/// ```text
/// screen_x = cx + ax * scrz * x_view / z_view      (GS pixels, y down)
/// screen_y = cy + ay * scrz * y_view / z_view
/// ```
///
/// The GS y axis points **down**, the world/camera y axis points up, hence
/// the sign flip on y. `ndc.z` is filled with a linear depth compatible
/// with WebGPU's 0..1 clip range using the GS near/far planes
/// (`1 .. 16777215`) scaled to a usable window.
#[inline]
pub fn project_ndc(view: &Vec4, width: f32, height: f32, ax: f32, ay: f32, scrz: f32) -> [f32; 3] {
    let z = if view[2].abs() < 1e-6 { 1e-6 } else { view[2] };
    let sx = ax * scrz * view[0] / z;
    let sy = ay * scrz * view[1] / z;
    // NDC in [-1,1]; GS y down -> NDC y up is a negation.
    let nx = sx / (width * 0.5);
    let ny = -sy / (height * 0.5);
    let nz = (z / (z + 64.0)).clamp(0.0, 1.0);
    [nx, ny, nz]
}

/// Projection matrix for WebGPU (row-major, column-vector convention),
/// equivalent to `project_ndc`. Depth uses a reversed-free 0..1 range.
pub fn projection_matrix(width: f32, height: f32, ax: f32, ay: f32, scrz: f32) -> Mat4 {
    let mut m = [[0.0f32; 4]; 4];
    m[0][0] = ax * scrz / (width * 0.5);
    m[1][1] = -ay * scrz / (height * 0.5);
    // Linear depth ramp: z_ndc = z/(z+64); expressed as (a*z + b)/z with w=z.
    m[2][2] = 1.0;
    m[2][3] = 0.0;
    m[3][2] = 1.0;
    m[3][3] = 0.0;
    // The far/near scale is folded into the shader's depth handling; the
    // matrix above yields w' = z_view and z' = z_view, so the shader writes
    // z_view/(z_view+64) explicitly. Kept here for documentation and tests.
    m
}

pub fn len3(v: Vec3) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

pub fn normalize3(v: Vec3) -> Vec3 {
    let l = len3(v);
    if l < 1e-12 {
        [0.0, 0.0, 0.0]
    } else {
        [v[0] / l, v[1] / l, v[2] / l]
    }
}

pub fn normalize4(v: &Vec4) -> Vec4 {
    let n = normalize3([v[0], v[1], v[2]]);
    [n[0], n[1], n[2], 0.0]
}

pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn dot3(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_matrix_is_identity() {
        let m = unit_matrix();
        for r in 0..4 {
            for c in 0..4 {
                let want = if r == c { 1.0 } else { 0.0 };
                assert_eq!(m[r][c], want);
            }
        }
    }

    #[test]
    fn apply_identity_preserves_vector() {
        let v = v4(1.5, -2.25, 3.0, 1.0);
        let out = apply(&unit_matrix(), &v);
        assert_eq!(out, v);
    }

    #[test]
    fn translate_moves_column_three() {
        let m = translate(&unit_matrix(), &v4(1.0, 2.0, 3.0, 0.0));
        let out = apply(&m, &v4(0.0, 0.0, 0.0, 1.0));
        assert_eq!([out[0], out[1], out[2]], [1.0, 2.0, 3.0]);
    }

    #[test]
    fn rot_z_quarter_turn_maps_x_to_y() {
        let m = rot_z(&unit_matrix(), PI_2);
        let out = apply(&m, &v4(1.0, 0.0, 0.0, 1.0));
        assert!((out[0]).abs() < 1e-5, "x {}", out[0]);
        assert!((out[1] - 1.0).abs() < 1e-5, "y {}", out[1]);
    }

    #[test]
    fn camera_matrix_identity_orientation() {
        // Camera at origin looking down +z with up +y: view == identity.
        let m = camera_matrix(
            &v4(0.0, 0.0, 0.0, 0.0),
            &v4(0.0, 0.0, 1.0, 1.0),
            &v4(0.0, 1.0, 0.0, 1.0),
        );
        for (r, row) in m.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                let want = if r == c { 1.0 } else { 0.0 };
                assert!((v - want).abs() < 1e-6, "m[{r}][{c}] = {v}");
            }
        }
    }

    #[test]
    fn camera_matrix_moves_world_point_behind_camera() {
        let m = camera_matrix(
            &v4(0.0, 0.0, 16.0, 0.0),
            &v4(0.0, 0.0, 1.0, 1.0),
            &v4(0.0, 1.0, 0.0, 1.0),
        );
        // A point 10 units in front of the camera maps to view z = +10.
        let out = apply(&m, &v4(0.0, 0.0, 26.0, 1.0));
        assert!((out[2] - 10.0).abs() < 1e-5, "z {}", out[2]);
    }

    #[test]
    fn camera_roll_tilts_up_vector() {
        // up = (sin r, cos r, 0) as in the original opening.
        let r = 0.25f32;
        let m = camera_matrix(
            &v4(0.0, 0.0, 0.0, 0.0),
            &v4(0.0, 0.0, 1.0, 1.0),
            &v4(r.sin(), r.cos(), 0.0, 1.0),
        );
        let out = apply(&m, &v4(0.0, 1.0, 0.0, 1.0));
        // A positive roll tilts the world up axis to -x in view space (the
        // horizon tilts the other way on screen); mirrors sceVu0CameraMatrix
        // with the reconstruction's upDir = (sin r, cos r, 0).
        assert!((out[0] + r.sin()).abs() < 1e-5, "x {}", out[0]);
        assert!((out[1] - r.cos()).abs() < 1e-5, "y {}", out[1]);
    }

    #[test]
    fn projection_maps_centre_to_centre() {
        let ndc = project_ndc(
            &v4(0.0, 0.0, 100.0, 100.0),
            640.0,
            224.0,
            1.0,
            VIEW_SCREEN_AY_NTSC,
            1024.0,
        );
        assert_eq!(ndc[0], 0.0);
        assert_eq!(ndc[1], 0.0);
    }

    #[test]
    fn projection_hits_right_edge_at_half_screen_units() {
        // At z = scrz, x_view = 320 projects to the right screen edge.
        let ndc = project_ndc(
            &v4(320.0, 0.0, 1024.0, 1024.0),
            640.0,
            224.0,
            1.0,
            VIEW_SCREEN_AY_NTSC,
            1024.0,
        );
        assert!((ndc[0] - 1.0).abs() < 1e-5, "ndc.x {}", ndc[0]);
        let y = 112.0 / VIEW_SCREEN_AY_NTSC;
        let ndc = project_ndc(
            &v4(0.0, y, 1024.0, 1024.0),
            640.0,
            224.0,
            1.0,
            VIEW_SCREEN_AY_NTSC,
            1024.0,
        );
        assert!((ndc[1] + 1.0).abs() < 1e-5, "ndc.y {}", ndc[1]);
    }

    #[test]
    fn projection_matches_the_gs_non_square_pixel_aspect() {
        // The GS window is 640x224 NTSC with non-square pixels: at z = scrz
        // the half-width is 320 world units while the half-height is
        // 112 / ay = 212.9 world units. Displaying the 640x224 image in a
        // 4:3 frame (640x480) stretches y by 480/224 = 2.143, so a world
        // square lands on screen 1.13x taller than wide -- exactly the PS2's
        // on-TV behaviour. The renderer keeps the virtual 640x224 screen and
        // presents it in a 4:3 box for that reason (docs/RENDERING.md).
        let half_height_world: f32 = 112.0 / VIEW_SCREEN_AY_NTSC;
        let ratio: f32 = half_height_world / 320.0;
        assert!((ratio - 0.665_06).abs() < 1e-3, "ratio {ratio}");
        let display_y_scale: f32 = 480.0 / 224.0;
        let net: f32 = VIEW_SCREEN_AY_NTSC * display_y_scale;
        assert!((net - 1.127_7).abs() < 1e-3, "net aspect {net}");
        let tan_half_fov_y: f32 = half_height_world / VIEW_SCREEN_DISTANCE;
        let fov_y: f32 = 2.0 * tan_half_fov_y.atan().to_degrees();
        assert!((fov_y - 23.49).abs() < 0.1, "vertical FOV {fov_y}");
        let fov_x: f32 = 2.0 * (320.0f32 / VIEW_SCREEN_DISTANCE).atan().to_degrees();
        assert!((fov_x - 34.69).abs() < 0.1, "horizontal FOV {fov_x}");
    }

    #[test]
    fn projection_handles_zero_depth() {
        let ndc = project_ndc(
            &v4(1.0, 1.0, 0.0, 0.0),
            640.0,
            224.0,
            1.0,
            VIEW_SCREEN_AY_NTSC,
            1024.0,
        );
        assert!(ndc[0].is_finite() && ndc[1].is_finite() && ndc[2].is_finite());
    }

    #[test]
    fn clamp_orders_bounds() {
        assert_eq!(clamp(5.0, 0.0, 1.0), 1.0);
        assert_eq!(clamp(-5.0, 0.0, 1.0), 0.0);
        assert_eq!(clamp(0.5, 0.0, 1.0), 0.5);
        // NaN fails both comparisons and is returned unchanged; callers that
        // need a bound must guard explicitly (see Camera::step).
        assert!(clamp(f32::NAN, 0.0, 1.0).is_nan());
    }

    #[test]
    fn normalize_handles_zero_vector() {
        assert_eq!(normalize3([0.0, 0.0, 0.0]), [0.0, 0.0, 0.0]);
        let n = normalize3([0.0, 0.0, 4.0]);
        assert!((n[2] - 1.0).abs() < 1e-6);
    }
}
