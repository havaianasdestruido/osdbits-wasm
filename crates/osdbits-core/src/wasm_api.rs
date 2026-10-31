//! Unified `Engine` controller and C-ABI WASM exports (`P02-006`).

use crate::bios_import::{
    ImportError, build_synthetic_clean_teximage, validate_and_extract_import,
};
use crate::menu::{MenuAction, MenuPage, MenuScene, MenuTuning, RtcClock};
use crate::opening::{OpeningScene, OpeningTuning, OpeningVariant};
use crate::procedural_assets::{RgbaTexture, generate_procedural_textures};
use crate::render_list::{RenderBatch, RenderList, RenderVertex};

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppMode {
    Opening = 0,
    Idle = 1,
    Illegal = 2,
    Menu = 3,
}

impl AppMode {
    #[must_use]
    pub const fn from_u32(v: u32) -> Self {
        match v {
            1 => Self::Idle,
            2 => Self::Illegal,
            3 => Self::Menu,
            _ => Self::Opening,
        }
    }
}

/// `0 = TransitionToMenu` (authentic boot flow), `1 = TransitionToIdle`, `2 = LoopOpening`.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootCompletionPolicy {
    TransitionToMenu = 0,
    TransitionToIdle = 1,
    LoopOpening = 2,
}

/// 64-byte `#[repr(C)]` per-frame summary read directly from WASM linear memory.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameTelemetry {
    pub mode: u32,
    pub frame_count: u32,
    pub vertex_count: u32,
    pub batch_count: u32,
    pub seed: u32,
    pub opening_state: u32,
    pub menu_page: u32,
    pub audio_cues: u32,
    pub clear_color: [f32; 4],
    pub cam_pos: [f32; 3],
    pub completed_flag: u32,
}

const _: () = assert!(core::mem::size_of::<FrameTelemetry>() == 64);

pub struct Engine {
    pub mode: AppMode,
    pub boot_completion_policy: BootCompletionPolicy,
    pub seed: u32,
    pub opening_tuning: OpeningTuning,
    pub menu_tuning: MenuTuning,
    pub opening: OpeningScene,
    pub menu: MenuScene,
    pub render_list: RenderList,
    pub textures: Vec<RgbaTexture>,
    pub texture_generation: u32,
    pub using_imported_assets: bool,
    pub imported_sha256: [u8; 64],
    pub telemetry: FrameTelemetry,
}

impl Engine {
    #[must_use]
    pub fn new(seed: u32) -> Self {
        let opening_tuning = OpeningTuning::default();
        let menu_tuning = MenuTuning::default();
        let opening = OpeningScene::new(OpeningVariant::Boot, seed, opening_tuning);
        let menu = MenuScene::new();
        let textures = generate_procedural_textures(0x05d5_2000);
        Self {
            mode: AppMode::Opening,
            boot_completion_policy: BootCompletionPolicy::TransitionToMenu,
            seed,
            opening_tuning,
            menu_tuning,
            opening,
            menu,
            render_list: RenderList::default(),
            textures,
            texture_generation: 1,
            using_imported_assets: false,
            imported_sha256: [b'0'; 64],
            telemetry: FrameTelemetry::default(),
        }
    }

    pub fn set_mode(&mut self, mode: AppMode, reset_timeline: bool) {
        self.mode = mode;
        match mode {
            AppMode::Opening => {
                if reset_timeline || self.opening.variant != OpeningVariant::Boot {
                    self.opening
                        .reset(OpeningVariant::Boot, self.seed, self.opening_tuning);
                }
            }
            AppMode::Idle => {
                if reset_timeline || self.opening.variant != OpeningVariant::Idle {
                    self.opening
                        .reset(OpeningVariant::Idle, self.seed, self.opening_tuning);
                }
            }
            AppMode::Illegal => {
                if reset_timeline || self.opening.variant != OpeningVariant::Illegal {
                    self.opening
                        .reset(OpeningVariant::Illegal, self.seed, self.opening_tuning);
                }
            }
            AppMode::Menu => {
                if reset_timeline {
                    self.menu.page = MenuPage::MainMenu;
                    self.menu.cfg_anim = 0.0;
                    self.menu.reset_entry_animation();
                }
            }
        }
    }

    pub fn set_seed(&mut self, seed: u32) {
        self.seed = seed;
        let variant = match self.mode {
            AppMode::Idle => OpeningVariant::Idle,
            AppMode::Illegal => OpeningVariant::Illegal,
            _ => OpeningVariant::Boot,
        };
        self.opening.reset(variant, seed, self.opening_tuning);
    }

    pub fn step_and_render(&mut self, dt_scale: f32) -> &FrameTelemetry {
        let mut cues = 0u32;
        let mut completed = 0u32;

        match self.mode {
            AppMode::Opening | AppMode::Idle | AppMode::Illegal => {
                if dt_scale > 0.0 {
                    self.opening.step(dt_scale, self.opening_tuning);
                }
                if self.mode == AppMode::Opening && self.opening.completed {
                    completed = 1;
                    match self.boot_completion_policy {
                        BootCompletionPolicy::TransitionToMenu => {
                            self.mode = AppMode::Menu;
                            self.menu.reset_entry_animation();
                            cues |= crate::menu::audio_cue::TRANSITION_IN;
                            self.menu
                                .emit_frame(&mut self.render_list, self.menu_tuning);
                        }
                        BootCompletionPolicy::TransitionToIdle => {
                            self.mode = AppMode::Idle;
                            self.opening.reset(
                                OpeningVariant::Idle,
                                self.seed,
                                self.opening_tuning,
                            );
                            self.opening
                                .emit_frame(&mut self.render_list, self.opening_tuning);
                        }
                        BootCompletionPolicy::LoopOpening => {
                            self.opening.reset(
                                OpeningVariant::Boot,
                                self.seed,
                                self.opening_tuning,
                            );
                            self.opening
                                .emit_frame(&mut self.render_list, self.opening_tuning);
                        }
                    }
                } else {
                    self.opening
                        .emit_frame(&mut self.render_list, self.opening_tuning);
                }
            }
            AppMode::Menu => {
                if dt_scale > 0.0 {
                    self.menu.step(dt_scale, self.menu_tuning);
                }
                cues |= self.menu.take_audio_cues();
                self.menu
                    .emit_frame(&mut self.render_list, self.menu_tuning);
            }
        }

        let (frame_count, cam_pos) = match self.mode {
            AppMode::Menu => (self.menu.frame_count, [10.436, 0.0, -103.0]),
            _ => (
                self.opening.frame_count,
                [
                    self.opening.cam_pos.x,
                    self.opening.cam_pos.y,
                    self.opening.cam_pos.z,
                ],
            ),
        };

        self.telemetry = FrameTelemetry {
            mode: self.mode as u32,
            frame_count,
            vertex_count: self.render_list.vertices.len() as u32,
            batch_count: self.render_list.batches.len() as u32,
            seed: self.seed,
            opening_state: self.opening.opening_state as u32,
            menu_page: self.menu.page as u32,
            audio_cues: cues,
            clear_color: self.render_list.clear_color,
            cam_pos,
            completed_flag: completed,
        };
        &self.telemetry
    }

    pub fn apply_imported_container(&mut self, bytes: &[u8]) -> Result<usize, ImportError> {
        let bundle = validate_and_extract_import(bytes)?;
        let count = bundle.textures.len();
        // Start from clean-room base set and overlay imported textures by slot
        let mut base = generate_procedural_textures(0x05d5_2000);
        for imp in bundle.textures {
            if let Some(existing) = base.iter_mut().find(|t| t.slot == imp.slot) {
                *existing = imp;
            }
        }
        self.textures = base;
        self.texture_generation = self.texture_generation.wrapping_add(1);
        self.using_imported_assets = true;
        let hex_bytes = bundle.sha256_hex.as_bytes();
        self.imported_sha256.copy_from_slice(&hex_bytes[..64]);
        Ok(count)
    }

    pub fn reset_to_procedural_assets(&mut self) {
        self.textures = generate_procedural_textures(0x05d5_2000);
        self.texture_generation = self.texture_generation.wrapping_add(1);
        self.using_imported_assets = false;
        self.imported_sha256 = [b'0'; 64];
    }
}

fn map_import_err(err: ImportError) -> i32 {
    match err {
        ImportError::InputEmpty => -1,
        ImportError::InputTooLarge { .. } => -2,
        ImportError::RomdirNotFound => -3,
        ImportError::RomdirTruncated => -4,
        ImportError::RomdirTooManyEntries => -5,
        ImportError::EntryOutOfBounds { .. } => -6,
        ImportError::ExpandHeaderTruncated => -7,
        ImportError::ExpandSizeTooLarge { .. } => -8,
        ImportError::ExpandStreamTruncated => -9,
        ImportError::ExpandInvalidBackref { .. } => -10,
        ImportError::NoRecognizedTextures => -11,
    }
}

// =========================================================================
// C-ABI Exports for Zero-Copy WebAssembly Host Access
// =========================================================================

#[unsafe(no_mangle)]
pub extern "C" fn osdbits_sizeof_render_vertex() -> u32 {
    core::mem::size_of::<RenderVertex>() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn osdbits_sizeof_render_batch() -> u32 {
    core::mem::size_of::<RenderBatch>() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn osdbits_sizeof_telemetry() -> u32 {
    core::mem::size_of::<FrameTelemetry>() as u32
}

/// # Safety
/// `ptr` must be a valid pointer to `Engine`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_using_imported_assets(ptr: *const Engine) -> u32 {
    let Some(eng) = (unsafe { ptr.as_ref() }) else {
        return 0;
    };
    u32::from(eng.using_imported_assets)
}

#[unsafe(no_mangle)]
pub extern "C" fn osdbits_engine_new(seed: u32) -> *mut Engine {
    Box::into_raw(Box::new(Engine::new(seed)))
}

/// # Safety
/// `ptr` must be null or a valid pointer returned by `osdbits_engine_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_engine_free(ptr: *mut Engine) {
    if !ptr.is_null() {
        // SAFETY: Caller guarantees `ptr` was allocated by `osdbits_engine_new` and not yet freed.
        unsafe {
            drop(Box::from_raw(ptr));
        }
    }
}

/// # Safety
/// `ptr` must be a valid pointer to `Engine`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_set_mode(ptr: *mut Engine, mode: u32, reset_timeline: u32) {
    if let Some(eng) = unsafe { ptr.as_mut() } {
        eng.set_mode(AppMode::from_u32(mode), reset_timeline != 0);
    }
}

/// # Safety
/// `ptr` must be a valid pointer to `Engine`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_set_boot_policy(ptr: *mut Engine, policy: u32) {
    if let Some(eng) = unsafe { ptr.as_mut() } {
        eng.boot_completion_policy = match policy {
            1 => BootCompletionPolicy::TransitionToIdle,
            2 => BootCompletionPolicy::LoopOpening,
            _ => BootCompletionPolicy::TransitionToMenu,
        };
    }
}

/// # Safety
/// `ptr` must be a valid pointer to `Engine`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_set_seed(ptr: *mut Engine, seed: u32) {
    if let Some(eng) = unsafe { ptr.as_mut() } {
        eng.set_seed(seed);
    }
}

/// # Safety
/// `ptr` must be a valid pointer to `Engine`.
#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_set_tuning(
    ptr: *mut Engine,
    is_pal: u32,
    speed_scale: f32,
    idle_drift: f32,
    fog: u32,
    bloom: u32,
    refraction: u32,
    reduced_motion: u32,
    screen_size: u32,
    show_banner: u32,
    language: u32,
    confirm_circle: u32,
    tower_games: u32,
) {
    if let Some(eng) = unsafe { ptr.as_mut() } {
        let prev_games = eng.opening_tuning.tower_density_games;
        let next_games = (tower_games.min(21)) as u8;
        eng.opening_tuning = OpeningTuning {
            is_pal: is_pal != 0,
            speed_scale: speed_scale.clamp(0.1, 3.0),
            idle_drift: idle_drift.clamp(0.0, 2.5),
            fog_enabled: fog != 0,
            bloom_enabled: bloom != 0,
            refraction_enabled: refraction != 0,
            reduced_motion: reduced_motion != 0,
            letterbox_16_9: screen_size == 2,
            show_banner: show_banner != 0,
            language: (language % 7) as u8,
            tower_density_games: next_games,
        };
        eng.menu_tuning = MenuTuning {
            is_pal: is_pal != 0,
            speed_scale: speed_scale.clamp(0.1, 3.0),
            bloom_enabled: bloom != 0,
            refraction_enabled: refraction != 0,
            reduced_motion: reduced_motion != 0,
        };
        eng.menu.values.screen_size = (screen_size % 3) as u8;
        eng.menu.values.language = (language % 7) as u8;
        eng.menu.values.confirm_with_circle = confirm_circle != 0;
        if prev_games != next_games {
            let v = eng.opening.variant;
            eng.opening.reset(v, eng.seed, eng.opening_tuning);
        }
    }
}

/// # Safety
/// `ptr` must be a valid pointer to `Engine`.
#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_set_clock(
    ptr: *mut Engine,
    year: u32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    lock_external: u32,
) {
    if let Some(eng) = unsafe { ptr.as_mut() } {
        eng.menu.set_clock(
            RtcClock {
                year: (year.clamp(2000, 2099)) as u16,
                month: (month.clamp(1, 12)) as u8,
                day: (day.clamp(1, 31)) as u8,
                hour: (hour % 24) as u8,
                minute: (minute % 60) as u8,
                second: (second % 60) as u8,
            },
            lock_external != 0,
        );
    }
}

/// # Safety
/// `ptr` must be a valid pointer to `Engine`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_menu_action(ptr: *mut Engine, action: u32) -> u32 {
    let Some(eng) = (unsafe { ptr.as_mut() }) else {
        return 0;
    };
    let act = match action {
        0 => MenuAction::Up,
        1 => MenuAction::Down,
        2 => MenuAction::Left,
        3 => MenuAction::Right,
        4 => MenuAction::Confirm,
        5 => MenuAction::Cancel,
        _ => MenuAction::VersionInfo,
    };
    eng.menu.handle_action(act)
}

/// # Safety
/// `ptr` must be a valid pointer to `Engine`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_pointer_click(ptr: *mut Engine, nx: f32, ny: f32) -> u32 {
    let Some(eng) = (unsafe { ptr.as_mut() }) else {
        return 0;
    };
    eng.menu
        .handle_pointer_click(nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0))
}

/// # Safety
/// `ptr` must be a valid pointer to `Engine`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_step_and_render(
    ptr: *mut Engine,
    dt_scale: f32,
) -> *const FrameTelemetry {
    let Some(eng) = (unsafe { ptr.as_mut() }) else {
        return core::ptr::null();
    };
    let dt = if dt_scale.is_finite() {
        dt_scale.clamp(0.0, 4.0)
    } else {
        1.0
    };
    eng.step_and_render(dt) as *const FrameTelemetry
}

/// # Safety
/// `ptr` must be a valid pointer to `Engine`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_vertices_ptr(ptr: *const Engine) -> *const RenderVertex {
    let Some(eng) = (unsafe { ptr.as_ref() }) else {
        return core::ptr::null();
    };
    eng.render_list.vertices.as_ptr()
}

/// # Safety
/// `ptr` must be a valid pointer to `Engine`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_batches_ptr(ptr: *const Engine) -> *const RenderBatch {
    let Some(eng) = (unsafe { ptr.as_ref() }) else {
        return core::ptr::null();
    };
    eng.render_list.batches.as_ptr()
}

/// # Safety
/// `ptr` must be a valid pointer to `Engine`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_texture_generation(ptr: *const Engine) -> u32 {
    let Some(eng) = (unsafe { ptr.as_ref() }) else {
        return 0;
    };
    eng.texture_generation
}

/// # Safety
/// `ptr` must be a valid pointer to `Engine`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_texture_count(ptr: *const Engine) -> u32 {
    let Some(eng) = (unsafe { ptr.as_ref() }) else {
        return 0;
    };
    eng.textures.len() as u32
}

/// # Safety
/// `ptr` must be a valid pointer to `Engine`, and `out_meta` must point to 3 writable `u32`s (`[slot, width, height]`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_texture_meta(
    ptr: *const Engine,
    index: u32,
    out_meta: *mut u32,
) -> *const u8 {
    let Some(eng) = (unsafe { ptr.as_ref() }) else {
        return core::ptr::null();
    };
    let Some(tex) = eng.textures.get(index as usize) else {
        return core::ptr::null();
    };
    if !out_meta.is_null() {
        unsafe {
            *out_meta.add(0) = tex.slot as u32;
            *out_meta.add(1) = tex.width;
            *out_meta.add(2) = tex.height;
        }
    }
    tex.pixels.as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn osdbits_alloc_bytes(len: usize) -> *mut u8 {
    if len == 0 || len > 16 * 1024 * 1024 {
        return core::ptr::null_mut();
    }
    let mut buf = vec![0u8; len].into_boxed_slice();
    let ptr = buf.as_mut_ptr();
    core::mem::forget(buf);
    ptr
}

/// # Safety
/// `ptr` must have been allocated by `osdbits_alloc_bytes(len)` and not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_free_bytes(ptr: *mut u8, len: usize) {
    if !ptr.is_null() && len > 0 {
        unsafe {
            let slice_ptr = core::ptr::slice_from_raw_parts_mut(ptr, len);
            drop(Box::from_raw(slice_ptr));
        }
    }
}

/// # Safety
/// `ptr` must be a valid pointer to `Engine`; `data_ptr` must be valid for `data_len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_import_container(
    ptr: *mut Engine,
    data_ptr: *const u8,
    data_len: usize,
) -> i32 {
    let Some(eng) = (unsafe { ptr.as_mut() }) else {
        return -100;
    };
    if data_ptr.is_null() || data_len == 0 {
        return map_import_err(ImportError::InputEmpty);
    }
    let bytes = unsafe { core::slice::from_raw_parts(data_ptr, data_len) };
    match eng.apply_imported_container(bytes) {
        Ok(n) => n as i32,
        Err(e) => map_import_err(e),
    }
}

/// # Safety
/// `ptr` must be a valid pointer to `Engine`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_import_sha256_ptr(ptr: *const Engine) -> *const u8 {
    let Some(eng) = (unsafe { ptr.as_ref() }) else {
        return core::ptr::null();
    };
    eng.imported_sha256.as_ptr()
}

/// # Safety
/// `ptr` must be a valid pointer to `Engine`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_reset_procedural_textures(ptr: *mut Engine) {
    if let Some(eng) = unsafe { ptr.as_mut() } {
        eng.reset_to_procedural_assets();
    }
}

/// Clean-room test utility: allocate a synthetic valid `TEXIMAGE` container in WASM memory.
///
/// # Safety
/// `out_len` must be a valid pointer to `usize`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osdbits_build_synthetic_teximage(
    seed: u32,
    out_len: *mut usize,
) -> *mut u8 {
    let vec = build_synthetic_clean_teximage(seed);
    let len = vec.len();
    if !out_len.is_null() {
        unsafe {
            *out_len = len;
        }
    }
    let mut boxed = vec.into_boxed_slice();
    let p = boxed.as_mut_ptr();
    core::mem::forget(boxed);
    p
}
