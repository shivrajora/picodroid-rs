/*
 * Minimal pico.h stub – shadows the real pico-sdk pico.h.
 * Only the macros consumed by the FreeRTOS RP2040/RP2350 port are defined here.
 */

#ifndef PICO_H
#define PICO_H

#include "pico_shim.h"

/* Tell portmacro.h to use the pico-sdk v2 spin_try_lock_unsafe definition
 * (avoids the inline fallback in portmacro.h line ~205). */
#define PICO_SDK_VERSION_MAJOR  2

/* Use the static linker-alias vector table (cortex-m-rt handles SVCall/PendSV/SysTick).
 * This causes rp2040_config.h to set configUSE_DYNAMIC_EXCEPTION_HANDLERS=0
 * for the exception handlers, while we still use irq_set_exclusive_handler for
 * hardware IRQs (FIFO/doorbell). */
#define PICO_NO_RAM_VECTOR_TABLE  1

/* Enable multicore FIFO / doorbell support in port.c */
#define LIB_PICO_MULTICORE  1

/* Disable pico_sync and pico_time interop (we don't link against pico_sync) */
#define LIB_PICO_SYNC  0
#define LIB_PICO_TIME  0

/* No interrupt-disabled integer divider (not using pico-sdk divider stubs) */
#define PICO_DIVIDER_DISABLE_INTERRUPTS  1

/* panic_unsupported: called from prvTaskExitError when a task returns (should never happen). */
static inline void panic_unsupported(void) {
    while (1) { __asm volatile("bkpt #0" ::: "memory"); }
}

#endif /* PICO_H */
