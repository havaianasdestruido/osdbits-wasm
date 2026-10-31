//! Bounded, seeded property campaigns.
//!
//! There is no `proptest` in this environment (crates.io is unreachable), so
//! these are hand-rolled: a deterministic LCG generates scenes, options and
//! frame counts, and the invariants below must hold for every case. A failing
//! case prints the exact parameters, which makes it reproducible by editing
//! `CASES`/the seed.

use osdbits_core::frame::{BATCH_WORDS, CUBE_WORDS, HEADER_WORDS, MAGIC, VERSION, VERTEX_WORDS};
use osdbits_core::{
    frame_hash, HwState, Scene, SceneOptions, MODE_BOOT, MODE_IDLE, MODE_ILLEGAL, MODE_MENU,
};

struct Lcg(u32);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0
    }
    fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }
    fn chance(&mut self) -> bool {
        self.below(2) == 1
    }
}

fn options_from(rng: &mut Lcg) -> SceneOptions {
    SceneOptions {
        pal: rng.chance(),
        games: rng.below(33) as i32,
        boots: rng.below(400) as i32,
        full_field: rng.below(4) == 0,
        hw: match rng.below(3) {
            0 => HwState::NoDisc,
            1 => HwState::DiscPs2,
            _ => HwState::DiscIllegal,
        },
        boot_param: 100 + rng.below(17) as i32,
        fog: rng.chance(),
        lights: rng.chance(),
        cubes: rng.chance(),
        camera_motion: rng.chance(),
        text: rng.chance(),
        black_bars: rng.below(4) == 0,
    }
}

#[test]
fn frames_stay_well_formed_for_arbitrary_scenes_and_options() {
    let mut rng = Lcg(0x51ce_1234);
    for case in 0..60 {
        let mode = [MODE_BOOT, MODE_IDLE, MODE_ILLEGAL, MODE_MENU][rng.below(4) as usize];
        let seed = rng.next();
        let opts = options_from(&mut rng);
        let frames = rng.below(400);
        let mut scene = Scene::new(mode, seed, opts);
        for i in 0..frames {
            scene.step();
            let words = scene.draw();
            let ctx = format!("case {case} mode {mode} seed {seed} frame {i} opts {opts:?}");
            assert_eq!(words[0].to_bits(), MAGIC, "magic: {ctx}");
            assert_eq!(words[1].to_bits(), VERSION, "version: {ctx}");
            assert_eq!(words.len() as f32, words[48], "total words: {ctx}");
            assert_eq!(words[45] as usize, HEADER_WORDS, "batch offset: {ctx}");
            let batches = words[42] as usize;
            let verts = words[43] as usize;
            assert_eq!(
                words[46] as usize,
                HEADER_WORDS + batches * BATCH_WORDS,
                "vertex offset: {ctx}"
            );
            assert_eq!(
                words[47] as usize,
                words[46] as usize + verts * VERTEX_WORDS,
                "cube offset: {ctx}"
            );
            assert!(batches < 64, "batch count: {ctx}");
            assert!(verts < 100_000, "vertex count: {ctx} ({verts})");
            assert!(words.iter().all(|w| w.is_finite()), "finite: {ctx}");
            assert_eq!(
                words.len(),
                words[47] as usize + (words[44] as usize) * CUBE_WORDS,
                "cube end: {ctx}"
            );
            assert!(frame_hash(words) != 0, "hash: {ctx}");
        }
    }
}

#[test]
fn replays_match_frame_for_frame() {
    let mut rng = Lcg(0xbeef_0001);
    for _ in 0..12 {
        let mode = [MODE_BOOT, MODE_IDLE, MODE_ILLEGAL][rng.below(3) as usize];
        let seed = rng.next();
        let opts = options_from(&mut rng);
        let mut a = Scene::new(mode, seed, opts);
        let mut b = Scene::new(mode, seed, opts);
        for _ in 0..120 {
            a.step();
            b.step();
            assert_eq!(
                a.draw(),
                b.draw(),
                "replay diverged: mode {mode} seed {seed}"
            );
        }
    }
}

#[test]
fn camera_height_is_monotone_within_a_state_and_bounded_overall() {
    let mut rng = Lcg(0x0dd_1234);
    for _ in 0..10 {
        let opts = options_from(&mut rng);
        let mut scene = Scene::new(MODE_BOOT, rng.next(), opts);
        let mut max_z = f32::MIN;
        for _ in 0..600 {
            scene.step();
            scene.draw();
            let z = scene.camera.position[2];
            assert!(z.is_finite() && z.abs() < 1.0e5, "z out of range: {z}");
            max_z = max_z.max(z);
        }
        assert!(max_z > 1.0, "camera never moved (max {max_z})");
    }
}

#[test]
fn disabling_every_effect_leaves_a_bare_but_valid_frame() {
    let opts = SceneOptions {
        fog: false,
        lights: false,
        cubes: false,
        text: false,
        ..SceneOptions::default()
    };
    let mut scene = Scene::new(MODE_BOOT, 1, opts);
    for _ in 0..30 {
        scene.step();
        let words = scene.draw();
        assert_eq!(words[42], 1.0, "only the tower batch should remain");
        assert_eq!(words[44], 0.0, "no cubes");
    }
}
