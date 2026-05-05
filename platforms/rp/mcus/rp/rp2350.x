MEMORY {
    FLASH       : ORIGIN = 0x10000000, LENGTH = 2816K               /* program image (reduced to make room for FS region) */
    FS_FLASH    : ORIGIN = 0x102C0000, LENGTH = 256K                /* LittleFS region (64 × 4KB sectors) */
    PAPK_FLASH  : ORIGIN = 0x10300000, LENGTH = 1024K               /* persistent PAPK slot (last 1MB of 4MB) */
    RAM         : ORIGIN = 0x20000000, LENGTH = 520K
}

__fs_start = ORIGIN(FS_FLASH);
__fs_end   = ORIGIN(FS_FLASH) + LENGTH(FS_FLASH);

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
