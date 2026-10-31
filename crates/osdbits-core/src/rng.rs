//! The two LCGs used by the original reconstruction.
//!
//! * `TowerRand` is the tower field generator's private LCG
//!   (`0x2b992ddf` seed, `opening.c` `towerRand`, "so field generation never
//!   touches the opening's osdRand() stream").
//! * `OsdRand` is the opening's own generator (`osdRand`), seeded with 1 and
//!   advanced by light setup and the illegal-scene fog init. The original
//!   notes the real boot's `lightsSeed` is draw #386 = 4002, i.e. the stream
//!   state at that point is part of the reconstructed behaviour.
//!
//! Both are bit-exact 32-bit wrapping LCGs. Tests pin concrete outputs so a
//! refactor cannot silently change the reconstructed imagery.

/// `towerRand()` — private LCG of the tower field generator.
#[derive(Clone, Debug)]
pub struct TowerRand {
    state: u32,
}

impl TowerRand {
    /// Real seed (`0x2b992ddf` in `opening.c`).
    pub const DEFAULT_SEED: u32 = 0x2b99_2ddf;

    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// Mix an external seed in, exactly like
    /// `towerRandState ^= argInt(base+0, cycles);`.
    pub fn xor_seed(&mut self, seed: u32) {
        self.state ^= seed;
    }

    pub fn state(&self) -> u32 {
        self.state
    }

    /// `state = state*1103515245 + 12345; return (state >> 16) & 0x7fff;`
    pub fn next_value(&mut self) -> i32 {
        self.state = self.state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        ((self.state >> 16) & 0x7fff) as i32
    }

    /// `towerRand() % n` with the C semantics for `n > 0`.
    pub fn below(&mut self, n: i32) -> i32 {
        assert!(n > 0, "modulus must be positive");
        self.next_value() % n
    }
}

/// `osdRand()` — the opening's generator, seeded with 1.
#[derive(Clone, Debug)]
pub struct OsdRand {
    state: u32,
}

impl OsdRand {
    pub fn new() -> Self {
        Self { state: 1 }
    }

    /// `state = state*1103515245 + 12345; return state & 0x7fffffff;`
    pub fn next_value(&mut self) -> i32 {
        self.state = self.state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (self.state & 0x7fff_ffff) as i32
    }

    pub fn below(&mut self, n: i32) -> i32 {
        assert!(n > 0, "modulus must be positive");
        self.next_value() % n
    }
}

impl Default for OsdRand {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tower_rand_pinned_sequence() {
        let mut r = TowerRand::new(TowerRand::DEFAULT_SEED);
        // Reference values computed from the C LCG
        // (state = state*1103515245 + 12345; (state>>16)&0x7fff).
        let got: Vec<i32> = (0..8).map(|_| r.next_value()).collect();
        assert_eq!(
            got,
            vec![21932, 24678, 24036, 7820, 5200, 29015, 18101, 17748]
        );
    }

    #[test]
    fn osd_rand_pinned_sequence() {
        let mut r = OsdRand::new();
        let got: Vec<i32> = (0..8).map(|_| r.next_value()).collect();
        assert_eq!(
            got,
            vec![
                1_103_527_590,
                377_401_575,
                662_824_084,
                1_147_902_781,
                2_035_015_474,
                368_800_899,
                1_508_029_952,
                486_256_185
            ]
        );
    }

    #[test]
    fn osd_rand_never_sets_sign_bit() {
        let mut r = OsdRand::new();
        for _ in 0..10_000 {
            assert!(r.next_value() >= 0);
        }
    }

    #[test]
    fn tower_rand_is_bounded() {
        let mut r = TowerRand::new(1234);
        for _ in 0..10_000 {
            let v = r.next_value();
            assert!((0..=0x7fff).contains(&v), "{v}");
        }
    }

    #[test]
    fn xor_seed_is_reversible_and_changes_stream() {
        let mut a = TowerRand::new(TowerRand::DEFAULT_SEED);
        let mut b = TowerRand::new(TowerRand::DEFAULT_SEED);
        b.xor_seed(0x1234_5678);
        assert_ne!(a.next_value(), b.next_value());

        let mut c = TowerRand::new(TowerRand::DEFAULT_SEED);
        c.xor_seed(0x1234_5678);
        c.xor_seed(0x1234_5678);
        let mut d = TowerRand::new(TowerRand::DEFAULT_SEED);
        assert_eq!(c.next_value(), d.next_value());
    }

    #[test]
    fn below_stays_in_range() {
        let mut r = OsdRand::new();
        for n in 1..40 {
            for _ in 0..200 {
                let v = r.below(n);
                assert!((0..n).contains(&v));
            }
        }
    }

    #[test]
    #[should_panic(expected = "modulus must be positive")]
    fn below_rejects_zero() {
        let mut r = OsdRand::new();
        r.below(0);
    }
}
