MEMORY {
    BOOT2      : ORIGIN = 0x10000000, LENGTH = 0x100
    FLASH      : ORIGIN = 0x10000100, LENGTH = 896K - 0x100    /* program image (reduced to make room for FS region) */
    FS_FLASH   : ORIGIN = 0x100E0000, LENGTH = 128K            /* LittleFS region (32 × 4KB sectors) */
    PAPK_FLASH : ORIGIN = 0x10100000, LENGTH = 1024K           /* persistent PAPK slot (last 1MB of 2MB) */
    RAM        : ORIGIN = 0x20000000, LENGTH = 256K
}

__fs_start = ORIGIN(FS_FLASH);
__fs_end   = ORIGIN(FS_FLASH) + LENGTH(FS_FLASH);

EXTERN(BOOT2_FIRMWARE)

SECTIONS {
    /* ### Boot loader */
    .boot2 ORIGIN(BOOT2) :
    {
        KEEP(*(.boot2));
    } > BOOT2
} INSERT BEFORE .text;