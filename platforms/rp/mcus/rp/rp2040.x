/* The MEMORY block (BOOT2, FLASH, FS_FLASH, PAPK_FLASH, RAM) and the __fs_*
   symbols are generated in front of this file by build.rs from rp2040.toml
   and the board's overrides (build_support/flash_layout.rs); only the
   SECTIONS this MCU needs live here. */

EXTERN(BOOT2_FIRMWARE)

SECTIONS {
    /* ### Boot loader */
    .boot2 ORIGIN(BOOT2) :
    {
        KEEP(*(.boot2));
    } > BOOT2
} INSERT BEFORE .text;