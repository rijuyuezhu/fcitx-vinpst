//! Shared borrowed UTF-8 views at the C ABI boundary.

use std::ptr;

/// Borrowed UTF-8 byte view valid while its owner remains alive.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxStringView {
    /// UTF-8 bytes, or null when `len` is zero.
    pub data: *const u8,
    /// Number of readable bytes.
    pub len: usize,
}

/// Borrows a UTF-8 string from a raw pointer and byte length.
///
/// # Safety
///
/// A non-null pointer must reference `len` readable bytes for the returned lifetime.
pub(crate) unsafe fn text_input<'a>(data: *const u8, len: usize) -> Option<&'a str> {
    if data.is_null() {
        return (len == 0).then_some("");
    }
    // SAFETY: Forwarded from this function's caller contract.
    std::str::from_utf8(unsafe { std::slice::from_raw_parts(data, len) }).ok()
}

/// Borrows a UTF-8 string from a C string view.
///
/// # Safety
///
/// A non-null view pointer must reference `view.len` readable bytes for the returned lifetime.
pub(crate) unsafe fn text_view_input<'a>(view: VinputFcitxStringView) -> Option<&'a str> {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { text_input(view.data, view.len) }
}

/// Creates a borrowed C view over a Rust string.
pub(crate) fn string_view(value: &str) -> VinputFcitxStringView {
    VinputFcitxStringView {
        data: if value.is_empty() {
            ptr::null()
        } else {
            value.as_ptr()
        },
        len: value.len(),
    }
}
