// Embed the PAPK flash initialiser section.
// build.rs generates papk_flash_init.rs which declares a static array placed
// in the .papk_flash_init section at the PAPK_FLASH address.  probe-rs writes
// this section when flashing the ELF, so the persistent PAPK region is always
// updated to match the baked-in APK on every probe flash.
#[cfg(not(any(test, feature = "sim")))]
include!(concat!(env!("OUT_DIR"), "/papk_flash_init.rs"));

// Re-export all flash operations and constants from the HAL.
pub use crate::hal::flash::*;
