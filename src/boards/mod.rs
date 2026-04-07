//! Board configuration — selects pin mapping, SPI bus, and calibration
//! constants for a specific hardware board.

#[cfg(all(not(any(feature = "sim", test)), feature = "board-testbench"))]
mod testbench;
#[cfg(all(not(any(feature = "sim", test)), feature = "board-testbench"))]
pub use testbench::*;

#[cfg(all(not(any(feature = "sim", test)), feature = "board-pico-enviro-mon"))]
mod pico_enviro_mon;
#[cfg(all(not(any(feature = "sim", test)), feature = "board-pico-enviro-mon"))]
pub use pico_enviro_mon::*;
