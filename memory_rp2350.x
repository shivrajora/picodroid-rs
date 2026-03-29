MEMORY {
    FLASH      : ORIGIN = 0x10000000, LENGTH = 3072K             /* reduced by 1MB for PAPK_FLASH */
    PAPK_FLASH : ORIGIN = 0x10300000, LENGTH = 1024K            /* persistent PAPK slot (last 1MB of 4MB) */
    RAM        : ORIGIN = 0x20000000, LENGTH = 520K
}
