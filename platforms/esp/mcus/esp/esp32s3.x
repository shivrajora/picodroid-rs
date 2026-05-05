MEMORY {
  /* Xtensa exception/interrupt vectors — first 4 KiB of IRAM */
  vectors_seg (rx) : ORIGIN = 0x40370000, LENGTH = 4K
  /* IRAM: code/data that must run from RAM (interrupt handlers, cache-unsafe) */
  RWTEXT (rwx)     : ORIGIN = 0x40371000, LENGTH = 316K
  /* 8 MiB flash accessed via cache (XIP read-only) */
  ROTEXT (rx)      : ORIGIN = 0x42000000, LENGTH = 4M
  RODATA (r)       : ORIGIN = 0x42400000, LENGTH = 4M
  /* 512 KiB DRAM (data RAM, read-write) */
  RWDATA (rw)      : ORIGIN = 0x3FC80000, LENGTH = 512K
}
_stack_start_cpu0 = ORIGIN(RWDATA) + LENGTH(RWDATA);
