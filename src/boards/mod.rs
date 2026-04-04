//! Board configuration — selects pin mapping, SPI bus, and calibration
//! constants for a specific hardware board.

#[cfg(all(not(any(feature = "sim", test)), feature = "board-waveshare-pico-28"))]
mod waveshare_pico_28;
#[cfg(all(not(any(feature = "sim", test)), feature = "board-waveshare-pico-28"))]
pub use waveshare_pico_28::*;
