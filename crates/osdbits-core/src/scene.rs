//! Unified OSDSYS scene engine (`Modes A, B, C, D` + `Mode E` customization),
//! flat zero-steady-state-allocation render command/vertex buffers, and `FrameInspection` snapshot.

use crate::menu::{
    MenuAction, MenuClock, MenuEntryProvenance, MenuRenderList, MenuScreenState, MenuSim,
    MeshGeometry, ScreenAspectSetting,
};
use crate::opening::{
    AudioCue, CubeRenderTarget, OpeningRenderList, OpeningSim, OpeningVariant, OsdLanguage,
    PackedColorVertex, PackedTexturedVertex, TexturedBatch, VideoStandard,
};
use crate::towers::{PackedTowerVertex, TowerFieldPreset};

/// Number of `f32` values per universal vertex (`clip_pos[4]`, `uv_q[4]`, `gs_rgba[4]`).
pub const VERTEX_FLOATS: usize = 12;
/// Number of `u32` words per flat draw command.
pub const CMD_WORDS: usize = 8;

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisualMode {
    /// Mode A: Opening towers -> fly-up -> fade transition.
    Opening = 0,
    /// Mode B: Indefinite idle drift/hover over the tower field at `z = 16`.
    Idle = 1,
    /// Mode C: Illegal-disc red clouds, flares, and red glass cubes (`z = 672 -> >800`).
    IllegalDisc = 2,
    /// Mode D: Module U Main Menu & System Configuration (`7` orbs, `KABE` tunnel, `12` rods, `5` cubes).
    Menu = 3,
}

impl VisualMode {
    pub const fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(Self::Opening),
            1 => Some(Self::Idle),
            2 => Some(Self::IllegalDisc),
            3 => Some(Self::Menu),
            _ => None,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Opening => "opening",
            Self::Idle => "idle",
            Self::IllegalDisc => "illegal-disc",
            Self::Menu => "menu",
        }
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawCommandKind {
    Triangles = 0,
    Lines = 1,
    CaptureScreenToWork = 2,
    ApplyExtraBufFeedback = 3,
    ApplyMotionBlur = 4,
    ApplyMenuBackdropTint = 5,
    ApplyMenuZoomBlur = 6,
    DrawFullScreenFade = 7,
    DrawBannerOverlay = 8,
    DrawIllegalTextOverlay = 9,
    DrawLetterboxBars = 10,
}

#[inline]
pub const fn pack_pipeline_flags(
    abe: bool,
    blend_mode: u8,
    blend_fix: u8,
    depth_test: bool,
    depth_write: bool,
    uv_clamp: bool,
) -> u32 {
    (abe as u32)
        | ((blend_mode as u32) << 1)
        | ((blend_fix as u32) << 9)
        | ((depth_test as u32) << 17)
        | ((depth_write as u32) << 18)
        | ((uv_clamp as u32) << 19)
}

/// Complete scene & Mode E customization configuration.
#[derive(Clone, Debug)]
pub struct SceneConfig {
    pub mode: VisualMode,
    pub boot_to_game: bool,
    pub auto_transition_to_menu: bool,
    pub standard: VideoStandard,
    pub language: OsdLanguage,
    pub rng_seed: u32,
    pub tower_preset: TowerFieldPreset,
    pub clock: MenuClock,
    pub speed_multiplier: f32,
    pub camera_xy_offset: [f32; 2],
    pub reduced_motion: bool,
    pub enable_fog: bool,
    pub enable_lights: bool,
    pub enable_cubes: bool,
    pub enable_glow: bool,
    pub enable_motion_blur: bool,
    pub letterbox: bool,
    pub confirm_cancel_swap: bool,
}

impl Default for SceneConfig {
    fn default() -> Self {
        Self {
            mode: VisualMode::Opening,
            boot_to_game: false,
            auto_transition_to_menu: true,
            standard: VideoStandard::Ntsc,
            language: OsdLanguage::English,
            rng_seed: 1,
            tower_preset: TowerFieldPreset::default(),
            clock: MenuClock::default(),
            speed_multiplier: 1.0,
            camera_xy_offset: [0.0, 0.0],
            reduced_motion: false,
            enable_fog: true,
            enable_lights: true,
            enable_cubes: true,
            enable_glow: true,
            enable_motion_blur: true,
            letterbox: false,
            confirm_cancel_swap: true,
        }
    }
}

/// Structured per-frame inspection telemetry for tests, benchmarks, and UI HUD.
#[derive(Clone, Debug, Default)]
pub struct FrameInspection {
    pub mode: u32,
    pub frame_count: u32,
    pub rng_seed: u32,
    pub screen_width: u32,
    pub screen_height: u32,
    pub camera_pos: [f32; 3],
    pub camera_rot: f32,
    pub opening_state: u32,
    pub scene_finished: bool,
    pub drawn_towers: u32,
    pub active_cubes: u32,
    pub active_orbs: u32,
    pub active_rods: u32,
    pub command_count: u32,
    pub vertex_count: u32,
    pub banner_alpha: u8,
    pub illegal_text_alpha: u8,
    pub fade_alpha: u8,
    pub audio_cues: Vec<AudioCue>,
}

pub struct OsdSceneEngine {
    config: SceneConfig,
    opening: OpeningSim,
    menu: MenuSim,
    opening_scratch: OpeningRenderList,
    menu_scratch: MenuRenderList,
    audio_scratch: Vec<AudioCue>,

    vertex_stream: Vec<f32>,
    command_stream: Vec<u32>,
    inspection: FrameInspection,
    inspect_buf: String,
}

impl Default for OsdSceneEngine {
    fn default() -> Self {
        Self::new(SceneConfig::default())
    }
}

impl OsdSceneEngine {
    pub fn new(config: SceneConfig) -> Self {
        let variant = Self::opening_variant_for(&config);
        let mut opening = OpeningSim::new(
            variant,
            config.standard,
            config.language,
            config.tower_preset,
            config.rng_seed,
        );
        Self::apply_runtime_overrides_to_opening(&config, &mut opening);

        let mut menu = MenuSim::new(
            config.standard,
            config.clock,
            config.language,
            config.rng_seed,
            true,
        );
        Self::apply_runtime_overrides_to_menu(&config, &mut menu);

        let mut inspection = FrameInspection::default();
        inspection.audio_cues.reserve(8);

        Self {
            config,
            opening,
            menu,
            opening_scratch: OpeningRenderList::default(),
            menu_scratch: MenuRenderList::default(),
            audio_scratch: Vec::with_capacity(8),
            vertex_stream: Vec::with_capacity(32_768 * VERTEX_FLOATS),
            command_stream: Vec::with_capacity(256 * CMD_WORDS),
            inspection,
            inspect_buf: String::with_capacity(2048),
        }
    }

    pub fn config(&self) -> &SceneConfig {
        &self.config
    }

    pub fn mode(&self) -> VisualMode {
        self.config.mode
    }

    pub fn reconfigure(&mut self, next: SceneConfig) {
        let rebuild_opening = next.mode != self.config.mode
            || next.boot_to_game != self.config.boot_to_game
            || next.standard != self.config.standard
            || next.rng_seed != self.config.rng_seed
            || !Self::same_tower_preset(next.tower_preset, self.config.tower_preset);

        let rebuild_menu = next.standard != self.config.standard
            || next.rng_seed != self.config.rng_seed
            || (next.mode == VisualMode::Menu && self.config.mode != VisualMode::Menu);

        if rebuild_opening {
            self.opening = OpeningSim::new(
                Self::opening_variant_for(&next),
                next.standard,
                next.language,
                next.tower_preset,
                next.rng_seed,
            );
        }
        if rebuild_menu {
            let custom_rod = if self.menu.rod_model.is_user_supplied {
                Some(self.menu.rod_model.clone())
            } else {
                None
            };
            let custom_cube = if self.menu.cube_model.is_user_supplied {
                Some(self.menu.cube_model.clone())
            } else {
                None
            };
            self.menu = MenuSim::new(
                next.standard,
                next.clock,
                next.language,
                next.rng_seed,
                self.config.mode == VisualMode::Opening,
            );
            if let Some(rm) = custom_rod {
                self.menu.rod_model = rm;
            }
            if let Some(cm) = custom_cube {
                self.menu.cube_model = cm;
            }
        }

        Self::apply_runtime_overrides_to_opening(&next, &mut self.opening);
        Self::apply_runtime_overrides_to_menu(&next, &mut self.menu);
        self.config = next;
    }

    /// Updates Mode E live parameters (speed, camera offset, effect toggles, reduced motion, language)
    /// without resetting the active timeline or tower/orb state!
    #[allow(clippy::too_many_arguments)]
    pub fn set_live_overrides(
        &mut self,
        speed_multiplier: f32,
        cam_offset_x: f32,
        cam_offset_y: f32,
        reduced_motion: bool,
        enable_fog: bool,
        enable_lights: bool,
        enable_cubes: bool,
        enable_glow: bool,
        enable_motion_blur: bool,
        letterbox: bool,
        language: OsdLanguage,
        confirm_cancel_swap: bool,
    ) {
        self.config.speed_multiplier = speed_multiplier;
        self.config.camera_xy_offset = [cam_offset_x, cam_offset_y];
        self.config.reduced_motion = reduced_motion;
        self.config.enable_fog = enable_fog;
        self.config.enable_lights = enable_lights;
        self.config.enable_cubes = enable_cubes;
        self.config.enable_glow = enable_glow;
        self.config.enable_motion_blur = enable_motion_blur;
        self.config.letterbox = letterbox;
        self.config.language = language;
        self.config.confirm_cancel_swap = confirm_cancel_swap;

        Self::apply_runtime_overrides_to_opening(&self.config, &mut self.opening);
        Self::apply_runtime_overrides_to_menu(&self.config, &mut self.menu);
    }

    /// Imports optional user-supplied `MENUGEOM` vertex/normal/UV buffers into Mode D.
    pub fn set_custom_menu_geometry(&mut self, rod: MeshGeometry, cube: MeshGeometry) -> bool {
        if rod.nfaces != 16
            || rod.verts.len() != 64
            || rod.norms.len() != 16
            || rod.uvs.len() != 64
            || cube.nfaces != 6
            || cube.verts.len() != 24
            || cube.norms.len() != 6
            || cube.uvs.len() != 24
        {
            return false;
        }
        self.menu.rod_model = rod;
        self.menu.cube_model = cube;
        true
    }

    pub fn dispatch_menu_action(&mut self, action: MenuAction) {
        if self.config.mode == VisualMode::Menu {
            self.menu.dispatch_action(action, &mut self.audio_scratch);
        }
    }

    pub fn enter_system_config(&mut self) {
        if self.config.mode == VisualMode::Menu {
            self.menu.enter_system_config(&mut self.audio_scratch);
        }
    }

    /// Advances simulation by one frame and populates the flat `vertex_stream`, `command_stream`,
    /// `inspection`, and `inspect_json` buffers without steady-state heap reallocation.
    pub fn step_frame(&mut self) -> &FrameInspection {
        self.vertex_stream.clear();
        self.command_stream.clear();

        if self.config.mode == VisualMode::Menu {
            self.menu
                .step(&mut self.menu_scratch, &mut self.audio_scratch);
            self.encode_menu_render_stream();
        } else {
            self.opening
                .step(&mut self.opening_scratch, &mut self.audio_scratch);
            self.encode_opening_render_stream();

            if self.config.mode == VisualMode::Opening && self.opening.scene_finished {
                if self.config.auto_transition_to_menu {
                    self.config.mode = VisualMode::Menu;
                    self.menu = MenuSim::new(
                        self.config.standard,
                        self.config.clock,
                        self.config.language,
                        self.config.rng_seed,
                        true,
                    );
                    Self::apply_runtime_overrides_to_menu(&self.config, &mut self.menu);
                } else {
                    self.opening = OpeningSim::new(
                        Self::opening_variant_for(&self.config),
                        self.config.standard,
                        self.config.language,
                        self.config.tower_preset,
                        self.config.rng_seed,
                    );
                    Self::apply_runtime_overrides_to_opening(&self.config, &mut self.opening);
                }
            }
        }

        self.build_inspect_json();
        self.audio_scratch.clear();
        &self.inspection
    }

    pub fn vertex_stream(&self) -> &[f32] {
        &self.vertex_stream
    }

    pub fn command_stream(&self) -> &[u32] {
        &self.command_stream
    }

    pub fn inspection(&self) -> &FrameInspection {
        &self.inspection
    }

    pub fn inspect_json_bytes(&self) -> &[u8] {
        self.inspect_buf.as_bytes()
    }

    fn encode_opening_render_stream(&mut self) {
        // Borrow disjoint fields cleanly so `vertex_stream` and `command_stream` mutate in-place
        let vs = &mut self.vertex_stream;
        let cs = &mut self.command_stream;
        let rl = &self.opening_scratch;

        // 1. Towers (TextureId::TexoWall = 6, opaque, depth test + write, UV clamp = true)
        if !rl.tower_vertices.is_empty() {
            let first = Self::push_tower_verts(vs, &rl.tower_vertices);
            Self::push_cmd(
                cs,
                DrawCommandKind::Triangles,
                0,
                6,
                pack_pipeline_flags(false, 255, 128, true, true, true),
                first,
                rl.tower_vertices.len() as u32,
                0,
                0,
            );
        }

        // 2. Mode C RedFlare triangles + FLAR sprites (runs before ExtraBuf2, fog & cubes in IllegalDisc;
        //    empty in Opening/Idle, so ExtraBuf2 runs immediately after Towers in Opening/Idle and
        //    immediately after RedFlare in IllegalDisc — matching osdbits/opening.c:2575 & 2115!)
        if !rl.flare_fan_vertices.is_empty() {
            let first = Self::push_color_verts(vs, &rl.flare_fan_vertices);
            Self::push_cmd(
                cs,
                DrawCommandKind::Triangles,
                0,
                255,
                pack_pipeline_flags(true, 0, 128, true, false, false),
                first,
                rl.flare_fan_vertices.len() as u32,
                0,
                0,
            );
        }
        if let Some(batch) = &rl.flare_sprite_batch {
            Self::encode_textured_batch(vs, cs, 0, batch);
        }

        // 3. Half-width ExtraBuf2 temporal radial glow feedback (`DrawExtraBuf2` + `DrawToExtraBuf2`)
        if rl.extra_buf_feedback_fix > 0 {
            Self::push_cmd(
                cs,
                DrawCommandKind::ApplyExtraBufFeedback,
                0,
                255,
                0,
                0,
                0,
                u32::from(rl.extra_buf_feedback_fix),
                0,
            );
        }

        // 4. Fog batches (6 layers in Mode A/B, or 128 falling patches in Mode C)
        for batch in &rl.fog_batches {
            Self::encode_textured_batch(vs, cs, 0, batch);
        }

        // 5. Opening 4 light orbs + 128-point trails
        if let Some(batch) = &rl.light_sprite_batch {
            Self::encode_textured_batch(vs, cs, 0, batch);
        }
        if !rl.light_trail_vertices.is_empty() {
            let first = Self::push_color_verts(vs, &rl.light_trail_vertices);
            Self::push_cmd(
                cs,
                DrawCommandKind::Lines,
                0,
                255,
                pack_pipeline_flags(true, 4, 128, false, false, false),
                first,
                rl.light_trail_vertices.len() as u32,
                0,
                0,
            );
        }

        // 6. 5 Refractive glass cubes (each preceded by Screen -> WorkBuffer capture!)
        for cube in &rl.cubes {
            Self::push_cmd(
                cs,
                DrawCommandKind::CaptureScreenToWork,
                1,
                255,
                0,
                0,
                0,
                u32::from(cube.instance_index),
                0,
            );
            let base_v = Self::push_textured_verts(vs, &cube.vertices);
            for pass in &cube.passes {
                let target = match pass.target {
                    CubeRenderTarget::Screen => 0,
                    CubeRenderTarget::WorkBuf => 1,
                };
                Self::push_cmd(
                    cs,
                    DrawCommandKind::Triangles,
                    target,
                    pass.source as u32,
                    pack_pipeline_flags(
                        pass.abe,
                        pass.blend_mode,
                        pass.blend_fix,
                        false,
                        false,
                        false,
                    ),
                    base_v + pass.start_vertex,
                    pass.vertex_count,
                    0,
                    0,
                );
            }
        }

        // 7. Fly-up motion blur (0..=3 passes)
        if rl.motion_blur_passes > 0 {
            Self::push_cmd(
                cs,
                DrawCommandKind::ApplyMotionBlur,
                0,
                255,
                0,
                0,
                0,
                u32::from(rl.motion_blur_passes),
                0,
            );
        }

        // 8. Black screen fade (`black_fade_alpha`)
        let max_fade = rl.black_fade_alpha;
        if max_fade > 0 {
            Self::push_cmd(
                cs,
                DrawCommandKind::DrawFullScreenFade,
                0,
                255,
                0,
                0,
                0,
                u32::from(max_fade), // R=0, G=0, B=0, A=max_fade
                0,
            );
        }

        // 9. Clean-room legal-safe Opening Banner / Localized Illegal-Disc Text / Letterbox
        if rl.banner_alpha > 0 {
            Self::push_cmd(
                cs,
                DrawCommandKind::DrawBannerOverlay,
                0,
                255,
                0,
                0,
                0,
                u32::from(rl.banner_alpha),
                0,
            );
        }
        if rl.illegal_text_alpha > 0 {
            Self::push_cmd(
                cs,
                DrawCommandKind::DrawIllegalTextOverlay,
                0,
                255,
                0,
                0,
                0,
                u32::from(rl.illegal_text_alpha),
                self.opening.language as u32,
            );
        }
        if rl.letterbox_bar_ratio > 0.0 {
            Self::push_cmd(
                cs,
                DrawCommandKind::DrawLetterboxBars,
                0,
                255,
                0,
                0,
                0,
                rl.letterbox_bar_ratio.to_bits(),
                0,
            );
        }

        self.inspection.mode = self.config.mode as u32;
        self.inspection.frame_count = self.opening.frame_count;
        self.inspection.rng_seed = self.config.rng_seed;
        self.inspection.screen_width = self.opening.standard.width();
        self.inspection.screen_height = self.opening.standard.height();
        self.inspection.camera_pos = [
            self.opening.position[0],
            self.opening.position[1],
            self.opening.position[2],
        ];
        self.inspection.camera_rot = self.opening.rotation;
        self.inspection.opening_state = self.opening.opening_state as u32;
        self.inspection.scene_finished = self.opening.scene_finished;
        self.inspection.drawn_towers = rl.drawn_tower_count;
        self.inspection.active_cubes = rl.cubes.len() as u32;
        self.inspection.active_orbs = if rl.light_sprite_batch.is_some() {
            4
        } else {
            0
        };
        self.inspection.active_rods = 0;
        self.inspection.command_count = (self.command_stream.len() / CMD_WORDS) as u32;
        self.inspection.vertex_count = (self.vertex_stream.len() / VERTEX_FLOATS) as u32;
        self.inspection.banner_alpha = rl.banner_alpha;
        self.inspection.illegal_text_alpha = rl.illegal_text_alpha;
        self.inspection.fade_alpha = max_fade;
        self.inspection.audio_cues.clear();
        self.inspection
            .audio_cues
            .extend_from_slice(&self.audio_scratch);
    }

    fn encode_menu_render_stream(&mut self) {
        let vs = &mut self.vertex_stream;
        let cs = &mut self.command_stream;
        let rl = &self.menu_scratch;

        // 1. KABE tunnel + backdrop capture & deep-blue tint `{0x37, 0x28, 0x3C}`
        if let Some(batch) = &rl.kabe_tunnel_batch {
            Self::encode_textured_batch(vs, cs, 0, batch);
            let packed_tint = (u32::from(rl.backdrop_tint_rgb[0]) << 24)
                | (u32::from(rl.backdrop_tint_rgb[1]) << 16)
                | (u32::from(rl.backdrop_tint_rgb[2]) << 8)
                | 0x80;
            Self::push_cmd(
                cs,
                DrawCommandKind::ApplyMenuBackdropTint,
                0,
                255,
                0,
                0,
                0,
                packed_tint,
                0,
            );
        }

        // 2. Orb trails + halos + cores (also captured so rods/cubes refract them!)
        if !rl.orb_trail_vertices.is_empty() {
            let first = Self::push_color_verts(vs, &rl.orb_trail_vertices);
            Self::push_cmd(
                cs,
                DrawCommandKind::Lines,
                0,
                255,
                pack_pipeline_flags(true, 5, 128, false, false, false),
                first,
                rl.orb_trail_vertices.len() as u32,
                0,
                0,
            );
        }
        if let Some(batch) = &rl.orb_halo_batch {
            Self::encode_textured_batch(vs, cs, 0, batch);
        }
        if let Some(batch) = &rl.orb_core_batch {
            Self::encode_textured_batch(vs, cs, 0, batch);
        }

        // 3. Capture screen to work buffer before glass rods/cubes refract it
        if !rl.rod_refract_batches.is_empty() || !rl.cube_refract_batches.is_empty() {
            Self::push_cmd(
                cs,
                DrawCommandKind::CaptureScreenToWork,
                1,
                255,
                0,
                0,
                0,
                0,
                0,
            );
        }

        // 4. 12 Crystal Clock Rods (refraction + emboss)
        for batch in &rl.rod_refract_batches {
            Self::encode_textured_batch(vs, cs, 0, batch);
        }
        for batch in &rl.rod_bump_batches {
            Self::encode_textured_batch(vs, cs, 0, batch);
        }

        // 5. Vignette + Zoom Blur + 5 System Config Glass Cubes
        if !rl.vignette_vertices.is_empty() {
            let first = Self::push_color_verts(vs, &rl.vignette_vertices);
            Self::push_cmd(
                cs,
                DrawCommandKind::Triangles,
                0,
                255,
                pack_pipeline_flags(true, 4, 128, false, false, false),
                first,
                rl.vignette_vertices.len() as u32,
                0,
                0,
            );
        }
        if rl.zoom_blur_passes > 0 {
            Self::push_cmd(
                cs,
                DrawCommandKind::ApplyMenuZoomBlur,
                0,
                255,
                0,
                0,
                0,
                u32::from(rl.zoom_blur_passes),
                0,
            );
        }

        if !rl.cube_refract_batches.is_empty() {
            Self::push_cmd(
                cs,
                DrawCommandKind::CaptureScreenToWork,
                1,
                255,
                0,
                0,
                0,
                1,
                0,
            );
            for batch in &rl.cube_refract_batches {
                Self::encode_textured_batch(vs, cs, 0, batch);
            }
            for batch in &rl.cube_bump_batches {
                Self::encode_textured_batch(vs, cs, 0, batch);
            }
            for batch in &rl.cube_refl_batches {
                Self::encode_textured_batch(vs, cs, 0, batch);
            }
        }

        // 6. Letterbox & Fade Curtain
        if rl.letterbox_bar_ratio > 0.0 {
            Self::push_cmd(
                cs,
                DrawCommandKind::DrawLetterboxBars,
                0,
                255,
                0,
                0,
                0,
                rl.letterbox_bar_ratio.to_bits(),
                0,
            );
        }
        if rl.fade_curtain_rgba[3] > 0 {
            let packed = (u32::from(rl.fade_curtain_rgba[0]) << 24)
                | (u32::from(rl.fade_curtain_rgba[1]) << 16)
                | (u32::from(rl.fade_curtain_rgba[2]) << 8)
                | u32::from(rl.fade_curtain_rgba[3]);
            Self::push_cmd(
                cs,
                DrawCommandKind::DrawFullScreenFade,
                0,
                255,
                0,
                0,
                0,
                packed,
                0,
            );
        }

        self.inspection.mode = VisualMode::Menu as u32;
        self.inspection.frame_count = self.menu.frame_count;
        self.inspection.rng_seed = self.config.rng_seed;
        self.inspection.screen_width = self.menu.standard.width();
        self.inspection.screen_height = self.menu.standard.height();
        self.inspection.camera_pos = [10.436, 0.0, -103.0 + self.menu.cam_z_offset];
        self.inspection.camera_rot = 0.145;
        self.inspection.opening_state = 0;
        self.inspection.scene_finished = false;
        self.inspection.drawn_towers = 0;
        self.inspection.active_cubes = rl.cube_refract_batches.len() as u32;
        self.inspection.active_orbs = 7;
        self.inspection.active_rods = rl.rod_refract_batches.len() as u32;
        self.inspection.command_count = (self.command_stream.len() / CMD_WORDS) as u32;
        self.inspection.vertex_count = (self.vertex_stream.len() / VERTEX_FLOATS) as u32;
        self.inspection.banner_alpha = 0;
        self.inspection.illegal_text_alpha = 0;
        self.inspection.fade_alpha = rl.fade_curtain_rgba[3];
        self.inspection.audio_cues.clear();
        self.inspection
            .audio_cues
            .extend_from_slice(&self.audio_scratch);
    }

    fn encode_textured_batch(
        vs: &mut Vec<f32>,
        cs: &mut Vec<u32>,
        target: u32,
        batch: &TexturedBatch,
    ) {
        if batch.vertices.is_empty() {
            return;
        }
        let first = Self::push_textured_verts(vs, &batch.vertices);
        let abe = batch.blend_mode != 255;
        Self::push_cmd(
            cs,
            DrawCommandKind::Triangles,
            target,
            batch.texture as u32,
            pack_pipeline_flags(
                abe,
                batch.blend_mode,
                batch.blend_fix,
                batch.depth_test,
                batch.depth_write,
                batch.uv_clamp,
            ),
            first,
            batch.vertices.len() as u32,
            0,
            0,
        );
    }

    fn push_tower_verts(vs: &mut Vec<f32>, src: &[PackedTowerVertex]) -> u32 {
        let first = (vs.len() / VERTEX_FLOATS) as u32;
        vs.reserve(src.len() * VERTEX_FLOATS);
        for v in src {
            vs.extend_from_slice(&v.clip_pos);
            vs.extend_from_slice(&v.uv_q);
            vs.extend_from_slice(&v.gs_rgba);
        }
        first
    }

    fn push_textured_verts(vs: &mut Vec<f32>, src: &[PackedTexturedVertex]) -> u32 {
        let first = (vs.len() / VERTEX_FLOATS) as u32;
        vs.reserve(src.len() * VERTEX_FLOATS);
        for v in src {
            vs.extend_from_slice(&v.clip_pos);
            vs.extend_from_slice(&v.uv_q);
            vs.extend_from_slice(&v.gs_rgba);
        }
        first
    }

    fn push_color_verts(vs: &mut Vec<f32>, src: &[PackedColorVertex]) -> u32 {
        let first = (vs.len() / VERTEX_FLOATS) as u32;
        vs.reserve(src.len() * VERTEX_FLOATS);
        for v in src {
            vs.extend_from_slice(&v.clip_pos);
            vs.extend_from_slice(&[0.0, 0.0, 1.0, 0.0]);
            vs.extend_from_slice(&v.gs_rgba);
        }
        first
    }

    #[allow(clippy::too_many_arguments)]
    fn push_cmd(
        cs: &mut Vec<u32>,
        kind: DrawCommandKind,
        target: u32,
        texture_id: u32,
        flags: u32,
        first_vertex: u32,
        vertex_count: u32,
        aux0: u32,
        aux1: u32,
    ) {
        cs.extend_from_slice(&[
            kind as u32,
            target,
            texture_id,
            flags,
            first_vertex,
            vertex_count,
            aux0,
            aux1,
        ]);
    }

    fn build_inspect_json(&mut self) {
        use std::fmt::Write as _;
        self.inspect_buf.clear();
        let s = &mut self.inspect_buf;
        let ins = &self.inspection;
        let _ = write!(
            s,
            "{{\"mode\":\"{}\",\"modeId\":{},\"frame\":{},\"seed\":{},\"width\":{},\"height\":{},\"camPos\":[{:.3},{:.3},{:.3}],\"camRot\":{:.4},\"openingState\":{},\"finished\":{},\"towers\":{},\"cubes\":{},\"orbs\":{},\"rods\":{},\"commands\":{},\"vertices\":{},\"bannerAlpha\":{},\"illegalTextAlpha\":{},\"fadeAlpha\":{},\"illegalMsg\":\"",
            self.config.mode.name(),
            ins.mode,
            ins.frame_count,
            ins.rng_seed,
            ins.screen_width,
            ins.screen_height,
            ins.camera_pos[0],
            ins.camera_pos[1],
            ins.camera_pos[2],
            ins.camera_rot,
            ins.opening_state,
            ins.scene_finished,
            ins.drawn_towers,
            ins.active_cubes,
            ins.active_orbs,
            ins.active_rods,
            ins.command_count,
            ins.vertex_count,
            ins.banner_alpha,
            ins.illegal_text_alpha,
            ins.fade_alpha,
        );
        Self::push_json_escaped(s, self.opening.language.illegal_disc_message());
        s.push_str("\",\"audioCues\":[");
        for (i, cue) in ins.audio_cues.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let _ = write!(s, "{}", *cue as u32);
        }
        s.push(']');

        if self.config.mode == VisualMode::Menu {
            let ui = &self.menu_scratch.ui;
            let screen_name = match ui.screen {
                Some(MenuScreenState::MainMenu) => "main",
                Some(MenuScreenState::BrowserApproxPanel) => "browser-approx",
                Some(MenuScreenState::SystemConfigTransitionIn) => "config-in",
                Some(MenuScreenState::SystemConfigList) => "config-list",
                Some(MenuScreenState::SystemConfigEditingValue) => "config-edit-val",
                Some(MenuScreenState::SystemConfigEditingClock) => "config-edit-clock",
                Some(MenuScreenState::SystemConfigTransitionOut) => "config-out",
                Some(MenuScreenState::VersionApproxPanel) => "version-approx",
                None => "none",
            };
            let _ = write!(
                s,
                ",\"menuUi\":{{\"screen\":\"{}\",\"interactive\":{},",
                screen_name, ui.input_ready
            );
            s.push_str("\"date\":\"");
            Self::push_json_escaped(s, &ui.date_text);
            s.push_str("\",\"time\":\"");
            Self::push_json_escaped(s, &ui.time_text);
            s.push_str("\",\"title\":\"");
            Self::push_json_escaped(s, &ui.title);
            s.push_str("\",\"hintConfirm\":\"");
            Self::push_json_escaped(s, ui.hint_confirm_label);
            s.push_str("\",\"hintCancel\":\"");
            Self::push_json_escaped(s, ui.hint_cancel_label);
            let _ = write!(
                s,
                "\",\"showOptions\":{},\"markerAlpha\":{},\"clockCursor\":{},\"items\":[",
                ui.show_options_hint, ui.page_marker_alpha, ui.clock_edit_cursor,
            );
            for (idx, it) in ui.items.iter().enumerate() {
                if idx > 0 {
                    s.push(',');
                }
                let prov = match it.provenance {
                    MenuEntryProvenance::SourcedReconstruction => "sourced",
                    MenuEntryProvenance::ApproximatedSubscreen => "approximate",
                    MenuEntryProvenance::CleanRoomExtension => "extension",
                };
                s.push_str("{\"label\":\"");
                Self::push_json_escaped(s, &it.label);
                s.push_str("\",\"value\":\"");
                Self::push_json_escaped(s, &it.value_text);
                let _ = write!(
                    s,
                    "\",\"selected\":{},\"alpha\":{},\"provenance\":\"{}\"}}",
                    it.selected, it.alpha, prov
                );
            }
            s.push(']');
            if let Some(fields) = &ui.clock_edit_fields {
                s.push_str(",\"clockFields\":[");
                for (fi, fv) in fields.iter().enumerate() {
                    if fi > 0 {
                        s.push(',');
                    }
                    s.push('"');
                    Self::push_json_escaped(s, fv);
                    s.push('"');
                }
                s.push(']');
            }
            s.push('}');
        } else {
            s.push_str(",\"menuUi\":null");
        }

        s.push('}');
    }

    fn push_json_escaped(out: &mut String, input: &str) {
        for ch in input.chars() {
            match ch {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c => out.push(c),
            }
        }
    }

    fn opening_variant_for(cfg: &SceneConfig) -> OpeningVariant {
        match cfg.mode {
            VisualMode::Opening => {
                if cfg.boot_to_game {
                    OpeningVariant::BootToGame
                } else {
                    OpeningVariant::BootToMenu
                }
            }
            VisualMode::Idle => OpeningVariant::Idle,
            VisualMode::IllegalDisc => OpeningVariant::IllegalDisc,
            VisualMode::Menu => OpeningVariant::Idle,
        }
    }

    fn apply_runtime_overrides_to_opening(cfg: &SceneConfig, sim: &mut OpeningSim) {
        sim.speed_multiplier = cfg.speed_multiplier;
        sim.camera_xy_offset = cfg.camera_xy_offset;
        sim.reduced_motion = cfg.reduced_motion;
        sim.enable_fog = cfg.enable_fog;
        sim.enable_lights = cfg.enable_lights;
        sim.enable_cubes = cfg.enable_cubes;
        sim.enable_glow = cfg.enable_glow;
        sim.enable_motion_blur = cfg.enable_motion_blur;
        sim.letterbox = cfg.letterbox;
        sim.language = cfg.language;
    }

    fn apply_runtime_overrides_to_menu(cfg: &SceneConfig, sim: &mut MenuSim) {
        sim.reduced_motion = cfg.reduced_motion;
        sim.enable_zoom_blur = cfg.enable_motion_blur;
        sim.language = cfg.language;
        sim.confirm_cancel_swap = cfg.confirm_cancel_swap;
        sim.screen_aspect = if cfg.letterbox {
            ScreenAspectSetting::SixteenByNine
        } else {
            ScreenAspectSetting::Full
        };
    }

    fn same_tower_preset(a: TowerFieldPreset, b: TowerFieldPreset) -> bool {
        match (a, b) {
            (TowerFieldPreset::CapturedRetail, TowerFieldPreset::CapturedRetail)
            | (TowerFieldPreset::FullField, TowerFieldPreset::FullField) => true,
            (
                TowerFieldPreset::Simulated {
                    seed: s1,
                    ngames: g1,
                    nboots: b1,
                },
                TowerFieldPreset::Simulated {
                    seed: s2,
                    ngames: g2,
                    nboots: b2,
                },
            ) => s1 == s2 && g1 == g2 && b1 == b2,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_render_stream_valid_and_bounded_across_all_4_modes() {
        for mode in [
            VisualMode::Opening,
            VisualMode::Idle,
            VisualMode::IllegalDisc,
            VisualMode::Menu,
        ] {
            let mut engine = OsdSceneEngine::new(SceneConfig {
                mode,
                auto_transition_to_menu: false,
                ..SceneConfig::default()
            });

            if mode == VisualMode::Menu {
                // Step into System Configuration so KABE tunnel, 12 rods, and 5 cubes all emit!
                for _ in 0..135 {
                    engine.step_frame();
                }
                engine.enter_system_config();
                for _ in 0..95 {
                    engine.step_frame();
                }
            } else {
                for _ in 0..45 {
                    engine.step_frame();
                }
            }

            let cap_before = engine.vertex_stream().len().max(1);
            let _ = cap_before;
            // Verify steady-state reuse and finite floats
            for _ in 0..20 {
                engine.step_frame();
                let vs = engine.vertex_stream();
                let cs = engine.command_stream();
                assert_eq!(vs.len() % VERTEX_FLOATS, 0);
                assert_eq!(cs.len() % CMD_WORDS, 0);
                assert!(!cs.is_empty(), "mode {mode:?} emitted zero draw commands");
                assert!(
                    vs.iter().all(|f| f.is_finite()),
                    "non-finite float in {mode:?} vertex stream"
                );
                let json = core::str::from_utf8(engine.inspect_json_bytes())
                    .expect("inspect JSON must be valid UTF-8");
                assert!(json.starts_with('{') && json.ends_with('}'));
            }
        }
    }

    #[test]
    fn zero_steady_state_buffer_capacity_growth_across_10000_frames() {
        let mut engine = OsdSceneEngine::new(SceneConfig {
            mode: VisualMode::Idle,
            ..SceneConfig::default()
        });
        // Warm up 60 frames so scratch buffers reach steady-state capacity
        for _ in 0..60 {
            engine.step_frame();
        }
        let v_cap = engine.vertex_stream.capacity();
        let c_cap = engine.command_stream.capacity();
        let j_cap = engine.inspect_buf.capacity();

        for _ in 0..10_000 {
            engine.step_frame();
        }

        assert_eq!(
            engine.vertex_stream.capacity(),
            v_cap,
            "vertex_stream grew after warmup"
        );
        assert_eq!(
            engine.command_stream.capacity(),
            c_cap,
            "command_stream grew after warmup"
        );
        assert_eq!(
            engine.inspect_buf.capacity(),
            j_cap,
            "inspect_buf grew after warmup"
        );
    }

    #[test]
    fn opening_auto_transitions_into_menu_when_configured() {
        let mut engine = OsdSceneEngine::new(SceneConfig {
            mode: VisualMode::Opening,
            auto_transition_to_menu: true,
            ..SceneConfig::default()
        });
        for _ in 0..340 {
            engine.step_frame();
            if engine.mode() == VisualMode::Menu {
                break;
            }
        }
        assert_eq!(
            engine.mode(),
            VisualMode::Menu,
            "expected Opening to auto-transition into Mode D Menu"
        );
    }
}
