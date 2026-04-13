/*
 * heap_regions.c – define FreeRTOS heap_5 memory regions from linker symbols.
 *
 * heap_5 requires vPortDefineHeapRegions() to be called before any
 * pvPortMalloc (i.e. before Task::new or start_scheduler).
 *
 * Memory layout (cortex-m-rt):
 *   0x20000000  [.data] [.bss] [.uninit]  __sheap  ...heap...  [ISR stack]  _stack_start
 *
 * We define a single region spanning __sheap to (_stack_start - 4 KB reserve).
 * The 4 KB ISR stack reserve is generous: Cortex-M exception frames are 32 bytes
 * (M0+) to ~100 bytes (M33+FPU), and FreeRTOS nesting is bounded to 2-3 levels.
 */

#include "FreeRTOS.h"
#include "portable.h"

/* Linker symbols — these are addresses, not real variables.  Taking &symbol
 * yields the linker address.  Cast through uintptr_t to avoid GCC's
 * -Warray-bounds false positive on pointer arithmetic past a 1-byte extern. */
extern uint8_t __sheap;       /* first byte after .uninit (from cortex-m-rt) */
extern uint8_t _stack_start;  /* top of RAM (from memory.x via cortex-m-rt) */

#define ISR_STACK_RESERVE  4096u  /* 4 KB for MSP / ISR stack */

void picodroid_define_heap_regions(void) {
    static HeapRegion_t regions[2];
    uint8_t *heap_start = &__sheap;
    uint8_t *heap_end   = (uint8_t *)((uintptr_t)&_stack_start - ISR_STACK_RESERVE);

    configASSERT(heap_end > heap_start);

    regions[0].pucStartAddress = heap_start;
    regions[0].xSizeInBytes    = (size_t)(heap_end - heap_start);
    regions[1].pucStartAddress = NULL;
    regions[1].xSizeInBytes    = 0;

    vPortDefineHeapRegions(regions);
}
