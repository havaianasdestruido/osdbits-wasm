//! Deterministic PlayStation 2 OSDSYS visual simulation core (`osdbits-core`).
//!
//! Zero external crate dependencies; `#![forbid(unsafe_code)]`; bit-reproducible
//! across host targets (`x86_64`) and WebAssembly (`wasm32-unknown-unknown`).

pub mod expand;
pub mod math;
pub mod menu;
pub mod opening;
pub mod scene;
pub mod towers;

/// ABI handshake version shared between `osdbits-core`, `osdbits-wasm`, and the TypeScript bridge.
pub const ABI_VERSION: u32 = 1;

/// Magic identifier (`"OSDB"`) verifying WASM instance integrity.
pub const ABI_MAGIC: u32 = 0x4F53_4442;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_constants_are_stable() {
        assert_eq!(ABI_VERSION, 1);
        assert_eq!(ABI_MAGIC, 0x4F53_4442);
    }
}
