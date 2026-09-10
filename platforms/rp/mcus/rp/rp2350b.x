/* The MEMORY block (FLASH, FS_FLASH, PAPK_FLASH, RAM) and the __fs_* symbols
   are generated in front of this file by build.rs from rp2350b.toml and the
   board's overrides (build_support/flash_layout.rs); only the SECTIONS this
   MCU needs live here.

   Identical to rp2350.x today: the A and B variants share one bootrom and one
   IMAGE_DEF contract, and `build_support/boards.rs` resolves the tail by MCU
   name, so the B variant needs a file of its own regardless. It is a separate
   file rather than a shared one because what is B-only lands here — the 8 MB
   of PSRAM the A variant does not have needs its own MEMORY region and its own
   section placement (docs/designs/psram-rp2350b-2026-09.md). */

SECTIONS {
    /* RP2350 IMAGE_DEF block — placed right after the vector table so the
       bootrom can find it within the first 4KB sector of flash.  The vector
       table stays at ORIGIN(FLASH) = 0x10000000 (cortex-m-rt default), so
       the bootrom reads SP/Reset from 0x10000000 naturally. */
    .start_block ADDR(.vector_table) + SIZEOF(.vector_table) :
    {
        KEEP(*(.start_block));
    } > FLASH
} INSERT BEFORE .text;

/* Tell cortex-m-rt to start .text after .start_block */
_stext = ADDR(.start_block) + SIZEOF(.start_block);
