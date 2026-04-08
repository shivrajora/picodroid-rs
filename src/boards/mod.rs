//! Board configuration — selects pin mapping, SPI bus, and calibration
//! constants for a specific hardware board.

#[cfg(all(not(any(feature = "sim", test)), feature = "board-testbench-rp2040"))]
#[path = "../../boards/testbench_rp2040/mod.rs"]
mod testbench;
#[cfg(all(not(any(feature = "sim", test)), feature = "board-testbench-rp2040"))]
pub use testbench::*;

#[cfg(all(not(any(feature = "sim", test)), feature = "board-testbench-rp2350"))]
#[path = "../../boards/testbench_rp2350/mod.rs"]
mod testbench;
#[cfg(all(not(any(feature = "sim", test)), feature = "board-testbench-rp2350"))]
pub use testbench::*;

#[cfg(all(not(any(feature = "sim", test)), feature = "board-testbench-rp2350w"))]
#[path = "../../boards/testbench_rp2350w/mod.rs"]
mod testbench;
#[cfg(all(not(any(feature = "sim", test)), feature = "board-testbench-rp2350w"))]
pub use testbench::*;

#[cfg(all(not(any(feature = "sim", test)), feature = "board-pico-enviro-mon"))]
#[path = "../../boards/pico_enviro_mon/mod.rs"]
mod pico_enviro_mon;
#[cfg(all(not(any(feature = "sim", test)), feature = "board-pico-enviro-mon"))]
pub use pico_enviro_mon::*;
