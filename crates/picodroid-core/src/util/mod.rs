// SPDX-License-Identifier: GPL-3.0-only
pub mod local;
pub mod local_ring;
// Reaches the JVM natives' logging path, which host tests do not build.
#[cfg(not(test))]
pub mod log;
