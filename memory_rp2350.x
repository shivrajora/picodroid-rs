MEMORY {
    START_BLOCK : ORIGIN = 0x10000000, LENGTH = 0x100
    FLASH       : ORIGIN = 0x10000100, LENGTH = 3072K - 0x100    /* reduced by 1MB for PAPK_FLASH */
    PAPK_FLASH  : ORIGIN = 0x10300000, LENGTH = 1024K            /* persistent PAPK slot (last 1MB of 4MB) */
    RAM         : ORIGIN = 0x20000000, LENGTH = 520K
}

SECTIONS {
    /* RP2350 IMAGE_DEF block — the bootrom scans for this at the start of flash. */
    .start_block ORIGIN(START_BLOCK) :
    {
        KEEP(*(.start_block));
    } > START_BLOCK
} INSERT BEFORE .text;
