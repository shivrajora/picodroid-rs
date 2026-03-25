MEMORY {
    FLASH      : ORIGIN = 0x10000000, LENGTH = 3968K             /* reduced by 128K for PAPK_FLASH */
    PAPK_FLASH : ORIGIN = 0x103E0000, LENGTH = 128K             /* persistent PAPK slot (last 128K of 4MB) */
    RAM        : ORIGIN = 0x20000000, LENGTH = 520K
}
