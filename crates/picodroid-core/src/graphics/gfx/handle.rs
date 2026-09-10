// SPDX-License-Identifier: GPL-3.0-only
//! Backend-agnostic widget handle.
//!
//! Java-side `nativeHandle` is a 32-bit `int`; this newtype is the in-Rust
//! representation that crosses the [`super::Gfx`] trait surface. The concrete
//! meaning of the bits (raw pointer, table index, …) is a backend-private
//! detail.

/// Opaque widget handle. `0` is reserved as "null/invalid".
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Handle(u32);

impl Handle {
    /// Sentinel for "no widget".
    pub const NULL: Handle = Handle(0);

    /// Wrap a raw 32-bit id from the backend's storage.
    #[inline(always)]
    pub fn from_raw(raw: u32) -> Self {
        Handle(raw)
    }

    /// Unwrap to the raw 32-bit id.
    #[inline(always)]
    pub fn raw(self) -> u32 {
        self.0
    }

    /// Wrap a Java `nativeHandle` (i32) for boundary conversion.
    /// Negative values map to [`Handle::NULL`].
    #[inline(always)]
    pub fn from_java(id: i32) -> Self {
        if id <= 0 {
            Handle::NULL
        } else {
            Handle(id as u32)
        }
    }

    /// Convert back to a Java `nativeHandle` (i32) at the boundary.
    #[inline(always)]
    pub fn to_java(self) -> i32 {
        self.0 as i32
    }

    #[inline(always)]
    pub fn is_null(self) -> bool {
        self.0 == 0
    }
}
