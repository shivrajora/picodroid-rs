MEMORY {
    BOOT2      : ORIGIN = 0x10000000, LENGTH = 0x100
    FLASH      : ORIGIN = 0x10000100, LENGTH = 1024K - 0x100   /* reduced by 1MB for PAPK_FLASH */
    PAPK_FLASH : ORIGIN = 0x10100000, LENGTH = 1024K            /* persistent PAPK slot (last 1MB of 2MB) */
    RAM        : ORIGIN = 0x20000000, LENGTH = 256K
}

EXTERN(BOOT2_FIRMWARE)

SECTIONS {
    /* ### Boot loader */
    .boot2 ORIGIN(BOOT2) :
    {
        KEEP(*(.boot2));
    } > BOOT2
} INSERT BEFORE .text;