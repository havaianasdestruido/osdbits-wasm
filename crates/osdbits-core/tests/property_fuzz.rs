//! Phase 10 (`P10-001`) — Deterministic Property-Based & Adversarial Fuzz Suite (`12,000+` cases).
//!
//! Uses a zero-dependency SplitMix64 generator with explicit counterexample shrinking so any
//! violation prints a minimal reproducible seed/payload without requiring external crates.

#![forbid(unsafe_code)]

use osdbits_core::expand::{
    encode_ps2_lz_clean, expand_ps2_lz, find_romdir, parse_romdir, MAX_EXPANDED_SIZE,
};
use osdbits_core::menu::{MenuAction, MenuClock, MenuSim};
use osdbits_core::opening::{OsdLanguage, VideoStandard};
use osdbits_core::scene::{OsdSceneEngine, SceneConfig, VisualMode, CMD_WORDS, VERTEX_FLOATS};
use osdbits_core::towers::{TowerField, TowerFieldPreset, TOWER_COLS, TOWER_ROWS, TOWER_SLOTS};

#[derive(Clone, Copy)]
struct SplitMix64(u64);

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    fn next_range(&mut self, max_exclusive: usize) -> usize {
        if max_exclusive <= 1 {
            0
        } else {
            (self.next_u64() as usize) % max_exclusive
        }
    }

    fn next_f32(&mut self, min: f32, max: f32) -> f32 {
        let unit = (self.next_u32() as f64) / (u32::MAX as f64);
        min + (max - min) * (unit as f32)
    }

    fn fill_bytes(&mut self, buf: &mut [u8]) {
        for chunk in buf.chunks_mut(8) {
            let bytes = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
    }
}

/// Shrinks a failing byte vector down to a minimal prefix/subslice that still triggers `predicate`.
fn shrink_bytes<F: Fn(&[u8]) -> bool>(initial: &[u8], predicate: F) -> Vec<u8> {
    let mut best = initial.to_vec();
    let mut step = best.len() / 2;
    while step >= 1 {
        if best.len() > step {
            let candidate = best[..best.len() - step].to_vec();
            if predicate(&candidate) {
                best = candidate;
                continue;
            }
        }
        step /= 2;
    }
    best
}

#[test]
fn fuzz_lz_expand_and_romdir_adversarial_and_roundtrip_4500_cases() {
    let mut rng = SplitMix64::new(0x5031_305F_4655_5A5A);

    // 1. 3,000 adversarial random / mutated / bomb-header byte streams
    for case_idx in 0..3_000_usize {
        let len = rng.next_range(256);
        let mut raw = vec![0_u8; len];
        rng.fill_bytes(&mut raw);

        // Inject targeted adversarial headers on every 4th case:
        // huge declared length, zero length, small valid length with backreference before start, etc.
        if len >= 4 {
            match case_idx & 7 {
                0 => {
                    // Declared length just over MAX_EXPANDED_SIZE (zip-bomb guard)
                    let bomb = (MAX_EXPANDED_SIZE as u32).saturating_add(1 + (case_idx as u32));
                    raw[0..4].copy_from_slice(&bomb.to_le_bytes());
                }
                1 => {
                    // Small declared length (1..128) to deeply exercise inner decompression loop
                    let small = (rng.next_range(128) as u32) + 1;
                    raw[0..4].copy_from_slice(&small.to_le_bytes());
                }
                2 => {
                    // Zero-byte stream header
                    raw[0..4].copy_from_slice(&0_u32.to_le_bytes());
                }
                3 if len >= 32 => {
                    // Synthetic ROMDIR prefix followed by random fuzzed entries
                    raw[0..10].copy_from_slice(b"RESET\0\0\0\0\0");
                    raw[16..22].copy_from_slice(b"ROMDIR");
                }
                _ => {}
            }
        }

        if let Ok(expanded) = expand_ps2_lz(&raw) {
            if expanded.len() > MAX_EXPANDED_SIZE {
                let minimal = shrink_bytes(&raw, |b| {
                    expand_ps2_lz(b)
                        .map(|out| out.len() > MAX_EXPANDED_SIZE)
                        .unwrap_or(false)
                });
                panic!(
                    "Expand output exceeded safety cap on case #{case_idx}; minimal input: {minimal:?}"
                );
            }
        }

        if let Ok(base) = find_romdir(&raw) {
            let _ = parse_romdir(&raw, base);
        }
        let random_base = rng.next_range(raw.len().saturating_add(4));
        let _ = parse_romdir(&raw, random_base);
    }

    // 2. 1,500 round-trip property cases over structured & random payloads
    for case_idx in 0..1_500_usize {
        let len = rng.next_range(512);
        let mut payload = vec![0_u8; len];
        for (i, b) in payload.iter_mut().enumerate() {
            // Mix compressible patterns and high-entropy bytes
            *b = if (i + case_idx) % 5 < 3 {
                ((i / 3) ^ (case_idx & 0xFF)) as u8
            } else {
                rng.next_u32() as u8
            };
        }
        let encoded = encode_ps2_lz_clean(&payload);
        let decoded = expand_ps2_lz(&encoded).unwrap_or_else(|err| {
            panic!("Round-trip decode failed on case #{case_idx} (len={len}): {err}");
        });
        assert_eq!(decoded, payload, "Round-trip mismatch on case #{case_idx}");
    }
}

#[test]
fn property_tower_field_sprawl_bounds_and_determinism_3000_cases() {
    let mut rng = SplitMix64::new(0x544F_5745_525F_5052);

    for case_idx in 0..3_000_usize {
        let seed = rng.next_u32();
        let ngames = (rng.next_range(400)) as u16;
        let nboots = if (case_idx & 1) == 0 {
            None
        } else {
            Some((rng.next_range(1000)) as u16)
        };

        let preset = match case_idx % 16 {
            0 => TowerFieldPreset::CapturedRetail,
            1 => TowerFieldPreset::FullField,
            _ => TowerFieldPreset::Simulated {
                seed,
                ngames,
                nboots,
            },
        };

        let f1 = TowerField::new(preset);
        let f2 = TowerField::new(preset);

        assert!(
            f1.active_count <= TOWER_SLOTS,
            "Active count {} > {} on case #{case_idx} ({preset:?})",
            f1.active_count,
            TOWER_SLOTS
        );

        let mut counted_enabled = 0_usize;
        for r in 0..TOWER_ROWS {
            for c in 0..TOWER_COLS {
                let s1 = f1.slots[r][c];
                let s2 = f2.slots[r][c];
                assert_eq!(
                    s1.enabled, s2.enabled,
                    "Non-deterministic slot ({r},{c}) on {preset:?}"
                );
                assert_eq!(s1.pos, s2.pos);
                assert_eq!(s1.tower_c.to_bits(), s2.tower_c.to_bits());
                assert_eq!(s1.tower_d.to_bits(), s2.tower_d.to_bits());
                for coord in s1.pos {
                    assert!(coord.is_finite(), "Non-finite tower pos on {preset:?}");
                }
                assert!(s1.tower_a.is_finite() && s1.tower_b.is_finite());
                assert!(s1.tower_c.is_finite() && s1.tower_d.is_finite());
                if s1.enabled {
                    counted_enabled += 1;
                }
            }
        }
        assert_eq!(counted_enabled, f1.active_count);
    }
}

#[test]
fn property_calendar_and_menu_input_state_machine_3000_cases() {
    let mut rng = SplitMix64::new(0x4D45_4E55_5F52_5443);
    let actions = [
        MenuAction::Up,
        MenuAction::Down,
        MenuAction::Left,
        MenuAction::Right,
        MenuAction::Confirm,
        MenuAction::Cancel,
        MenuAction::Options,
    ];

    for case_idx in 0..3_000_usize {
        // Test arbitrary out-of-range & boundary dates (leap Feb 29, 2000 / 2024 vs non-leap 2001 / 2099, Dec 31 23:59:59)
        let year = 1995 + (rng.next_range(120) as u16);
        let month = rng.next_range(16) as u8;
        let day = rng.next_range(35) as u8;
        let hour = rng.next_range(28) as u8;
        let minute = rng.next_range(65) as u8;
        let second = rng.next_range(65) as u8;

        let clock = MenuClock::new(year, month, day, hour, minute, second);
        assert!((2000..=2099).contains(&clock.year));
        assert!((1..=12).contains(&clock.month));
        let max_d = MenuClock::days_in_month(clock.year, clock.month);
        assert!(
            (1..=max_d).contains(&clock.day),
            "Invalid clamped day {} for {}-{} on case #{case_idx}",
            clock.day,
            clock.year,
            clock.month
        );
        assert!(clock.hour < 24 && clock.minute < 60 && clock.second < 60);

        // Exercise MenuSim every 10th case with randomized input action bursts
        if case_idx % 10 == 0 {
            let std = if (case_idx & 1) == 0 {
                VideoStandard::Ntsc
            } else {
                VideoStandard::Pal
            };
            let mut menu = MenuSim::new(
                std,
                clock,
                OsdLanguage::from_u8((case_idx % 8) as u8),
                rng.next_u32(),
                (case_idx & 2) != 0,
            );
            let mut cues = Vec::new();
            let mut rl = Default::default();
            for _ in 0..12 {
                let act = actions[rng.next_range(actions.len())];
                menu.dispatch_action(act, &mut cues);
                menu.step(&mut rl, &mut cues);
                assert!((2000..=2099).contains(&menu.clock.year));
                assert!((1..=12).contains(&menu.clock.month));
                let md = MenuClock::days_in_month(menu.clock.year, menu.clock.month);
                assert!((1..=md).contains(&menu.clock.day));
            }
        }
    }
}

#[test]
fn property_scene_engine_finite_geometry_and_rapid_mode_switches_2000_cases() {
    let mut rng = SplitMix64::new(0x5343_454E_455F_465A);
    let mut engine = OsdSceneEngine::default();
    let modes = [
        VisualMode::Opening,
        VisualMode::Idle,
        VisualMode::IllegalDisc,
        VisualMode::Menu,
    ];

    for case_idx in 0..2_000_usize {
        if case_idx % 20 == 0 {
            let cfg = SceneConfig {
                mode: modes[rng.next_range(modes.len())],
                boot_to_game: (rng.next_u32() & 1) != 0,
                auto_transition_to_menu: (rng.next_u32() & 2) != 0,
                standard: if (rng.next_u32() & 4) != 0 {
                    VideoStandard::Pal
                } else {
                    VideoStandard::Ntsc
                },
                language: OsdLanguage::from_u8((rng.next_u32() % 8) as u8),
                rng_seed: rng.next_u32(),
                tower_preset: TowerFieldPreset::Simulated {
                    seed: rng.next_u32(),
                    ngames: (rng.next_range(130)) as u16,
                    nboots: None,
                },
                clock: MenuClock::new(
                    2000 + (rng.next_range(100) as u16),
                    1 + (rng.next_range(12) as u8),
                    1 + (rng.next_range(28) as u8),
                    rng.next_range(24) as u8,
                    rng.next_range(60) as u8,
                    rng.next_range(60) as u8,
                ),
                speed_multiplier: rng.next_f32(0.0, 4.0),
                camera_xy_offset: [rng.next_f32(-40.0, 40.0), rng.next_f32(-40.0, 40.0)],
                reduced_motion: (rng.next_u32() & 8) != 0,
                enable_fog: (rng.next_u32() & 16) != 0,
                enable_lights: (rng.next_u32() & 32) != 0,
                enable_cubes: (rng.next_u32() & 64) != 0,
                enable_glow: (rng.next_u32() & 128) != 0,
                enable_motion_blur: (rng.next_u32() & 256) != 0,
                letterbox: (rng.next_u32() & 512) != 0,
                confirm_cancel_swap: (rng.next_u32() & 1024) != 0,
            };
            engine.reconfigure(cfg);
            if engine.mode() == VisualMode::Menu && (rng.next_u32() & 1) == 0 {
                engine.enter_system_config();
            }
        }

        let snap = engine.step_frame().clone();
        for c in snap.camera_pos {
            assert!(c.is_finite(), "Non-finite camera_pos at step #{case_idx}");
        }
        assert!(snap.camera_rot.is_finite());

        let vs = engine.vertex_stream();
        let cs = engine.command_stream();
        assert_eq!(vs.len() % VERTEX_FLOATS, 0);
        assert_eq!(cs.len() % CMD_WORDS, 0);
        for (vi, &f) in vs.iter().enumerate() {
            assert!(
                f.is_finite(),
                "Non-finite vertex float at step #{case_idx}, float_idx={vi}"
            );
        }
        let total_verts = (vs.len() / VERTEX_FLOATS) as u32;
        for (ci, cmd) in cs.chunks_exact(CMD_WORDS).enumerate() {
            let kind = cmd[0];
            if kind <= 1 {
                let first_v = cmd[4];
                let count_v = cmd[5];
                assert!(
                    first_v.saturating_add(count_v) <= total_verts,
                    "Command #{ci} out-of-bounds vertex range ({first_v} + {count_v} > {total_verts}) at step #{case_idx}"
                );
            }
        }
        let json_str = std::str::from_utf8(engine.inspect_json_bytes())
            .expect("inspect_json_bytes must always be valid UTF-8");
        assert!(json_str.starts_with('{') && json_str.ends_with('}'));
    }
}
