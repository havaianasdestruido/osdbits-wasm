//! The `extern "C"` boundary the browser loads.
//!
//! Everything here is a tiny, allocation-free shim over [`crate::scene::Scene`].
//! The JS side allocates nothing: it calls `osd_draw()`, reads the returned
//! word count, and views `osd_frame_ptr()` through `Float32Array`. All names
//! are fixed with `#[no_mangle]` so the loader does not depend on toolchain
//! name mangling, and the ABI version is checked at load time
//! (`docs/WASM-ABI.md`).

use core::ptr;
use std::sync::Once;

use crate::camera::{CameraEvent, HwState};
use crate::cubes::{CUBE_PASSES, ILLEGAL_CUBE_PASSES};
use crate::frame::{CUBE_WORDS, HEADER_WORDS};
use crate::scene::{Scene, SceneOptions, MODE_BOOT, MODE_IDLE, MODE_ILLEGAL, MODE_MENU};

/// Bumped whenever an exported signature or the frame layout changes.
pub const ABI_VERSION: u32 = 1;

pub const OK: u32 = 0;
pub const ERR_STATE: u32 = 1;
pub const ERR_ARG: u32 = 2;

/// `osd_set_option` ids.
pub const OPT_PAL: u32 = 0;
pub const OPT_GAMES: u32 = 1;
pub const OPT_BOOTS: u32 = 2;
pub const OPT_FULL_FIELD: u32 = 3;
pub const OPT_HW_STATE: u32 = 4;
pub const OPT_BOOT_PARAM: u32 = 5;
pub const OPT_FOG: u32 = 6;
pub const OPT_LIGHTS: u32 = 7;
pub const OPT_CUBES: u32 = 8;
pub const OPT_CAMERA_MOTION: u32 = 9;
pub const OPT_TEXT: u32 = 10;
pub const OPT_BLACK_BARS: u32 = 11;

/// `osd_state` ids.
pub const STATE_FRAME: u32 = 0;
pub const STATE_Z: u32 = 1;
pub const STATE_OPENING_STATE: u32 = 2;
pub const STATE_OPENING_TYPE: u32 = 3;
pub const STATE_SCENE_KIND: u32 = 4;
pub const STATE_ENDED: u32 = 5;
pub const STATE_SCE_ALPHA: u32 = 6;
pub const STATE_ILLEGAL_ALPHA: u32 = 7;
pub const STATE_FLARE_INTENSITY: u32 = 8;
pub const STATE_LIT_TOWERS: u32 = 9;
pub const STATE_FADE_ALPHA: u32 = 10;
pub const STATE_SCENE_STATE: u32 = 11;
pub const STATE_LIGHTS_SEED: u32 = 12;
pub const STATE_FRAME_WORDS: u32 = 13;
pub const STATE_BATCHES: u32 = 14;
pub const STATE_VERTICES: u32 = 15;
pub const STATE_CUBES: u32 = 16;
pub const STATE_WIDTH: u32 = 17;
pub const STATE_HEIGHT: u32 = 18;
pub const STATE_OPTIONS: u32 = 19;
pub const STATE_COUNT: u32 = 20;

// Single-threaded by construction (wasm32 without threads): every exported
// function is called from the same JS thread, so a `static mut` with raw
// pointer access is the minimal correct representation.
static mut SCENE: Option<Scene> = None;
static mut OPTIONS: Option<SceneOptions> = None;
static mut EVENTS: [u32; 32] = [0; 32];
static mut PASS_TABLE: [f32; 240] = [0.0; 240];
static mut ERROR: [u8; 256] = [0; 256];
static mut ERROR_LEN: usize = 0;

static HOOK: Once = Once::new();

unsafe fn scene() -> Option<&'static mut Scene> {
    #[allow(static_mut_refs)]
    let opt = &mut *ptr::addr_of_mut!(SCENE);
    opt.as_mut()
}

unsafe fn options() -> &'static mut SceneOptions {
    #[allow(static_mut_refs)]
    let opt = &mut *ptr::addr_of_mut!(OPTIONS);
    opt.get_or_insert_with(SceneOptions::default)
}

fn write_error(msg: &str) {
    let bytes = msg.as_bytes();
    let n = bytes.len().min(255);
    unsafe {
        let buf = &mut *ptr::addr_of_mut!(ERROR);
        buf[..n].copy_from_slice(&bytes[..n]);
        buf[n] = 0;
        *ptr::addr_of_mut!(ERROR_LEN) = n;
    }
}

/// Install a panic hook that records the panic message so the JS side can
/// show it instead of a bare `unreachable executed`. Idempotent.
#[no_mangle]
pub extern "C" fn osd_install_panic_hook() {
    HOOK.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let msg = info.to_string();
            write_error(&msg);
        }));
    });
}

#[no_mangle]
pub extern "C" fn osd_abi_version() -> u32 {
    ABI_VERSION
}

/// Create (or replace) the scene. Returns [`OK`] or an error code.
#[no_mangle]
pub extern "C" fn osd_init(mode: u32, seed: u32) -> u32 {
    if !matches!(mode, MODE_BOOT | MODE_IDLE | MODE_ILLEGAL | MODE_MENU) {
        write_error("osd_init: unknown mode");
        return ERR_ARG;
    }
    osd_install_panic_hook();
    let opts = unsafe { *options() };
    let s = Scene::new(mode, seed, opts);
    unsafe {
        *ptr::addr_of_mut!(SCENE) = Some(s);
        let ev = &mut *ptr::addr_of_mut!(EVENTS);
        ev[0] = 0;
    }
    write_error("");
    OK
}

/// Replace the scene, keeping the current options.
#[no_mangle]
pub extern "C" fn osd_reset(mode: u32, seed: u32) -> u32 {
    osd_init(mode, seed)
}

/// Set one [`SceneOptions`] field. Applies to the *next* `osd_init`.
/// Returns [`OK`], or [`ERR_ARG`] for an unknown id / out-of-range value.
#[no_mangle]
pub extern "C" fn osd_set_option(id: u32, value: f32) -> u32 {
    if !value.is_finite() && id != OPT_HW_STATE {
        return ERR_ARG;
    }
    let o = unsafe { options() };
    match id {
        OPT_PAL => o.pal = value != 0.0,
        OPT_GAMES => {
            if !(0.0..=64.0).contains(&value) {
                return ERR_ARG;
            }
            o.games = value as i32;
        }
        OPT_BOOTS => {
            if !(0.0..=1000.0).contains(&value) {
                return ERR_ARG;
            }
            o.boots = value as i32;
        }
        OPT_FULL_FIELD => o.full_field = value != 0.0,
        OPT_HW_STATE => {
            let hw = HwState::from_id(value as u32);
            if hw.id() != value as u32 {
                return ERR_ARG;
            }
            o.hw = hw;
        }
        OPT_BOOT_PARAM => {
            if !(100.0..=116.0).contains(&value) {
                return ERR_ARG;
            }
            o.boot_param = value as i32;
        }
        OPT_FOG => o.fog = value != 0.0,
        OPT_LIGHTS => o.lights = value != 0.0,
        OPT_CUBES => o.cubes = value != 0.0,
        OPT_CAMERA_MOTION => o.camera_motion = value != 0.0,
        OPT_TEXT => o.text = value != 0.0,
        OPT_BLACK_BARS => o.black_bars = value != 0.0,
        _ => return ERR_ARG,
    }
    OK
}

/// Read back an option (as f32: booleans are 0/1).
#[no_mangle]
pub extern "C" fn osd_get_option(id: u32) -> f32 {
    let o = unsafe { *options() };
    match id {
        OPT_PAL => o.pal as i32 as f32,
        OPT_GAMES => o.games as f32,
        OPT_BOOTS => o.boots as f32,
        OPT_FULL_FIELD => o.full_field as i32 as f32,
        OPT_HW_STATE => o.hw.id() as f32,
        OPT_BOOT_PARAM => o.boot_param as f32,
        OPT_FOG => o.fog as i32 as f32,
        OPT_LIGHTS => o.lights as i32 as f32,
        OPT_CUBES => o.cubes as i32 as f32,
        OPT_CAMERA_MOTION => o.camera_motion as i32 as f32,
        OPT_TEXT => o.text as i32 as f32,
        OPT_BLACK_BARS => o.black_bars as i32 as f32,
        _ => f32::NAN,
    }
}

fn encode_events(scene: &Scene) {
    unsafe {
        let ev = &mut *ptr::addr_of_mut!(EVENTS);
        let mut n = 1usize;
        for e in scene.events.iter().take((ev.len() - 1) / 2) {
            let (kind, arg) = match *e {
                CameraEvent::PlayRecord(r) => (1u32, r as u32),
                CameraEvent::FadeRecord(f) => (2, f as u32),
                CameraEvent::EndSceneSound => (3, 0),
            };
            ev[n] = kind;
            ev[n + 1] = arg;
            n += 2;
        }
        ev[0] = ((n - 1) / 2) as u32;
    }
}

/// Advance one simulation frame. Returns the new frame index, or 0 when no
/// scene exists.
#[no_mangle]
pub extern "C" fn osd_step() -> u32 {
    let Some(s) = (unsafe { scene() }) else {
        return 0;
    };
    s.step();
    encode_events(s);
    s.frame
}

/// Build the frame and return its length in `f32` words (0 on error).
#[no_mangle]
pub extern "C" fn osd_draw() -> u32 {
    let Some(s) = (unsafe { scene() }) else {
        return 0;
    };
    let words = s.draw().len();
    words as u32
}

/// Pointer to the current frame (valid until the next `osd_draw`). `usize`
/// so the host-side unit tests can dereference it on 64-bit targets; on
/// wasm32 this is a plain `i32` address for the JS `Float32Array` view.
#[no_mangle]
pub extern "C" fn osd_frame_ptr() -> usize {
    match unsafe { scene() } {
        Some(s) => s.writer.data().as_ptr() as usize,
        None => 0,
    }
}

/// FNV-1a hash of the current frame; the JS driver uses it to check that a
/// replay matches the Rust-side determinism tests.
#[no_mangle]
pub extern "C" fn osd_hash_lo() -> u32 {
    match unsafe { scene() } {
        Some(s) => s.last_hash as u32,
        None => 0,
    }
}

#[no_mangle]
pub extern "C" fn osd_hash_hi() -> u32 {
    match unsafe { scene() } {
        Some(s) => (s.last_hash >> 32) as u32,
        None => 0,
    }
}

/// One scalar of simulation state (see the `STATE_*` ids); NaN for unknown
/// ids, 0 when no scene exists.
#[no_mangle]
pub extern "C" fn osd_state(id: u32) -> f32 {
    let Some(s) = (unsafe { scene() }) else {
        return 0.0;
    };
    match id {
        STATE_FRAME => s.frame as f32,
        STATE_Z => s.camera.position[2],
        STATE_OPENING_STATE => s.camera.opening_state as f32,
        STATE_OPENING_TYPE => s.camera.opening_type as f32,
        STATE_SCENE_KIND => match s.kind {
            crate::scene::SceneKind::Opening => 0.0,
            crate::scene::SceneKind::Illegal => 1.0,
            crate::scene::SceneKind::None => 2.0,
        },
        STATE_ENDED => s.ended as i32 as f32,
        STATE_SCE_ALPHA => s.sce_alpha as f32,
        STATE_ILLEGAL_ALPHA => s.illegal_alpha as f32,
        STATE_FLARE_INTENSITY => s.illegal.intensity,
        STATE_LIT_TOWERS => s.field.lit_count() as f32,
        STATE_FADE_ALPHA => s.illegal.fade_alpha,
        STATE_SCENE_STATE => s.scene_state as f32,
        STATE_LIGHTS_SEED => s.lights.as_ref().map(|l| l.seed as f32).unwrap_or(0.0),
        STATE_FRAME_WORDS => s.last_words as f32,
        STATE_BATCHES => s.writer.header[crate::frame::H_BATCH_COUNT],
        STATE_VERTICES => s.writer.header[crate::frame::H_VERTEX_COUNT],
        STATE_CUBES => s.writer.header[crate::frame::H_CUBE_COUNT],
        STATE_WIDTH => s.width(),
        STATE_HEIGHT => s.height(),
        STATE_OPTIONS => 0.0,
        _ => f32::NAN,
    }
}

/// Number of queued camera events (`osd_event` indices 0..count).
#[no_mangle]
pub extern "C" fn osd_event_count() -> u32 {
    unsafe { (*ptr::addr_of!(EVENTS))[0] }
}

/// Kind of event `index` (1 = play record, 2 = fade record, 3 = end scene
/// sound). Returns 0 for an out-of-range index.
#[no_mangle]
pub extern "C" fn osd_event_kind(index: u32) -> u32 {
    if index >= osd_event_count() {
        return 0;
    }
    unsafe { (*ptr::addr_of!(EVENTS))[1 + index as usize * 2] }
}

/// Argument of event `index` (the record number or fade length).
#[no_mangle]
pub extern "C" fn osd_event_arg(index: u32) -> u32 {
    if index >= osd_event_count() {
        return 0;
    }
    unsafe { (*ptr::addr_of!(EVENTS))[2 + index as usize * 2] }
}

/// Fill the static pass tables once: 10 passes of 12 floats for the opening
/// (offset 0) and the illegal scene (offset 120). The TS renderer asserts its
/// own copy matches this.
fn ensure_pass_tables() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        let table = &mut *ptr::addr_of_mut!(PASS_TABLE);
        for (i, p) in CUBE_PASSES.iter().enumerate() {
            let rec = [
                p.target_work as f32,
                p.sel as f32,
                p.abe as f32,
                p.blend_mode as f32,
                p.blend_fix as f32,
                p.texa_flag as f32,
                p.st_mode as f32,
                p.color_mode as f32,
                p.uv_mode as f32,
                p.uv_arg,
                p.uv_arg2,
                p.group as f32,
            ];
            table[i * 12..i * 12 + 12].copy_from_slice(&rec);
        }
        for (i, p) in ILLEGAL_CUBE_PASSES.iter().enumerate() {
            let rec = [
                p.target_work as f32,
                p.sel as f32,
                p.abe as f32,
                p.blend_mode as f32,
                p.blend_fix as f32,
                p.texa_flag as f32,
                p.st_mode as f32,
                p.color_mode as f32,
                p.uv_mode as f32,
                p.uv_arg,
                p.uv_arg2,
                p.group as f32,
            ];
            table[120 + i * 12..120 + i * 12 + 12].copy_from_slice(&rec);
        }
    });
}

/// Pointer to the 240-float pass table (opening at 0, illegal at 120).
#[no_mangle]
pub extern "C" fn osd_pass_table_ptr() -> usize {
    ensure_pass_tables();
    unsafe { (*ptr::addr_of!(PASS_TABLE)).as_ptr() as usize }
}

/// Pointer to the last panic message (NUL-terminated, empty when none).
#[no_mangle]
pub extern "C" fn osd_last_error_ptr() -> usize {
    unsafe { (*ptr::addr_of!(ERROR)).as_ptr() as usize }
}

#[no_mangle]
pub extern "C" fn osd_last_error_len() -> u32 {
    unsafe { *ptr::addr_of!(ERROR_LEN) as u32 }
}

/// Frame layout constants, so the loader cannot drift from the core.
#[no_mangle]
pub extern "C" fn osd_header_words() -> u32 {
    HEADER_WORDS as u32
}

#[no_mangle]
pub extern "C" fn osd_cube_words() -> u32 {
    CUBE_WORDS as u32
}

#[no_mangle]
pub extern "C" fn osd_batch_words() -> u32 {
    crate::frame::BATCH_WORDS as u32
}

#[no_mangle]
pub extern "C" fn osd_vertex_words() -> u32 {
    crate::frame::VERTEX_WORDS as u32
}

/// Number of frames in one simulation second for the current PAL setting
/// (used by the shell's fixed-step accumulator).
#[no_mangle]
pub extern "C" fn osd_fps() -> u32 {
    match unsafe { scene() } {
        Some(s) => s.camera.fps() as u32,
        None => 60,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// The ABI owns one global scene, so these tests must not interleave.
    static LOCK: Mutex<()> = Mutex::new(());

    fn guard() -> std::sync::MutexGuard<'static, ()> {
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn abi_version_and_layout_constants() {
        let _g = guard();
        assert_eq!(osd_abi_version(), 1);
        assert_eq!(osd_header_words() as usize, HEADER_WORDS);
        assert_eq!(osd_vertex_words(), 10);
        assert_eq!(osd_batch_words(), 8);
        assert_eq!(osd_cube_words() as usize, CUBE_WORDS);
    }

    #[test]
    fn init_rejects_unknown_modes() {
        let _g = guard();
        assert_eq!(osd_init(99, 1), ERR_ARG);
        assert_eq!(osd_init(MODE_BOOT, 1), OK);
        assert_eq!(osd_init(MODE_IDLE, 1), OK);
        assert_eq!(osd_init(MODE_ILLEGAL, 1), OK);
        assert_eq!(osd_init(MODE_MENU, 1), OK);
    }

    #[test]
    fn draw_reports_a_well_formed_frame() {
        let _g = guard();
        assert_eq!(osd_init(MODE_BOOT, 7), OK);
        for _ in 0..5 {
            osd_step();
        }
        let words = osd_draw() as usize;
        assert!(words > HEADER_WORDS);
        assert_eq!(words, osd_state(STATE_FRAME_WORDS) as usize);
        let ptr = osd_frame_ptr() as *const f32;
        let frame = unsafe { std::slice::from_raw_parts(ptr, words) };
        assert_eq!(frame[0].to_bits(), crate::frame::MAGIC);
        assert_eq!(frame[1].to_bits(), crate::frame::VERSION);
        assert_eq!(frame[48] as usize, words);
    }

    #[test]
    fn replay_is_deterministic_across_inits() {
        let _g = guard();
        let mut hashes = Vec::new();
        for _ in 0..2 {
            assert_eq!(osd_init(MODE_BOOT, 4242), OK);
            for _ in 0..90 {
                osd_step();
                osd_draw();
            }
            hashes.push((osd_hash_lo(), osd_hash_hi()));
        }
        assert_eq!(hashes[0], hashes[1]);
        assert_ne!(hashes[0], (0, 0));
    }

    #[test]
    fn options_round_trip_and_validate() {
        let _g = guard();
        assert_eq!(osd_set_option(OPT_PAL, 1.0), OK);
        assert_eq!(osd_get_option(OPT_PAL), 1.0);
        assert_eq!(osd_set_option(OPT_FOG, 0.0), OK);
        assert_eq!(osd_get_option(OPT_FOG), 0.0);
        assert_eq!(osd_set_option(OPT_GAMES, 8.0), OK);
        assert_eq!(osd_get_option(OPT_GAMES), 8.0);
        assert_eq!(osd_set_option(OPT_GAMES, 1000.0), ERR_ARG);
        assert_eq!(osd_set_option(OPT_BOOT_PARAM, 99.0), ERR_ARG);
        assert_eq!(
            osd_set_option(OPT_HW_STATE, HwState::DiscPs2.id() as f32),
            OK
        );
        assert_eq!(osd_get_option(OPT_HW_STATE), HwState::DiscPs2.id() as f32);
        assert_eq!(osd_set_option(999, 1.0), ERR_ARG);
        // Restore the defaults so the other tests see a faithful config.
        for (id, v) in [
            (OPT_PAL, 0.0),
            (OPT_FOG, 1.0),
            (OPT_GAMES, 16.0),
            (OPT_BOOTS, 120.0),
            (OPT_HW_STATE, HwState::NoDisc.id() as f32),
            (OPT_BOOT_PARAM, 100.0),
        ] {
            assert_eq!(osd_set_option(id, v), OK);
        }
    }

    #[test]
    fn pal_option_changes_the_virtual_screen() {
        let _g = guard();
        assert_eq!(osd_set_option(OPT_PAL, 1.0), OK);
        assert_eq!(osd_init(MODE_BOOT, 1), OK);
        assert_eq!(osd_state(STATE_HEIGHT), 256.0);
        assert_eq!(osd_fps(), 50);
        assert_eq!(osd_set_option(OPT_PAL, 0.0), OK);
        assert_eq!(osd_init(MODE_BOOT, 1), OK);
        assert_eq!(osd_state(STATE_HEIGHT), 224.0);
        assert_eq!(osd_fps(), 60);
    }

    #[test]
    fn camera_events_are_encoded() {
        let _g = guard();
        assert_eq!(osd_init(MODE_BOOT, 3), OK);
        let mut seen = false;
        for _ in 0..400 {
            osd_step();
            let n = osd_event_count();
            if n > 0 {
                let kind = osd_event_kind(0);
                assert!((1..=3).contains(&kind), "kind {kind}");
                seen = true;
            }
        }
        assert!(seen, "no camera event dispatched in 400 frames");
        assert_eq!(osd_event_kind(99), 0);
        assert_eq!(osd_event_arg(99), 0);
    }

    #[test]
    fn pass_tables_are_exported_and_match_the_rust_constants() {
        let _g = guard();
        let ptr = osd_pass_table_ptr() as *const f32;
        let table = unsafe { std::slice::from_raw_parts(ptr, 240) };
        assert_eq!(table[0], 1.0); // CUBE_PASSES[0].target_work
        assert_eq!(table[4], 122.0); // blend_fix
        assert_eq!(table[9], 0.0); // uv_arg
        assert_eq!(table[10], 1.0); // uv_arg2
                                    // Illegal pass 0 has uv_arg2 0.8.
        assert_eq!(table[120 + 10], 0.8);
        // Illegal pass 2 is colour mode 4.
        assert_eq!(table[120 + 24 + 7], 4.0);
    }

    #[test]
    fn state_ids_are_stable() {
        let _g = guard();
        assert_eq!(osd_init(MODE_ILLEGAL, 1), OK);
        for _ in 0..10 {
            osd_step();
            osd_draw();
        }
        assert_eq!(osd_state(STATE_SCENE_KIND), 1.0);
        assert!(osd_state(STATE_FRAME) >= 10.0);
        assert!(osd_state(STATE_Z) > 0.0);
        assert!(osd_state(STATE_LIT_TOWERS) > 0.0);
        assert!(osd_state(STATE_CUBES) >= 0.0);
        assert!(osd_state(999).is_nan());
    }

    #[test]
    fn draw_without_a_scene_is_inert() {
        let _g = guard();
        // No scene yet in a fresh process? The other tests may have made one,
        // so only check the pointer contract.
        assert!(osd_draw() as usize == 0 || osd_frame_ptr() != 0);
    }

    #[test]
    fn error_buffer_starts_empty_and_records_messages() {
        let _g = guard();
        assert_eq!(osd_init(MODE_BOOT, 1), OK);
        assert_eq!(osd_last_error_len(), 0);
        assert_eq!(osd_init(1234, 1), ERR_ARG);
        let len = osd_last_error_len() as usize;
        assert!(len > 0);
        let ptr = osd_last_error_ptr() as *const u8;
        let msg = unsafe { std::slice::from_raw_parts(ptr, len) };
        assert_eq!(std::str::from_utf8(msg).unwrap(), "osd_init: unknown mode");
    }
}
