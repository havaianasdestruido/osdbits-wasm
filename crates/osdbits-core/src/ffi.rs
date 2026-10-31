//! C-ABI / WASM exports consumed by `src/wasm/bridge.ts`.
//!
//! The engine is a single-threaded global (WASM is single-threaded here). TypeScript reads the
//! vertex, command and audio buffers as typed-array views over the exact pointers returned by
//! the accessors below, so a frame costs no serialization.

use std::cell::RefCell;

use crate::assets::{AssetPack, STATIC_TEXTURE_COUNT};
use crate::illegal::IllegalDiscScene;
use crate::input_audio::{AudioMailboxEntry, PadCooker, SoundMailbox};
use crate::menu::MenuScene;
use crate::menuconfig::ConfigScene;
use crate::menutext::{emit_ui, OsdUi, ScreenMode};
use crate::opening::OpeningScene;
use crate::render_cmd::{FrameCommandBuffer, RenderPassCmd};
use crate::resource::expand_lzss;

/// Default memory-card boot history: a plausible "played for a few weeks" card, so the
/// opening's tower field is populated on a fresh visit. Users can replace it in Mode D's
/// Browser screen or with the presets in the customisation panel.
pub const DEFAULT_BOOT_HISTORY: [u8; 21] = [
    9, 6, 4, 3, 3, 2, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// Scene modes exposed to the browser UI.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneMode {
    /// Mode A - the boot opening: drift, fly-up, SCE text, transition to the menu.
    Opening = 0,
    /// Mode B - indefinite tower-field idle.
    Idle = 1,
    /// Mode C - the illegal-disc warning screen.
    Illegal = 2,
    /// Mode D - main menu / System Configuration / browser.
    Menu = 3,
}

struct Engine {
    mode: SceneMode,
    opening: OpeningScene,
    illegal: IllegalDiscScene,
    menu: MenuScene,
    config: ConfigScene,
    ui: OsdUi,
    pad: PadCooker,
    sound: SoundMailbox,
    pack: AssetPack,
    frame: FrameCommandBuffer,
    audio_events: Vec<AudioMailboxEntry>,
    texture_staging: Vec<u8>,
    json_buf: Vec<u8>,
    seed: u32,
    speed: f32,
    reduced_motion: bool,
    fog_intensity: f32,
    refraction_scale: f32,
    paused: bool,
    /// Set once by Mode A when the opening's state machine reports the scene end.
    opening_finished: bool,
    /// Text/format version reported to the UI, bumped when persisted layout changes.
    format_version: u32,
}

impl Engine {
    fn new() -> Self {
        let seed = 0x1357_9BDFu32;
        let pack = AssetPack::new_procedural();
        Self {
            mode: SceneMode::Opening,
            opening: OpeningScene::new(seed, false, DEFAULT_BOOT_HISTORY),
            illegal: IllegalDiscScene::new(seed, pack_language(&pack)),
            menu: MenuScene::new(seed, 12, 34, 56),
            config: ConfigScene::new(),
            ui: {
                let mut ui = OsdUi::new();
                ui.boot_history_play_counts = DEFAULT_BOOT_HISTORY;
                ui
            },
            pad: PadCooker::new(true),
            sound: SoundMailbox::new(),
            pack,
            frame: FrameCommandBuffer::new(),
            audio_events: Vec::new(),
            texture_staging: Vec::new(),
            json_buf: Vec::new(),
            seed,
            speed: 1.0,
            reduced_motion: false,
            fog_intensity: 1.0,
            refraction_scale: 1.0,
            paused: false,
            opening_finished: false,
            format_version: 1,
        }
    }

    fn set_boot_history(&mut self, counts: &[u8]) {
        for (i, slot) in self.ui.boot_history_play_counts.iter_mut().enumerate() {
            *slot = counts.get(i).copied().unwrap_or(0).min(99);
        }
        self.opening.boot_history_play_counts = self.ui.boot_history_play_counts;
        self.opening.rebuild_towers();
    }

    fn rebuild_from_history(&mut self) {
        self.opening.boot_history_play_counts = self.ui.boot_history_play_counts;
        self.opening.rebuild_towers();
    }

    fn step(&mut self, physical_buttons: u16) {
        self.pad.step(physical_buttons);
        // Audio events accumulate until the browser consumes them with `osd_drain_audio()`,
        // so a frame that the host does not poll can never lose a sound. The cap keeps a host
        // that never polls from growing the buffer without bound.
        if self.audio_events.len() > 512 {
            self.audio_events.drain(0..256);
        }

        if self.paused {
            // Keep the render list valid but do not advance any simulation time.
            self.emit();
            return;
        }

        let was_config = self.menu.config_active;
        self.ui
            .handle_input(&self.pad, &mut self.sound, &mut self.menu, &mut self.config);

        if was_config != self.menu.config_active || !self.menu.config_active {
            self.rebuild_from_history();
        }

        match self.mode {
            SceneMode::Opening => {
                self.opening
                    .step_frame(self.speed, self.reduced_motion, &mut self.sound);
                if self.opening.transition_to_menu_ready && !self.opening_finished {
                    self.opening_finished = true;
                    self.mode = SceneMode::Menu;
                }
            }
            SceneMode::Idle => {
                self.opening
                    .step_frame(self.speed, self.reduced_motion, &mut self.sound);
            }
            SceneMode::Illegal => {
                self.illegal
                    .step_frame(self.speed, self.reduced_motion, &mut self.sound);
            }
            SceneMode::Menu => {
                self.menu
                    .step_frame(self.speed, self.reduced_motion, &mut self.sound);
                self.menu.advance_config_alpha(self.speed);
                self.config.step_frame(self.speed, &self.menu);
                self.ui.clock_hours = self.menu.clock.hours;
                self.ui.clock_minutes = self.menu.clock.minutes;
                self.ui.clock_seconds = self.menu.clock.seconds;
            }
        }
        self.ui.step_notification();

        // Drain again so messages queued by this frame's simulation reach the browser this frame.
        self.sound.drain_frame(&mut self.audio_events);

        self.emit();
    }

    fn emit(&mut self) {
        self.frame.clear();
        match self.mode {
            SceneMode::Opening | SceneMode::Idle => {
                self.opening.emit_render_commands(
                    &mut self.frame,
                    self.fog_intensity,
                    self.refraction_scale,
                );
            }
            SceneMode::Illegal => {
                self.illegal.emit_render_commands(
                    &mut self.frame,
                    self.fog_intensity,
                    self.refraction_scale,
                );
            }
            SceneMode::Menu => {
                self.menu
                    .emit_render_commands(&mut self.frame, self.reduced_motion);
                self.config
                    .emit_render_commands(&mut self.frame, &self.pack, &self.menu);
                emit_ui(&mut self.frame, &self.ui, &self.menu, &self.pack);
            }
        }
    }

    fn state_json(&mut self) -> &[u8] {
        let j = format!(
            concat!(
                "{{\"mode\":{},\"frame\":{},\"fps_target\":{},\"opening_finished\":{},",
                "\"vertices\":{},\"commands\":{},\"audio\":{},\"format_version\":{},",
                "\"language\":{},\"screen_size\":{},\"digital_out\":{},\"component_out\":{},",
                "\"region_japan\":{},\"screen\":\"{}\",\"clock\":[{},{},{}],",
                "\"boot_history\":[{}],\"active_towers\":{},\"asset_source\":\"{}\",",
                "\"asset_imported\":{},\"ui_texture_count\":{}}}"
            ),
            self.mode as u32,
            match self.mode {
                SceneMode::Opening | SceneMode::Idle => self.opening.frame,
                SceneMode::Illegal => self.illegal.frame,
                SceneMode::Menu => self.menu.frame,
            },
            50,
            self.opening_finished,
            self.frame.vertices.len(),
            self.frame.commands.len(),
            self.audio_events.len(),
            self.format_version,
            self.ui.settings.language,
            self.ui.settings.screen_size,
            self.ui.settings.digital_out,
            self.ui.settings.component_out,
            self.ui.settings.region_japan,
            screen_name(self.ui.screen),
            self.ui.clock_hours,
            self.ui.clock_minutes,
            self.ui.clock_seconds,
            self.ui
                .boot_history_play_counts
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(","),
            self.opening.active_tower_count,
            self.pack.source_label.replace('"', "'"),
            self.pack.is_user_imported,
            STATIC_TEXTURE_COUNT,
        );
        self.json_buf.clear();
        self.json_buf.extend_from_slice(j.as_bytes());
        self.json_buf.push(0);
        &self.json_buf
    }
}

fn pack_language(pack: &AssetPack) -> u8 {
    let _ = pack;
    1
}

fn screen_name(s: ScreenMode) -> &'static str {
    match s {
        ScreenMode::MainMenu => "main_menu",
        ScreenMode::SystemConfig => "system_config",
        ScreenMode::ClockEditor => "clock_editor",
        ScreenMode::ScreenSizeEditor => "screen_size",
        ScreenMode::DigitalOutEditor => "digital_out",
        ScreenMode::ComponentEditor => "component",
        ScreenMode::LanguageEditor => "language",
        ScreenMode::VersionInfo => "version_info",
        ScreenMode::MemoryCardBrowser => "browser",
    }
}

thread_local! {
    static ENGINE: RefCell<Engine> = RefCell::new(Engine::new());
}

fn with<R>(f: impl FnOnce(&mut Engine) -> R) -> R {
    ENGINE.with(|e| f(&mut e.borrow_mut()))
}

// ---------------------------------------------------------------- lifecycle

#[no_mangle]
pub extern "C" fn osd_init(seed: u32, region_japan: u32) -> u32 {
    with(|e| {
        e.seed = if seed == 0 { 0x1357_9BDF } else { seed };
        e.pad = PadCooker::new(!region_japan != 0);
        e.ui.settings.region_japan = region_japan != 0;
        e.mode = SceneMode::Opening;
        e.opening = OpeningScene::new(e.seed, false, e.ui.boot_history_play_counts);
        e.illegal = IllegalDiscScene::new(e.seed, e.ui.settings.language);
        e.menu = MenuScene::new(e.seed, 12, 34, 56);
        e.config = ConfigScene::new();
        e.ui = OsdUi::new();
        e.ui.settings.region_japan = region_japan != 0;
        e.sound = SoundMailbox::new();
        e.audio_events.clear();
        e.opening_finished = false;
        e.step(0);
        e.format_version
    })
}

#[no_mangle]
pub extern "C" fn osd_set_mode(mode: u32) {
    with(|e| {
        let next = match mode {
            0 => SceneMode::Opening,
            1 => SceneMode::Idle,
            2 => SceneMode::Illegal,
            _ => SceneMode::Menu,
        };
        if next == e.mode {
            return;
        }
        e.mode = next;
        e.opening_finished = false;
        match next {
            SceneMode::Opening => {
                e.opening = OpeningScene::new(e.seed, false, e.ui.boot_history_play_counts);
            }
            SceneMode::Idle => {
                e.opening = OpeningScene::new(e.seed, true, e.ui.boot_history_play_counts);
            }
            SceneMode::Illegal => {
                e.illegal = IllegalDiscScene::new(e.seed, e.ui.settings.language);
            }
            SceneMode::Menu => {
                e.menu = MenuScene::new(
                    e.seed,
                    e.ui.clock_hours,
                    e.ui.clock_minutes,
                    e.ui.clock_seconds,
                );
                e.config = ConfigScene::new();
                e.ui.screen = ScreenMode::MainMenu;
            }
        }
        e.step(0);
    })
}

#[no_mangle]
pub extern "C" fn osd_get_mode() -> u32 {
    with(|e| e.mode as u32)
}

/// Advances one frame. `physical_buttons` is the active-high physical pad word (0x0020 = the
/// physical Circle bit, 0x0040 = the physical Cross bit; the region swap happens in Rust).
#[no_mangle]
pub extern "C" fn osd_step(physical_buttons: u32) {
    with(|e| e.step(physical_buttons as u16));
}

#[no_mangle]
pub extern "C" fn osd_set_paused(p: u32) {
    with(|e| e.paused = p != 0);
}

#[no_mangle]
pub extern "C" fn osd_set_speed(speed: f32) {
    with(|e| e.speed = speed.clamp(0.05, 4.0));
}

#[no_mangle]
pub extern "C" fn osd_set_reduced_motion(on: u32) {
    with(|e| e.reduced_motion = on != 0);
}

#[no_mangle]
pub extern "C" fn osd_set_fx(fog_intensity: f32, refraction_scale: f32) {
    with(|e| {
        e.fog_intensity = fog_intensity.clamp(0.0, 2.0);
        e.refraction_scale = refraction_scale.clamp(0.0, 2.0);
    })
}

// ---------------------------------------------------------------- buffers

#[no_mangle]
pub extern "C" fn osd_vertices_ptr() -> *const u8 {
    with(|e| e.frame.vertices.as_ptr() as *const u8)
}

#[no_mangle]
pub extern "C" fn osd_vertex_count() -> u32 {
    with(|e| e.frame.vertices.len() as u32)
}

#[no_mangle]
pub extern "C" fn osd_commands_ptr() -> *const u8 {
    with(|e| e.frame.commands.as_ptr() as *const u8)
}

#[no_mangle]
pub extern "C" fn osd_command_count() -> u32 {
    with(|e| e.frame.commands.len() as u32)
}

#[no_mangle]
pub extern "C" fn osd_audio_ptr() -> *const u8 {
    with(|e| e.audio_events.as_ptr() as *const u8)
}

#[no_mangle]
pub extern "C" fn osd_audio_count() -> u32 {
    with(|e| e.audio_events.len() as u32)
}

#[no_mangle]
pub extern "C" fn osd_vertex_stride() -> u32 {
    std::mem::size_of::<crate::render_cmd::GpuVertex>() as u32
}

#[no_mangle]
pub extern "C" fn osd_command_stride() -> u32 {
    std::mem::size_of::<RenderPassCmd>() as u32
}

/// Clears the audio event buffer (the host calls this after turning the events into sound).
#[no_mangle]
pub extern "C" fn osd_drain_audio() {
    with(|e| e.audio_events.clear());
}

#[no_mangle]
pub extern "C" fn osd_audio_stride() -> u32 {
    std::mem::size_of::<AudioMailboxEntry>() as u32
}

// ---------------------------------------------------------------- textures

#[no_mangle]
pub extern "C" fn osd_texture_count() -> u32 {
    STATIC_TEXTURE_COUNT as u32
}

/// Returns a pointer into the engine's staging buffer holding `texture_id` as tightly packed RGBA8.
#[no_mangle]
pub extern "C" fn osd_texture_ptr(texture_id: u32) -> *const u8 {
    with(|e| {
        let idx = texture_id as usize;
        if idx >= e.pack.textures.len() {
            e.texture_staging.clear();
            return e.texture_staging.as_ptr();
        }
        e.texture_staging.clear();
        e.texture_staging
            .extend_from_slice(&e.pack.textures[idx].rgba);
        e.texture_staging.as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn osd_texture_width(texture_id: u32) -> u32 {
    with(|e| {
        e.pack
            .textures
            .get(texture_id as usize)
            .map(|t| t.width)
            .unwrap_or(0)
    })
}

#[no_mangle]
pub extern "C" fn osd_texture_height(texture_id: u32) -> u32 {
    with(|e| {
        e.pack
            .textures
            .get(texture_id as usize)
            .map(|t| t.height)
            .unwrap_or(0)
    })
}

// ---------------------------------------------------------------- settings / persistence

#[no_mangle]
pub extern "C" fn osd_set_language(lang: u32) {
    with(|e| {
        let l = if (1..=7).contains(&lang) {
            lang as u8
        } else {
            1
        };
        e.ui.settings.language = l;
        e.illegal.language = l;
    })
}

#[no_mangle]
pub extern "C" fn osd_set_config(screen_size: u32, digital_out: u32, component_out: u32) {
    with(|e| {
        e.ui.settings.screen_size = (screen_size as u8).min(2);
        e.ui.settings.digital_out = (digital_out as u8).min(1);
        e.ui.settings.component_out = (component_out as u8).min(1);
    })
}

#[no_mangle]
pub extern "C" fn osd_set_region_japan(on: u32) {
    with(|e| {
        e.ui.settings.region_japan = on != 0;
        e.pad.region_swap = on == 0;
    })
}

#[no_mangle]
pub extern "C" fn osd_set_clock(hours: u32, minutes: u32, seconds: u32) {
    with(|e| {
        e.menu.clock.set(hours, minutes, seconds);
        e.ui.clock_hours = e.menu.clock.hours;
        e.ui.clock_minutes = e.menu.clock.minutes;
        e.ui.clock_seconds = e.menu.clock.seconds;
    })
}

/// Copies a 21-byte play-count array into the engine and rebuilds the tower field.
#[no_mangle]
pub extern "C" fn osd_set_boot_history(ptr: *const u8, len: u32) {
    if ptr.is_null() || len < 21 {
        return;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let mut counts = [0u8; 21];
    counts.copy_from_slice(&slice[..21]);
    with(|e| {
        e.set_boot_history(&counts);
        e.mode = match e.mode {
            SceneMode::Idle => {
                e.opening = OpeningScene::new(e.seed, true, counts);
                SceneMode::Idle
            }
            m => m,
        };
    })
}

// ---------------------------------------------------------------- JSON state

#[no_mangle]
pub extern "C" fn osd_state_json_ptr() -> *const u8 {
    with(|e| e.state_json().as_ptr())
}

#[no_mangle]
pub extern "C" fn osd_state_json_len() -> u32 {
    with(|e| {
        let _ = e.state_json();
        e.json_buf.len() as u32
    })
}

// ---------------------------------------------------------------- asset import

/// Expands an LZSS-compressed resource blob (`ps2expand.c`) into the engine's staging buffer.
/// Returns the decompressed length, or 0 on failure (the caller should treat 0 as "rejected").
#[no_mangle]
pub extern "C" fn osd_expand_lzss(ptr: *const u8, len: u32) -> u32 {
    if ptr.is_null() || len == 0 {
        return 0;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    match expand_lzss(slice) {
        Ok(out) => with(|e| {
            e.texture_staging.clear();
            e.texture_staging.extend_from_slice(&out);
            out.len() as u32
        }),
        Err(_) => 0,
    }
}

#[no_mangle]
pub extern "C" fn osd_expand_result_ptr() -> *const u8 {
    with(|e| e.texture_staging.as_ptr())
}

/// Imports a whole PS2 BIOS image or a bare ROMDIR container (TEXIMAGE / FNTIMAGE / SNDIMAGE).
/// Returns the number of resources that were decoded and merged into the asset pack.
#[no_mangle]
pub extern "C" fn osd_import_romdir(ptr: *const u8, len: u32) -> u32 {
    if ptr.is_null() || len == 0 {
        return 0;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    with(|e| match e.pack.import_from_romdir_blob(slice) {
        Ok(n) => n as u32,
        Err(_) => 0,
    })
}

#[no_mangle]
pub extern "C" fn osd_reset_assets() {
    with(|e| e.pack = AssetPack::new_procedural());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_steps_all_modes_without_panicking() {
        with(|e| {
            e.audio_events.clear();
            e.mode = SceneMode::Opening;
            for _ in 0..120 {
                e.step(0);
            }
            assert!(!e.frame.vertices.is_empty());
            assert!(!e.frame.commands.is_empty());

            for mode in [SceneMode::Idle, SceneMode::Illegal, SceneMode::Menu] {
                e.mode = mode;
                for _ in 0..60 {
                    e.step(0);
                }
                assert!(
                    !e.frame.commands.is_empty(),
                    "mode {mode:?} produced no commands"
                );
            }
        });
    }

    #[test]
    fn test_state_json_is_well_formed_and_reflects_settings() {
        with(|e| {
            e.ui.settings.language = 4;
            e.set_boot_history(&[3u8; 21]);
            let json = String::from_utf8_lossy(e.state_json()).to_string();
            assert!(json.starts_with('{') && json.ends_with("}\0"));
            assert!(json.contains("\"language\":4"));
            assert!(json.contains("\"active_towers\":126"));
            assert!(json.contains("\"asset_imported\":false"));
        });
    }
}
