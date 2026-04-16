//! Board configuration — peripheral declarations live in board.toml;
//! build.rs generates display_config.rs / touch_config.rs / sensor_table.rs.
//! Board mod.rs files only exist for board-specific constants (e.g. WiFi pins).

#[cfg(all(not(any(feature = "sim", test)), feature = "board-testbench-rp2350w"))]
#[path = "../../boards/testbench_rp2350w/mod.rs"]
mod testbench;
#[cfg(all(not(any(feature = "sim", test)), feature = "board-testbench-rp2350w"))]
pub use testbench::*;
