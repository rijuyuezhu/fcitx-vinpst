//! Narrow C ABI for the retained Fcitx C++ adapter.
//!
//! Unsafe code is confined to raw-pointer translation modules. The frontend
//! behavior itself remains in `vinput-fcitx-core`, where it is safe and directly
//! testable.

#[allow(unsafe_code)]
mod raw;
#[allow(unsafe_code)]
mod trigger;
