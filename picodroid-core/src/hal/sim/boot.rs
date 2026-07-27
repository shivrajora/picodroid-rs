// SPDX-License-Identifier: GPL-3.0-only
//! Simulator boot stub: there is no hardware clock tree to bring up.
//!
//! Empty for every family, which is why it is here rather than copied into
//! each one. A family whose simulator does need setup at this point defines
//! its own `boot` module instead of re-exporting this.

#[allow(dead_code)]
pub fn clock_init() {}
