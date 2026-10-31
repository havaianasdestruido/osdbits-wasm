//! The two text overlays' state machines.
//!
//! Ported from `DoSCEText` (`0x214c20`) and `DoIllegalText` (`0x214e60`).
//! Only the *state machines* live here: the original draws proprietary bitmap
//! strips from the BIOS, so the browser renders our own typographic
//! reconstruction of the wording and fades it with these alphas. That
//! substitution is recorded in `docs/ASSET-POLICY.md` / `docs/LIMITATIONS.md`.
//!
//! * SCE text: armed once the camera passes height 18, alpha ramps 0 -> 240
//!   in steps of 4, then back down, then disarms (`state = -1`). Drawn at
//!   `min(alpha, 112)`.
//! * Illegal-disc text: armed past height 800; alpha ramps +1/frame, or
//!   -1/frame once the scene end flag is set, drawn clamped to 112.

/// `-1` disarmed, `0` armed, `1` fading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextOverlay {
    pub sce_state: i32,
    pub sce_alpha: i32,
    pub sce_step: i32,
    pub illegal_state: i32,
    pub illegal_alpha: i32,
}

impl Default for TextOverlay {
    fn default() -> Self {
        Self::new(0)
    }
}

impl TextOverlay {
    /// `initTextShit` (`0x214f20`): arm the overlay that matches the opening
    /// type and reset the fade.
    pub fn new(opening_type: i32) -> Self {
        let mut t = TextOverlay {
            sce_state: 0,
            sce_alpha: 0,
            sce_step: 4,
            illegal_state: 0,
            illegal_alpha: 0,
        };
        if opening_type == 0 {
            t.sce_state = 0;
            t.illegal_state = -1;
        } else {
            t.illegal_state = 0;
            t.sce_state = -1;
        }
        t
    }

    /// Advance both overlays one frame. Returns `(sce_draw_alpha,
    /// illegal_draw_alpha)`, each `None` when the overlay is not drawing this
    /// frame.
    pub fn step(&mut self, pos_z: f32, opening_end_flag: i32) -> (Option<i32>, Option<i32>) {
        let sce = self.step_sce(pos_z);
        let illegal = self.step_illegal(pos_z, opening_end_flag);
        (sce, illegal)
    }

    fn step_sce(&mut self, pos_z: f32) -> Option<i32> {
        if pos_z > 18.0 && self.sce_state == 0 {
            self.sce_state = 1;
        }
        if self.sce_state != 1 {
            return None;
        }
        let alpha = self.sce_alpha + self.sce_step;
        self.sce_alpha = alpha;
        if alpha == 240 {
            self.sce_step = -4;
        }
        if alpha == 0 {
            self.sce_step = 4;
            self.sce_state = -1;
        }
        Some(alpha.min(112))
    }

    fn step_illegal(&mut self, pos_z: f32, opening_end_flag: i32) -> Option<i32> {
        if pos_z > 800.0 && self.illegal_state == 0 {
            self.illegal_state = 1;
        }
        if self.illegal_state != 1 {
            return None;
        }
        let mut alpha = self.illegal_alpha;
        if opening_end_flag == 0 {
            alpha += 1;
        } else if alpha > 0 {
            alpha -= 1;
        }
        let drawn = alpha.min(112);
        self.illegal_alpha = drawn;
        Some(drawn)
    }

    /// The SCE overlay's screen rectangles for NTSC/PAL, in virtual-screen
    /// pixels (`0x2a4318`): two 256x16 strips either side of the centre.
    pub fn sce_rects(pal: bool) -> [(f32, f32, f32, f32); 2] {
        if pal {
            [(120.0, 120.0, 256.0, 18.0), (326.0, 120.0, 256.0, 18.0)]
        } else {
            [(120.0, 105.0, 256.0, 16.0), (326.0, 105.0, 256.0, 16.0)]
        }
    }

    /// The illegal-disc text's screen rect (`0x2a4578`), with the PAL
    /// y/height scaling from `DrawIllegalText`.
    pub fn illegal_rect(pal: bool) -> (f32, f32, f32, f32) {
        let (mut y, mut h) = (88.0f32, 64.0f32);
        if pal {
            y *= 0.526_271_05 / 0.457_627;
            h *= 0.526_271_05 / 0.457_627;
        }
        (64.0, y, 512.0, h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_arms_exactly_one_overlay() {
        let t = TextOverlay::new(0);
        assert_eq!(t.sce_state, 0);
        assert_eq!(t.illegal_state, -1);
        let t = TextOverlay::new(1);
        assert_eq!(t.sce_state, -1);
        assert_eq!(t.illegal_state, 0);
        assert_eq!(t.sce_alpha, 0);
        assert_eq!(t.sce_step, 4);
    }

    #[test]
    fn sce_text_is_inert_below_height_18() {
        let mut t = TextOverlay::new(0);
        for _ in 0..100 {
            assert_eq!(t.step(17.9, 0).0, None);
        }
        assert_eq!(t.sce_alpha, 0);
    }

    #[test]
    fn sce_text_ramps_up_holds_and_fades_out() {
        let mut t = TextOverlay::new(0);
        let mut max_alpha = 0;
        let mut frames = 0;
        let mut saw_reverse = false;
        while let Some(a) = t.step(20.0, 0).0 {
            frames += 1;
            assert!((0..=112).contains(&a));
            if a < max_alpha {
                saw_reverse = true;
            }
            max_alpha = max_alpha.max(a);
            assert!(frames < 200, "overlay never disarmed");
        }
        assert_eq!(max_alpha, 112, "drawn alpha is min(alpha,112)");
        assert!(saw_reverse, "fade-out never observed");
        assert_eq!(t.sce_state, -1);
        assert_eq!(t.sce_alpha, 0);
        // 60 frames up (4..240) and 60 down (236..0): 120 drawn frames.
        assert_eq!(frames, 120);
    }

    #[test]
    fn illegal_text_ramps_and_clamps_at_112() {
        let mut t = TextOverlay::new(1);
        for _ in 0..10 {
            assert_eq!(t.step(799.0, 0).1, None);
        }
        let mut last = 0;
        for _ in 0..300 {
            if let Some(a) = t.step(850.0, 0).1 {
                assert!(a <= 112);
                last = a;
            }
        }
        assert_eq!(last, 112);
        assert_eq!(t.illegal_alpha, 112);
    }

    #[test]
    fn illegal_text_fades_down_when_the_end_flag_sets() {
        let mut t = TextOverlay::new(1);
        for _ in 0..50 {
            t.step(850.0, 0);
        }
        let before = t.illegal_alpha;
        assert!(before > 0);
        for _ in 0..(before + 5) {
            t.step(850.0, 1);
        }
        assert_eq!(t.illegal_alpha, 0);
        // Never goes negative.
        for _ in 0..10 {
            t.step(850.0, 1);
        }
        assert_eq!(t.illegal_alpha, 0);
    }

    #[test]
    fn rects_match_the_reconstruction() {
        let ntsc = TextOverlay::sce_rects(false);
        assert_eq!(ntsc[0], (120.0, 105.0, 256.0, 16.0));
        assert_eq!(ntsc[1], (326.0, 105.0, 256.0, 16.0));
        let pal = TextOverlay::sce_rects(true);
        assert_eq!(pal[0].1, 120.0);
        assert_eq!(pal[0].3, 18.0);
        let (x, y, w, h) = TextOverlay::illegal_rect(false);
        assert_eq!((x, y, w, h), (64.0, 88.0, 512.0, 64.0));
        let (_, y_pal, _, h_pal) = TextOverlay::illegal_rect(true);
        assert!(y_pal > 88.0 && h_pal > 64.0);
    }

    #[test]
    fn overlay_states_are_independent() {
        let mut t = TextOverlay::new(1);
        // The SCE overlay stays disarmed in the illegal scene.
        for _ in 0..300 {
            assert_eq!(t.step(900.0, 0).0, None);
        }
        assert_eq!(t.sce_state, -1);
    }
}
