//! osdbits-core — deterministic PlayStation 2 OSDSYS simulation, resource decoder,
//! and render command generator for WebAssembly.
//!
//! Lint policy: this crate is a line-by-line port of numeric C code (`osdbits/*.c`) and of the
//! PS2 GS/VU register model, so index-driven loops and wide vertex/emit signatures are the
//! notation, not an accident. Those specific lints are allowed here and everywhere else the
//! workspace keeps `-D warnings`.
#![allow(clippy::needless_range_loop)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

pub mod assets;
pub mod ffi;
pub mod illegal;
pub mod input_audio;
pub mod math;
pub mod menu;
pub mod menuconfig;
pub mod menutext;
pub mod opening;
pub mod render_cmd;
pub mod resource;
