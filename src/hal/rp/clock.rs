//! Chip clock constants — single source of truth for the RP family.

#[cfg(feature = "chip-rp2040")]
pub const PCLK_HZ: u32 = 125_000_000;
#[cfg(feature = "chip-rp2350-hal")]
pub const PCLK_HZ: u32 = 150_000_000;
