//! The five refractive glass cubes.
//!
//! Ported from `CubeTransformAndClip` (`cube_21BF88`), `CubeFaceSetup`
//! (`cube_21BC90`), `InitCubes` (part of `OpeningInitLightsCubes`,
//! `0x217ab0`) and the illegal-disc variant `sub_219cb8` / `sub_21b690`.
//!
//! What is reproduced exactly:
//!
//! * per-instance anchors, rotation rates and start angles from the real seed
//!   tables (`0x27a210` opening, `0x27af50` illegal);
//! * the rotation wrap to [-PI, PI] per axis, every frame;
//! * the world/view transforms and the clip test against the 640x224 GS
//!   window (2048-centred);
//! * the per-face world normal, screen-projected inward normal, the
//!   screen-space winding sign that decides front/back, and the per-vertex
//!   view-angle "light" `|dot(faceNormal, dirToVertex)|` that drives the
//!   edge glow;
//! * the ten-pass descriptor table (blend selectors, texture selectors,
//!   UV modes, colour modes) that the renderer executes.
//!
//! What is *approximated* (documented in `docs/LIMITATIONS.md`): the exact
//! GS packet layering. A WebGPU renderer cannot sample the in-progress
//! framebuffer of a GS pass by pass, so the renderer composites the same
//! passes against a single scene-capture texture. Colour modes and blend
//! factors are the real ones.

use crate::camera::Camera;
use crate::frame::{TEX_BLP, TEX_BLPR, TEX_REF, TEX_SCREEN, TEX_WORK};
use crate::math::{
    apply, mul, normalize3, normalize4, rot_matrix, translate, unit_matrix, Mat4, Vec4, PI,
};

pub const INSTANCES: usize = 5;
pub const CUBE_HALF_EXTENT: f32 = 1.8;

/// `0x27a210`, 5 x (3 floats + pad): the opening's placement seeds.
pub const CUBE_SEED_TABLE: [[f32; 4]; INSTANCES] = [
    [3.5679, 0.5447, 2.5932, 0.0],
    [-0.9042, -1.1173, 3.7952, 0.0],
    [3.2639, -2.6491, 4.1075, 0.0],
    [-3.7296, -2.3677, 4.3654, 0.0],
    [-3.1017, 2.2409, 4.5429, 0.0],
];

/// `0x27af50`: the illegal scene's own seed table.
pub const ILLEGAL_CUBE_SEED_TABLE: [[f32; 4]; INSTANCES] = [
    [-10.4068, 4.1636, 5.0429, 0.0],
    [-12.9184, -4.2708, 4.3654, 0.0],
    [2.7639, 0.1509, 4.1075, 0.0],
    [4.0958, -1.3173, 3.0952, 0.0],
    [-2.4321, 0.5447, 2.7732, 0.0],
];

/// `0x27afe0`: face -> corner indices.
pub const CUBE_FACE_VERTS: [[usize; 4]; 6] = [
    [0, 1, 2, 3],
    [5, 4, 7, 6],
    [4, 0, 6, 2],
    [2, 3, 6, 7],
    [1, 5, 3, 7],
    [4, 5, 0, 1],
];

/// `0x27b090`: axis-aligned face normals.
pub const CUBE_FACE_NORMAL: [[f32; 4]; 6] = [
    [0.0, 0.0, -1.0, 1.0],
    [0.0, 0.0, 1.0, 1.0],
    [-1.0, 0.0, 0.0, 1.0],
    [0.0, 1.0, 0.0, 1.0],
    [1.0, 0.0, 0.0, 1.0],
    [0.0, -1.0, 0.0, 1.0],
];

/// `0x27b040/50/60/70`: the grey light-colour vectors.
pub const CUBE_COLOR_AMBIENT: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
pub const CUBE_COLOR_1: [f32; 4] = [0.5, 0.5, 0.5, 0.0];
pub const CUBE_COLOR_2: [f32; 4] = [0.5, 0.5, 0.5, 0.0];
pub const CUBE_COLOR_BASE: [f32; 4] = [0.2, 0.2, 0.2, 1.0];

/// A cube draw pass (the real ten-struct table). Values mirror
/// `cubePasses` / `illegalCubePasses` field by field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubePass {
    /// 1 = render into the working buffer, 0 = onto the screen.
    pub target_work: u32,
    /// >= 0: texture slot; -2: previous screen; -3: working buffer.
    pub sel: i32,
    pub abe: u32,
    pub blend_mode: u32,
    pub blend_fix: i32,
    pub texa_flag: u32,
    pub st_mode: u32,
    pub color_mode: u32,
    pub uv_mode: u32,
    pub uv_arg: f32,
    pub uv_arg2: f32,
    /// 1 = back faces, 0 = front faces.
    pub group: u32,
}

/// The opening's pass stack.
pub const CUBE_PASSES: [CubePass; 10] = [
    CubePass {
        target_work: 1,
        sel: -2,
        abe: 0,
        blend_mode: 4,
        blend_fix: 122,
        texa_flag: 1,
        st_mode: 1,
        color_mode: 2,
        uv_mode: 0,
        uv_arg: 0.0,
        uv_arg2: 1.0,
        group: 1,
    },
    CubePass {
        target_work: 1,
        sel: TEX_BLPR as i32,
        abe: 1,
        blend_mode: 5,
        blend_fix: 128,
        texa_flag: 0,
        st_mode: 0,
        color_mode: 0,
        uv_mode: 1,
        uv_arg: -0.00375,
        uv_arg2: 0.0,
        group: 1,
    },
    CubePass {
        target_work: 1,
        sel: TEX_REF as i32,
        abe: 1,
        blend_mode: 8,
        blend_fix: 42,
        texa_flag: 0,
        st_mode: 0,
        color_mode: 3,
        uv_mode: 2,
        uv_arg: -0.25,
        uv_arg2: 0.0,
        group: 1,
    },
    CubePass {
        target_work: 1,
        sel: TEX_BLP as i32,
        abe: 1,
        blend_mode: 5,
        blend_fix: 128,
        texa_flag: 0,
        st_mode: 0,
        color_mode: 0,
        uv_mode: 1,
        uv_arg: 0.00375,
        uv_arg2: 0.0,
        group: 1,
    },
    CubePass {
        target_work: 1,
        sel: TEX_REF as i32,
        abe: 1,
        blend_mode: 8,
        blend_fix: 42,
        texa_flag: 0,
        st_mode: 0,
        color_mode: 3,
        uv_mode: 2,
        uv_arg: -0.25,
        uv_arg2: 0.0,
        group: 1,
    },
    CubePass {
        target_work: 0,
        sel: -3,
        abe: 0,
        blend_mode: 4,
        blend_fix: 240,
        texa_flag: 1,
        st_mode: 1,
        color_mode: 2,
        uv_mode: 0,
        uv_arg: -0.084,
        uv_arg2: 1.0,
        group: 0,
    },
    CubePass {
        target_work: 0,
        sel: TEX_BLPR as i32,
        abe: 1,
        blend_mode: 5,
        blend_fix: 128,
        texa_flag: 0,
        st_mode: 0,
        color_mode: 0,
        uv_mode: 1,
        uv_arg: -0.0075,
        uv_arg2: 0.0,
        group: 0,
    },
    CubePass {
        target_work: 0,
        sel: TEX_REF as i32,
        abe: 1,
        blend_mode: 8,
        blend_fix: 64,
        texa_flag: 0,
        st_mode: 0,
        color_mode: 3,
        uv_mode: 2,
        uv_arg: 0.5,
        uv_arg2: 0.0,
        group: 0,
    },
    CubePass {
        target_work: 0,
        sel: TEX_BLP as i32,
        abe: 1,
        blend_mode: 5,
        blend_fix: 128,
        texa_flag: 0,
        st_mode: 0,
        color_mode: 0,
        uv_mode: 1,
        uv_arg: 0.0075,
        uv_arg2: 0.0,
        group: 0,
    },
    CubePass {
        target_work: 0,
        sel: TEX_REF as i32,
        abe: 1,
        blend_mode: 8,
        blend_fix: 64,
        texa_flag: 0,
        st_mode: 0,
        color_mode: 3,
        uv_mode: 2,
        uv_arg: 0.5,
        uv_arg2: 0.0,
        group: 0,
    },
];

/// The illegal scene's pass stack: UV zoom 0.8, zero env offsets, colour
/// mode 4 (red tint).
pub const ILLEGAL_CUBE_PASSES: [CubePass; 10] = [
    CubePass {
        uv_arg2: 0.8,
        ..CUBE_PASSES[0]
    },
    CUBE_PASSES[1],
    CubePass {
        color_mode: 4,
        uv_arg: 0.0,
        ..CUBE_PASSES[2]
    },
    CUBE_PASSES[3],
    CubePass {
        color_mode: 4,
        uv_arg: 0.0,
        ..CUBE_PASSES[4]
    },
    CubePass {
        uv_arg2: 0.8,
        ..CUBE_PASSES[5]
    },
    CUBE_PASSES[6],
    CubePass {
        color_mode: 4,
        uv_arg: 0.0,
        ..CUBE_PASSES[7]
    },
    CUBE_PASSES[8],
    CubePass {
        color_mode: 4,
        uv_arg: 0.0,
        ..CUBE_PASSES[9]
    },
];

/// Everything the renderer needs for one visible cube.
#[derive(Clone, Debug)]
pub struct CubeInstance {
    pub visible: bool,
    /// Instance index 0..4.
    pub index: usize,
    /// Live rotation angles (radians).
    pub angles: [f32; 3],
    /// World-space cube centre = `cubeAnchor`.
    pub anchor: [f32; 4],
    /// 8 world-space corners.
    pub corners: [[f32; 4]; 8],
    /// Per-face screen-space winding sign (> 0 = front-facing).
    pub face_sign: [f32; 6],
    /// Per-face, per-vertex view-angle light in 0..1.
    pub vertex_light: [[f32; 4]; 6],
    /// Per-face inward normal projected into camera space, normalised.
    pub screen_normal: [[f32; 4]; 6],
    /// Per-corner normalised camera-space direction.
    pub view_dir: [[f32; 4]; 8],
    /// Per-corner `1/w`.
    pub corner_q: [f32; 8],
    /// Cube centre in GS window coordinates.
    pub screen_center: [f32; 2],
    /// GS-space projected corners (2048-centred, y down).
    pub screen_verts: [[f32; 4]; 8],
    /// Base colour, 0..127 (already clamped like `sub_21b690`).
    pub base_color: [f32; 3],
}

impl CubeInstance {
    fn empty(index: usize) -> Self {
        CubeInstance {
            visible: false,
            index,
            angles: [0.0; 3],
            anchor: [0.0; 4],
            corners: [[0.0; 4]; 8],
            face_sign: [0.0; 6],
            vertex_light: [[0.0; 4]; 6],
            screen_normal: [[0.0; 4]; 6],
            view_dir: [[0.0; 4]; 8],
            corner_q: [0.0; 8],
            screen_center: [0.0; 2],
            screen_verts: [[0.0; 4]; 8],
            base_color: [0.0; 3],
        }
    }
}

#[derive(Clone, Debug)]
pub struct Cubes {
    /// `cubeAnchor` — world positions (`0x27b0f0`).
    pub anchor: [[f32; 4]; INSTANCES],
    /// `cubeRate` (`0x27b190`).
    pub rate: [[f32; 4]; INSTANCES],
    /// `cubeOutB` (`0x27b140`) — live rotation angles.
    pub out_b: [[f32; 4]; INSTANCES],
    /// `0x27b3f0` — per-instance base colour.
    pub base_color: [f32; 3],
    /// Geometry half extent (real `0x2a714c` = 1.8, illegal 1.2).
    pub half_extent: f32,
    /// Last computed instances (debug / frame emission).
    pub instances: Vec<CubeInstance>,
    /// Cull results, exposed for tests (real `cubeCullResult`).
    pub cull_result: [i32; INSTANCES],
    /// Count of front-facing faces per instance (real `cubeFacesDrawn`).
    pub faces_drawn: [i32; INSTANCES],
}

/// `sub_21b690`'s colour clamp: `{x,y,z} + 128`, clamped to 0..127.
fn base_color(x: f32, y: f32, z: f32) -> [f32; 3] {
    [
        (x + 128.0).clamp(0.0, 127.0),
        (y + 128.0).clamp(0.0, 127.0),
        (z + 128.0).clamp(0.0, 127.0),
    ]
}

impl Default for Cubes {
    fn default() -> Self {
        Self::new_opening()
    }
}

impl Cubes {
    /// `InitCubes`: the opening's five cubes, half extent 1.8, base colour
    /// `{112,112,127}` (from `sub_21b690(1.8, -16, -16, 24)`).
    pub fn new_opening() -> Self {
        let mut c = Cubes {
            anchor: [[0.0; 4]; INSTANCES],
            rate: [[0.0; 4]; INSTANCES],
            out_b: [[0.0; 4]; INSTANCES],
            base_color: base_color(-16.0, -16.0, 24.0),
            half_extent: CUBE_HALF_EXTENT,
            instances: (0..INSTANCES).map(CubeInstance::empty).collect(),
            cull_result: [0; INSTANCES],
            faces_drawn: [0; INSTANCES],
        };
        for i in 0..INSTANCES {
            let d = if i == 2 { 0.9 } else { (i as f32 - 2.0) * 0.8 };
            c.anchor[i] = [
                CUBE_SEED_TABLE[i][0] * 3.5,
                CUBE_SEED_TABLE[i][1] * 3.5,
                CUBE_SEED_TABLE[i][2] * -15.0 + 150.0,
                0.0,
            ];
            c.rate[i] = [0.0031 / d, d * 0.0022, d / 1000.0 + 0.0013, 0.0];
            let v = d * (i % 3) as f32 * 3.7 + 0.2856;
            c.out_b[i] = [v, v, v, v];
        }
        c
    }

    /// `sub_219cb8`: the illegal scene's five cubes (half extent 1.2,
    /// base colour `{127,127,127}`, own seed table).
    pub fn new_illegal() -> Self {
        let mut c = Cubes {
            anchor: [[0.0; 4]; INSTANCES],
            rate: [[0.0; 4]; INSTANCES],
            out_b: [[0.0; 4]; INSTANCES],
            base_color: base_color(0.0, 0.0, 0.0),
            half_extent: 1.2,
            instances: (0..INSTANCES).map(CubeInstance::empty).collect(),
            cull_result: [0; INSTANCES],
            faces_drawn: [0; INSTANCES],
        };
        for i in 0..INSTANCES {
            let mut f = (i as f32 - 2.0) * 0.8;
            if f == 0.0 {
                f = 0.9;
            }
            c.anchor[i] = [
                ILLEGAL_CUBE_SEED_TABLE[i][0],
                ILLEGAL_CUBE_SEED_TABLE[i][1],
                (ILLEGAL_CUBE_SEED_TABLE[i][2] - 2.5) * 128.0 + 800.0 - 12.0,
                0.0,
            ];
            let i2 = (i * 2) as i32;
            c.out_b[i] = [
                f * (i2 % 9) as f32 * 0.25,
                f * (i2 & 7) as f32 / 5.0,
                f * (i2 % 7) as f32 / 6.0,
                f * (i2 % 9) as f32 * 0.25,
            ];
            c.rate[i] = [0.004 / f, f * 0.003, f / 800.0 + 0.002, 0.0];
        }
        c
    }

    /// Per-frame setup of every instance: integrate + wrap the angles,
    /// transform, clip, then set up all six faces.
    pub fn update(&mut self, cam: &Camera, width: f32, height: f32, ax: f32, ay: f32, scrz: f32) {
        for i in 0..INSTANCES {
            let inst = self.setup_instance(i, cam, width, height, ax, ay, scrz);
            // Faces that passed the winding test (the reconstruction's
            // cubeFacesDrawn[]).
            self.faces_drawn[i] = (0..6).filter(|&f| inst.face_sign[f] >= 0.0).count() as i32;
            self.instances[i] = inst;
        }
    }

    fn setup_instance(
        &mut self,
        i: usize,
        cam: &Camera,
        width: f32,
        height: f32,
        ax: f32,
        ay: f32,
        scrz: f32,
    ) -> CubeInstance {
        let mut inst = CubeInstance::empty(i);
        for k in 0..3 {
            self.out_b[i][k] += self.rate[i][k];
            if self.out_b[i][k] > PI {
                self.out_b[i][k] -= crate::math::TAU;
            } else if self.out_b[i][k] < -PI {
                self.out_b[i][k] += crate::math::TAU;
            }
        }
        inst.angles = [self.out_b[i][0], self.out_b[i][1], self.out_b[i][2]];
        inst.anchor = self.anchor[i];
        inst.base_color = self.base_color;

        let view = cam.view_matrix();
        let unit = unit_matrix();
        let rotated = rot_matrix(
            &unit,
            &[inst.angles[0], inst.angles[1], inst.angles[2], 0.0],
        );
        let translated = translate(&rotated, &self.anchor[i]);
        let transformed = mul(&view, &translated);

        // World corners.
        let mut world = [[0.0f32; 4]; 8];
        for k in 0..8 {
            let c = self.corner(k);
            world[k] = apply(&translated, &c);
        }

        // Clip test: cull only if EVERY corner is outside the GS window.
        let mut any_inside = false;
        for k in 0..8 {
            let v = apply(&transformed, &self.corner(k));
            let q = if v[3].abs() < 1e-6 { 1e-6 } else { v[3] };
            let sx = ax * scrz * v[0] / q + crate::math::GS_WINDOW_CENTRE;
            let sy = ay * scrz * v[1] / q + crate::math::GS_WINDOW_CENTRE;
            let _sz = v[2] * q;
            inst.screen_verts[k] = [sx, sy, _sz, q];
            inst.corner_q[k] = 1.0 / q;
            let inside = sx > crate::math::GS_WINDOW_CENTRE - width * 0.5
                && sx < crate::math::GS_WINDOW_CENTRE + width * 0.5
                && sy > crate::math::GS_WINDOW_CENTRE - height * 0.5
                && sy < crate::math::GS_WINDOW_CENTRE + height * 0.5
                && q > 0.0;
            if inside {
                any_inside = true;
            }
        }
        self.cull_result[i] = if any_inside { 0 } else { 1 };
        if !any_inside {
            return inst;
        }
        inst.visible = true;

        // Cube centre in GS window-float coords: the real cube_21BF88
        // applies the full view*screen matrix to the local origin and scales
        // by 1/w.
        {
            let origin = [0.0, 0.0, 0.0, 1.0];
            let c4 = apply(&transformed, &origin);
            let q = if c4[3].abs() < 1e-6 {
                1e-6
            } else {
                1.0 / c4[3]
            };
            let cx = c4[0] * q;
            let cy = c4[1] * q;
            inst.screen_center = [
                if cx.is_finite() { cx } else { 0.0 },
                if cy.is_finite() { cy } else { 0.0 },
            ];
        }

        for k in 0..8 {
            inst.corners[k] = world[k];
            let t = apply(&view, &world[k]);
            let n = normalize3([t[0], t[1], t[2]]);
            inst.view_dir[k] = [n[0], n[1], n[2], 0.0];
        }

        for face in 0..6 {
            // World normal: the axis normal rotated by the cube rotation.
            let n = apply(&rotated, &CUBE_FACE_NORMAL[face]);
            let wn = normalize3([n[0], n[1], n[2]]);
            // Inward normal, as in cube_21BC90 (OuterProduct of the face
            // edges points inward), projected through the camera matrix.
            let inv = normalize4(&[-wn[0], -wn[1], -wn[2], 0.0]);
            let sn = apply(&view, &inv);
            let sn = normalize3([sn[0], sn[1], sn[2]]);
            inst.screen_normal[face] = [sn[0], sn[1], sn[2], 0.0];

            // Winding sign from the projected (GS-space) edges.
            let v0 = inst.screen_verts[CUBE_FACE_VERTS[face][0]];
            let v1 = inst.screen_verts[CUBE_FACE_VERTS[face][1]];
            let v2 = inst.screen_verts[CUBE_FACE_VERTS[face][2]];
            let x0 = v0[0] / 16.0;
            let y0 = v0[1] / 16.0;
            let x1 = v1[0] / 16.0;
            let y1 = v1[1] / 16.0;
            let x2 = v2[0] / 16.0;
            let y2 = v2[1] / 16.0;
            let (e1x, e1y) = (x0 - x1, y0 - y1);
            let (e2x, e2y) = (x0 - x2, y0 - y2);
            let l1 = (e1x * e1x + e1y * e1y).sqrt();
            let l2 = (e2x * e2x + e2y * e2y).sqrt();
            inst.face_sign[face] = if l1 > 0.0 && l2 > 0.0 {
                (e1x / l1) * (e2y / l2) - (e1y / l1) * (e2x / l2)
            } else {
                0.0
            };

            // Per-vertex view-angle light.
            for k in 0..4 {
                let vi = CUBE_FACE_VERTS[face][k];
                let dir = [
                    world[vi][0] - cam.position[0],
                    world[vi][1] - cam.position[1],
                    world[vi][2] - cam.position[2],
                ];
                let dir = normalize3(dir);
                let mut l = wn[0] * dir[0] + wn[1] * dir[1] + wn[2] * dir[2];
                if l < 0.0 {
                    l = -l;
                }
                inst.vertex_light[face][k] = l;
            }
        }
        inst
    }

    fn corner(&self, k: usize) -> Vec4 {
        let h = self.half_extent;
        [
            if k & 1 != 0 { h } else { -h },
            if k & 2 != 0 { h } else { -h },
            if k & 4 != 0 { h } else { -h },
            1.0,
        ]
    }

    /// Pass table for the current scene flavour.
    pub fn passes(illegal: bool) -> &'static [CubePass; 10] {
        if illegal {
            &ILLEGAL_CUBE_PASSES
        } else {
            &CUBE_PASSES
        }
    }

    /// Per-pass per-vertex colour, from `CubeVertexColor`
    /// (`0x216f88` / `0x2192c0`). Returns 0..128 floats.
    pub fn vertex_color(
        &self,
        pass: &CubePass,
        inst: &CubeInstance,
        face: usize,
        k: usize,
    ) -> [f32; 3] {
        let l = inst.vertex_light[face][k];
        let e = (1.0 - l) * (1.0 - l) * 0.5;
        let s = pass.blend_fix as f32 / 128.0;
        let (r, g, b) = match pass.color_mode {
            2 => (
                (self.base_color[0] + e * 32.0) * s,
                (self.base_color[1] + e * 32.0) * s,
                (self.base_color[2] + e * 32.0) * s,
            ),
            3 => {
                let t = e * 0.4 * 0.6 + 0.2;
                (
                    self.base_color[0] * t * s,
                    self.base_color[1] * t * s,
                    self.base_color[2] * t * s,
                )
            }
            4 => {
                let t = e * 0.6 + 0.2;
                (
                    self.base_color[0] * t * s,
                    self.base_color[1] * t * s / 3.0,
                    self.base_color[2] * t * s / 3.0,
                )
            }
            _ => (128.0, 128.0, 128.0),
        };
        [
            r.clamp(0.0, 128.0) / 128.0,
            g.clamp(0.0, 128.0) / 128.0,
            b.clamp(0.0, 128.0) / 128.0,
        ]
    }

    /// UVs of one face for one pass (`cube_21B798` / `21BBE0` / `21BA08`).
    pub fn face_uv(
        &self,
        pass: &CubePass,
        inst: &CubeInstance,
        face: usize,
        width: f32,
        height: f32,
    ) -> [[f32; 2]; 4] {
        let mut uv = [[0.0f32; 2]; 4];
        for k in 0..4 {
            let vi = CUBE_FACE_VERTS[face][k];
            uv[k] = match pass.uv_mode {
                0 => {
                    let x = inst.screen_verts[vi][0] / 16.0;
                    let y = inst.screen_verts[vi][1] / 16.0;
                    let q = inst.corner_q[vi];
                    let u = x - (2048.0 - width * 0.5)
                        + inst.screen_normal[face][0] * (width * 0.5) * pass.uv_arg2 * q * 4.0
                        + (x - inst.screen_center[0]) * pass.uv_arg;
                    let v = y - (2048.0 - height * 0.5)
                        + inst.screen_normal[face][1] * (height * 0.5) * pass.uv_arg2 * q * 4.0
                        + (y - inst.screen_center[1]) * pass.uv_arg;
                    [
                        u.clamp(1.0, width - 1.0) / width,
                        v.clamp(1.0, height - 1.0) / height,
                    ]
                }
                1 => {
                    let quad = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
                    [quad[k][0] + pass.uv_arg, quad[k][1] - pass.uv_arg]
                }
                _ => [
                    inst.view_dir[vi][1] + inst.screen_normal[face][1] * pass.uv_arg + 0.5,
                    inst.view_dir[vi][0] + inst.screen_normal[face][0] * pass.uv_arg + 0.5,
                ],
            };
        }
        uv
    }

    /// Texture selector resolution to a draw-list texture id.
    pub fn pass_texture(pass: &CubePass) -> u32 {
        match pass.sel {
            -2 => TEX_SCREEN,
            -3 => TEX_WORK,
            s if s >= 0 => s as u32,
            _ => TEX_WORK,
        }
    }
}

/// Map a GS blend-mode index (the reconstruction's `BlendModes[]` table,
/// `pktSetAlphaBlend(type, mode, fix)`) to the frame format's blend selector.
///
/// PS2 ALPHA register semantics: `((A - B) * C) >> 7 + D` with A/B selecting a
/// colour (`0 = Cs`, `1 = Cd`, `2 = zero`) and C an alpha source
/// (`0 = As`, `1 = Ad`, `2 = FIX`). Hence:
///
/// * mode 0 `{0,2,2,1}` = `Cs*As + Cd`  -> additive (fog)
/// * mode 1 `{2,0,2,1}` = `Cd - Cs*FIX` -> subtractive (black bars)
/// * mode 2 `{0,1,2,1}` = `(Cs-Cd)*FIX + Cd` -> fixed-alpha blend
/// * mode 3 `{1,2,2,0}` = `0`            -> no-op (kept for completeness)
/// * mode 4 `{0,1,0,1}` = `(Cs-Cd)*As + Cd` -> alpha blend
/// * mode 5 `{0,2,0,1}` = `Cs*As + Cd`  -> additive (lights, illegal text)
/// * mode 8 `{0,2,1,1}` = `Cs*Ad + Cd`  -> additive gated by dest alpha
pub fn blend_for_mode(mode: u32) -> u32 {
    use crate::frame::{
        BLEND_ADD, BLEND_ADD_DEST_ALPHA, BLEND_ALPHA, BLEND_FIXALPHA, BLEND_NONE, BLEND_SUB,
    };
    match mode {
        0 => BLEND_ADD,
        1 => BLEND_SUB,
        2 => BLEND_FIXALPHA,
        3 => BLEND_NONE,
        4 => BLEND_ALPHA,
        5 => BLEND_ADD,
        8 => BLEND_ADD_DEST_ALPHA,
        _ => BLEND_ALPHA,
    }
}

/// Exposed for documentation/tests: the rotation matrix used for a cube.
pub fn cube_rotation(inst: &CubeInstance) -> Mat4 {
    rot_matrix(
        &unit_matrix(),
        &[inst.angles[0], inst.angles[1], inst.angles[2], 0.0],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;

    fn cam_at(z: f32) -> Camera {
        let mut c = Camera::new();
        c.init_opening_scene();
        c.position[2] = z;
        c
    }

    #[test]
    fn opening_cubes_have_the_documented_anchors_and_rates() {
        let c = Cubes::new_opening();
        assert_eq!(c.base_color, [112.0, 112.0, 127.0]);
        assert_eq!(c.half_extent, 1.8);
        // i=2 -> d = 0.9
        assert!((c.rate[2][0] - 0.0031 / 0.9).abs() < 1e-9);
        assert!((c.rate[2][2] - (0.9 / 1000.0 + 0.0013)).abs() < 1e-9);
        assert_eq!(c.anchor[0][0], CUBE_SEED_TABLE[0][0] * 3.5);
        assert_eq!(c.anchor[0][2], CUBE_SEED_TABLE[0][2] * -15.0 + 150.0);
    }

    #[test]
    fn illegal_cubes_come_from_their_own_seed_table() {
        let c = Cubes::new_illegal();
        assert_eq!(c.base_color, [127.0, 127.0, 127.0]);
        assert_eq!(c.half_extent, 1.2);
        assert_eq!(c.anchor[0][0], ILLEGAL_CUBE_SEED_TABLE[0][0]);
        assert!(
            (c.anchor[0][2] - ((ILLEGAL_CUBE_SEED_TABLE[0][2] - 2.5) * 128.0 + 800.0 - 12.0)).abs()
                < 1e-3
        );
        // Rate sign flips with d.
        assert!(c.rate[0][0] < 0.0);
    }

    #[test]
    fn angles_wrap_to_pi() {
        let mut c = Cubes::new_opening();
        let cam = cam_at(16.0);
        c.rate[0] = [3.0, 3.0, 3.0, 0.0];
        for _ in 0..100 {
            c.update(&cam, 640.0, 224.0, 1.0, 0.526_271_05, 1024.0);
            for a in c.instances[0].angles {
                assert!((-PI..=PI).contains(&a), "{a}");
            }
        }
    }

    #[test]
    fn cull_reports_hidden_for_cubes_behind_the_camera() {
        let mut c = Cubes::new_opening();
        let mut cam = cam_at(16.0);
        cam.position[2] = 900.0; // far past the field => cubes behind
        cam.fwd = [0.0, 0.0, 1.0, 1.0];
        cam.up = [0.0, 1.0, 0.0, 1.0];
        c.update(&cam, 640.0, 224.0, 1.0, 0.526_271_05, 1024.0);
        let visible = c.instances.iter().filter(|i| i.visible).count();
        assert_eq!(visible, 0, "cubes behind the camera must be culled");
        assert!(c.cull_result.iter().all(|&r| r == 1));
    }

    #[test]
    fn front_faces_are_about_half_of_six() {
        let mut c = Cubes::new_opening();
        // Put the camera in the middle of the field so cubes are close.
        let cam = cam_at(150.0);
        c.update(&cam, 640.0, 224.0, 1.0, 0.526_271_05, 1024.0);
        for inst in &c.instances {
            if inst.visible {
                let front = inst.face_sign.iter().filter(|&&s| s >= 0.0).count();
                assert!(front <= 6);
                assert!(front >= 1, "at least one face must face the camera");
            }
        }
    }

    #[test]
    fn vertex_light_is_in_unit_range() {
        let mut c = Cubes::new_opening();
        let cam = cam_at(100.0);
        c.update(&cam, 640.0, 224.0, 1.0, 0.526_271_05, 1024.0);
        for inst in &c.instances {
            for face in 0..6 {
                for k in 0..4 {
                    let v = inst.vertex_light[face][k];
                    assert!((0.0..=1.0 + 1e-6).contains(&v), "{v}");
                }
            }
        }
    }

    #[test]
    fn corners_match_the_half_extent() {
        let c = Cubes::new_opening();
        for k in 0..8 {
            let p = c.corner(k);
            assert_eq!(p[0].abs(), 1.8);
            assert_eq!(p[1].abs(), 1.8);
            assert_eq!(p[2].abs(), 1.8);
            assert_eq!(p[3], 1.0);
        }
    }

    #[test]
    fn pass_tables_match_the_reconstruction() {
        assert_eq!(CUBE_PASSES[0].blend_fix, 122);
        assert_eq!(CUBE_PASSES[5].blend_fix, 240);
        assert_eq!(CUBE_PASSES[5].uv_arg, -0.084);
        assert_eq!(CUBE_PASSES[7].uv_arg, 0.5);
        assert_eq!(ILLEGAL_CUBE_PASSES[7].uv_arg, 0.0);
        assert_eq!(ILLEGAL_CUBE_PASSES[7].color_mode, 4);
        assert_eq!(ILLEGAL_CUBE_PASSES[0].uv_arg2, 0.8);
        // Group 1 (back faces) is the first five passes.
        assert!(CUBE_PASSES[..5].iter().all(|p| p.group == 1));
        assert!(CUBE_PASSES[5..].iter().all(|p| p.group == 0));
    }

    #[test]
    fn colour_modes_follow_the_documented_formulas() {
        let c = Cubes::new_opening();
        let mut inst = CubeInstance::empty(0);
        inst.base_color = [112.0, 112.0, 127.0];
        inst.vertex_light = [[0.0; 4]; 6];
        // e = (1-0)^2*0.5 = 0.5 for light 0.
        let p = CUBE_PASSES[0]; // color_mode 2, fix 122
        let col = c.vertex_color(&p, &inst, 0, 0);
        let want: f32 = ((112.0f32 + 0.5 * 32.0) * 122.0 / 128.0).clamp(0.0, 128.0) / 128.0;
        assert!((col[0] - want).abs() < 1e-6, "{} vs {}", col[0], want);

        // Mode 3 with light = 1 => e = 0 => t = 0.2.
        inst.vertex_light = [[1.0; 4]; 6];
        let p3 = CUBE_PASSES[2]; // color_mode 3, fix 42
        let c3 = c.vertex_color(&p3, &inst, 0, 0);
        let want3: f32 = (112.0f32 * 0.2 * 42.0 / 128.0).clamp(0.0, 128.0) / 128.0;
        assert!((c3[0] - want3).abs() < 1e-6);

        // Mode 4 is the red-tinted pass: the base colour is {112,112,127} and
        // green/blue are divided by three, so red dominates while the g:b
        // ratio of the base survives the division.
        let mut inst4 = CubeInstance::empty(0);
        inst4.vertex_light = [[0.0; 4]; 6];
        let p4 = ILLEGAL_CUBE_PASSES[2];
        assert_eq!(p4.color_mode, 4);
        let c4 = c.vertex_color(&p4, &inst4, 0, 0);
        assert!(c4[0] > c4[2], "{c4:?}");
        let ratio = c4[1] / c4[2];
        assert!((ratio - 112.0 / 127.0).abs() < 1e-6, "{c4:?}");
    }

    #[test]
    fn pass_texture_selectors_resolve() {
        assert_eq!(Cubes::pass_texture(&CUBE_PASSES[0]), TEX_SCREEN);
        assert_eq!(Cubes::pass_texture(&CUBE_PASSES[1]), TEX_BLPR);
        assert_eq!(Cubes::pass_texture(&CUBE_PASSES[5]), TEX_WORK);
        assert_eq!(blend_for_mode(8), crate::frame::BLEND_ADD_DEST_ALPHA);
        assert_eq!(blend_for_mode(0), crate::frame::BLEND_ADD);
        assert_eq!(blend_for_mode(1), crate::frame::BLEND_SUB);
        assert_eq!(blend_for_mode(4), crate::frame::BLEND_ALPHA);
        assert_eq!(blend_for_mode(3), crate::frame::BLEND_NONE);
    }

    #[test]
    fn uv_modes_are_bounded() {
        let mut c = Cubes::new_opening();
        let cam = cam_at(150.0);
        c.update(&cam, 640.0, 224.0, 1.0, 0.526_271_05, 1024.0);
        let inst = &c.instances[0];
        for pass in CUBE_PASSES.iter() {
            let uv = c.face_uv(pass, inst, 0, 640.0, 224.0);
            for p in uv {
                assert!(p[0].is_finite() && p[1].is_finite());
                if pass.uv_mode == 1 {
                    assert!((-1.0..=1.5).contains(&p[0]));
                    assert!((-1.0..=1.5).contains(&p[1]));
                }
            }
        }
    }

    #[test]
    fn update_is_independent_per_instance() {
        let mut a = Cubes::new_opening();
        let cam = cam_at(150.0);
        a.update(&cam, 640.0, 224.0, 1.0, 0.526_271_05, 1024.0);
        let first: Vec<[f32; 3]> = a.instances.iter().map(|i| i.angles).collect();
        assert_eq!(a.instances.len(), INSTANCES);
        // Distinct start angles by construction.
        for i in 1..INSTANCES {
            assert_ne!(first[i], first[0]);
        }
    }
}
