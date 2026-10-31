//! WebAssembly C-ABI bridge for `osdbits-core`.
//!
//! Zero `unsafe {}` blocks in function bodies; thread/reentrancy-safe state cell;
//! zero-copy linear-memory typed-array views for render commands & vertices; and
//! bounds-checked staging buffers for optional user-supplied `LZ` / `ROMDIR` validation.

use std::sync::Mutex;

use osdbits_core::expand::{
    expand_ps2_lz, find_romdir, parse_romdir, ResourceError, MAX_EXPANDED_SIZE,
};
use osdbits_core::menu::{MenuAction, MenuClock};
use osdbits_core::opening::{OsdLanguage, VideoStandard};
use osdbits_core::scene::{OsdSceneEngine, SceneConfig, VisualMode, CMD_WORDS, VERTEX_FLOATS};
use osdbits_core::towers::TowerFieldPreset;
use osdbits_core::{ABI_MAGIC, ABI_VERSION};

/// Maximum guest upload staging size (`16 MiB`).
pub const MAX_UPLOAD_BYTES: usize = MAX_EXPANDED_SIZE;

struct BridgeState {
    engine: OsdSceneEngine,
    upload_buf: Vec<u8>,
    unpacked_buf: Vec<u8>,
}

impl BridgeState {
    fn new() -> Self {
        Self {
            engine: OsdSceneEngine::default(),
            upload_buf: Vec::new(),
            unpacked_buf: Vec::new(),
        }
    }
}

static STATE: Mutex<Option<BridgeState>> = Mutex::new(None);

fn with_state<R>(f: impl FnOnce(&mut BridgeState) -> R) -> R {
    let mut guard = match STATE.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let state = guard.get_or_insert_with(BridgeState::new);
    f(state)
}

fn decode_menu_action(code: u32) -> Option<MenuAction> {
    match code {
        0 => Some(MenuAction::Up),
        1 => Some(MenuAction::Down),
        2 => Some(MenuAction::Left),
        3 => Some(MenuAction::Right),
        4 => Some(MenuAction::Confirm),
        5 => Some(MenuAction::Cancel),
        6 => Some(MenuAction::Options),
        7 => Some(MenuAction::Display),
        _ => None,
    }
}

/// Returns `"OSDB"` magic constant (`0x4F534442`) to verify module integrity.
#[no_mangle]
pub extern "C" fn osdbits_abi_magic() -> u32 {
    ABI_MAGIC
}

/// Returns the ABI protocol version expected by the TypeScript host.
#[no_mangle]
pub extern "C" fn osdbits_abi_version() -> u32 {
    ABI_VERSION
}

/// Returns the number of `f32` scalars per packed vertex (`12`).
#[no_mangle]
pub extern "C" fn osdbits_vertex_stride_floats() -> u32 {
    VERTEX_FLOATS as u32
}

/// Returns the number of `u32` words per packed draw command (`8`).
#[no_mangle]
pub extern "C" fn osdbits_cmd_stride_words() -> u32 {
    CMD_WORDS as u32
}

/// Initializes or reconfigures the deterministic OSDSYS scene engine.
///
/// `option_flags` bit layout:
/// - bit 0 (`1`): `boot_to_game`
/// - bit 1 (`2`): `auto_transition_to_menu`
/// - bit 2 (`4`): `pal` (`1 = PAL 50Hz`, `0 = NTSC 60Hz`)
/// - bit 3 (`8`): `reduced_motion`
/// - bit 4 (`16`): `enable_fog`
/// - bit 5 (`32`): `enable_lights`
/// - bit 6 (`64`): `enable_cubes`
/// - bit 7 (`128`): `enable_glow`
/// - bit 8 (`256`): `enable_motion_blur`
/// - bit 9 (`512`): `letterbox`
/// - bit 10 (`1024`): `confirm_cancel_swap`
///
/// Returns `1` on success, or `0` if `mode`, `language`, or `tower_preset_kind` is invalid.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn osdbits_init_scene(
    mode: u32,
    option_flags: u32,
    language: u32,
    rng_seed: u32,
    tower_preset_kind: u32,
    tower_seed: u32,
    tower_ngames: u32,
    tower_nboots: u32,
    year: u32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> u32 {
    let Some(visual_mode) = VisualMode::from_u32(mode) else {
        return 0;
    };
    let tower_preset = match tower_preset_kind {
        0 => TowerFieldPreset::CapturedRetail,
        1 => TowerFieldPreset::FullField,
        2 => TowerFieldPreset::Simulated {
            seed: tower_seed,
            ngames: tower_ngames.min(126) as u16,
            nboots: if tower_nboots == 0 {
                None
            } else {
                Some(tower_nboots.min(50_000) as u16)
            },
        },
        _ => return 0,
    };
    if language > 7 {
        return 0;
    }

    let cfg = SceneConfig {
        mode: visual_mode,
        boot_to_game: (option_flags & (1 << 0)) != 0,
        auto_transition_to_menu: (option_flags & (1 << 1)) != 0,
        standard: if (option_flags & (1 << 2)) != 0 {
            VideoStandard::Pal
        } else {
            VideoStandard::Ntsc
        },
        language: OsdLanguage::from_u8(language as u8),
        rng_seed,
        tower_preset,
        clock: MenuClock::new(
            year as u16,
            month as u8,
            day as u8,
            hour as u8,
            minute as u8,
            second as u8,
        ),
        speed_multiplier: 1.0,
        camera_xy_offset: [0.0, 0.0],
        reduced_motion: (option_flags & (1 << 3)) != 0,
        enable_fog: (option_flags & (1 << 4)) != 0,
        enable_lights: (option_flags & (1 << 5)) != 0,
        enable_cubes: (option_flags & (1 << 6)) != 0,
        enable_glow: (option_flags & (1 << 7)) != 0,
        enable_motion_blur: (option_flags & (1 << 8)) != 0,
        letterbox: (option_flags & (1 << 9)) != 0,
        confirm_cancel_swap: (option_flags & (1 << 10)) != 0,
    };

    with_state(|st| {
        st.engine.reconfigure(cfg);
    });
    1
}

/// Applies live Mode E runtime overrides without resetting the active timeline.
///
/// Rejects non-finite floats, speed outside `[0.0, 4.0]`, camera offsets outside `[-64.0, 64.0]`,
/// or `language > 7`, returning `0` without mutating simulation state.
#[no_mangle]
pub extern "C" fn osdbits_set_live_overrides(
    speed_multiplier: f32,
    cam_offset_x: f32,
    cam_offset_y: f32,
    option_flags: u32,
    language: u32,
) -> u32 {
    if !speed_multiplier.is_finite()
        || !(0.0..=4.0).contains(&speed_multiplier)
        || !cam_offset_x.is_finite()
        || !(-64.0..=64.0).contains(&cam_offset_x)
        || !cam_offset_y.is_finite()
        || !(-64.0..=64.0).contains(&cam_offset_y)
        || language > 7
    {
        return 0;
    }

    with_state(|st| {
        st.engine.set_live_overrides(
            speed_multiplier,
            cam_offset_x,
            cam_offset_y,
            (option_flags & (1 << 3)) != 0,
            (option_flags & (1 << 4)) != 0,
            (option_flags & (1 << 5)) != 0,
            (option_flags & (1 << 6)) != 0,
            (option_flags & (1 << 7)) != 0,
            (option_flags & (1 << 8)) != 0,
            (option_flags & (1 << 9)) != 0,
            OsdLanguage::from_u8(language as u8),
            (option_flags & (1 << 10)) != 0,
        );
    });
    1
}

/// Dispatches a Mode D menu navigation action (`0..=7`). Returns `1` if valid, `0` otherwise.
#[no_mangle]
pub extern "C" fn osdbits_dispatch_action(action_code: u32) -> u32 {
    let Some(action) = decode_menu_action(action_code) else {
        return 0;
    };
    with_state(|st| {
        st.engine.dispatch_menu_action(action);
    });
    1
}

/// Enters System Configuration directly when in Mode D (`Menu`).
#[no_mangle]
pub extern "C" fn osdbits_enter_system_config() {
    with_state(|st| {
        st.engine.enter_system_config();
    });
}

/// Steps the simulation by 1 frame and returns the emitted draw command count.
#[no_mangle]
pub extern "C" fn osdbits_step_frame() -> u32 {
    with_state(|st| st.engine.step_frame().command_count)
}

/// Returns linear-memory byte pointer to the current frame's `f32` vertex buffer.
#[no_mangle]
pub extern "C" fn osdbits_vertex_ptr() -> usize {
    with_state(|st| st.engine.vertex_stream().as_ptr() as usize)
}

/// Returns total number of `f32` elements in the current frame's vertex buffer.
#[no_mangle]
pub extern "C" fn osdbits_vertex_float_len() -> u32 {
    with_state(|st| st.engine.vertex_stream().len() as u32)
}

/// Returns linear-memory byte pointer to the current frame's `u32` command stream.
#[no_mangle]
pub extern "C" fn osdbits_cmd_ptr() -> usize {
    with_state(|st| st.engine.command_stream().as_ptr() as usize)
}

/// Returns total number of `u32` elements in the current frame's command stream.
#[no_mangle]
pub extern "C" fn osdbits_cmd_u32_len() -> u32 {
    with_state(|st| st.engine.command_stream().len() as u32)
}

/// Returns linear-memory byte pointer to the current frame's UTF-8 JSON inspection snapshot.
#[no_mangle]
pub extern "C" fn osdbits_inspect_ptr() -> usize {
    with_state(|st| st.engine.inspect_json_bytes().as_ptr() as usize)
}

/// Returns byte length of the current frame's UTF-8 JSON inspection snapshot.
#[no_mangle]
pub extern "C" fn osdbits_inspect_len() -> u32 {
    with_state(|st| st.engine.inspect_json_bytes().len() as u32)
}

/// Resizes the guest upload buffer for optional user-supplied resource validation and returns
/// its linear-memory pointer (`0` if `byte_len == 0` or `byte_len > MAX_UPLOAD_BYTES`).
#[no_mangle]
pub extern "C" fn osdbits_prepare_upload(byte_len: u32) -> usize {
    let n = byte_len as usize;
    if n == 0 || n > MAX_UPLOAD_BYTES {
        return 0;
    }
    with_state(|st| {
        st.upload_buf.clear();
        st.upload_buf.resize(n, 0);
        st.upload_buf.as_mut_ptr() as usize
    })
}

fn map_resource_err(err: ResourceError) -> i32 {
    match err {
        ResourceError::TruncatedHeader => -1,
        ResourceError::OutputTooLarge(_) => -2,
        ResourceError::UnexpectedEof => -3,
        ResourceError::InvalidBackReference { .. } => -4,
        ResourceError::NoRomdirFound => -5,
        ResourceError::MalformedRomdir => -6,
    }
}

/// Decompresses the staged `ps2expand` LZ payload into `unpacked_buf`, returning unpacked byte
/// length (`>= 0`) or a negative `ResourceError` code (`-1..-6`) on malformed input.
#[no_mangle]
pub extern "C" fn osdbits_unpack_lz_upload() -> i32 {
    with_state(|st| match expand_ps2_lz(&st.upload_buf) {
        Ok(bytes) => {
            let len = bytes.len() as i32;
            st.unpacked_buf = bytes;
            len
        }
        Err(e) => {
            st.unpacked_buf.clear();
            map_resource_err(e)
        }
    })
}

/// Parses a `ROMDIR` table inside the staged upload buffer, writes a UTF-8 JSON manifest into
/// `unpacked_buf`, and returns the entry count (`>= 0`) or a negative `ResourceError` code.
#[no_mangle]
pub extern "C" fn osdbits_inspect_romdir_upload() -> i32 {
    use std::fmt::Write as _;
    with_state(|st| {
        let base = match find_romdir(&st.upload_buf) {
            Ok(b) => b,
            Err(e) => {
                st.unpacked_buf.clear();
                return map_resource_err(e);
            }
        };
        match parse_romdir(&st.upload_buf, base) {
            Ok(entries) => {
                let mut json = String::from("[");
                for (i, e) in entries.iter().enumerate() {
                    if i > 0 {
                        json.push(',');
                    }
                    let _ = write!(
                        json,
                        "{{\"name\":\"{}\",\"offset\":{},\"size\":{}}}",
                        e.name, e.offset, e.size
                    );
                }
                json.push(']');
                st.unpacked_buf = json.into_bytes();
                entries.len() as i32
            }
            Err(e) => {
                st.unpacked_buf.clear();
                map_resource_err(e)
            }
        }
    })
}

/// Pointer to `unpacked_buf` populated by `osdbits_unpack_lz_upload` or `osdbits_inspect_romdir_upload`.
#[no_mangle]
pub extern "C" fn osdbits_unpacked_ptr() -> usize {
    with_state(|st| st.unpacked_buf.as_ptr() as usize)
}

/// Byte length of `unpacked_buf`.
#[no_mangle]
pub extern "C" fn osdbits_unpacked_len() -> u32 {
    with_state(|st| st.unpacked_buf.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use osdbits_core::expand::encode_ps2_lz_clean;

    #[test]
    fn wasm_abi_exports_match_core_and_validate_inputs() {
        assert_eq!(osdbits_abi_magic(), 0x4F53_4442);
        assert_eq!(osdbits_abi_version(), 1);
        assert_eq!(osdbits_vertex_stride_floats(), 12);
        assert_eq!(osdbits_cmd_stride_words(), 8);

        // Reject invalid mode / language / NaN overrides cleanly without panic
        assert_eq!(
            osdbits_init_scene(99, 0, 1, 1, 0, 0, 0, 0, 2026, 10, 31, 12, 0, 0),
            0
        );
        assert_eq!(
            osdbits_init_scene(0, 0, 99, 1, 0, 0, 0, 0, 2026, 10, 31, 12, 0, 0),
            0
        );
        assert_eq!(osdbits_set_live_overrides(f32::NAN, 0.0, 0.0, 0, 1), 0);
        assert_eq!(osdbits_dispatch_action(99), 0);

        // Valid initialization + stepping emits commands & valid JSON
        let default_flags = (1 << 1) | (1 << 4) | (1 << 5) | (1 << 6) | (1 << 7) | (1 << 8);
        assert_eq!(
            osdbits_init_scene(0, default_flags, 1, 1, 0, 0, 0, 0, 2026, 10, 31, 12, 0, 0),
            1
        );
        let cmds = osdbits_step_frame();
        assert!(cmds > 0);
        assert!(osdbits_vertex_float_len() > 0);
        assert_eq!(osdbits_cmd_u32_len(), cmds * 8);
        assert!(osdbits_inspect_len() > 20);

        // Upload & LZ unpack roundtrip + oversize guard
        assert_eq!(osdbits_prepare_upload(0), 0);
        assert_eq!(osdbits_prepare_upload((MAX_UPLOAD_BYTES + 1) as u32), 0);

        let compressed = encode_ps2_lz_clean(b"OSDBITS-WASM CLEAN ROOM TEST");
        let ptr = osdbits_prepare_upload(compressed.len() as u32);
        assert_ne!(ptr, 0);
        with_state(|st| st.upload_buf.copy_from_slice(&compressed));
        let unpacked_len = osdbits_unpack_lz_upload();
        assert_eq!(unpacked_len, 28);
        assert_eq!(osdbits_unpacked_len(), 28);
    }
}
