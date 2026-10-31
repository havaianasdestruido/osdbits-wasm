//! PlayStation 2 controller input cooker (`osdbits/pad.c`) and `OSDDispatch`
//! sound mailbox (`osdbits/sound.c`).
//!
//! Directly ported from `osdbits/pad.c`, `docs/pad.md`, `osdbits/sound.c`, and `docs/sound.md`.

pub const PAD_L2: u16 = 0x0001;
pub const PAD_R2: u16 = 0x0002;
pub const PAD_L1: u16 = 0x0004;
pub const PAD_R1: u16 = 0x0008;
pub const PAD_TRIANGLE: u16 = 0x0010;
/// Canonical confirm bit (`0x0020`, physical Circle in Japan, physical Cross in World).
pub const PAD_CIRCLE: u16 = 0x0020;
/// Canonical cancel bit (`0x0040`, physical Cross in Japan, physical Circle in World).
pub const PAD_CROSS: u16 = 0x0040;
pub const PAD_SQUARE: u16 = 0x0080;
pub const PAD_SELECT: u16 = 0x0100;
pub const PAD_L3: u16 = 0x0200;
pub const PAD_R3: u16 = 0x0400;
pub const PAD_START: u16 = 0x0800;
pub const PAD_UP: u16 = 0x1000;
pub const PAD_RIGHT: u16 = 0x2000;
pub const PAD_DOWN: u16 = 0x4000;
pub const PAD_LEFT: u16 = 0x8000;

/// Physical button that maps to canonical `PAD_CIRCLE` (Confirm) in the World arrangement.
/// Feed this to [`PadCooker::step`], which takes *physical* buttons.
pub const PHYS_CONFIRM_WORLD: u16 = PAD_CROSS;
/// Physical button that maps to canonical `PAD_CROSS` (Cancel) in the World arrangement.
pub const PHYS_CANCEL_WORLD: u16 = PAD_CIRCLE;
/// In the Japan arrangement the physical buttons are the canonical ones.
pub const PHYS_CONFIRM_JAPAN: u16 = PAD_CIRCLE;
pub const PHYS_CANCEL_JAPAN: u16 = PAD_CROSS;

/// Exact port of `osdbits/pad.c` (`UpdatePad`, `0x22BE30..0x22BF54` and `0x207024..0x207058`).
///
/// Key retail invariants (`docs/pad.md`):
/// 1. Region swap (`region_swap != 0`, default `1` = World): physical Circle (`0x0020`) and
///    physical Cross (`0x0040`) trade places so bit `0x0020` (`PAD_CIRCLE`) is always "Confirm"
///    and bit `0x0040` (`PAD_CROSS`) is always "Cancel".
/// 2. UP/DOWN repeat counter (`up_hold`, `down_hold`):
///    - `-1` while released, `0` on the press frame.
///    - Fires at `count == 0` (press edge) and `count >= 31 && count % 3 == 0` (first repeat at frame 33, then every 3 frames).
///    - Quirk at `0x22BF20`: a fresh DOWN press (`down_hold == 0`) STORES `PAD_DOWN` (`0x4000`) over `rep`,
///      clobbering an UP repeat due on the exact same frame.
/// 3. LEFT/RIGHT never auto-repeat anywhere in the ROM (`dir_press` uses press-edges only for LEFT/RIGHT).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PadCooker {
    pub btns: u16,
    pub old_btns: u16,
    pub press: u16,
    pub release: u16,
    pub dir_press: u16,
    pub up_hold: i32,
    pub down_hold: i32,
    pub region_swap: bool,
}

impl Default for PadCooker {
    fn default() -> Self {
        Self::new(true)
    }
}

impl PadCooker {
    pub const fn new(region_swap: bool) -> Self {
        Self {
            btns: 0,
            old_btns: 0,
            press: 0,
            release: 0,
            dir_press: 0,
            up_hold: -1,
            down_hold: -1,
            region_swap,
        }
    }

    pub fn reset(&mut self) {
        self.btns = 0;
        self.old_btns = 0;
        self.press = 0;
        self.release = 0;
        self.dir_press = 0;
        self.up_hold = -1;
        self.down_hold = -1;
    }

    /// Shuffles a composed active-high physical button word according to `region_swap` (`0x207024`).
    #[inline]
    pub fn apply_region_swap(physical_btns: u16, region_swap: bool) -> u16 {
        if region_swap {
            (physical_btns & !0x0060)
                | ((physical_btns & 0x0020) << 1)
                | ((physical_btns & 0x0040) >> 1)
        } else {
            physical_btns
        }
    }

    /// Steps the input cooker by one frame given active-high physical button state `physical_btns`.
    pub fn step(&mut self, physical_btns: u16) {
        self.old_btns = self.btns;
        self.btns = Self::apply_region_swap(physical_btns, self.region_swap);
        let chg = self.btns ^ self.old_btns;
        self.press = chg & self.btns;
        self.release = chg & !self.btns;

        // 0x22BE30..0x22BF54: UP/DOWN repeat word (`gp-30308`)
        let mut rep: u16 = 0;
        if (self.btns & PAD_UP) != 0 {
            self.up_hold += 1;
            if self.up_hold == 0 {
                rep = PAD_UP; // 0x22BEC8: sw (stores over empty word)
            } else if self.up_hold >= 31 && (self.up_hold % 3) == 0 {
                rep |= PAD_UP;
            }
        } else {
            self.up_hold = -1;
        }

        if (self.btns & PAD_DOWN) != 0 {
            self.down_hold += 1;
            if self.down_hold == 0 {
                rep = PAD_DOWN; // 0x22BF20 quirk: fresh DOWN press stores over `rep`, clobbering UP repeat
            } else if self.down_hold >= 31 && (self.down_hold % 3) == 0 {
                rep |= PAD_DOWN;
            }
        } else {
            self.down_hold = -1;
        }

        self.dir_press = rep | (self.press & (PAD_LEFT | PAD_RIGHT));
    }
}

/// Sound RPC function numbers (`docs/sound.md`).
pub const SND_CMD_BGM_SET_VOL: u16 = 20499; // 0x5013 StSetBgmVol(slot, vol)
pub const SND_CMD_BGM_PLAY: u16 = 20500; // 0x5014 StBgmPlay(slot)
pub const SND_CMD_BGM_STOP: u16 = 20501; // 0x5015 StBgmStop(slot, unused, fade)
pub const SND_CMD_REVERB_DEPTH: u16 = 20493; // 0x500d StSetReverbDepth(core, l, r)
pub const SND_CMD_SE_PLAY: u16 = 20992; // 0x5200 StSePlay(vab_id=1, se_num)

/// BGM slot indices (`0..=7`) from `osdbits/sound.c` (`bgmRes[8]`).
pub const BGM_SLOT_BOOTS: u16 = 0; // SNDBOOTS (vol 66)
pub const BGM_SLOT_TNNLS: u16 = 1; // SNDTNNLS (vol 42)
pub const BGM_SLOT_CLOKS: u16 = 2; // SNDCLOKS (vol 45)
pub const BGM_SLOT_TM30S: u16 = 3; // SNDTM30S (vol 27)
pub const BGM_SLOT_TM60S: u16 = 4; // SNDTM60S (vol 27)
pub const BGM_SLOT_LOGOS: u16 = 5; // SNDLOGOS (vol 27)
pub const BGM_SLOT_WARNS: u16 = 6; // SNDWARNS (vol 54)
pub const BGM_SLOT_RCLKS: u16 = 7; // SNDRCLKS (vol 54)

pub const BGM_DEFAULT_VOLUMES: [u8; 8] = [66, 42, 45, 27, 27, 27, 54, 54];

/// UI Sound Effect indices (`StSePlay(1, se)`).
pub const SE_CONFIRM: u16 = 4;
pub const SE_TOGGLE: u16 = 5;
pub const SE_CURSOR: u16 = 6;
pub const SE_CANCEL: u16 = 10;

/// Flat `#[repr(C)]` audio command drained by the TypeScript Web Audio engine each frame.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AudioMailboxEntry {
    pub msg: u16,
    pub arg_a: u16,
    pub arg_b: u16,
    pub arg_c: u16,
}

/// Exact port of `OSDDispatch` 128-entry ring mailbox and 60-tick reverb ramp (`osdbits/sound.c`).
#[derive(Debug, Clone)]
pub struct SoundMailbox {
    ring: [AudioMailboxEntry; 128],
    read_idx: usize,
    write_idx: usize,
    reverb_step: u32,
    pub current_reverb_depth: u16,
    pub active_bgm_mask: u8,
}

impl Default for SoundMailbox {
    fn default() -> Self {
        Self::new()
    }
}

impl SoundMailbox {
    pub const fn new() -> Self {
        Self {
            ring: [AudioMailboxEntry {
                msg: 0,
                arg_a: 0,
                arg_b: 0,
                arg_c: 0,
            }; 128],
            read_idx: 0,
            write_idx: 0,
            reverb_step: 0,
            current_reverb_depth: 0,
            active_bgm_mask: 0,
        }
    }

    /// Enqueues a sound message into the 128-entry ring buffer (`0x200b80`).
    /// Returns `0` on success, `-1` if the ring is full.
    pub fn dispatch(&mut self, msg: i32, a: i32, b: i32, c: i32) -> i32 {
        let next = (self.write_idx + 1) & 127;
        if next == self.read_idx {
            return -1;
        }
        let u_msg = msg as u16;
        let u_a = a as u16;
        let u_b = b as u16;
        let u_c = c as u16;

        if u_msg == SND_CMD_BGM_PLAY && u_a < 8 {
            self.active_bgm_mask |= 1 << u_a;
        } else if u_msg == SND_CMD_BGM_STOP && u_a < 8 {
            self.active_bgm_mask &= !(1 << u_a);
        }

        self.ring[self.write_idx] = AudioMailboxEntry {
            msg: u_msg,
            arg_a: u_a,
            arg_b: u_b,
            arg_c: u_c,
        };
        self.write_idx = next;
        0
    }

    /// Drains all queued messages into `out` (appending, so a caller can drain twice in one
    /// frame without losing the first batch) and advances the 60-tick reverb fade-in ramp
    /// (`0x2009b0(60)`: `depth = step * 0x9ffec / 127 / 60`).
    pub fn drain_frame(&mut self, out: &mut Vec<AudioMailboxEntry>) {
        while self.read_idx != self.write_idx {
            let entry = self.ring[self.read_idx];
            self.ring[self.read_idx] = AudioMailboxEntry::default();
            self.read_idx = (self.read_idx + 1) & 127;
            if entry.msg != 0 {
                out.push(entry);
            }
        }

        if self.reverb_step < 60 {
            self.reverb_step += 1;
            let depth = ((self.reverb_step * 0x9ffec) / 127 / 60) as u16;
            self.current_reverb_depth = depth;
            out.push(AudioMailboxEntry {
                msg: SND_CMD_REVERB_DEPTH,
                arg_a: 0,
                arg_b: depth,
                arg_c: depth,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::SplitMix64;

    /// Exhaustive 65,536-pair verification matching `swapcheck.c` in `docs/pad.md`:
    /// The ROM's raw active-low byte shuffle + inversion equals `PadCooker::apply_region_swap`.
    #[test]
    fn test_pad_region_swap_exhaustive_65536() {
        for raw in 0u32..=0xFFFFu32 {
            let hi = ((raw >> 8) & 0xFF) as u8;
            let lo = (raw & 0xFF) as u8;

            // ROM stage 1 (`0x207024..0x207058`): shuffle active-low low byte, then invert in stage 2
            let rom_lo_swapped = (lo & 0x9F) | ((lo & 0x20) << 1) | ((lo & 0x40) >> 1);
            let rom_held = !(((hi as u16) << 8) | (rom_lo_swapped as u16));

            // Port word-level shuffle on composed active-high word
            let physical_active_high = !(((hi as u16) << 8) | (lo as u16));
            let port_held = PadCooker::apply_region_swap(physical_active_high, true);

            assert_eq!(rom_held, port_held, "mismatch at raw={raw:#06x}");
        }
    }

    /// 400-frame scripted + randomized hold pattern matching `repcheck.c` in `docs/pad.md`.
    #[test]
    fn test_pad_up_down_repeat_and_down_clobber_quirk_400_frames() {
        let mut pad = PadCooker::new(true);

        // Hold UP for 45 frames: fires at frame 0, then first repeat at frame 33, then 36, 39, 42
        let mut fired_frames = Vec::new();
        for f in 0..45 {
            pad.step(PAD_UP);
            if (pad.dir_press & PAD_UP) != 0 {
                fired_frames.push(f);
            }
        }
        assert_eq!(fired_frames, vec![0, 33, 36, 39, 42]);

        // Test the 0x22BF20 quirk: when UP is due a repeat on frame 45 (`up_hold == 45`),
        // pressing DOWN freshly (`down_hold == 0`) clobbers the UP repeat!
        pad.step(PAD_UP | PAD_DOWN);
        assert_eq!(pad.dir_press, PAD_DOWN);

        // Test that LEFT/RIGHT only fire on press edge and never auto-repeat
        pad.reset();
        let mut left_fires = Vec::new();
        for f in 0..50 {
            pad.step(PAD_LEFT);
            if (pad.dir_press & PAD_LEFT) != 0 {
                left_fires.push(f);
            }
        }
        assert_eq!(left_fires, vec![0]);

        // 400-frame randomized comparison against direct reference state machine
        let mut rng = SplitMix64::new(0x22BE_3000);
        pad.reset();
        let mut ref_prev: u16 = 0;
        let mut ref_up: i32 = -1;
        let mut ref_dn: i32 = -1;
        for _ in 0..400 {
            let phys = (rng.next_u32() & 0xFFFF) as u16;
            pad.step(phys);

            let held = PadCooker::apply_region_swap(phys, true);
            let press = (held ^ ref_prev) & held;
            ref_prev = held;
            let mut rep = 0u16;
            if (held & PAD_UP) != 0 {
                ref_up += 1;
                if ref_up == 0 {
                    rep = PAD_UP;
                } else if ref_up >= 31 && ref_up % 3 == 0 {
                    rep |= PAD_UP;
                }
            } else {
                ref_up = -1;
            }
            if (held & PAD_DOWN) != 0 {
                ref_dn += 1;
                if ref_dn == 0 {
                    rep = PAD_DOWN;
                } else if ref_dn >= 31 && ref_dn % 3 == 0 {
                    rep |= PAD_DOWN;
                }
            } else {
                ref_dn = -1;
            }
            assert_eq!(pad.dir_press, rep | (press & (PAD_LEFT | PAD_RIGHT)));
        }
    }

    #[test]
    fn test_sound_mailbox_ring_and_60_frame_reverb_ramp() {
        let mut mbox = SoundMailbox::new();
        assert_eq!(
            mbox.dispatch(SND_CMD_BGM_PLAY as i32, BGM_SLOT_TNNLS as i32, 0, 0),
            0
        );
        assert_eq!(
            mbox.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CONFIRM as i32, 0),
            0
        );

        let mut events = Vec::new();
        mbox.drain_frame(&mut events);
        // 2 explicit messages + 1 reverb ramp message
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].msg, SND_CMD_BGM_PLAY);
        assert_eq!(events[0].arg_a, BGM_SLOT_TNNLS);
        assert_eq!(events[1].msg, SND_CMD_SE_PLAY);
        assert_eq!(events[1].arg_b, SE_CONFIRM);
        assert_eq!(events[2].msg, SND_CMD_REVERB_DEPTH);

        // Drain 59 more frames to reach reverb step 60
        for _ in 1..60 {
            events.clear();
            mbox.drain_frame(&mut events);
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].msg, SND_CMD_REVERB_DEPTH);
        }
        let max_depth = ((60u32 * 0x9ffec) / 127 / 60) as u16;
        assert_eq!(mbox.current_reverb_depth, max_depth);

        // Frame 61 should emit no further reverb ramp messages
        events.clear();
        mbox.drain_frame(&mut events);
        assert!(events.is_empty());
    }
}
