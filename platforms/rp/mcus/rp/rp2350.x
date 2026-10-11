/* The MEMORY block (FLASH, FS_FLASH, PAPK_FLASH, RAM) and the __fs_* symbols
   are generated in front of this file by build.rs from rp2350.toml and the
   board's overrides (build_support/flash_layout.rs); only the SECTIONS this
   MCU needs live here. */

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

/* Tell cortex-m-rt to start .text after .start_block. .text carries
   16-byte-aligned input sections and the IMAGE_DEF block ends 8-aligned, so
   round up here: an explicit section address is not aligned by the linker,
   which otherwise warns "address of section .text is not a multiple of
   alignment (16)". */
_stext = ALIGN(ADDR(.start_block) + SIZEOF(.start_block), 16);
