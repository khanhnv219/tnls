//! Pure domain logic for tnls: tunnel profiles, runtime states, errors.
//!
//! This crate intentionally has no Tauri dependency so `cargo test`
//! runs without linking the WebView2 stack (which breaks test binaries
//! on some Windows toolchains).

pub mod persistence;
pub mod tunnel;
