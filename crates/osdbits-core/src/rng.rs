//! Exact PlayStation 2 OSDSYS pseudo-random number generators and deterministic
//! property-testing seed utilities.

/// Exact reconstruction of `osdRand()` (`osdbits/opening.c:1145` and `menu.c`):
/// 32-bit newlib-1999 LCG (`seed = seed * 1103515245 + 12345`), returning the
/// non-negative 31-bit value `seed & 0x7fff_ffff`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OsdRng {
    state: u32,
    draws: u64,
}

impl OsdRng {
    pub const DEFAULT_SEED: u32 = 1;
    const INV_2_POW_31: f32 = 4.656_613e-10_f32;

    #[must_use]
    pub const fn new(seed: u32) -> Self {
        Self {
            state: seed,
            draws: 0,
        }
    }

    #[must_use]
    pub const fn state(&self) -> u32 {
        self.state
    }

    #[must_use]
    pub const fn draws(&self) -> u64 {
        self.draws
    }

    pub fn reset(&mut self, seed: u32) {
        self.state = seed;
        self.draws = 0;
    }

    #[inline]
    pub fn next_u31(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        self.draws += 1;
        self.state & 0x7fff_ffff
    }

    #[inline]
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u31() as f32) * Self::INV_2_POW_31
    }

    #[inline]
    pub fn next_range_f32(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }

    #[inline]
    pub fn next_mod(&mut self, modulus: u32) -> u32 {
        if modulus == 0 {
            return 0;
        }
        self.next_u31() % modulus
    }
}

impl Default for OsdRng {
    fn default() -> Self {
        Self::new(Self::DEFAULT_SEED)
    }
}

/// Exact reconstruction of `towerRand()` (`osdbits/opening.c:3239`):
/// Separate LCG seeded at `0x2b992ddf ^ user_seed` returning `(state >> 16) & 0x7fff`
/// so tower-field history simulation never perturbs `osdRand()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TowerRng {
    state: u32,
}

impl TowerRng {
    pub const BASE_SEED: u32 = 0x2b99_2ddf;

    #[must_use]
    pub const fn new(user_seed: u32) -> Self {
        Self {
            state: Self::BASE_SEED ^ user_seed,
        }
    }

    #[must_use]
    pub const fn state(&self) -> u32 {
        self.state
    }

    #[inline]
    pub fn next_u15(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (self.state >> 16) & 0x7fff
    }

    #[inline]
    pub fn next_mod(&mut self, modulus: u32) -> u32 {
        if modulus == 0 {
            return 0;
        }
        self.next_u15() % modulus
    }
}

/// Fast deterministic SplitMix64 generator used exclusively for reproducible
/// property-based and bounded-fuzz test case generation.
#[derive(Clone, Copy, Debug)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    pub fn next_f32_unit(&mut self) -> f32 {
        ((self.next_u64() >> 40) as f32) * (1.0 / ((1u32 << 24) as f32))
    }

    pub fn next_f32_range(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32_unit() * (max - min)
    }

    pub fn fill_bytes(&mut self, out: &mut [u8]) {
        for chunk in out.chunks_mut(8) {
            let bytes = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osd_rng_draw_386_matches_savestate_verified_lights_seed() {
        // Upstream opening.c:1141 notes:
        // "lightsSeed = draw #386 = 4002, savestate-verified"
        // where lightsSeed = osdRand() % 2345 + 3456 on draw #386 from seed = 1!
        let mut rng = OsdRng::new(1);
        for _ in 0..385 {
            let _ = rng.next_u31();
        }
        let draw_386 = rng.next_u31();
        assert_eq!(rng.draws(), 386);
        let lights_seed = draw_386 % 2345 + 3456;
        assert_eq!(lights_seed, 4002);
    }

    #[test]
    fn osd_rng_and_tower_rng_are_deterministic_on_reset() {
        let mut prop = SplitMix64::new(0x5eed_cafe_1234_5678);
        for _ in 0..512 {
            let s = prop.next_u32();
            let mut a = OsdRng::new(s);
            let mut b = OsdRng::new(0);
            b.reset(s);
            for _ in 0..16 {
                let va = a.next_u31();
                let vb = b.next_u31();
                assert_eq!(va, vb);
                assert!(va <= 0x7fff_ffff);
            }

            let mut ta = TowerRng::new(s);
            let mut tb = TowerRng::new(s);
            for _ in 0..16 {
                let x = ta.next_u15();
                assert_eq!(x, tb.next_u15());
                assert!(x <= 0x7fff);
            }
        }
    }
}
