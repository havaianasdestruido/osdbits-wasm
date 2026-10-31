#![forbid(unsafe_op_in_unsafe_fn)]
#![deny(clippy::all)]

//! `osdbits-core`: Deterministic simulation, procedural clean-room asset generation,
//! safe user-imported `ROMDIR` + LZ decompression, and zero-copy render command buffer
//! preparation for `osdbits-wasm`.

pub mod bios_import;
pub mod math;
pub mod menu;
pub mod opening;
pub mod osd_font;
pub mod procedural_assets;
pub mod render_list;
pub mod rng;
pub mod wasm_api;

/// ABI protocol version shared between Rust/WASM and TypeScript host.
pub const ENGINE_ABI_VERSION: u32 = 1;

/// Query the ABI protocol version exported by the WASM module.
#[unsafe(no_mangle)]
pub extern "C" fn osdbits_abi_version() -> u32 {
    ENGINE_ABI_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bios_import::build_synthetic_clean_teximage;
    use crate::wasm_api::{AppMode, BootCompletionPolicy, Engine};

    #[test]
    fn abi_version_and_engine_boot_to_menu_transition_work() {
        assert_eq!(osdbits_abi_version(), 1);

        let mut eng = Engine::new(2000);
        eng.boot_completion_policy = BootCompletionPolicy::TransitionToMenu;
        assert_eq!(eng.mode, AppMode::Opening);

        for _ in 0..450 {
            let _ = eng.step_and_render(1.0);
            if eng.mode == AppMode::Menu {
                break;
            }
        }
        assert_eq!(eng.mode, AppMode::Menu);
        assert!(eng.telemetry.vertex_count > 100);
    }

    #[test]
    fn engine_steady_state_render_list_capacity_never_grows_unbounded() {
        let mut eng = Engine::new(42);
        eng.set_mode(AppMode::Idle, true);
        // Warm up 5 frames
        for _ in 0..5 {
            let _ = eng.step_and_render(1.0);
        }
        let cap_v = eng.render_list.vertices.capacity();
        let cap_b = eng.render_list.batches.capacity();

        for _ in 0..240 {
            let _ = eng.step_and_render(1.0);
        }
        assert_eq!(eng.render_list.vertices.capacity(), cap_v);
        assert_eq!(eng.render_list.batches.capacity(), cap_b);
    }

    #[test]
    fn engine_import_and_reset_procedural_assets_round_trip() {
        let mut eng = Engine::new(1);
        let gen0 = eng.texture_generation;
        let container = build_synthetic_clean_teximage(99);
        let count = eng.apply_imported_container(&container).unwrap();
        assert_eq!(count, 3);
        assert!(eng.using_imported_assets);
        assert_eq!(eng.texture_generation, gen0 + 1);

        eng.reset_to_procedural_assets();
        assert!(!eng.using_imported_assets);
        assert_eq!(eng.texture_generation, gen0 + 2);
    }
}
