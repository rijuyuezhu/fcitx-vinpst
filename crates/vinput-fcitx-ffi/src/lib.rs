//! Narrow C ABI for the retained Fcitx C++ adapter.
//!
//! Unsafe code is confined to raw-pointer translation modules. The frontend
//! behavior itself remains in `vinput-fcitx-core`, where it is safe and directly
//! testable.

#[allow(unsafe_code)]
mod asr_projection;
#[allow(unsafe_code)]
mod daemon;
#[allow(unsafe_code)]
mod daemon_signal;
#[allow(unsafe_code)]
mod frontend;
#[allow(unsafe_code)]
mod menu;
#[allow(unsafe_code)]
mod menu_snapshot;
#[allow(unsafe_code)]
mod scene_projection;
#[allow(unsafe_code)]
mod trigger;
