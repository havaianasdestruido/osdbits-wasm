//! osdbits-core — deterministic simulation core of the browser reconstruction
//! of the PS2 OSDSYS opening / illegal-disc scenes.
//!
//! The crate is dependency-free (see `docs/ARCHITECTURE.md`), so it builds
//! natively for tests and as `wasm32-unknown-unknown` for the browser. The
//! supported boundary is the `extern "C"` ABI in [`abi`]; everything else is
//! internal and covered by unit/property tests.

// The port keeps the reconstruction's explicit index arithmetic (`for i in
// 0..N { a[i] = f(b[i], c[i]) }`) rather than rewriting it as iterator
// chains: the loops index several parallel arrays with the same index and the
// faithfulness of the arithmetic matters more than the lint. The two lints
// below are therefore allowed crate-wide, deliberately and documented in
// docs/PORTING-NOTES.md.
#![allow(clippy::needless_range_loop)]
#![allow(clippy::too_many_arguments)]

pub mod abi;
pub mod camera;
pub mod cubes;
pub mod field;
pub mod fog;
pub mod frame;
pub mod illegal;
pub mod lights;
pub mod math;
pub mod rng;
pub mod scene;
pub mod text;

pub use camera::{Camera, CameraEvent, HwState};
pub use cubes::{CubeInstance, CubePass, Cubes};
pub use field::{HistEntry, TowerField};
pub use fog::Fog;
pub use frame::{
    frame_hash, FrameWriter, BATCH_WORDS, CUBE_WORDS, HEADER_WORDS, MAGIC, VERSION, VERTEX_WORDS,
};
pub use illegal::IllegalScene;
pub use lights::Lights;
pub use math::{Mat4, Vec4};
pub use rng::{OsdRand, TowerRand};
pub use scene::{Scene, SceneKind, SceneOptions, MODE_BOOT, MODE_IDLE, MODE_ILLEGAL, MODE_MENU};
pub use text::TextOverlay;
