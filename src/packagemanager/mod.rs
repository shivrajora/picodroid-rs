// SPDX-License-Identifier: GPL-3.0-only
#[cfg(not(any(test, feature = "sim")))]
pub mod flash;
#[cfg(not(any(test, feature = "sim")))]
pub mod install;
#[cfg(not(any(test, feature = "sim")))]
pub mod transport;
