//! Deterministic simulation of OSDSYS Mode A (Opening), Mode B (Idle), and Mode C (Illegal Disc).
//!
//! Faithfully implements `ProcessOpeningAnimation` (`0x215fd0`), `InitFog` / `DrawFog` (`0x214f78` /
//! `0x215238`), `DrawLights` (`0x216b28`), `DrawCube` / `DrawIllegalCubes` (`0x217520` / `0x219ea8`),
//! `DrawRedFlare` (`0x21b4c8`), `DrawIllegalFog` (`0x215a20`), `sub_218b20` motion blur,
//! `DoSCEText` / `DoIllegalText`, and `sub_218bd0` / `DrawIllegalFades`.

use core::f32::consts::PI;

use crate::math::{
    clamp_f32, gs_all_behind_or_outside_guard, gs_projected_to_webgpu_clip, gs_transform_vertex,
    mat_apply, mat_camera, mat_mul, mat_rot_xyz, mat_trans, mat_unit, mat_view_screen,
    osd_cos_14bit, osd_sin_14bit, pixel_to_ndc, vec_dot3, vec_normalize, wrap_pi, Mat4, OsdRng,
    Vec4,
};
use crate::towers::{PackedTowerVertex, TowerField, TowerFieldPreset};

pub const OPENING_STATE_LEVELS: [i32; 8] = [16, 56, 104, 320, 672, 800, 1160, 1160];
pub const CUBE_INSTANCES: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoStandard {
    /// NTSC 60 Hz (`640x224` field buffer, `timestep = 1.0`, `screenAY = 0.457627`).
    Ntsc,
    /// PAL 50 Hz (`640x256` field buffer, `timestep = 1.2`, `screenAY = 0.52627105`).
    Pal,
}

impl VideoStandard {
    #[inline]
    pub const fn timestep(self) -> f32 {
        match self {
            Self::Ntsc => 1.0,
            Self::Pal => 1.2,
        }
    }

    #[inline]
    pub const fn fps(self) -> u32 {
        match self {
            Self::Ntsc => 60,
            Self::Pal => 50,
        }
    }

    #[inline]
    pub const fn width(self) -> u32 {
        640
    }

    #[inline]
    pub const fn height(self) -> u32 {
        match self {
            Self::Ntsc => 224,
            Self::Pal => 256,
        }
    }

    #[inline]
    pub const fn screen_dims(self) -> (f32, f32) {
        match self {
            Self::Ntsc => (640.0, 224.0),
            Self::Pal => (640.0, 256.0),
        }
    }

    #[inline]
    pub const fn opening_screen_ay(self) -> f32 {
        match self {
            Self::Ntsc => 0.457_627,
            Self::Pal => 0.526_271_05,
        }
    }

    #[inline]
    pub const fn menu_screen_ay(self) -> f32 {
        match self {
            Self::Ntsc => 0.47,
            Self::Pal => 0.5405,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpeningVariant {
    /// Mode A default: no disc inserted (`osdBootParam = 106`), drifts 2s then flies up into Menu.
    BootToMenu,
    /// Mode A disc variant: game disc present (`osdBootParam = 108`), gentler fly-up curve.
    BootToGame,
    /// Mode B: indefinite hover over the tower field (`positionSpeed.z = 0`).
    Idle,
    /// Mode C: red illegal-disc scene (`openingType = 1`, `osdBootParam = 101`).
    IllegalDisc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OsdLanguage {
    Japanese = 0,
    English = 1,
    French = 2,
    Spanish = 3,
    German = 4,
    Italian = 5,
    Dutch = 6,
    Portuguese = 7,
}

impl OsdLanguage {
    pub const fn from_u8(v: u8) -> Self {
        match v % 8 {
            0 => Self::Japanese,
            1 => Self::English,
            2 => Self::French,
            3 => Self::Spanish,
            4 => Self::German,
            5 => Self::Italian,
            6 => Self::Dutch,
            _ => Self::Portuguese,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Japanese => "Japanese",
            Self::English => "English",
            Self::French => "French",
            Self::Spanish => "Spanish",
            Self::German => "German",
            Self::Italian => "Italian",
            Self::Dutch => "Dutch",
            Self::Portuguese => "Portuguese",
        }
    }

    /// Clean-room localized error prompt matching the 8 OSDSYS language slots without Sony trademarks.
    pub const fn illegal_disc_message(self) -> &'static str {
        match self {
            Self::Japanese => "DISC FORMAT ERROR - PLEASE INSERT A SUPPORTED DISC",
            Self::English => "Please insert a supported format disc.",
            Self::French => "Veuillez inserer un disque au format pris en charge.",
            Self::Spanish => "Por favor, inserte un disco de formato compatible.",
            Self::German => "Bitte legen Sie eine Disc im unterstutzten Format ein.",
            Self::Italian => "Inserire un disco di formato compatibile.",
            Self::Dutch => "Plaats een schijf van een ondersteund formaat.",
            Self::Portuguese => "Por favor, insira um disco de formato compativel.",
        }
    }
}

/// Logical texture identifiers shared between Rust render preparation and WebGPU texture table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum TextureId {
    Banner = 0,
    Fog0 = 1,
    Fog1 = 2,
    Fog2 = 3,
    Fog3 = 4,
    Fog4 = 5,
    Wal0 = 6,
    Crle = 7,
    Crbl = 8,
    Flar = 9,
    Ref = 10,
    Blp = 11,
    Blpr = 12,
    IllegalMsg = 13,
    MenuKabe = 14,
    MenuBlur = 15,
    MenuNavi = 16,
    MenuBump = 17,
    MenuBinv = 18,
    MenuFlow = 19,
    MenuRefa = 20,
    MenuMaru = 21,
    GlyphAtlas = 22,
    /// Dynamic current-frame capture (`CubeCaptureBuffer`, `sel == -2`).
    CapturedScreen = 100,
    /// Dynamic offscreen working buffer (`extraBuf2`, `sel == -3`).
    CubeWorkBuf = 101,
    /// Dynamic menu `wb3` buffer.
    MenuWorkBuf3 = 102,
    /// Dynamic menu `wb4` buffer.
    MenuWorkBuf4 = 103,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum CubeRenderTarget {
    Screen = 0,
    WorkBuf = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AudioCue {
    StartOpeningChime = 1,
    StartIllegalWarningDrone = 2,
    OpeningFlyUpWhoosh = 3,
    FadeOutBgm = 4,
    MenuCursorMove = 5,
    MenuConfirm = 6,
    MenuCancel = 7,
    StartMenuAmbience = 8,
    MenuValueStep = 9,
}

/// Textured quad/triangle vertex (`12` floats = `48` bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PackedTexturedVertex {
    pub clip_pos: [f32; 4],
    pub uv_q: [f32; 4],
    pub gs_rgba: [f32; 4],
}

/// Untextured Gouraud / line vertex (`8` floats = `32` bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PackedColorVertex {
    pub clip_pos: [f32; 4],
    pub gs_rgba: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct TexturedBatch {
    pub texture: TextureId,
    /// Index into PS2 `BlendModes[0..=10]` (`opening.c:368`), or `255` for opaque (`abe = 0`).
    pub blend_mode: u8,
    pub blend_fix: u8,
    pub depth_test: bool,
    pub depth_write: bool,
    pub uv_clamp: bool,
    pub vertices: Vec<PackedTexturedVertex>,
}

#[derive(Clone, Debug)]
pub struct CubePassDraw {
    pub target: CubeRenderTarget,
    pub source: TextureId,
    pub abe: bool,
    pub blend_mode: u8,
    pub blend_fix: u8,
    pub start_vertex: u32,
    pub vertex_count: u32,
}

#[derive(Clone, Debug)]
pub struct CubeInstanceDraw {
    pub instance_index: u8,
    pub passes: Vec<CubePassDraw>,
    pub vertices: Vec<PackedTexturedVertex>,
}

#[derive(Clone, Debug, Default)]
pub struct OpeningRenderList {
    pub tower_vertices: Vec<PackedTowerVertex>,
    pub drawn_tower_count: u32,
    pub extra_buf_feedback_fix: u8,
    pub fog_batches: Vec<TexturedBatch>,
    pub light_sprite_batch: Option<TexturedBatch>,
    pub light_trail_vertices: Vec<PackedColorVertex>,
    pub flare_fan_vertices: Vec<PackedColorVertex>,
    pub flare_sprite_batch: Option<TexturedBatch>,
    pub cubes: Vec<CubeInstanceDraw>,
    pub motion_blur_passes: u8,
    pub banner_alpha: u8,
    pub illegal_text_alpha: u8,
    pub black_fade_alpha: u8,
    pub letterbox_bar_ratio: f32,
}

impl OpeningRenderList {
    pub fn clear(&mut self) {
        self.tower_vertices.clear();
        self.drawn_tower_count = 0;
        self.extra_buf_feedback_fix = 0;
        self.fog_batches.clear();
        self.light_sprite_batch = None;
        self.light_trail_vertices.clear();
        self.flare_fan_vertices.clear();
        self.flare_sprite_batch = None;
        self.cubes.clear();
        self.motion_blur_passes = 0;
        self.banner_alpha = 0;
        self.illegal_text_alpha = 0;
        self.black_fade_alpha = 0;
        self.letterbox_bar_ratio = 0.0;
    }
}

#[derive(Clone, Copy)]
struct CubePassSpec {
    target: CubeRenderTarget,
    source: TextureId,
    abe: bool,
    blend_mode: u8,
    blend_fix: u8,
    st_screen_space: bool,
    color_mode: u8,
    uv_mode: u8,
    uv_arg: f32,
    uv_arg2: f32,
    back_faces: bool,
}

const OPENING_CUBE_PASSES: [CubePassSpec; 10] = [
    CubePassSpec {
        target: CubeRenderTarget::WorkBuf,
        source: TextureId::CapturedScreen,
        abe: false,
        blend_mode: 4,
        blend_fix: 122,
        st_screen_space: true,
        color_mode: 2,
        uv_mode: 0,
        uv_arg: 0.0,
        uv_arg2: 1.0,
        back_faces: true,
    },
    CubePassSpec {
        target: CubeRenderTarget::WorkBuf,
        source: TextureId::Blpr,
        abe: true,
        blend_mode: 5,
        blend_fix: 128,
        st_screen_space: false,
        color_mode: 0,
        uv_mode: 1,
        uv_arg: -0.003_75,
        uv_arg2: 0.0,
        back_faces: true,
    },
    CubePassSpec {
        target: CubeRenderTarget::WorkBuf,
        source: TextureId::Ref,
        abe: true,
        blend_mode: 8,
        blend_fix: 42,
        st_screen_space: false,
        color_mode: 3,
        uv_mode: 2,
        uv_arg: -0.25,
        uv_arg2: 0.0,
        back_faces: true,
    },
    CubePassSpec {
        target: CubeRenderTarget::WorkBuf,
        source: TextureId::Blp,
        abe: true,
        blend_mode: 5,
        blend_fix: 128,
        st_screen_space: false,
        color_mode: 0,
        uv_mode: 1,
        uv_arg: 0.003_75,
        uv_arg2: 0.0,
        back_faces: true,
    },
    CubePassSpec {
        target: CubeRenderTarget::WorkBuf,
        source: TextureId::Ref,
        abe: true,
        blend_mode: 8,
        blend_fix: 42,
        st_screen_space: false,
        color_mode: 3,
        uv_mode: 2,
        uv_arg: -0.25,
        uv_arg2: 0.0,
        back_faces: true,
    },
    CubePassSpec {
        target: CubeRenderTarget::Screen,
        source: TextureId::CubeWorkBuf,
        abe: false,
        blend_mode: 4,
        blend_fix: 240,
        st_screen_space: true,
        color_mode: 2,
        uv_mode: 0,
        uv_arg: -0.084,
        uv_arg2: 1.0,
        back_faces: false,
    },
    CubePassSpec {
        target: CubeRenderTarget::Screen,
        source: TextureId::Blpr,
        abe: true,
        blend_mode: 5,
        blend_fix: 128,
        st_screen_space: false,
        color_mode: 0,
        uv_mode: 1,
        uv_arg: -0.0075,
        uv_arg2: 0.0,
        back_faces: false,
    },
    CubePassSpec {
        target: CubeRenderTarget::Screen,
        source: TextureId::Ref,
        abe: true,
        blend_mode: 8,
        blend_fix: 64,
        st_screen_space: false,
        color_mode: 3,
        uv_mode: 2,
        uv_arg: 0.5,
        uv_arg2: 0.0,
        back_faces: false,
    },
    CubePassSpec {
        target: CubeRenderTarget::Screen,
        source: TextureId::Blp,
        abe: true,
        blend_mode: 5,
        blend_fix: 128,
        st_screen_space: false,
        color_mode: 0,
        uv_mode: 1,
        uv_arg: 0.0075,
        uv_arg2: 0.0,
        back_faces: false,
    },
    CubePassSpec {
        target: CubeRenderTarget::Screen,
        source: TextureId::Ref,
        abe: true,
        blend_mode: 8,
        blend_fix: 64,
        st_screen_space: false,
        color_mode: 3,
        uv_mode: 2,
        uv_arg: 0.5,
        uv_arg2: 0.0,
        back_faces: false,
    },
];

const ILLEGAL_CUBE_PASSES: [CubePassSpec; 10] = [
    CubePassSpec {
        target: CubeRenderTarget::WorkBuf,
        source: TextureId::CapturedScreen,
        abe: false,
        blend_mode: 4,
        blend_fix: 122,
        st_screen_space: true,
        color_mode: 2,
        uv_mode: 0,
        uv_arg: 0.0,
        uv_arg2: 0.8,
        back_faces: true,
    },
    CubePassSpec {
        target: CubeRenderTarget::WorkBuf,
        source: TextureId::Blpr,
        abe: true,
        blend_mode: 5,
        blend_fix: 128,
        st_screen_space: false,
        color_mode: 0,
        uv_mode: 1,
        uv_arg: -0.003_75,
        uv_arg2: 0.0,
        back_faces: true,
    },
    CubePassSpec {
        target: CubeRenderTarget::WorkBuf,
        source: TextureId::Ref,
        abe: true,
        blend_mode: 8,
        blend_fix: 42,
        st_screen_space: false,
        color_mode: 4,
        uv_mode: 2,
        uv_arg: 0.0,
        uv_arg2: 0.0,
        back_faces: true,
    },
    CubePassSpec {
        target: CubeRenderTarget::WorkBuf,
        source: TextureId::Blp,
        abe: true,
        blend_mode: 5,
        blend_fix: 128,
        st_screen_space: false,
        color_mode: 0,
        uv_mode: 1,
        uv_arg: 0.003_75,
        uv_arg2: 0.0,
        back_faces: true,
    },
    CubePassSpec {
        target: CubeRenderTarget::WorkBuf,
        source: TextureId::Ref,
        abe: true,
        blend_mode: 8,
        blend_fix: 42,
        st_screen_space: false,
        color_mode: 4,
        uv_mode: 2,
        uv_arg: 0.0,
        uv_arg2: 0.0,
        back_faces: true,
    },
    CubePassSpec {
        target: CubeRenderTarget::Screen,
        source: TextureId::CubeWorkBuf,
        abe: false,
        blend_mode: 4,
        blend_fix: 240,
        st_screen_space: true,
        color_mode: 2,
        uv_mode: 0,
        uv_arg: -0.084,
        uv_arg2: 0.8,
        back_faces: false,
    },
    CubePassSpec {
        target: CubeRenderTarget::Screen,
        source: TextureId::Blpr,
        abe: true,
        blend_mode: 5,
        blend_fix: 128,
        st_screen_space: false,
        color_mode: 0,
        uv_mode: 1,
        uv_arg: -0.0075,
        uv_arg2: 0.0,
        back_faces: false,
    },
    CubePassSpec {
        target: CubeRenderTarget::Screen,
        source: TextureId::Ref,
        abe: true,
        blend_mode: 8,
        blend_fix: 64,
        st_screen_space: false,
        color_mode: 4,
        uv_mode: 2,
        uv_arg: 0.0,
        uv_arg2: 0.0,
        back_faces: false,
    },
    CubePassSpec {
        target: CubeRenderTarget::Screen,
        source: TextureId::Blp,
        abe: true,
        blend_mode: 5,
        blend_fix: 128,
        st_screen_space: false,
        color_mode: 0,
        uv_mode: 1,
        uv_arg: 0.0075,
        uv_arg2: 0.0,
        back_faces: false,
    },
    CubePassSpec {
        target: CubeRenderTarget::Screen,
        source: TextureId::Ref,
        abe: true,
        blend_mode: 8,
        blend_fix: 64,
        st_screen_space: false,
        color_mode: 4,
        uv_mode: 2,
        uv_arg: 0.0,
        uv_arg2: 0.0,
        back_faces: false,
    },
];

const CUBE_SEED_TABLE: [Vec4; CUBE_INSTANCES] = [
    [3.5679, 0.5447, 2.5932, 0.0],
    [-0.9042, -1.1173, 3.7952, 0.0],
    [3.2639, -2.6491, 4.1075, 0.0],
    [-3.7296, -2.3677, 4.3654, 0.0],
    [-3.1017, 2.2409, 4.5429, 0.0],
];

const ILLEGAL_CUBE_SEED_TABLE: [Vec4; CUBE_INSTANCES] = [
    [-10.4068, 4.1636, 5.0429, 0.0],
    [-12.9184, -4.2708, 4.3654, 0.0],
    [2.7639, 0.1509, 4.1075, 0.0],
    [4.0958, -1.3173, 3.0952, 0.0],
    [-2.4321, 0.5447, 2.7732, 0.0],
];

const CUBE_FACE_VERTS: [[usize; 4]; 6] = [
    [0, 1, 2, 3],
    [5, 4, 7, 6],
    [4, 0, 6, 2],
    [2, 3, 6, 7],
    [1, 5, 3, 7],
    [4, 5, 0, 1],
];

const CUBE_FACE_NORMALS: [Vec4; 6] = [
    [0.0, 0.0, -1.0, 1.0],
    [0.0, 0.0, 1.0, 1.0],
    [-1.0, 0.0, 0.0, 1.0],
    [0.0, 1.0, 0.0, 1.0],
    [1.0, 0.0, 0.0, 1.0],
    [0.0, -1.0, 0.0, 1.0],
];

const CUBE_UV_UNIT: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];

const LIGHT_COLORS: [[f32; 3]; 4] = [
    [32.0, 128.0, 0.0],
    [128.0, 32.0, 64.0],
    [128.0, 0.0, 0.0],
    [64.0, 32.0, 128.0],
];

const LIGHT_SPRITE_VERTS: [[Vec4; 4]; 2] = [
    [
        [-0.8, -0.8, 0.0, 1.0],
        [0.8, -0.8, 0.0, 1.0],
        [-0.8, 0.8, 0.0, 1.0],
        [0.8, 0.8, 0.0, 1.0],
    ],
    [
        [-0.25, -0.25, 0.0, 1.0],
        [0.25, -0.25, 0.0, 1.0],
        [-0.25, 0.25, 0.0, 1.0],
        [0.25, 0.25, 0.0, 1.0],
    ],
];

const FOG_STRIPS: [[usize; 4]; 4] = [[0, 1, 3, 4], [1, 2, 4, 5], [3, 4, 6, 7], [4, 5, 7, 8]];
const FOG_UV_3X3: [[f32; 2]; 9] = [
    [0.0, 0.0],
    [0.5, 0.0],
    [1.0, 0.0],
    [0.0, 0.5],
    [0.5, 0.5],
    [1.0, 0.5],
    [0.0, 1.0],
    [0.5, 1.0],
    [1.0, 1.0],
];

#[derive(Clone, Debug)]
pub struct OpeningSim {
    pub variant: OpeningVariant,
    pub standard: VideoStandard,
    pub language: OsdLanguage,
    pub frame_count: u32,
    pub opening_state: usize,
    pub opening_type: u8,
    pub scene_finished: bool,

    pub position: Vec4,
    pub fwd_dir: Vec4,
    pub up_dir: Vec4,
    pub rotation: f32,
    pub position_speed: Vec4,
    pub position_accel1: Vec4,
    pub position_accel2: Vec4,
    pub rotation_speed: Vec4,
    pub rotation_accel: Vec4,

    opening_go: bool,
    boot_request: i32,
    osd_boot_param: i32,
    has_disc: bool,
    disc_type: i32,
    opening_end_flag: bool,
    opening_end_frame: u32,
    illegal_fade_counter: i32,

    pub camera_matrix: Mat4,
    pub view_screen_matrix: Mat4,
    pub camera_screen_matrix: Mat4,

    pub tower_field: TowerField,

    // Fog state (Opening 17x17)
    fog_animation: [f32; 6],
    fog_vertices: Box<[[Vec4; 17]; 17]>,
    fog_blue: Box<[[i32; 17]; 17]>,

    // Lights & 128-point trails
    rng: OsdRng,
    lights_seed: i32,
    light_matrix_history: Box<[[Mat4; 4]; 4]>,
    light_trail_pos: Box<[[Vec4; 128]; 4]>,
    light_trail_start: usize,
    light_trail_end: usize,

    // Cubes (Opening & Illegal)
    cube_corners: [Vec4; 8],
    cube_base_color: [f32; 3],
    cube_anchor: [Vec4; CUBE_INSTANCES],
    cube_rate: [Vec4; CUBE_INSTANCES],
    cube_angles: [Vec4; CUBE_INSTANCES],

    // Illegal-disc flare & red fog
    flare_ring_small: Box<[[Vec4; 17]; 7]>,
    flare_ring_large: Box<[[Vec4; 17]; 7]>,
    flare_rot: [Vec4; 7],
    flare_pos: [Vec4; 7],
    red_flare_intensity: f32,
    illegal_scene_warm: bool,

    illegal_fog_verts: [Vec4; 9],
    illegal_fog_colors: [[i32; 4]; 9],
    illegal_fog_pos: Box<[Vec4; 128]>,
    illegal_fog_rot: Box<[Vec4; 128]>,

    // Text overlay state
    sce_text_state: i32,
    sce_text_alpha: i32,
    sce_text_step: i32,
    illegal_text_state: i32,
    illegal_text_alpha: i32,

    // Customization knobs (Mode E)
    pub speed_multiplier: f32,
    pub camera_xy_offset: [f32; 2],
    pub reduced_motion: bool,
    pub enable_fog: bool,
    pub enable_lights: bool,
    pub enable_cubes: bool,
    pub enable_glow: bool,
    pub enable_motion_blur: bool,
    pub letterbox: bool,
}

impl OpeningSim {
    pub fn new(
        variant: OpeningVariant,
        standard: VideoStandard,
        language: OsdLanguage,
        tower_preset: TowerFieldPreset,
        rng_seed: u32,
    ) -> Self {
        let mut rng = OsdRng::new(if rng_seed == 0 { 1 } else { rng_seed });
        let tower_field = TowerField::new(tower_preset);

        // 1. InitFog (matching/src/fog.c & opening.c:1032)
        let mut fog_vertices = Box::new([[Vec4::default(); 17]; 17]);
        let mut fog_blue = Box::new([[0_i32; 17]; 17]);
        let radius_sq = 5202.0_f32.sqrt();
        let angle_0 = 0_i32;
        let c0 = osd_cos_14bit(angle_0);
        let s0 = osd_sin_14bit(angle_0);
        for y in 0..17 {
            for x in 0..17 {
                fog_vertices[x][y] = [
                    (x as f32) * 6.0 - 48.0 + (c0 + c0),
                    (y as f32) * 6.0 - 48.0 + (s0 + s0),
                    134.0,
                    1.0,
                ];
                let dx = -5.099_999_4 - (((2 * x as i32 - 16) as f32) * 3.0 + 3.0);
                let dy = -(((2 * y as i32 - 16) as f32) * 3.0 + 3.0);
                let d = (dx * dx + dy * dy).sqrt();
                let col_d = clamp_f32((radius_sq - d * 4.0) * 96.0 / radius_sq, 0.0, 127.0);
                fog_blue[x][y] = col_d as i32;
            }
        }

        // 2. sub_219f08: sub_21a438 (flare rings) + sub_215798 (385 rand draws!) + sub_219cb8 (illegal cubes)
        let mut flare_ring_small = Box::new([[Vec4::default(); 17]; 7]);
        let mut flare_ring_large = Box::new([[Vec4::default(); 17]; 7]);
        let mut flare_rot = [[0.0_f32; 4]; 7];
        let mut flare_pos = [[0.0_f32; 4]; 7];
        for i in 0..7 {
            flare_ring_small[i][0] = [0.0, 0.0, 0.0, 1.0];
            flare_ring_large[i][0] = [0.0, 0.0, 0.0, 1.0];
            let r_large = (i as f32) * 8.0 + 28.0;
            for k in 0..16 {
                let a = (k as f32) * PI * 2.0 * 0.0625;
                let (sa, ca) = a.sin_cos();
                flare_ring_small[i][1 + k] = [ca * 8.0, sa * 8.0, 0.0, 1.0];
                flare_ring_large[i][1 + k] = [ca * r_large, sa * r_large, 0.0, 1.0];
            }
            flare_pos[i] = [0.0, 0.0, 1160.0, 1.0];
            flare_rot[i] = [0.0, 0.0, (i as f32) * 0.925 * PI * 2.0 / 7.0, 0.0];
        }

        let mut illegal_fog_verts = [[0.0_f32; 4]; 9];
        let mut illegal_fog_colors = [[0_i32; 4]; 9];
        for j in 0..3 {
            for i in 0..3 {
                illegal_fog_verts[j * 3 + i] =
                    [(i as f32 - 1.0) * 15.0, (j as f32 - 1.0) * 15.0, 0.0, 1.0];
            }
        }
        for j in 0..3 {
            for i in 0..3 {
                let c = if j == 1 && i == 1 {
                    (rng.next_i31() % 64) + 64
                } else {
                    0
                };
                let gb = (c * 96) / 128;
                illegal_fog_colors[j * 3 + i] = [c, gb, gb, 128];
            }
        }
        let mut illegal_fog_pos = Box::new([[0.0_f32; 4]; 128]);
        let mut illegal_fog_rot = Box::new([[0.0_f32; 4]; 128]);
        for i in 0..128 {
            illegal_fog_pos[i][0] = ((rng.next_i31() % 4800) - 2400) as f32 * 0.01;
            illegal_fog_pos[i][1] = ((rng.next_i31() % 4800) - 2400) as f32 * 0.01;
            illegal_fog_pos[i][3] = 0.0;
            illegal_fog_rot[i] = [0.0, 0.0, 0.0, 1.0];
            illegal_fog_pos[i][2] = ((rng.next_i31() % 805) + 477) as f32;
        }

        // Default to illegal cubes first (just like sub_219cb8 at init); Opening overwrites in InitOpeningScene.
        let mut cube_corners = [[0.0_f32; 4]; 8];
        let mut cube_base_color = [127.0_f32; 3];
        let mut cube_anchor = [[0.0_f32; 4]; CUBE_INSTANCES];
        let mut cube_rate = [[0.0_f32; 4]; CUBE_INSTANCES];
        let mut cube_angles = [[0.0_f32; 4]; CUBE_INSTANCES];

        Self::setup_cube_geometry(1.2, 0.0, 0.0, 0.0, &mut cube_corners, &mut cube_base_color);
        for i in 0..CUBE_INSTANCES {
            let mut f = (i as i32 - 2) as f32 * 0.8;
            if f == 0.0 {
                f = 0.9;
            }
            cube_anchor[i] = [
                ILLEGAL_CUBE_SEED_TABLE[i][0],
                ILLEGAL_CUBE_SEED_TABLE[i][1],
                (ILLEGAL_CUBE_SEED_TABLE[i][2] - 2.5) * 128.0 + 800.0 - 12.0,
                0.0,
            ];
            cube_angles[i] = [
                f * (((i * 2) % 9) as f32) * 0.25,
                f * (((i * 2) & 7) as f32) / 5.0,
                f * (((i * 2) % 7) as f32) / 6.0,
                f * (((i * 2) % 9) as f32) * 0.25,
            ];
            cube_rate[i] = [0.004 / f, f * 0.003, f / 800.0 + 0.002, 0.0];
        }

        let is_illegal = variant == OpeningVariant::IllegalDisc;
        let opening_type = u8::from(is_illegal);

        let (
            position,
            fwd_dir,
            rotation,
            opening_state,
            position_speed,
            position_accel2,
            rotation_speed,
            osd_boot_param,
            has_disc,
            disc_type,
        ): (Vec4, Vec4, f32, usize, Vec4, Vec4, Vec4, i32, bool, i32) = match variant {
            OpeningVariant::IllegalDisc => (
                [0.0, 0.0, 672.0, 0.0],
                [0.0, -0.03, 1.0, 1.0],
                0.0,
                4_usize,
                [0.0, 0.0, f32::from_bits(0x400A_3D68), 0.0], // exact 2.1599939f (.lit4 0x2a70c8)
                [0.0, 0.0, -0.0178, 0.0],
                [0.0, 0.0, 0.00462, 0.0],
                101,
                false,
                0,
            ),
            OpeningVariant::Idle => (
                [0.0, 0.0, 16.0, 0.0],
                [0.0, 0.0, 1.0, 1.0],
                -0.12,
                0_usize,
                [0.0, 0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0, 0.0],
                [0.0, 0.0, 0.001, 0.0],
                106,
                false,
                0,
            ),
            OpeningVariant::BootToGame => (
                [0.0, 0.0, 16.0, 0.0],
                [0.0, 0.0, 1.0, 1.0],
                -0.12,
                0_usize,
                [0.0, 0.0, 0.04, 0.0],
                [0.0, 0.0, 0.0, 0.0],
                [0.0, 0.0, 0.001, 0.0],
                108,
                true,
                2,
            ),
            OpeningVariant::BootToMenu => (
                [0.0, 0.0, 16.0, 0.0],
                [0.0, 0.0, 1.0, 1.0],
                -0.12,
                0_usize,
                [0.0, 0.0, 0.04, 0.0],
                [0.0, 0.0, 0.0, 0.0],
                [0.0, 0.0, 0.001, 0.0],
                106,
                false,
                0,
            ),
        };

        let mut lights_seed = 4002;
        if !is_illegal {
            // InitOpeningScene -> InitLightsCubes overwrites cubes with Opening params & consumes draw #386!
            Self::setup_cube_geometry(
                1.8,
                -16.0,
                -16.0,
                24.0,
                &mut cube_corners,
                &mut cube_base_color,
            );
            for i in 0..CUBE_INSTANCES {
                let d = if i == 2 {
                    0.9
                } else {
                    (i as i32 - 2) as f32 * 0.8
                };
                cube_anchor[i] = [
                    CUBE_SEED_TABLE[i][0] * 3.5,
                    CUBE_SEED_TABLE[i][1] * 3.5,
                    CUBE_SEED_TABLE[i][2] * -15.0 + 150.0,
                    0.0,
                ];
                cube_rate[i] = [0.0031 / d, d * 0.0022, d / 1000.0 + 0.0013, 0.0];
                let v = d * ((i % 3) as f32) * 3.7 + 0.2856;
                cube_angles[i] = [v, v, v, v];
            }
            lights_seed = (rng.next_i31() % 2345) + 3456;
        }

        let light_matrix_history = Box::new([[mat_unit(); 4]; 4]);
        let light_trail_pos = Box::new([[[0.0_f32; 4]; 128]; 4]);

        let mut sim = Self {
            variant,
            standard,
            language,
            frame_count: 0,
            opening_state,
            opening_type,
            scene_finished: false,
            position,
            fwd_dir,
            up_dir: [rotation.sin(), rotation.cos(), 0.0, 1.0],
            rotation,
            position_speed,
            position_accel1: [0.0; 4],
            position_accel2,
            rotation_speed,
            rotation_accel: [0.0; 4],
            opening_go: false,
            boot_request: 1,
            osd_boot_param,
            has_disc,
            disc_type,
            opening_end_flag: false,
            opening_end_frame: 0,
            illegal_fade_counter: 0,
            camera_matrix: mat_unit(),
            view_screen_matrix: mat_unit(),
            camera_screen_matrix: mat_unit(),
            tower_field,
            fog_animation: [0.0; 6],
            fog_vertices,
            fog_blue,
            rng,
            lights_seed,
            light_matrix_history,
            light_trail_pos,
            light_trail_start: 0,
            light_trail_end: 0,
            cube_corners,
            cube_base_color,
            cube_anchor,
            cube_rate,
            cube_angles,
            flare_ring_small,
            flare_ring_large,
            flare_rot,
            flare_pos,
            red_flare_intensity: 0.0,
            illegal_scene_warm: false,
            illegal_fog_verts,
            illegal_fog_colors,
            illegal_fog_pos,
            illegal_fog_rot,
            sce_text_state: if is_illegal { -1 } else { 0 },
            sce_text_alpha: 0,
            sce_text_step: 4,
            illegal_text_state: if is_illegal { 0 } else { -1 },
            illegal_text_alpha: 0,
            speed_multiplier: 1.0,
            camera_xy_offset: [0.0, 0.0],
            reduced_motion: false,
            enable_fog: true,
            enable_lights: true,
            enable_cubes: true,
            enable_glow: true,
            enable_motion_blur: true,
            letterbox: false,
        };
        sim.update_camera_matrices();
        sim
    }

    fn setup_cube_geometry(
        half: f32,
        x: f32,
        y: f32,
        z: f32,
        corners: &mut [Vec4; 8],
        base_color: &mut [f32; 3],
    ) {
        for (i, c) in corners.iter_mut().enumerate() {
            *c = [
                if (i & 1) != 0 { half } else { -half },
                if (i & 2) != 0 { half } else { -half },
                if (i & 4) != 0 { half } else { -half },
                1.0,
            ];
        }
        base_color[0] = clamp_f32(x + 128.0, 0.0, 127.0);
        base_color[1] = clamp_f32(y + 128.0, 0.0, 127.0);
        base_color[2] = clamp_f32(z + 128.0, 0.0, 127.0);
    }

    fn update_camera_matrices(&mut self) {
        self.up_dir[0] = self.rotation.sin();
        self.up_dir[1] = self.rotation.cos();
        self.up_dir[2] = 0.0;
        self.up_dir[3] = 1.0;

        let effective_pos = [
            self.position[0] + self.camera_xy_offset[0],
            self.position[1] + self.camera_xy_offset[1],
            self.position[2],
            0.0,
        ];
        self.camera_matrix = mat_camera(effective_pos, self.fwd_dir, self.up_dir);
        self.view_screen_matrix = mat_view_screen(
            1024.0,
            1.0,
            self.standard.opening_screen_ay(),
            2048.0,
            2048.0,
            1.0,
            16_777_215.0,
            1.0,
            65536.0,
        );
        self.camera_screen_matrix = mat_mul(&self.view_screen_matrix, &self.camera_matrix);
    }

    /// Advances simulation by 1 frame and populates `render_out` and `audio_out`.
    pub fn step(&mut self, render_out: &mut OpeningRenderList, audio_out: &mut Vec<AudioCue>) {
        render_out.clear();
        if self.frame_count == 0 {
            audio_out.push(if self.opening_type == 1 {
                AudioCue::StartIllegalWarningDrone
            } else {
                AudioCue::StartOpeningChime
            });
        }

        self.step_camera_animation(audio_out);

        let (screen_w, screen_h) = self.standard.screen_dims();
        if self.opening_type == 0 {
            self.build_opening_frame(screen_w, screen_h, render_out);
        } else {
            self.build_illegal_frame(screen_w, screen_h, render_out);
        }

        // DoText (0x214f58) + letterbox
        self.step_banner_and_illegal_text(render_out);
        if self.letterbox {
            let imgh = screen_w * 9.0 * self.standard.opening_screen_ay() / 16.0;
            let bar = ((screen_h - imgh + 1.0) * 0.5).max(0.0);
            render_out.letterbox_bar_ratio = clamp_f32(bar / screen_h, 0.0, 0.35);
        }

        self.frame_count = self.frame_count.saturating_add(1);
    }

    fn step_camera_animation(&mut self, audio_out: &mut Vec<AudioCue>) {
        let motion_scale = if self.reduced_motion {
            0.25
        } else {
            clamp_f32(self.speed_multiplier, 0.1, 4.0)
        };
        let dt = self.standard.timestep() * motion_scale;
        let fps = self.standard.fps();

        if self.variant != OpeningVariant::Idle
            && self.opening_state < 7
            && (OPENING_STATE_LEVELS[self.opening_state] as f32) < self.position[2]
        {
            self.opening_state += 1;
        }

        let mut next_type = self.opening_type;

        match self.opening_state {
            1 => {
                if self.has_disc {
                    self.rotation_speed[2] = 0.0004;
                    if self.frame_count < (fps * 20) / 6 {
                        self.position_accel2[2] = -0.00014;
                    } else {
                        self.position_accel2[2] = 0.000_025;
                    }
                    if self.disc_type != 0 {
                        self.position_accel2[2] = 0.003;
                        self.opening_go = true;
                        self.opening_state += 1;
                    } else if self.frame_count > fps * 20 {
                        self.has_disc = false;
                    }
                } else {
                    self.position_accel1[2] = 0.000_000_4;
                    if matches!(
                        self.osd_boot_param,
                        100 | 106..=112 | 114..=116
                    ) && self.frame_count > fps * 2
                    {
                        self.opening_go = true;
                        self.opening_state += 1;
                    }
                }
            }
            2 => {
                if self.frame_count > fps * 10 {
                    self.opening_go = true;
                }
                if self.opening_go {
                    if self.boot_request == 1 {
                        audio_out.push(AudioCue::OpeningFlyUpWhoosh);
                        self.boot_request = -1;
                    }
                    if self.has_disc || self.disc_type != 0 {
                        self.position_accel1[2] = 0.0004;
                        self.rotation_accel[2] = 0.00008;
                    } else {
                        self.position_accel2[2] = 0.0099;
                        self.rotation_accel[2] = 0.000_195;
                    }
                    self.position_accel2[0] = 0.0;
                    self.position_accel2[1] = 0.0;
                    self.rotation_accel[0] = 0.0;
                    self.rotation_accel[1] = 0.0;
                }
            }
            3 => {
                // Real ProcessOpeningAnimation (0x215fd0, opening.c:2892):
                // at z > 104, fade-to-black `(z - 72) * 4 >= 128` has reached full black and
                // state 3 immediately advances `type` to end the opening scene.
                if self.opening_type == 0 {
                    next_type = 2;
                }
            }
            6 => {
                self.position_accel1 = [0.0; 4];
                self.position_accel2 = [0.0; 4];
                self.position_speed = [0.0; 4];
                self.rotation_accel = [0.0; 4];
                match self.osd_boot_param {
                    100 | 106..=112 | 115 => {
                        self.opening_end_flag = true;
                        if self.opening_end_frame == 0 {
                            audio_out.push(AudioCue::FadeOutBgm);
                            self.opening_end_frame = self.frame_count;
                        } else if self.frame_count > self.opening_end_frame + 128 {
                            next_type = 2;
                        }
                    }
                    _ => {
                        if self.opening_end_flag && self.frame_count > self.opening_end_frame + 128
                        {
                            next_type = 2;
                        }
                    }
                }
            }
            7 => {
                next_type = 2;
            }
            _ => {}
        }

        if next_type == 2 {
            self.scene_finished = true;
        }

        for axis in 0..3 {
            self.rotation_speed[axis] += self.rotation_accel[axis] * dt;
            self.position_speed[axis] +=
                (2.0 * self.position_accel2[axis] + self.position_accel1[axis]) * 0.5 * dt;
        }
        self.position_accel2[2] += self.position_accel1[2] * dt;

        if self.reduced_motion && self.variant == OpeningVariant::Idle {
            // Gentle slow drift under reduced motion
            self.rotation = wrap_pi(self.rotation + 0.0002 * dt);
        } else {
            self.rotation = wrap_pi(
                self.rotation + (2.0 * self.rotation_speed[2] + self.rotation_accel[2]) * 0.5 * dt,
            );
        }

        if self.variant != OpeningVariant::Idle {
            for axis in 0..3 {
                self.position[axis] +=
                    (2.0 * self.position_speed[axis] + self.position_accel2[axis]) * 0.5 * dt;
            }
        }

        self.update_camera_matrices();
    }

    fn build_opening_frame(&mut self, screen_w: f32, screen_h: f32, out: &mut OpeningRenderList) {
        // 1. DrawTowers
        out.drawn_tower_count = self.tower_field.emit_towers(
            self.frame_count,
            &self.camera_screen_matrix,
            screen_w,
            screen_h,
            self.reduced_motion,
            &mut out.tower_vertices,
        );

        // 2. DrawExtraBuf2(80) + DrawToExtraBuf2
        if self.enable_glow {
            out.extra_buf_feedback_fix = 80;
        }

        // 3. DrawFog (6 cloud layers over 17x17 grid)
        if self.enable_fog {
            self.emit_opening_fog(screen_w, screen_h, out);
        }

        // 4. DrawLightsAndCubes (gated on position.z < 73.0 in real DrawOpeningScene 0x218d80!)
        if self.position[2] < 73.0 {
            if self.enable_lights {
                self.emit_opening_lights(screen_w, screen_h, out);
            }
            if self.enable_cubes {
                self.emit_cubes(false, screen_w, screen_h, out);
            }
        }

        // 5. sub_218b20 fly-up motion blur (z > 56)
        if self.enable_motion_blur && !self.reduced_motion && self.position[2] > 56.0 {
            let n = ((self.position[2] - 56.0) / 12.0) as i32;
            out.motion_blur_passes = n.clamp(0, 3) as u8;
        }

        // 6. sub_218bd0 black fades
        let mut fade = 0_i32;
        if self.frame_count < 2 {
            fade = 128;
        }
        if self.position[2] > 72.0 {
            let f = ((self.position[2] - 72.0) * 128.0 * 0.03125) as i32;
            fade = fade.max(f.clamp(0, 128));
        }
        if self.position[2] >= 320.0 {
            fade = 128;
        }
        out.black_fade_alpha = fade as u8;
    }

    fn build_illegal_frame(&mut self, screen_w: f32, screen_h: f32, out: &mut OpeningRenderList) {
        if !self.illegal_scene_warm {
            self.illegal_scene_warm = true;
            out.black_fade_alpha = 128;
            return;
        }

        // 1. DrawRedFlare + DrawExtraBuf2(112)
        self.emit_red_flare(screen_w, screen_h, out);
        if self.enable_glow {
            out.extra_buf_feedback_fix = 112;
        }

        // 2. DrawIllegalFog (128 falling red cloud patches)
        if self.enable_fog {
            self.emit_illegal_fog(screen_w, screen_h, out);
        }

        // 3. DrawIllegalCubes (5 red refractive cubes)
        if self.enable_cubes {
            self.emit_cubes(true, screen_w, screen_h, out);
        }

        // 4. DrawIllegalFades
        let z = self.position[2];
        let mut fade = 0_i32;
        if z < 800.0 {
            fade = (128 - (z - 672.0) as i32).clamp(0, 128);
            self.illegal_fade_counter = 0;
        } else if z > 1128.0 {
            let f = clamp_f32((32.0 - (1160.0 - z)) * 4.0, 0.0, 128.0) as i32;
            fade = f;
            self.illegal_fade_counter = 0;
        }
        if self.opening_end_flag {
            self.illegal_fade_counter = (self.illegal_fade_counter + 1).min(128);
            fade = fade.max(self.illegal_fade_counter);
        }
        out.black_fade_alpha = fade as u8;
    }

    fn emit_opening_fog(&mut self, screen_w: f32, screen_h: f32, out: &mut OpeningRenderList) {
        const LAYER_TEX: [TextureId; 6] = [
            TextureId::Fog4,
            TextureId::Fog2,
            TextureId::Fog1,
            TextureId::Fog4,
            TextureId::Fog2,
            TextureId::Fog1,
        ];
        // Update subtle 14-bit sine wobble from matching/src/fog.c
        let ang = ((self.frame_count as i32).wrapping_mul(51)) & 0x3FFF;
        let c = if self.reduced_motion {
            1.0
        } else {
            osd_cos_14bit(ang)
        };
        let s = if self.reduced_motion {
            0.0
        } else {
            osd_sin_14bit(ang)
        };
        for y in 0..17 {
            for x in 0..17 {
                self.fog_vertices[x][y][0] = (x as f32) * 6.0 - 48.0 + (c + c);
                self.fog_vertices[x][y][1] = (y as f32) * 6.0 - 48.0 + (s + s);
            }
        }

        let scroll_scale = if self.reduced_motion { 0.25 } else { 1.0 };
        for (l, &layer_tex) in LAYER_TEX.iter().enumerate() {
            self.fog_animation[l] +=
                ((14 - l) as f32) * 0.0001 * ((l + 1) as f32) * 0.5 * scroll_scale;
            if self.fog_animation[l] > 1.0 {
                self.fog_animation[l] -= 1.0;
            }
            let dz = (l as f32) * 5.0;
            let scroll = self.fog_animation[l];

            let mut verts = Vec::with_capacity(16 * 16 * 6);
            for y in 0..16 {
                for x in 0..16 {
                    let mut cell = [[0.0_f32; 4]; 4];
                    for (k, cell_v) in cell.iter_mut().enumerate() {
                        let v = self.fog_vertices[x + (k % 2)][y + (k / 2)];
                        *cell_v = [v[0], v[1], v[2] - dz, 1.0];
                    }
                    if gs_all_behind_or_outside_guard(
                        &self.camera_screen_matrix,
                        &cell,
                        5.0,
                        screen_w,
                        screen_h,
                    ) {
                        continue;
                    }
                    let mut qv = [PackedTexturedVertex::default(); 4];
                    for (k, dst_qv) in qv.iter_mut().enumerate() {
                        let proj = gs_transform_vertex(&self.camera_screen_matrix, cell[k]);
                        let clip_pos = gs_projected_to_webgpu_clip(&proj, screen_w, screen_h, 1.0);
                        let u = ((k % 2) as f32) * 0.5 + ((x % 2) as f32) * 0.5 - scroll;
                        let v = ((k / 2) as f32) * 0.5 + ((y % 2) as f32) * 0.5;
                        let cb = self.fog_blue[x + (k % 2)][y + (k / 2)];
                        let cr = (cb / 4) as f32;
                        let cg = ((cb * 2) / 5) as f32;
                        *dst_qv = PackedTexturedVertex {
                            clip_pos,
                            uv_q: [u, v, proj.q, 0.0],
                            gs_rgba: [cr, cg, cb as f32, 128.0],
                        };
                    }
                    verts.push(qv[0]);
                    verts.push(qv[1]);
                    verts.push(qv[2]);
                    verts.push(qv[2]);
                    verts.push(qv[1]);
                    verts.push(qv[3]);
                }
            }
            if !verts.is_empty() {
                out.fog_batches.push(TexturedBatch {
                    texture: layer_tex,
                    blend_mode: 0, // (Cs - 0) * FIX + Cd, fix = 20
                    blend_fix: 20,
                    depth_test: true,
                    depth_write: false,
                    uv_clamp: false,
                    vertices: verts,
                });
            }
        }
    }

    fn emit_opening_lights(&mut self, screen_w: f32, screen_h: f32, out: &mut OpeningRenderList) {
        let mut sprite_verts = Vec::with_capacity(4 * 4 * 2 * 6);
        let light_alphas = [24_i32, 12_i32];

        for (l, base_col) in LIGHT_COLORS.iter().enumerate() {
            let fc = (self.frame_count as i32 + self.lights_seed + (l as i32) * 17) as f32;
            let fs = (self.frame_count as i32 + self.lights_seed + (l as i32) * 15) as f32;
            let c = (fc * 0.01 * ((l + 10) as f32) * 0.1).cos();
            let s = (fs * 0.005 * ((l + 10) as f32) * 0.1).sin();

            for i in 0..4 {
                let world_screen = if i == 3 {
                    let pos = [
                        (10.0 - l as f32) * c,
                        (3.0 + l as f32) * s,
                        c * 12.0 + 88.0,
                        0.0,
                    ];
                    let world = mat_trans(&mat_unit(), pos);
                    let ws = mat_mul(&self.camera_screen_matrix, &world);
                    self.light_matrix_history[l][3] = ws;
                    ws
                } else {
                    self.light_matrix_history[l][i] = self.light_matrix_history[l][i + 1];
                    self.light_matrix_history[l][i]
                };

                if i == 3 {
                    // Store world-space origin of this orb for crisp re-projection along the trail!
                    let pos = [
                        (10.0 - l as f32) * c,
                        (3.0 + l as f32) * s,
                        c * 12.0 + 88.0,
                        1.0,
                    ];
                    self.light_trail_pos[l][self.light_trail_end] = pos;
                }

                for (j, quad) in LIGHT_SPRITE_VERTS.iter().enumerate() {
                    if gs_all_behind_or_outside_guard(&world_screen, quad, 5.0, screen_w, screen_h)
                    {
                        continue;
                    }
                    let mut qv = [PackedTexturedVertex::default(); 4];
                    for (k, dst_qv) in qv.iter_mut().enumerate() {
                        let proj = gs_transform_vertex(&world_screen, quad[k]);
                        let clip_pos = gs_projected_to_webgpu_clip(&proj, screen_w, screen_h, 1.0);
                        let _ = self.rng.next_i31(); // preserve exact ROM rand() phase (0x216a54)

                        let a = ((light_alphas[j] * (i as i32 + 1)) / 5) as f32;
                        let (r, g, b) = if j == 0 {
                            (base_col[0] * 0.5, base_col[1] * 0.5, base_col[2] * 0.5)
                        } else {
                            (128.0, 128.0, 128.0)
                        };
                        *dst_qv = PackedTexturedVertex {
                            clip_pos,
                            uv_q: [CUBE_UV_UNIT[k][0], CUBE_UV_UNIT[k][1], proj.q, 0.0],
                            gs_rgba: [r, g, b, a],
                        };
                    }
                    sprite_verts.push(qv[0]);
                    sprite_verts.push(qv[1]);
                    sprite_verts.push(qv[2]);
                    sprite_verts.push(qv[2]);
                    sprite_verts.push(qv[1]);
                    sprite_verts.push(qv[3]);
                }
            }
        }

        self.light_trail_end = (self.light_trail_end + 1) % 128;
        if self.light_trail_end == self.light_trail_start {
            self.light_trail_start = (self.light_trail_start + 1) % 128;
        }

        if !sprite_verts.is_empty() {
            out.light_sprite_batch = Some(TexturedBatch {
                texture: TextureId::Crbl,
                blend_mode: 5, // (Cs - 0) * As + Cd
                blend_fix: 128,
                depth_test: false,
                depth_write: false,
                uv_clamp: false,
                vertices: sprite_verts,
            });
        }

        // Emit line-list segments for the 4 trails (every 8th sample, matching `if ((j & 7) != 0) continue`)
        for (l, base_col) in LIGHT_COLORS.iter().enumerate() {
            let start = (self.light_trail_start + 1) % 128;
            let mut k = (self.light_trail_end + 127) % 128;
            let intensity = ((self.light_trail_end + 127 + 128 - start) % 128) as i32;
            if intensity <= 1 {
                continue;
            }
            let mut prev: Option<PackedColorVertex> = None;
            let mut j = 0_i32;
            while k != start && j < 128 {
                if (j & 7) == 0 {
                    let pos = self.light_trail_pos[l][k];
                    if pos[3] > 0.0 {
                        let proj = gs_transform_vertex(&self.camera_screen_matrix, pos);
                        if proj.w >= 5.0 {
                            let clip_pos =
                                gs_projected_to_webgpu_clip(&proj, screen_w, screen_h, 1.0);
                            let a = ((intensity - j) * 64 / intensity).max(0) as f32;
                            let r = base_col[0] * a / 128.0;
                            let g = base_col[1] * a / 128.0;
                            let b = base_col[2] * a / 128.0;
                            let cur = PackedColorVertex {
                                clip_pos,
                                gs_rgba: [r, g, b, (a * 2.0).min(128.0)],
                            };
                            if let Some(p0) = prev {
                                out.light_trail_vertices.push(p0);
                                out.light_trail_vertices.push(cur);
                            }
                            prev = Some(cur);
                        } else {
                            prev = None;
                        }
                    }
                }
                k = (k + 127) % 128;
                j += 1;
            }
        }
    }

    fn emit_cubes(
        &mut self,
        illegal: bool,
        screen_w: f32,
        screen_h: f32,
        out: &mut OpeningRenderList,
    ) {
        let pass_specs = if illegal {
            &ILLEGAL_CUBE_PASSES
        } else {
            &OPENING_CUBE_PASSES
        };
        let rot_scale = if self.reduced_motion { 0.25 } else { 1.0 };

        for inst in 0..CUBE_INSTANCES {
            for axis in 0..3 {
                self.cube_angles[inst][axis] =
                    wrap_pi(self.cube_angles[inst][axis] + self.cube_rate[inst][axis] * rot_scale);
            }

            let rotated = mat_rot_xyz(&mat_unit(), self.cube_angles[inst]);
            let translated = mat_trans(&rotated, self.cube_anchor[inst]);
            let transformed = mat_mul(&self.camera_screen_matrix, &translated);

            if gs_all_behind_or_outside_guard(
                &transformed,
                &self.cube_corners,
                5.0,
                screen_w,
                screen_h,
            ) {
                continue;
            }

            let center_proj = gs_transform_vertex(&transformed, [0.0, 0.0, 0.0, 1.0]);
            let center_xy = [center_proj.gs_xyz[0], center_proj.gs_xyz[1]];

            let mut corner_proj = [gs_transform_vertex(&transformed, [0.0, 0.0, 0.0, 1.0]); 8];
            let mut world_corners = [[0.0_f32; 4]; 8];
            let mut view_dir = [[0.0_f32; 4]; 8];
            for k in 0..8 {
                corner_proj[k] = gs_transform_vertex(&transformed, self.cube_corners[k]);
                world_corners[k] = mat_apply(&translated, self.cube_corners[k]);
                let cam_c = mat_apply(&self.camera_matrix, world_corners[k]);
                view_dir[k] = vec_normalize([cam_c[0], cam_c[1], cam_c[2], 0.0]);
            }

            let mut screen_normal = [[0.0_f32; 4]; 6];
            let mut face_sign = [0.0_f32; 6];
            let mut vertex_light = [[0.0_f32; 4]; 6];

            for face in 0..6 {
                let mut wn = mat_apply(&rotated, CUBE_FACE_NORMALS[face]);
                wn = vec_normalize([wn[0], wn[1], wn[2], 0.0]);
                let inward = [-wn[0], -wn[1], -wn[2], 0.0];
                let sn = mat_apply(&self.camera_matrix, inward);
                screen_normal[face] = vec_normalize([sn[0], sn[1], sn[2], 0.0]);

                let p0 = corner_proj[CUBE_FACE_VERTS[face][0]].gs_xyz;
                let p1 = corner_proj[CUBE_FACE_VERTS[face][1]].gs_xyz;
                let p2 = corner_proj[CUBE_FACE_VERTS[face][2]].gs_xyz;
                let (e1x, e1y) = (p0[0] - p1[0], p0[1] - p1[1]);
                let (e2x, e2y) = (p0[0] - p2[0], p0[1] - p2[1]);
                let l1 = (e1x * e1x + e1y * e1y).sqrt();
                let l2 = (e2x * e2x + e2y * e2y).sqrt();
                face_sign[face] = if l1 > 0.0 && l2 > 0.0 {
                    (e1x / l1) * (e2y / l2) - (e1y / l1) * (e2x / l2)
                } else {
                    0.0
                };

                for k in 0..4 {
                    let vi = CUBE_FACE_VERTS[face][k];
                    let dir = vec_normalize([
                        world_corners[vi][0] - self.position[0],
                        world_corners[vi][1] - self.position[1],
                        world_corners[vi][2] - self.position[2],
                        0.0,
                    ]);
                    vertex_light[face][k] = vec_dot3(wn, dir).abs();
                }
            }

            let mut cube_verts = Vec::with_capacity(10 * 3 * 6);
            let mut passes = Vec::with_capacity(10);

            for spec in pass_specs {
                let start_vertex = cube_verts.len() as u32;
                for face in 0..6 {
                    let is_back = face_sign[face] < 0.0;
                    if is_back != spec.back_faces {
                        continue;
                    }

                    let mut qv = [PackedTexturedVertex::default(); 4];
                    for k in 0..4 {
                        let vi = CUBE_FACE_VERTS[face][k];
                        let proj = &corner_proj[vi];
                        let clip_pos = gs_projected_to_webgpu_clip(proj, screen_w, screen_h, 1.0);

                        let (u, v) = match spec.uv_mode {
                            0 => {
                                let x = proj.gs_xyz[0];
                                let y = proj.gs_xyz[1];
                                let q = proj.q;
                                let px = x - (2048.0 - screen_w * 0.5)
                                    + screen_normal[face][0]
                                        * (screen_w * 0.5)
                                        * spec.uv_arg2
                                        * q
                                        * 4.0
                                    + (x - center_xy[0]) * spec.uv_arg;
                                let py = y - (2048.0 - screen_h * 0.5)
                                    + screen_normal[face][1]
                                        * (screen_h * 0.5)
                                        * spec.uv_arg2
                                        * q
                                        * 4.0
                                    + (y - center_xy[1]) * spec.uv_arg;
                                let cu = clamp_f32(px, 1.0, screen_w - 1.0) / screen_w;
                                let cv = clamp_f32(py, 1.0, screen_h - 1.0) / screen_h;
                                (cu, cv)
                            }
                            1 => (
                                CUBE_UV_UNIT[k][0] + spec.uv_arg,
                                CUBE_UV_UNIT[k][1] - spec.uv_arg,
                            ),
                            _ => (
                                view_dir[vi][1] + screen_normal[face][1] * spec.uv_arg + 0.5,
                                view_dir[vi][0] + screen_normal[face][0] * spec.uv_arg + 0.5,
                            ),
                        };

                        let l = vertex_light[face][k];
                        let e = (1.0 - l) * (1.0 - l) * 0.5;
                        let s = f32::from(spec.blend_fix) / 128.0;
                        let (r, g, b) = match spec.color_mode {
                            2 => (
                                clamp_f32((self.cube_base_color[0] + e * 32.0) * s, 0.0, 128.0),
                                clamp_f32((self.cube_base_color[1] + e * 32.0) * s, 0.0, 128.0),
                                clamp_f32((self.cube_base_color[2] + e * 32.0) * s, 0.0, 128.0),
                            ),
                            3 => {
                                let t = e * 0.4 * 0.6 + 0.2;
                                (
                                    clamp_f32(self.cube_base_color[0] * t * s, 0.0, 128.0),
                                    clamp_f32(self.cube_base_color[1] * t * s, 0.0, 128.0),
                                    clamp_f32(self.cube_base_color[2] * t * s, 0.0, 128.0),
                                )
                            }
                            4 => {
                                let t = e * 0.6 + 0.2;
                                (
                                    clamp_f32(self.cube_base_color[0] * t * s, 0.0, 128.0),
                                    clamp_f32(self.cube_base_color[1] * t * s / 3.0, 0.0, 128.0),
                                    clamp_f32(self.cube_base_color[2] * t * s / 3.0, 0.0, 128.0),
                                )
                            }
                            _ => (128.0, 128.0, 128.0),
                        };

                        qv[k] = PackedTexturedVertex {
                            clip_pos,
                            uv_q: [u, v, if spec.st_screen_space { 1.0 } else { proj.q }, 0.0],
                            gs_rgba: [r.trunc(), g.trunc(), b.trunc(), 128.0],
                        };
                    }

                    cube_verts.push(qv[0]);
                    cube_verts.push(qv[1]);
                    cube_verts.push(qv[2]);
                    cube_verts.push(qv[2]);
                    cube_verts.push(qv[1]);
                    cube_verts.push(qv[3]);
                }

                let vertex_count = (cube_verts.len() as u32) - start_vertex;
                if vertex_count > 0 {
                    passes.push(CubePassDraw {
                        target: spec.target,
                        source: spec.source,
                        abe: spec.abe,
                        blend_mode: spec.blend_mode,
                        blend_fix: spec.blend_fix,
                        start_vertex,
                        vertex_count,
                    });
                }
            }

            if !passes.is_empty() {
                out.cubes.push(CubeInstanceDraw {
                    instance_index: inst as u8,
                    passes,
                    vertices: cube_verts,
                });
            }
        }
    }

    fn emit_red_flare(&mut self, screen_w: f32, screen_h: f32, out: &mut OpeningRenderList) {
        self.red_flare_intensity = (740.0 - (1160.0 - self.position[2])) * 128.0 / 740.0 * 0.6;
        let rot_scale = if self.reduced_motion { 0.2 } else { 1.0 };

        // flare_21A6D8
        let phase = ((self.frame_count % 201) as f32) * 0.03125 - PI;
        for i in 0..7 {
            let mult = (i + 1) as f32 * rot_scale;
            self.flare_rot[i][0] = wrap_pi(self.flare_rot[i][0] + mult * 0.2);
            self.flare_rot[i][2] = wrap_pi(self.flare_rot[i][2] + mult * 0.35);
            self.flare_rot[i][1] = wrap_pi(self.flare_rot[i][1] + mult * 0.27);
            let r = (((7 - i) * (7 - i)) as f32) * PI * 2.0 / 64.0;
            let f = wrap_pi(r + phase);
            self.flare_pos[i][0] = f.cos() * r * 0.5;
            self.flare_pos[i][1] = f.sin() * r * 0.5;
        }

        // Small bright cores + large dim haloes
        let small_center = [128.0_f32, 16.0, 40.0];
        let large_center = [48.0_f32, 8.0, 12.0];
        for (rings_is_small, base_c) in [(true, small_center), (false, large_center)] {
            let scale = self.red_flare_intensity * 0.015_625;
            let center_rgb = [
                clamp_f32(base_c[0] * scale, 0.0, 255.0).trunc(),
                clamp_f32(base_c[1] * scale, 0.0, 255.0).trunc(),
                clamp_f32(base_c[2] * scale, 0.0, 255.0).trunc(),
                128.0,
            ];
            let rim_rgb = [0.0, 0.0, 0.0, 128.0];

            for i in 0..7 {
                let m9 = mat_rot_xyz(&mat_unit(), self.flare_rot[i]);
                let world = mat_trans(&m9, self.flare_pos[i]);
                let ws = mat_mul(&self.camera_screen_matrix, &world);
                let ring = if rings_is_small {
                    &self.flare_ring_small[i]
                } else {
                    &self.flare_ring_large[i]
                };
                if gs_all_behind_or_outside_guard(&ws, ring, 5.0, screen_w, screen_h) {
                    continue;
                }
                let p_center = gs_transform_vertex(&ws, ring[0]);
                let v_center = PackedColorVertex {
                    clip_pos: gs_projected_to_webgpu_clip(&p_center, screen_w, screen_h, 1.0),
                    gs_rgba: center_rgb,
                };
                for k in 0..16 {
                    let v1_idx = 1 + k;
                    let v2_idx = 1 + ((k + 1) % 16);
                    let p1 = gs_transform_vertex(&ws, ring[v1_idx]);
                    let p2 = gs_transform_vertex(&ws, ring[v2_idx]);
                    out.flare_fan_vertices.push(v_center);
                    out.flare_fan_vertices.push(PackedColorVertex {
                        clip_pos: gs_projected_to_webgpu_clip(&p1, screen_w, screen_h, 1.0),
                        gs_rgba: rim_rgb,
                    });
                    out.flare_fan_vertices.push(PackedColorVertex {
                        clip_pos: gs_projected_to_webgpu_clip(&p2, screen_w, screen_h, 1.0),
                        gs_rgba: rim_rgb,
                    });
                }
            }
        }

        // FlareThing (5 billboard sprites using FLAR texture)
        let n = ((self.red_flare_intensity * 0.0625) as i32).min(8);
        let m = (n + 2).clamp(0, 128) as f32;
        let big_a = (n * 3 + (self.red_flare_intensity * 0.5) as i32).clamp(0, 128) as f32;
        let big_size = self.red_flare_intensity * 0.125 + 420.0;

        let ph = wrap_pi(((49 + (self.frame_count & 0x1f)) as f32) * 0.1);
        self.flare_pos[0][0] = ph.cos() * 49.0 * 0.004;
        self.flare_pos[0][1] = ph.sin() * 49.0 * 0.004;
        let m9 = mat_rot_xyz(&mat_unit(), self.flare_rot[0]);
        let world = mat_trans(&m9, self.flare_pos[0]);
        let ws = mat_mul(&self.camera_screen_matrix, &world);
        let center_p = gs_transform_vertex(&ws, self.flare_ring_small[0][0]);

        let specs: [(f32, f32, [f32; 3], f32); 5] = [
            (112.0, m, [128.0, 64.0, 64.0], 1.0),
            (170.0, m, [128.0, 64.0, 64.0], 1.0),
            (256.0, m, [128.0, 64.0, 64.0], 1.0),
            (448.0, m, [128.0, 64.0, 64.0], 1.0),
            (big_size, big_a, [128.0, 112.0, 96.0], 0.9),
        ];

        let mut sprite_verts = Vec::with_capacity(30);
        for (size, alpha_fix, rgb, scale) in specs {
            let cx = (center_p.gs_xyz[0] - 2048.0) * scale + screen_w * 0.5;
            let cy = (center_p.gs_xyz[1] - 2048.0) * scale + screen_h * 0.5;
            let x0 = cx - size;
            let y0 = cy - size * 0.5;
            let x1 = cx + size;
            let y1 = cy + size * 0.5;

            let c00 = pixel_to_ndc(x0, y0, screen_w, screen_h);
            let c10 = pixel_to_ndc(x1, y0, screen_w, screen_h);
            let c01 = pixel_to_ndc(x0, y1, screen_w, screen_h);
            let c11 = pixel_to_ndc(x1, y1, screen_w, screen_h);
            let col = [
                rgb[0] * (alpha_fix / 128.0),
                rgb[1] * (alpha_fix / 128.0),
                rgb[2] * (alpha_fix / 128.0),
                128.0,
            ];
            let qv = [
                PackedTexturedVertex {
                    clip_pos: [c00[0], c00[1], 0.5, 1.0],
                    uv_q: [0.0, 0.0, 1.0, 0.0],
                    gs_rgba: col,
                },
                PackedTexturedVertex {
                    clip_pos: [c10[0], c10[1], 0.5, 1.0],
                    uv_q: [1.0, 0.0, 1.0, 0.0],
                    gs_rgba: col,
                },
                PackedTexturedVertex {
                    clip_pos: [c01[0], c01[1], 0.5, 1.0],
                    uv_q: [0.0, 1.0, 1.0, 0.0],
                    gs_rgba: col,
                },
                PackedTexturedVertex {
                    clip_pos: [c11[0], c11[1], 0.5, 1.0],
                    uv_q: [1.0, 1.0, 1.0, 0.0],
                    gs_rgba: col,
                },
            ];
            sprite_verts.push(qv[0]);
            sprite_verts.push(qv[1]);
            sprite_verts.push(qv[2]);
            sprite_verts.push(qv[2]);
            sprite_verts.push(qv[1]);
            sprite_verts.push(qv[3]);
        }
        out.flare_sprite_batch = Some(TexturedBatch {
            texture: TextureId::Flar,
            blend_mode: 0,
            blend_fix: 128,
            depth_test: false,
            depth_write: false,
            uv_clamp: true,
            vertices: sprite_verts,
        });
    }

    fn emit_illegal_fog(&mut self, screen_w: f32, screen_h: f32, out: &mut OpeningRenderList) {
        let global_alpha = if self.position[2] < 672.0 {
            clamp_f32(
                (550.0 - (672.0 - self.position[2])) * 128.0 / 550.0 * 4.0,
                0.0,
                128.0,
            )
        } else {
            128.0
        };

        let fall_scale = if self.reduced_motion { 0.35 } else { 1.0 };
        let mut verts = Vec::with_capacity(128 * 4 * 6);

        for i in 0..128 {
            let mut rj = ((i + 1) as f32) * 0.0001;
            if (self.frame_count & 1) == 0 {
                rj = -rj;
            }
            self.illegal_fog_rot[i][2] = wrap_pi(self.illegal_fog_rot[i][2] + rj);
            let step = (((i + 1) as f32 * 0.02 + 1.2) as i32) as f32 * fall_scale;
            self.illegal_fog_pos[i][2] -= step;

            let d_top = (self.position[2] + 805.0) - self.illegal_fog_pos[i][2];
            let af = if d_top < 192.0 {
                if d_top < 0.0 {
                    continue;
                }
                d_top * 64.0 / 192.0
            } else {
                let d_bot = self.illegal_fog_pos[i][2] - self.position[2] - 32.0;
                if d_bot < 192.0 {
                    if d_bot < 0.0 {
                        self.illegal_fog_pos[i][2] = self.position[2] + 805.0;
                        continue;
                    }
                    d_bot * 64.0 / 192.0
                } else {
                    64.0
                }
            };
            let alpha_fix = clamp_f32(af, 0.0, 64.0) * global_alpha * 0.007_812_5;
            if alpha_fix < 0.5 {
                continue;
            }

            let m9 = mat_rot_xyz(&mat_unit(), self.illegal_fog_rot[i]);
            let world = mat_trans(&m9, self.illegal_fog_pos[i]);
            let ws = mat_mul(&self.camera_screen_matrix, &world);
            if gs_all_behind_or_outside_guard(
                &ws,
                &self.illegal_fog_verts[0..8],
                5.0,
                screen_w,
                screen_h,
            ) {
                continue;
            }

            let fix_factor = alpha_fix / 128.0;
            for strip in &FOG_STRIPS {
                let mut qv = [PackedTexturedVertex::default(); 4];
                for k in 0..4 {
                    let vi = strip[k];
                    let proj = gs_transform_vertex(&ws, self.illegal_fog_verts[vi]);
                    let clip_pos = gs_projected_to_webgpu_clip(&proj, screen_w, screen_h, 1.0);
                    let col = self.illegal_fog_colors[vi];
                    qv[k] = PackedTexturedVertex {
                        clip_pos,
                        uv_q: [FOG_UV_3X3[vi][0], FOG_UV_3X3[vi][1], proj.q, 0.0],
                        gs_rgba: [
                            (col[0] as f32) * fix_factor,
                            (col[1] as f32) * fix_factor,
                            (col[2] as f32) * fix_factor,
                            128.0,
                        ],
                    };
                }
                verts.push(qv[0]);
                verts.push(qv[1]);
                verts.push(qv[2]);
                verts.push(qv[2]);
                verts.push(qv[1]);
                verts.push(qv[3]);
            }
        }

        if !verts.is_empty() {
            out.fog_batches.push(TexturedBatch {
                texture: TextureId::Fog0,
                blend_mode: 0,
                blend_fix: 128, // per-patch alpha_fix pre-folded into vertex RGB!
                depth_test: false,
                depth_write: false,
                uv_clamp: false,
                vertices: verts,
            });
        }
    }

    fn step_banner_and_illegal_text(&mut self, out: &mut OpeningRenderList) {
        // DoSCEText (0x214c20)
        if self.position[2] > 18.0 && self.sce_text_state == 0 {
            self.sce_text_state = 1;
        }
        if self.sce_text_state == 1 {
            let alpha = self.sce_text_alpha + self.sce_text_step;
            self.sce_text_alpha = alpha;
            if alpha >= 240 {
                self.sce_text_step = -4;
            }
            if alpha <= 0 {
                self.sce_text_step = 4;
                self.sce_text_state = -1;
            } else {
                out.banner_alpha = alpha.min(112) as u8;
            }
        }

        // DoIllegalText (0x214e60)
        if self.position[2] > 800.0 && self.illegal_text_state == 0 {
            self.illegal_text_state = 1;
        }
        if self.illegal_text_state == 1 {
            let mut alpha = self.illegal_text_alpha;
            if !self.opening_end_flag {
                alpha += 1;
            } else if alpha > 0 {
                alpha -= 1;
            }
            alpha = alpha.min(112);
            self.illegal_text_alpha = alpha;
            if self.opening_type == 1 && alpha > 0 {
                out.illegal_text_alpha = alpha as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_a_boot_to_menu_progresses_through_states_and_finishes() {
        let mut sim = OpeningSim::new(
            OpeningVariant::BootToMenu,
            VideoStandard::Ntsc,
            OsdLanguage::English,
            TowerFieldPreset::default(),
            1,
        );
        let mut render = OpeningRenderList::default();
        let mut audio = Vec::new();

        let mut saw_banner = false;
        let mut saw_cubes = false;
        let mut saw_motion_blur = false;
        let mut prev_z = sim.position[2];

        for _ in 0..320 {
            sim.step(&mut render, &mut audio);
            assert!(sim.position[2] >= prev_z);
            prev_z = sim.position[2];
            if render.banner_alpha > 0 {
                saw_banner = true;
            }
            if !render.cubes.is_empty() {
                saw_cubes = true;
            }
            if render.motion_blur_passes > 0 {
                saw_motion_blur = true;
            }
            if sim.position[2] >= 73.0 {
                // Verify z >= 73 gate turns off lights & cubes
                assert!(render.light_sprite_batch.is_none());
                assert!(render.cubes.is_empty());
            }
        }

        assert!(saw_banner, "expected banner fade curve during opening");
        assert!(saw_cubes, "expected refractive glass cubes before z=73");
        assert!(saw_motion_blur, "expected fly-up motion blur past z=56");
        assert!(sim.scene_finished, "expected Mode A to complete cleanly");
        assert!(audio.contains(&AudioCue::StartOpeningChime));
        assert!(audio.contains(&AudioCue::OpeningFlyUpWhoosh));
    }

    #[test]
    fn mode_b_idle_holds_z_16_over_10000_frames_and_wraps_trails_and_cubes() {
        let mut sim = OpeningSim::new(
            OpeningVariant::Idle,
            VideoStandard::Ntsc,
            OsdLanguage::English,
            TowerFieldPreset::CapturedRetail,
            1,
        );
        let mut render = OpeningRenderList::default();
        let mut audio = Vec::new();
        let mut saw_trail_wrap = false;
        let initial_rot = sim.rotation;

        for frame in 0..10_000 {
            sim.step(&mut render, &mut audio);
            assert!((sim.position[2] - 16.0).abs() < 1e-6);
            assert!(sim.rotation.is_finite());
            assert_eq!(sim.opening_state, 0);
            assert_eq!(render.banner_alpha, 0);
            assert!(!sim.scene_finished);

            if sim.light_trail_start > 0 {
                saw_trail_wrap = true;
            }

            // Periodically check cube Fresnel RGB bounds on pass 9 (`colorMode 3`, `e in [0, 0.5]`, `t = e*0.24 + 0.2 in [0.2, 0.32]`, `s = 0.5`)
            if frame % 500 == 0 {
                for cube in &render.cubes {
                    if let Some(fresnel_pass) = cube.passes.last() {
                        let start = fresnel_pass.start_vertex as usize;
                        let end = start + fresnel_pass.vertex_count as usize;
                        for v in &cube.vertices[start..end] {
                            assert!(
                                (10.0..=21.0).contains(&v.gs_rgba[2]),
                                "Fresnel reflection blue channel out of [10, 21]: {}",
                                v.gs_rgba[2]
                            );
                        }
                    }
                }
            }
        }
        assert!(
            saw_trail_wrap,
            "expected 128-point light trail ring buffer wraparound"
        );
        assert!((sim.rotation - initial_rot).abs() > 1e-4);
        assert!(render.drawn_tower_count > 0);
        assert!(!render.light_trail_vertices.is_empty());
    }

    #[test]
    fn mode_a_pal_and_boot_to_game_and_bit_exact_determinism() {
        // PAL BootToGame (with disc present -> fast go transition) over 1,000+ frames check
        let mut pal_game = OpeningSim::new(
            OpeningVariant::BootToGame,
            VideoStandard::Pal,
            OsdLanguage::French,
            TowerFieldPreset::default(),
            777,
        );
        let mut render = OpeningRenderList::default();
        let mut audio = Vec::new();
        let mut finished_at = None;
        for f in 0..1_000 {
            let prev_z = pal_game.position[2];
            pal_game.step(&mut render, &mut audio);
            assert!(pal_game.position[2] >= prev_z);
            if pal_game.scene_finished && finished_at.is_none() {
                finished_at = Some(f);
            }
        }
        assert!(
            finished_at.is_some(),
            "expected PAL BootToGame to finish cleanly"
        );

        // Bit-exact deterministic replay for identical seed + frame index
        let mut a = OpeningSim::new(
            OpeningVariant::BootToMenu,
            VideoStandard::Ntsc,
            OsdLanguage::English,
            TowerFieldPreset::Simulated {
                seed: 2026,
                ngames: 12,
                nboots: Some(110),
            },
            1,
        );
        let mut b = OpeningSim::new(
            OpeningVariant::BootToMenu,
            VideoStandard::Ntsc,
            OsdLanguage::English,
            TowerFieldPreset::Simulated {
                seed: 2026,
                ngames: 12,
                nboots: Some(110),
            },
            1,
        );
        let mut ra = OpeningRenderList::default();
        let mut rb = OpeningRenderList::default();
        let mut aa = Vec::new();
        let mut ab = Vec::new();
        for _ in 0..90 {
            a.step(&mut ra, &mut aa);
            b.step(&mut rb, &mut ab);
        }
        assert_eq!(a.position[2].to_bits(), b.position[2].to_bits());
        assert_eq!(a.rotation.to_bits(), b.rotation.to_bits());
        assert_eq!(ra.tower_vertices.len(), rb.tower_vertices.len());
        assert_eq!(ra.cubes.len(), rb.cubes.len());
        assert_eq!(
            ra.tower_vertices[0].clip_pos[0].to_bits(),
            rb.tower_vertices[0].clip_pos[0].to_bits()
        );
    }

    #[test]
    fn mode_c_illegal_disc_idles_10000_frames_without_drift_and_supports_all_8_languages() {
        for code in 0..=7 {
            let lang = OsdLanguage::from_u8(code);
            assert!(!lang.illegal_disc_message().is_empty());
        }

        let mut sim = OpeningSim::new(
            OpeningVariant::IllegalDisc,
            VideoStandard::Pal,
            OsdLanguage::Portuguese,
            TowerFieldPreset::CapturedRetail,
            1,
        );
        let mut render = OpeningRenderList::default();
        let mut audio = Vec::new();

        let mut z_at_300 = 0.0_f32;
        let mut flare_pos1_at_400 = [0.0_f32; 4];
        let mut flare_pos1_at_601 = [0.0_f32; 4];
        let mut flare_pos0_at_400 = [0.0_f32; 4];
        let mut flare_pos0_at_432 = [0.0_f32; 4];

        for f in 0..10_000 {
            sim.step(&mut render, &mut audio);
            assert!(sim.position.iter().all(|x| x.is_finite()));
            assert!(sim.rotation.is_finite());

            if f == 300 {
                z_at_300 = sim.position[2];
            }
            if f == 400 {
                flare_pos1_at_400 = sim.flare_pos[1];
                flare_pos0_at_400 = sim.flare_pos[0];
            }
            if f == 432 {
                flare_pos0_at_432 = sim.flare_pos[0];
            }
            if f == 601 {
                flare_pos1_at_601 = sim.flare_pos[1];
            }
        }

        assert!(
            sim.position[2] > 800.0 && sim.position[2] < 825.0,
            "expected smooth brake just past z=800, got {}",
            sim.position[2]
        );
        // Zero Z drift between frame 300 and frame 10,000 once state 6 brakes to 0!
        assert_eq!(sim.position[2].to_bits(), z_at_300.to_bits());
        // Exact 201-frame orbit periodicity on RedFlare outer disc centres (`flare_pos[1..7]`)
        // and 32-frame periodicity on innermost `FlareThing` disc (`flare_pos[0]`)!
        assert!((flare_pos1_at_400[0] - flare_pos1_at_601[0]).abs() < 1e-4);
        assert!((flare_pos1_at_400[1] - flare_pos1_at_601[1]).abs() < 1e-4);
        assert!((flare_pos0_at_400[0] - flare_pos0_at_432[0]).abs() < 1e-4);
        assert!((flare_pos0_at_400[1] - flare_pos0_at_432[1]).abs() < 1e-4);
        // All 128 falling red fog patches remain within respawn height band `[position.z - 5, position.z + 810]`
        let min_fog_z = sim.position[2] - 5.0;
        let max_fog_z = sim.position[2] + 810.0;
        for p in sim.illegal_fog_pos.iter() {
            assert!(
                (min_fog_z..=max_fog_z).contains(&p[2]),
                "illegal fog patch z out of respawn band [{min_fog_z}, {max_fog_z}]: {}",
                p[2]
            );
        }
        assert_eq!(sim.opening_state, 6);
        assert_eq!(sim.position_speed[2], 0.0);
        assert_eq!(render.illegal_text_alpha, 112);
        assert!(!render.flare_fan_vertices.is_empty());
        assert!(!render.cubes.is_empty());
        assert!(!sim.scene_finished);
        assert!(audio.contains(&AudioCue::StartIllegalWarningDrone));
        assert!(sim.language.illegal_disc_message().contains("compat"));
    }
}
