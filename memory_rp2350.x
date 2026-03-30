MEMORY {
    FLASH       : ORIGIN = 0x10000000, LENGTH = 3072K               /* first 3MB of 4MB flash */
    PAPK_FLASH  : ORIGIN = 0x10300000, LENGTH = 1024K               /* persistent PAPK slot (last 1MB of 4MB) */
    RAM         : ORIGIN = 0x20000000, LENGTH = 520K
}

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
