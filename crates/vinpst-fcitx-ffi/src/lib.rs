//! Narrow C ABI for the retained Fcitx C++ adapter.
//!
//! Unsafe code is confined to raw-pointer translation modules. The frontend
//! behavior itself remains in `vinpst-fcitx-core`, where it is safe and directly
//! testable.

use std::panic::{AssertUnwindSafe, catch_unwind};

fn ffi_catch<T>(fallback: T, operation: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(operation)).unwrap_or(fallback)
}

#[allow(unsafe_code)]
mod daemon;
#[allow(unsafe_code)]
mod daemon_signal;
#[allow(unsafe_code)]
mod ffi_string;
#[allow(unsafe_code)]
mod frontend;
#[allow(unsafe_code)]
mod menu;
#[allow(unsafe_code)]
mod menu_controller;
#[allow(unsafe_code)]
mod menu_projection;
#[allow(unsafe_code)]
mod trigger;

#[cfg(test)]
mod tests {
    use super::ffi_catch;

    #[test]
    fn ffi_catch_maps_panics_to_the_declared_fallback() {
        assert_eq!(ffi_catch(7_u8, || panic!("ffi boundary test")), 7);
        assert_eq!(ffi_catch(7_u8, || 9), 9);
    }
}
